#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "dropping a HAL token must never be replaced by leaking peripheral ownership"
)]
#![deny(clippy::large_stack_frames)]

extern crate alloc;

use alloc::boxed::Box;

#[cfg(not(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb",
    feature = "board-t-deck-pro"
)))]
compile_error!("select exactly one board feature through `cargo xtask build --board <id>`");
#[cfg(any(
    all(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"),
    all(feature = "board-mks-esp32-foc-v1", feature = "board-mks-tinybee"),
    all(feature = "board-mks-esp32-foc-v1", feature = "board-mks-tinybee-4mb"),
    all(feature = "board-mks-esp32-foc-v1", feature = "board-t-deck-pro"),
    all(feature = "board-mks-tinybee", feature = "board-t-deck-pro"),
    all(feature = "board-mks-tinybee-4mb", feature = "board-t-deck-pro")
))]
compile_error!("multiple board features selected; Alumina images contain exactly one board");
#[cfg(any(
    feature = "hil-mks-tinybee-pcm-short-safe",
    feature = "hil-mks-tinybee-graph-input-timing-safe"
))]
compile_error!("HIL features are isolated to their named `alumina-hil-*` binaries");

mod capability;
mod clock;
mod configuration;
mod graph;
mod graph_platform;
mod hardware;
mod job;
mod motion;
mod network;
pub mod service;
#[cfg(any(
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb",
    feature = "board-t-deck-pro"
))]
mod storage;

use alumina_clock::{BootId, RealtimeClockReport};
use alumina_config::{
    CoreConfigurationAction, CoreConfigurationCommand, RealtimeConfigurationReport,
    RealtimeConfigurationService, RealtimeConfigurationState,
};
use alumina_diagnostics::transport::DiagnosticTransportLimits;
use alumina_diagnostics::{DiagnosticContext, DiagnosticLimits};
use alumina_graph_ir::{CoreGraphCommand, CoreGraphExecutionCommand};
use alumina_job::{
    JobScheduleAction, JobScheduleReport, JobScheduleState, JobStartObservation,
    JobStartObservationSource, RealtimeJobReport, RealtimeJobState,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest, FrameKind};
use alumina_runtime::graph::{
    GraphRuntimeAuthority, RealtimeGraphExecutionReport, RealtimeGraphReport,
};
use alumina_runtime::{
    APP_CORE_STACK_BYTES, APP_CORE_STACK_WORDS, DeadlineProbe, DefaultBoundary,
    DefaultRealtimeEndpoint, DefaultServiceEndpoint, IntercoreFrame, RuntimeBudget, UrgentKind,
};
use alumina_safety::{
    Conditions, Event as SafetyEvent, FaultCode, SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE,
    SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED, SafetyInputMonitor, SafetyInputReaction,
    SafetyInputStatus, SafetyMachine, SafetyObservationPolicy, SafetyObserver, SafetySnapshot,
    SafetyState, safety_input_facts_changed,
};
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, TICK_HZ, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::system::{Cpu, Stack};
use esp_hal::timer::timg::TimerGroup;
use panic_rtt_target as _;
use static_cell::StaticCell;

use alumina_service::diagnostics::{DiagnosticProviderPolicy, DiagnosticServiceState};
use capability::CapabilityService;
use clock::ClockService;
use configuration::ConfigurationService;
use graph::{GraphBridge, GraphService, RealtimeGraphExecutor};
use hardware::selected;
use job::{JobService, RealtimeJobService, ServiceJobContext};
use motion::{MotionAction, MotionService};
use service::{ServiceBridge, StorageServiceState, init_service_bridge};

type TargetSafetyInputMonitor = SafetyInputMonitor<{ selected::SAFETY_INPUT_CAPACITY }>;

static BOUNDARY: StaticCell<DefaultBoundary> = StaticCell::new();
static GRAPH_BRIDGE: StaticCell<GraphBridge> = StaticCell::new();
static APP_CORE_EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();
// Neither initial hardware target has a qualified acquisition provider yet.
// Keep the authenticated dispatcher and context reconciliation compiled, but
// do not reserve unusable request/event/record storage in scarce internal
// SRAM. A board composition must add explicit nonzero budgets at the same time
// that it installs and qualifies a provider.
type TargetDiagnosticService = DiagnosticServiceState<0, 0, 0, 0>;
static DIAGNOSTIC_SERVICE: StaticCell<TargetDiagnosticService> = StaticCell::new();

const SAFETY_OBSERVATION_MAX_AGE_CYCLES: u64 = Duration::from_millis(500).as_ticks();
/// Small general internal heap retained alongside the separate 64 KiB reclaimed region.
///
/// The permanent split-core graph actors now reserve both package images and
/// every execution arena statically. Retaining those fail-closed bounds costs
/// 28 KiB formerly available to this secondary region. Target qualification
/// must measure both allocator regions' low-water marks under Wi-Fi load.
const GENERAL_HEAP_BYTES: usize = 4 * 1_024;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();

    if let Err(reason) = selected::PACKAGE.validate() {
        error!(
            "invalid compile-time board package: {:?}",
            defmt::Debug2Format(&reason)
        );
        panic!("invalid compile-time board package");
    }
    let budget = RuntimeBudget {
        internal_bytes: 64 * 1_024,
        app_core_stack_words: APP_CORE_STACK_WORDS,
        maximum_service_stall_cycles: 10_000,
        minimum_realtime_horizon_cycles: 20_000,
    };
    if budget
        .validate_for::<
            { alumina_runtime::COMMAND_QUEUE_DEPTH },
            { alumina_runtime::TELEMETRY_QUEUE_DEPTH },
            { alumina_runtime::COMMAND_PAYLOAD_BYTES },
            { alumina_runtime::TELEMETRY_PAYLOAD_BYTES },
        >()
        .is_err()
    {
        panic!("invalid compile-time runtime budget");
    }

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let device_id = DeviceId::from_esp_base_mac(esp_hal::efuse::Efuse::read_base_mac_address());
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1_024);
    // The reclaimed region is the sole registered heap at this point, so this
    // permanent allocation deterministically reserves the core-1 stack there.
    // The remaining half stays available to later allocations.
    let app_stack = Box::leak(Box::write(
        Box::<Stack<APP_CORE_STACK_BYTES>>::new_uninit(),
        Stack::new(),
    ));
    esp_alloc::heap_allocator!(size: GENERAL_HEAP_BYTES);
    let mut split = selected::split(peripherals);

    let timer_group0 = TimerGroup::new(split.runtime.timer_group0);
    let software_interrupt = SoftwareInterruptControl::new(split.runtime.software_interrupt);
    esp_rtos::start(timer_group0.timer0);

    let boundary = BOUNDARY.init(DefaultBoundary::new());
    let graph_bridge: &'static GraphBridge = GRAPH_BRIDGE.init(GraphBridge::new());
    let (mut service_endpoint, realtime_endpoint) = boundary.split();
    esp_rtos::start_second_core(
        split.runtime.cpu_control,
        software_interrupt.software_interrupt0,
        software_interrupt.software_interrupt1,
        app_stack,
        move || {
            let app_executor = APP_CORE_EXECUTOR.init(esp_rtos::embassy::Executor::new());
            app_executor.run(move |realtime_spawner| {
                realtime_spawner.must_spawn(realtime_task(
                    split.realtime,
                    realtime_endpoint,
                    device_id,
                    graph_bridge,
                ));
            });
        },
    );

    // Hazardous outputs are established by their sole core-1 owner. Wi-Fi is
    // intentionally not initialized until a fresh contract-bound `Safe`
    // snapshot crosses the owned inter-core boundary.
    await_initial_safe_snapshot(&mut service_endpoint).await;
    let service_bridge = init_service_bridge();
    let network = network::start(
        spawner,
        split.service.take_wifi(),
        service_bridge,
        device_id,
    )
    .await;

    spawner.must_spawn(service_task(
        split.service,
        service_endpoint,
        network,
        service_bridge,
        graph_bridge,
    ));
    info!(
        "Alumina dual-core runtime started for {}",
        env!("ALUMINA_BOARD_ID")
    );

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

async fn await_initial_safe_snapshot(endpoint: &mut DefaultServiceEndpoint) {
    let mut observer = SafetyObserver::new(SafetyObservationPolicy {
        expected_contract: selected::SAFE_OUTPUT_CONTRACT,
        maximum_age_cycles: SAFETY_OBSERVATION_MAX_AGE_CYCLES,
    });
    loop {
        let frame = endpoint.receive_telemetry().await;
        let now = DeviceCycle(Instant::now().as_ticks());
        if frame.validate(FrameKind::Telemetry).is_err() {
            observer.invalidate();
            endpoint.publish_urgent(UrgentKind::EmergencyStop, 1);
            continue;
        }
        let payload = match frame.payload() {
            Ok(payload) => payload,
            Err(_) => {
                observer.invalidate();
                endpoint.publish_urgent(UrgentKind::EmergencyStop, 2);
                continue;
            }
        };
        if observer
            .observe_encoded(
                frame.header().sequence,
                frame.header().cycle.0,
                now.0,
                payload,
            )
            .is_err()
        {
            endpoint.publish_urgent(UrgentKind::EmergencyStop, 3);
            continue;
        }
        match observer.effective(now.0).state {
            SafetyState::Safe => {
                info!("core-1 safe-output contract established before Wi-Fi");
                return;
            }
            SafetyState::Fault => {
                error!("core-1 safe-output establishment faulted before Wi-Fi");
                panic!("safe-output establishment failed");
            }
            _ => {}
        }
    }
}

#[embassy_executor::task]
async fn service_task(
    mut resources: selected::ServiceResources,
    mut endpoint: DefaultServiceEndpoint,
    network: network::NetworkControl,
    service_bridge: &'static ServiceBridge,
    graph_bridge: &'static GraphBridge,
) {
    if Cpu::current() != Cpu::ProCpu {
        panic!("service executor started on the wrong core");
    }

    // Service admission and every future media/backend handle live only in the
    // core-0 task future. Core 1 receives verified owned blocks, never SD.
    let mut storage = StorageServiceState::new(
        selected::SAFE_OUTPUT_CONTRACT,
        SAFETY_OBSERVATION_MAX_AGE_CYCLES,
    );
    let mut storage_backend = resources.initialize_storage().await;
    let boot_id = BootId::new(network.boot_nonce().as_bytes())
        .unwrap_or_else(|_| panic!("network boot nonce did not form a clock identity"));
    let mut clocks = ClockService::new(boot_id);
    let mut jobs = JobService::new(boot_id);
    let mut configurations = ConfigurationService::new();
    let mut graphs = GraphService::new(network.device_id(), graph_bridge);
    let diagnostics = DIAGNOSTIC_SERVICE.init(TargetDiagnosticService::new(
        DiagnosticContext {
            device_id: network.device_id(),
            boot_id,
            capability: network.capability_identity(),
            config_digest: Digest::ZERO,
            clock_frequency_hz: TICK_HZ,
        },
        DiagnosticProviderPolicy::NONE,
        DiagnosticTransportLimits::native_control(),
        DiagnosticLimits::interactive(),
    ));
    let mut last_fault_generation = 0_u16;
    loop {
        while let Ok(frame) = endpoint.try_receive_telemetry() {
            let now = DeviceCycle(Instant::now().as_ticks());
            let valid = match frame.header().kind {
                FrameKind::Telemetry => {
                    frame.validate(FrameKind::Telemetry).is_ok()
                        && frame.payload().is_ok_and(|payload| {
                            storage
                                .observe_safety_snapshot(
                                    frame.header().sequence,
                                    frame.header().cycle,
                                    now,
                                    payload,
                                )
                                .is_ok()
                        })
                }
                FrameKind::Job => {
                    frame.validate(FrameKind::Job).is_ok()
                        && frame.payload().is_ok_and(|payload| {
                            match RealtimeJobReport::decode(payload) {
                                Ok(report) => jobs
                                    .observe_realtime(frame.header().config_digest, report)
                                    .is_ok(),
                                Err(_) => match JobScheduleReport::decode(payload) {
                                    Ok(report) => jobs
                                        .observe_schedule(frame.header().config_digest, report)
                                        .is_ok(),
                                    Err(_) => false,
                                },
                            }
                        })
                }
                FrameKind::ClockSample => {
                    frame.validate(FrameKind::ClockSample).is_ok()
                        && frame.payload().is_ok_and(|payload| {
                            RealtimeClockReport::decode(payload).is_ok_and(|report| {
                                clocks
                                    .observe_realtime(
                                        frame.header().sequence,
                                        frame.header().cycle,
                                        now,
                                        report,
                                    )
                                    .is_ok()
                            })
                        })
                }
                FrameKind::Configuration => {
                    frame.validate(FrameKind::Configuration).is_ok()
                        && frame.payload().is_ok_and(|payload| {
                            RealtimeConfigurationReport::decode(payload).is_ok_and(|report| {
                                frame.header().config_digest == report.active_digest
                                    && configurations.observe_realtime(report).is_ok()
                            })
                        })
                }
                FrameKind::Graph => {
                    frame.validate(FrameKind::Graph).is_ok()
                        && frame.payload().is_ok_and(|payload| {
                            if let Ok(report) = RealtimeGraphReport::decode(payload) {
                                graphs
                                    .observe_realtime(frame.header().config_digest, report)
                                    .is_ok()
                            } else {
                                RealtimeGraphExecutionReport::decode(payload).is_ok_and(|report| {
                                    graphs
                                        .observe_realtime_execution(
                                            frame.header().config_digest,
                                            report,
                                        )
                                        .is_ok()
                                })
                            }
                        })
                }
                _ => false,
            };
            if !valid {
                storage.invalidate_safety_observation();
                clocks.invalidate_realtime();
                endpoint.publish_urgent(UrgentKind::EmergencyStop, 1);
            }
        }
        if let Some(fault) = endpoint.fault_after(last_fault_generation) {
            last_fault_generation = fault.generation;
            storage.invalidate_safety_observation();
            clocks.invalidate_realtime();
            error!("RT fault code={} detail={}", fault.code, fault.detail);
        }

        let now = DeviceCycle(Instant::now().as_ticks());
        storage.set_service_job_active(jobs.excludes_storage_mutation(&endpoint));
        storage.set_configuration_transaction_active(
            configurations.blocks_job_admission() || graphs.blocks_storage_mutation(),
        );
        graphs.set_active_config(configurations.authorized_digest());
        let mut configuration_mutation = storage.configuration_mutation_context(now);
        configuration_mutation.realtime_job_active |= graphs.blocks_configuration_mutation();
        configurations
            .step(
                &mut storage_backend,
                &mut endpoint,
                now,
                configuration_mutation,
            )
            .await;
        graphs.set_active_config(configurations.authorized_digest());
        graphs
            .step(
                &mut storage_backend,
                &mut endpoint,
                now,
                storage.configuration_mutation_context(now),
            )
            .await;
        jobs.set_configuration_transition(
            configurations.blocks_job_admission() || graphs.blocks_job_admission(),
        );
        jobs.set_active_config(configurations.authorized_digest());
        storage.set_configuration_active(
            configurations.has_durable_active() || graphs.blocks_configuration_mutation(),
        );
        diagnostics.rebind_context(DiagnosticContext {
            device_id: network.device_id(),
            boot_id,
            capability: network.capability_identity(),
            config_digest: configurations.authorized_digest(),
            clock_frequency_hz: TICK_HZ,
        });

        while let Some(request) = service_bridge.try_receive() {
            storage.set_service_job_active(jobs.excludes_storage_mutation(&endpoint));
            storage.set_configuration_active(
                configurations.has_durable_active() || graphs.blocks_configuration_mutation(),
            );
            storage.set_configuration_transaction_active(
                configurations.blocks_job_admission() || graphs.blocks_storage_mutation(),
            );
            jobs.set_configuration_transition(
                configurations.blocks_job_admission() || graphs.blocks_job_admission(),
            );
            jobs.set_active_config(configurations.authorized_digest());
            graphs.set_active_config(configurations.authorized_digest());
            diagnostics.rebind_context(DiagnosticContext {
                device_id: network.device_id(),
                boot_id,
                capability: network.capability_identity(),
                config_digest: configurations.authorized_digest(),
                clock_frequency_hz: TICK_HZ,
            });
            let now = DeviceCycle(Instant::now().as_ticks());
            let response = if ClockService::handles(request.request()) {
                clocks.dispatch(
                    request.request(),
                    now,
                    &endpoint,
                    storage.effective_safety(now),
                    jobs.clock_facts(now),
                )
            } else if CapabilityService::handles(request.request()) {
                CapabilityService::dispatch(request.request(), now)
            } else if ConfigurationService::handles(request.request()) {
                let mut mutation = storage.configuration_mutation_context(now);
                mutation.realtime_job_active |= graphs.blocks_configuration_mutation();
                configurations
                    .dispatch(&mut storage_backend, request.request(), now, mutation)
                    .await
            } else if GraphService::handles(request.request()) {
                graphs
                    .dispatch(
                        &mut storage_backend,
                        request.request(),
                        now,
                        storage.configuration_mutation_context(now),
                    )
                    .await
            } else if JobService::handles(request.request()) {
                let latest_probe = clocks.latest_probe_id(now);
                jobs.dispatch(
                    &mut storage_backend,
                    &mut endpoint,
                    request.request(),
                    now,
                    ServiceJobContext::new(
                        latest_probe,
                        storage.effective_safety(now).state,
                        configurations.authorized_servo_configuration(),
                    ),
                )
                .await
            } else if TargetDiagnosticService::handles(request.request()) {
                diagnostics.dispatch(request.request(), now)
            } else {
                storage
                    .dispatch(&mut storage_backend, request.request(), now)
                    .await
            };
            service_bridge.respond(&request, response);
        }

        if jobs
            .prefetch_step(&mut storage_backend, &mut endpoint)
            .await
            .is_err()
        {
            endpoint.publish_urgent(UrgentKind::EmergencyStop, 4);
            error!("cached job prefetch faulted closed");
        }

        let _keep_service_state_core_local = (
            &resources,
            &storage,
            &storage_backend,
            &jobs,
            &clocks,
            &configurations,
            &graphs,
            network.supervisor(),
            network.credential_source(),
        );
        let ordinary_wake = Instant::now() + Duration::from_millis(10);
        let wake = graphs.next_release_cycle().map_or(ordinary_wake, |cycle| {
            ordinary_wake.min(Instant::from_ticks(cycle.0))
        });
        Timer::at(wake).await;
    }
}

#[embassy_executor::task]
async fn realtime_task(
    resources: selected::RealtimeResources,
    mut endpoint: DefaultRealtimeEndpoint,
    device_id: DeviceId,
    graph_bridge: &'static GraphBridge,
) {
    if Cpu::current() != Cpu::AppCpu {
        endpoint.publish_fault(1, 0);
        panic!("realtime executor started on the wrong core");
    }

    let mut resources = match resources.establish_safe_outputs() {
        Ok(resources) => resources,
        Err(_) => {
            hold_safe_output_fault(&mut endpoint, 0).await;
        }
    };
    let mut safety = SafetyMachine::new();
    let mut safe_outputs_established = true;
    if safety
        .apply(
            SafetyEvent::Initialize,
            Conditions {
                safe_outputs_established: true,
                ..Conditions::default()
            },
        )
        .is_err()
    {
        hold_safe_output_fault(&mut endpoint, 1).await;
    }

    let period = Duration::from_millis(1);
    let mut expected = Instant::now() + period;
    let mut probe = DeadlineProbe::default();
    let mut telemetry_sequence = 0_u32;
    let mut transition_generation = 1_u32;
    let mut divider = 0_u8;
    let mut urgent_generation = 0_u16;
    let mut jobs = RealtimeJobService::new();
    let mut motion = MotionService::new();
    let mut configurations =
        RealtimeConfigurationService::<{ selected::CONFIGURATION_BINDINGS }>::new(
            selected::PACKAGE,
        );
    let mut graphs = RealtimeGraphExecutor::new(device_id, graph_bridge);
    let mut safety_inputs: Option<TargetSafetyInputMonitor> = None;
    let mut safety_input_status = SafetyInputStatus::unconfigured();
    let mut last_job_active = false;
    let mut configuration_sequence = 0_u32;
    let mut graph_sequence = 0_u32;
    let mut clock_sequence = 0_u32;

    publish_safety_snapshot(
        &mut endpoint,
        &mut telemetry_sequence,
        Instant::now(),
        safety,
        transition_generation,
        safety_snapshot_flags(safe_outputs_established, false),
        safety_input_status,
        0,
    );

    loop {
        let schedule_wake = jobs.next_schedule_deadline();
        let motion_wake = motion.next_deadline();
        let graph_wake = graphs.next_release_cycle();
        let wake = minimum_wake_cycle(DeviceCycle(expected.as_ticks()), schedule_wake, motion_wake);
        let wake = graph_wake.map_or(wake, |deadline| wake.min(deadline));
        Timer::at(Instant::from_ticks(wake.0)).await;
        let observed = Instant::now();
        let management_due = observed >= expected;
        if management_due {
            probe.observe(
                DeviceCycle(expected.as_ticks()),
                DeviceCycle(observed.as_ticks()),
                100,
            );
            loop {
                expected += period;
                if expected > observed {
                    break;
                }
            }
        }

        let now = DeviceCycle(observed.as_ticks());
        let schedule_due = schedule_wake.is_some_and(|deadline| deadline <= now);
        // The first asynchronous fault is terminal until a qualified physical
        // reset path exists. Stop touching the faulted input monitor so a
        // deliberately non-healing sample-gap latch cannot retrigger work on
        // every real-time pass.
        if management_due
            && safety.state() != SafetyState::Fault
            && let Some(scan) = safety_inputs
                .as_mut()
                .map(|monitor| resources.scan_safety_inputs(monitor, now))
        {
            match scan {
                Ok(scan) => {
                    if safety_input_facts_changed(scan.status, safety_input_status) {
                        transition_generation = next_nonzero(transition_generation);
                    }
                    safety_input_status = scan.status;
                    match scan.reaction {
                        Some(SafetyInputReaction::Fault(fault)) => {
                            latch_realtime_fault(
                                &mut resources,
                                &mut motion,
                                &mut jobs,
                                &mut safety,
                                &mut endpoint,
                                &mut safe_outputs_established,
                                &mut transition_generation,
                                &mut telemetry_sequence,
                                now,
                                observed,
                                probe.maximum_lateness_cycles(),
                                safety_input_status,
                                fault,
                                scan.reaction_slot,
                            );
                        }
                        Some(SafetyInputReaction::Hold) => {
                            if matches!(
                                safety.state(),
                                SafetyState::Armed | SafetyState::Running | SafetyState::Hold
                            ) {
                                // Until a board backend has a qualified constrained
                                // hold, a probe request degrades to a safe stop.
                                request_realtime_stop(
                                    &mut resources,
                                    &mut motion,
                                    &mut jobs,
                                    &mut safety,
                                    &mut endpoint,
                                    &mut safe_outputs_established,
                                    &mut transition_generation,
                                    &mut telemetry_sequence,
                                    now,
                                    observed,
                                    probe.maximum_lateness_cycles(),
                                    safety_input_status,
                                    scan.reaction_slot,
                                );
                            }
                        }
                        Some(SafetyInputReaction::Clear) | None => {}
                    }
                }
                Err(_) => {
                    if let Some(monitor) = safety_inputs.as_ref() {
                        let status = monitor.status(now);
                        if safety_input_facts_changed(status, safety_input_status) {
                            transition_generation = next_nonzero(transition_generation);
                        }
                        safety_input_status = status;
                    }
                    latch_realtime_fault(
                        &mut resources,
                        &mut motion,
                        &mut jobs,
                        &mut safety,
                        &mut endpoint,
                        &mut safe_outputs_established,
                        &mut transition_generation,
                        &mut telemetry_sequence,
                        now,
                        observed,
                        probe.maximum_lateness_cycles(),
                        safety_input_status,
                        FaultCode::Identity,
                        6,
                    );
                }
            }
        }

        if let Some(urgent) = endpoint.urgent_after(urgent_generation) {
            urgent_generation = urgent.generation;
            match urgent.code {
                code if code == UrgentKind::Hold as u8 || code == UrgentKind::Stop as u8 => {
                    // The first target backend has no qualified constrained
                    // deceleration yet, so Hold intentionally degrades to Stop.
                    request_realtime_stop(
                        &mut resources,
                        &mut motion,
                        &mut jobs,
                        &mut safety,
                        &mut endpoint,
                        &mut safe_outputs_established,
                        &mut transition_generation,
                        &mut telemetry_sequence,
                        now,
                        observed,
                        probe.maximum_lateness_cycles(),
                        safety_input_status,
                        urgent.detail,
                    );
                }
                code if code == UrgentKind::EmergencyStop as u8 => latch_realtime_fault(
                    &mut resources,
                    &mut motion,
                    &mut jobs,
                    &mut safety,
                    &mut endpoint,
                    &mut safe_outputs_established,
                    &mut transition_generation,
                    &mut telemetry_sequence,
                    now,
                    observed,
                    probe.maximum_lateness_cycles(),
                    safety_input_status,
                    FaultCode::EmergencyStop,
                    urgent.detail,
                ),
                code if code == UrgentKind::ResetRequest as u8 => {
                    // Network intent alone never satisfies physical reset policy.
                }
                _ => latch_realtime_fault(
                    &mut resources,
                    &mut motion,
                    &mut jobs,
                    &mut safety,
                    &mut endpoint,
                    &mut safe_outputs_established,
                    &mut transition_generation,
                    &mut telemetry_sequence,
                    now,
                    observed,
                    probe.maximum_lateness_cycles(),
                    safety_input_status,
                    FaultCode::Identity,
                    7,
                ),
            }
        }
        if management_due {
            while let Ok(command) = endpoint.try_receive_command() {
                let mut safety_changed = false;
                let valid = match command.header().kind {
                    FrameKind::Job if graphs.active_identity().is_none() => jobs
                        .apply_command(
                            &mut endpoint,
                            &command,
                            DeviceCycle(observed.as_ticks()),
                            safety.state(),
                            probe.misses() == 0,
                            configurations.authorized_configuration(),
                        )
                        .is_ok(),
                    FrameKind::Job => false,
                    FrameKind::Configuration => match apply_configuration_command(
                        &mut configurations,
                        &mut resources,
                        &mut motion,
                        &mut safety_inputs,
                        &mut safety_input_status,
                        &mut jobs,
                        &mut safety,
                        &mut endpoint,
                        &command,
                        DeviceCycle(observed.as_ticks()),
                        period.as_ticks(),
                        &mut configuration_sequence,
                        &graphs,
                    ) {
                        Ok(changed) => {
                            safety_changed = changed;
                            true
                        }
                        Err(()) => false,
                    },
                    FrameKind::Graph => apply_graph_command(
                        &mut graphs,
                        &configurations,
                        &jobs,
                        &safety,
                        &mut endpoint,
                        &command,
                        DeviceCycle(observed.as_ticks()),
                        &mut graph_sequence,
                    )
                    .is_ok(),
                    FrameKind::Command => command.validate(FrameKind::Command).is_ok(),
                    _ => false,
                };
                if !valid {
                    latch_realtime_fault(
                        &mut resources,
                        &mut motion,
                        &mut jobs,
                        &mut safety,
                        &mut endpoint,
                        &mut safe_outputs_established,
                        &mut transition_generation,
                        &mut telemetry_sequence,
                        now,
                        observed,
                        probe.maximum_lateness_cycles(),
                        safety_input_status,
                        FaultCode::Identity,
                        3,
                    );
                }
                if safety_changed {
                    transition_generation = next_nonzero(transition_generation);
                    publish_safety_snapshot(
                        &mut endpoint,
                        &mut telemetry_sequence,
                        observed,
                        safety,
                        transition_generation,
                        safety_snapshot_flags(safe_outputs_established, jobs.active()),
                        safety_input_status,
                        probe.maximum_lateness_cycles(),
                    );
                }
            }
            if jobs
                .preadmit(&mut endpoint, DeviceCycle(observed.as_ticks()))
                .is_err()
            {
                latch_realtime_fault(
                    &mut resources,
                    &mut motion,
                    &mut jobs,
                    &mut safety,
                    &mut endpoint,
                    &mut safe_outputs_established,
                    &mut transition_generation,
                    &mut telemetry_sequence,
                    now,
                    observed,
                    probe.maximum_lateness_cycles(),
                    safety_input_status,
                    FaultCode::Identity,
                    4,
                );
            }

            let authorized_configuration = configurations.authorized_configuration();
            let arm_inputs = ArmReconciliationInputs {
                configuration_authorized: authorized_configuration.is_some(),
                target_configuration_ready: authorized_configuration.is_some_and(|configuration| {
                    resources.target_configuration_ready(configuration)
                }),
                safe_outputs_established,
                safety_inputs: safety_input_status,
                deadline_healthy: probe.misses() == 0,
            };
            match reconcile_arm_state(&jobs, &motion, &mut safety, arm_inputs) {
                Ok(true) => {
                    transition_generation = next_nonzero(transition_generation);
                    publish_safety_snapshot(
                        &mut endpoint,
                        &mut telemetry_sequence,
                        observed,
                        safety,
                        transition_generation,
                        safety_snapshot_flags(safe_outputs_established, jobs.active()),
                        safety_input_status,
                        probe.maximum_lateness_cycles(),
                    );
                }
                Ok(false) => {}
                Err(()) => latch_realtime_fault(
                    &mut resources,
                    &mut motion,
                    &mut jobs,
                    &mut safety,
                    &mut endpoint,
                    &mut safe_outputs_established,
                    &mut transition_generation,
                    &mut telemetry_sequence,
                    now,
                    observed,
                    probe.maximum_lateness_cycles(),
                    safety_input_status,
                    FaultCode::Identity,
                    8,
                ),
            }
        }

        let graph_before = graphs.execution_report();
        let graph_execution_allowed =
            matches!(safety.state(), SafetyState::Safe | SafetyState::Configured) && !jobs.active();
        let graph_release = graphs.release_due(now, graph_execution_allowed, |resource| {
            safety_inputs
                .as_ref()
                .and_then(|monitor| monitor.stable_active_by_resource(resource, now))
        });
        let graph_after = graphs.execution_report();
        if graph_release.is_err() || graph_after != graph_before {
            let _ = publish_graph_execution_report(
                &mut endpoint,
                &mut graph_sequence,
                now,
                configurations
                    .authorized_identity()
                    .map_or(Digest::ZERO, |identity| identity.digest),
                graph_after,
            );
        }

        let schedule_action = if management_due || schedule_due {
            jobs.advance_schedule(&mut endpoint, DeviceCycle(observed.as_ticks()))
                .unwrap_or(JobScheduleAction::MissedStart)
        } else {
            JobScheduleAction::None
        };
        let schedule_fault = match schedule_action {
            JobScheduleAction::None | JobScheduleAction::AbortUnconfirmed => None,
            JobScheduleAction::PrimeHardware {
                scheduled_cycle, ..
            } => match prime_realtime_motion(
                &mut resources,
                &mut motion,
                &mut jobs,
                &safety,
                &mut endpoint,
                scheduled_cycle,
                now,
                safety_input_status,
                probe.misses() == 0,
            ) {
                Ok(()) => None,
                Err(()) => Some(FaultCode::Driver),
            },
            JobScheduleAction::Start {
                scheduled_cycle, ..
            } => match start_realtime_motion(
                &mut motion,
                &mut jobs,
                &mut safety,
                scheduled_cycle,
                safety_input_status,
                probe.misses() == 0,
            ) {
                Ok(()) => {
                    transition_generation = next_nonzero(transition_generation);
                    publish_safety_snapshot(
                        &mut endpoint,
                        &mut telemetry_sequence,
                        observed,
                        safety,
                        transition_generation,
                        safety_snapshot_flags(safe_outputs_established, jobs.active()),
                        safety_input_status,
                        probe.maximum_lateness_cycles(),
                    );
                    None
                }
                Err(()) => Some(FaultCode::Identity),
            },
            JobScheduleAction::MissedStart => Some(FaultCode::Deadline),
            JobScheduleAction::LeaseExpired => Some(FaultCode::Watchdog),
        };
        if let Some(fault) = schedule_fault {
            latch_realtime_fault(
                &mut resources,
                &mut motion,
                &mut jobs,
                &mut safety,
                &mut endpoint,
                &mut safe_outputs_established,
                &mut transition_generation,
                &mut telemetry_sequence,
                now,
                observed,
                probe.maximum_lateness_cycles(),
                safety_input_status,
                fault,
                5,
            );
        }

        match service_realtime_motion(
            &mut resources,
            &mut motion,
            &mut jobs,
            &mut safety,
            &mut endpoint,
        ) {
            Ok(true) => {
                transition_generation = next_nonzero(transition_generation);
                publish_safety_snapshot(
                    &mut endpoint,
                    &mut telemetry_sequence,
                    Instant::now(),
                    safety,
                    transition_generation,
                    safety_snapshot_flags(safe_outputs_established, jobs.active()),
                    safety_input_status,
                    probe.maximum_lateness_cycles(),
                );
            }
            Ok(false) => {}
            Err(()) => {
                let fault_observed = Instant::now();
                latch_realtime_fault(
                    &mut resources,
                    &mut motion,
                    &mut jobs,
                    &mut safety,
                    &mut endpoint,
                    &mut safe_outputs_established,
                    &mut transition_generation,
                    &mut telemetry_sequence,
                    DeviceCycle(fault_observed.as_ticks()),
                    fault_observed,
                    probe.maximum_lateness_cycles(),
                    safety_input_status,
                    FaultCode::Driver,
                    9,
                );
            }
        }

        let job_active = jobs.active();
        if job_active != last_job_active {
            last_job_active = job_active;
            transition_generation = next_nonzero(transition_generation);
            publish_safety_snapshot(
                &mut endpoint,
                &mut telemetry_sequence,
                observed,
                safety,
                transition_generation,
                safety_snapshot_flags(safe_outputs_established, job_active),
                safety_input_status,
                probe.maximum_lateness_cycles(),
            );
        }

        if management_due {
            divider = divider.wrapping_add(1);
            if divider == 100 {
                divider = 0;
                publish_safety_snapshot(
                    &mut endpoint,
                    &mut telemetry_sequence,
                    observed,
                    safety,
                    transition_generation,
                    safety_snapshot_flags(safe_outputs_established, job_active),
                    safety_input_status,
                    probe.maximum_lateness_cycles(),
                );
                if jobs.has_job() {
                    let _ = jobs.publish_report(&mut endpoint, DeviceCycle(observed.as_ticks()));
                }
                let _ = publish_configuration_report(
                    &mut endpoint,
                    &mut configuration_sequence,
                    DeviceCycle(observed.as_ticks()),
                    configurations.report(),
                );
                let _ = publish_graph_report(
                    &mut endpoint,
                    &mut graph_sequence,
                    DeviceCycle(observed.as_ticks()),
                    configurations
                        .authorized_identity()
                        .map_or(Digest::ZERO, |identity| identity.digest),
                    graphs.lifecycle_report(),
                );
                let _ = publish_graph_execution_report(
                    &mut endpoint,
                    &mut graph_sequence,
                    DeviceCycle(observed.as_ticks()),
                    configurations
                        .authorized_identity()
                        .map_or(Digest::ZERO, |identity| identity.digest),
                    graphs.execution_report(),
                );
                let _ = publish_clock_report(
                    &mut endpoint,
                    &mut clock_sequence,
                    DeviceCycle(observed.as_ticks()),
                    probe,
                );
            }
        }

        let _keep_tokens_core_local = &resources;
    }
}

fn start_realtime_motion(
    motion: &mut MotionService,
    jobs: &mut RealtimeJobService,
    safety: &mut SafetyMachine,
    scheduled_cycle: DeviceCycle,
    safety_inputs: SafetyInputStatus,
    deadline_healthy: bool,
) -> Result<(), ()> {
    if safety.state() != SafetyState::Armed || !safety_inputs.ready_to_arm() || !deadline_healthy {
        return Err(());
    }
    if jobs.schedule_state() != Some(alumina_job::JobScheduleState::Running) {
        return Err(());
    }
    motion.start(scheduled_cycle).map_err(|_| ())?;
    safety
        .apply(
            SafetyEvent::Start,
            Conditions {
                safe_outputs_established: true,
                configuration_valid: true,
                interlocks_closed: true,
                buffer_ready: true,
                physical_reset_confirmed: false,
            },
        )
        .map_err(|_| ())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prime_realtime_motion(
    resources: &mut selected::EstablishedRealtimeResources,
    motion: &mut MotionService,
    jobs: &mut RealtimeJobService,
    safety: &SafetyMachine,
    endpoint: &mut DefaultRealtimeEndpoint,
    scheduled_cycle: DeviceCycle,
    observed: DeviceCycle,
    safety_inputs: SafetyInputStatus,
    deadline_healthy: bool,
) -> Result<(), ()> {
    if safety.state() != SafetyState::Armed || !safety_inputs.ready_to_arm() || !deadline_healthy {
        return Err(());
    }
    let descriptor = jobs.descriptor().ok_or(())?;
    let admitted = jobs.take_admitted().ok_or(())?;
    let lookahead = jobs.take_admitted();
    motion
        .prime(
            resources,
            descriptor,
            scheduled_cycle,
            admitted,
            lookahead,
            observed,
        )
        .map_err(|_| ())?;
    jobs.mark_hardware_primed(endpoint, observed)
}

fn minimum_wake_cycle(
    management: DeviceCycle,
    schedule: Option<DeviceCycle>,
    motion: Option<DeviceCycle>,
) -> DeviceCycle {
    let mut wake = management;
    if let Some(schedule) = schedule {
        wake = wake.min(schedule);
    }
    if let Some(motion) = motion {
        wake = wake.min(motion);
    }
    wake
}

/// Services every transaction due at the same observed cycle, while bounding
/// internal zero-time handoffs between adjacent cached blocks.
fn service_realtime_motion(
    resources: &mut selected::EstablishedRealtimeResources,
    motion: &mut MotionService,
    jobs: &mut RealtimeJobService,
    safety: &mut SafetyMachine,
    endpoint: &mut DefaultRealtimeEndpoint,
) -> Result<bool, ()> {
    if safety.state() != SafetyState::Running {
        return Ok(false);
    }
    if !motion.started() {
        return Err(());
    }
    let mut handoffs = 0_u8;
    loop {
        if handoffs == 16 {
            return Err(());
        }
        handoffs += 1;
        let now = DeviceCycle(Instant::now().as_ticks());
        match motion.poll(resources, now).map_err(|_| ())? {
            MotionAction::Future { at } => {
                if at <= now {
                    return Err(());
                }
                return Ok(false);
            }
            MotionAction::OutputCommitted => {}
            MotionAction::StartOutputCommitted {
                output_token,
                scheduled_at,
                committed_at,
            } => {
                let observation = JobStartObservation {
                    source: JobStartObservationSource::PeripheralLatch,
                    output_token,
                    scheduled_cycle: scheduled_at,
                    earliest_cycle: committed_at,
                    latest_cycle: committed_at,
                };
                if jobs.record_start_observation(endpoint, now, observation)?
                    == JobScheduleState::Faulted
                {
                    return Err(());
                }
            }
            MotionAction::NeedBlock { at } => {
                if at <= now {
                    return Err(());
                }
                jobs.preadmit(endpoint, now)?;
                if let Some(next) = jobs.take_admitted() {
                    motion.admit(next).map_err(|_| ())?;
                    continue;
                }
                return Ok(false);
            }
            MotionAction::WaitingForHardware => return Ok(false),
            MotionAction::BlockComplete(admitted) => {
                match jobs.acknowledge_executed(endpoint, now, admitted)? {
                    RealtimeJobState::Complete => motion.request_finish().map_err(|_| ())?,
                    RealtimeJobState::Prepared | RealtimeJobState::Admitted => {
                        jobs.preadmit(endpoint, now)?;
                        if let Some(next) = jobs.take_admitted() {
                            motion.admit(next).map_err(|_| ())?;
                        }
                    }
                    RealtimeJobState::Cancelled | RealtimeJobState::Faulted => return Err(()),
                }
            }
            MotionAction::JobComplete => {
                jobs.complete_schedule(endpoint, now)?;
                safety
                    .apply(SafetyEvent::Finish, Conditions::default())
                    .map_err(|_| ())?;
                return Ok(true);
            }
            MotionAction::Idle => return Err(()),
        }
    }
}

#[derive(Clone, Copy)]
struct ArmReconciliationInputs {
    configuration_authorized: bool,
    target_configuration_ready: bool,
    safe_outputs_established: bool,
    safety_inputs: SafetyInputStatus,
    deadline_healthy: bool,
}

fn reconcile_arm_state(
    jobs: &RealtimeJobService,
    motion: &MotionService,
    safety: &mut SafetyMachine,
    inputs: ArmReconciliationInputs,
) -> Result<bool, ()> {
    if safety.state() == SafetyState::Configured
        && inputs.configuration_authorized
        && inputs.target_configuration_ready
        && jobs.ready_to_arm()
        && motion.ready_to_arm(jobs.descriptor())
        && inputs.safe_outputs_established
        && inputs.safety_inputs.ready_to_arm()
        && inputs.deadline_healthy
    {
        safety
            .apply(
                SafetyEvent::Arm,
                Conditions {
                    safe_outputs_established: inputs.safe_outputs_established,
                    configuration_valid: true,
                    interlocks_closed: true,
                    buffer_ready: true,
                    physical_reset_confirmed: false,
                },
            )
            .map_err(|_| ())?;
        return Ok(true);
    }
    if safety.state() == SafetyState::Armed
        && !matches!(
            jobs.schedule_state(),
            Some(
                alumina_job::JobScheduleState::Installed
                    | alumina_job::JobScheduleState::Confirmed
                    | alumina_job::JobScheduleState::Priming
                    | alumina_job::JobScheduleState::Primed
            )
        )
    {
        safety
            .apply(SafetyEvent::Disarm, Conditions::default())
            .map_err(|_| ())?;
        return Ok(true);
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn latch_realtime_fault(
    resources: &mut selected::EstablishedRealtimeResources,
    motion: &mut MotionService,
    jobs: &mut RealtimeJobService,
    safety: &mut SafetyMachine,
    endpoint: &mut DefaultRealtimeEndpoint,
    safe_outputs_established: &mut bool,
    transition_generation: &mut u32,
    telemetry_sequence: &mut u32,
    now: DeviceCycle,
    observed: Instant,
    maximum_lateness_cycles: u64,
    safety_input_status: SafetyInputStatus,
    requested_fault: FaultCode,
    detail: u8,
) {
    let prior_state = safety.state();
    let prior_fault = safety.fault();
    let prior_job_active = jobs.active();
    let safe_applied = resources.force_safe_outputs().is_ok();
    *safe_outputs_established &= safe_applied;
    let motion_invalidated = motion.fault(now).is_ok();
    let fault = if !safe_applied {
        FaultCode::SafeOutput
    } else if !motion_invalidated {
        FaultCode::Identity
    } else {
        requested_fault
    };
    let _ = jobs.local_safety_fault(endpoint, now);
    let _ = safety.apply(SafetyEvent::Fault(fault), Conditions::default());
    let retained_fault = safety.fault().unwrap_or(fault);
    if prior_state != safety.state()
        || prior_fault != safety.fault()
        || prior_job_active != jobs.active()
        || !safe_applied
        || !motion_invalidated
    {
        *transition_generation = next_nonzero(*transition_generation);
    }
    endpoint.publish_fault(retained_fault.wire_value(), detail);
    publish_safety_snapshot(
        endpoint,
        telemetry_sequence,
        observed,
        *safety,
        *transition_generation,
        safety_snapshot_flags(*safe_outputs_established, jobs.active()),
        safety_input_status,
        maximum_lateness_cycles,
    );
}

#[allow(clippy::too_many_arguments)]
fn request_realtime_stop(
    resources: &mut selected::EstablishedRealtimeResources,
    motion: &mut MotionService,
    jobs: &mut RealtimeJobService,
    safety: &mut SafetyMachine,
    endpoint: &mut DefaultRealtimeEndpoint,
    safe_outputs_established: &mut bool,
    transition_generation: &mut u32,
    telemetry_sequence: &mut u32,
    now: DeviceCycle,
    observed: Instant,
    maximum_lateness_cycles: u64,
    safety_input_status: SafetyInputStatus,
    detail: u8,
) {
    if resources.force_safe_outputs().is_err() {
        // A later retry may make the pins safe, but it cannot erase evidence
        // that this stop transaction itself failed.
        *safe_outputs_established = false;
        latch_realtime_fault(
            resources,
            motion,
            jobs,
            safety,
            endpoint,
            safe_outputs_established,
            transition_generation,
            telemetry_sequence,
            now,
            observed,
            maximum_lateness_cycles,
            safety_input_status,
            FaultCode::SafeOutput,
            detail,
        );
        return;
    }
    if motion.fault(now).is_err() {
        latch_realtime_fault(
            resources,
            motion,
            jobs,
            safety,
            endpoint,
            safe_outputs_established,
            transition_generation,
            telemetry_sequence,
            now,
            observed,
            maximum_lateness_cycles,
            safety_input_status,
            FaultCode::Identity,
            detail,
        );
        return;
    }
    let prior_state = safety.state();
    let prior_job_active = jobs.active();
    if jobs.local_stop(endpoint, now).is_err() {
        latch_realtime_fault(
            resources,
            motion,
            jobs,
            safety,
            endpoint,
            safe_outputs_established,
            transition_generation,
            telemetry_sequence,
            now,
            observed,
            maximum_lateness_cycles,
            safety_input_status,
            FaultCode::Identity,
            detail,
        );
        return;
    }
    let transition = match safety.state() {
        SafetyState::Running | SafetyState::Hold => {
            safety.apply(SafetyEvent::Finish, Conditions::default())
        }
        SafetyState::Armed => safety.apply(SafetyEvent::Disarm, Conditions::default()),
        _ => Ok(safety.state()),
    };
    if transition.is_err() {
        latch_realtime_fault(
            resources,
            motion,
            jobs,
            safety,
            endpoint,
            safe_outputs_established,
            transition_generation,
            telemetry_sequence,
            now,
            observed,
            maximum_lateness_cycles,
            safety_input_status,
            FaultCode::Identity,
            detail,
        );
        return;
    }
    if prior_state != safety.state() || prior_job_active != jobs.active() {
        *transition_generation = next_nonzero(*transition_generation);
    }
    publish_safety_snapshot(
        endpoint,
        telemetry_sequence,
        observed,
        *safety,
        *transition_generation,
        safety_snapshot_flags(*safe_outputs_established, jobs.active()),
        safety_input_status,
        maximum_lateness_cycles,
    );
}

#[allow(clippy::too_many_arguments)]
fn apply_configuration_command(
    configurations: &mut RealtimeConfigurationService<
        'static,
        { selected::CONFIGURATION_BINDINGS },
    >,
    resources: &mut selected::EstablishedRealtimeResources,
    motion: &mut MotionService,
    safety_inputs: &mut Option<TargetSafetyInputMonitor>,
    safety_input_status: &mut SafetyInputStatus,
    jobs: &mut RealtimeJobService,
    safety: &mut SafetyMachine,
    endpoint: &mut DefaultRealtimeEndpoint,
    frame: &IntercoreFrame<{ alumina_runtime::COMMAND_PAYLOAD_BYTES }>,
    now: DeviceCycle,
    nominal_scan_period_cycles: u64,
    report_sequence: &mut u32,
    graphs: &RealtimeGraphExecutor,
) -> Result<bool, ()> {
    frame.validate(FrameKind::Configuration).map_err(|_| ())?;
    let command =
        CoreConfigurationCommand::decode(frame.payload().map_err(|_| ())?).map_err(|_| ())?;
    if frame.header().config_digest != command.digest {
        return Err(());
    }
    let action = command.action;
    let mutation_allowed = matches!(safety.state(), SafetyState::Safe | SafetyState::Configured)
        && !jobs.active()
        && graphs.active_identity().is_none()
        && graphs.candidate_identity().is_none()
        && endpoint.work_depth() == 0;
    if let Some(active) =
        configurations.authorization_preflight_configuration(command, mutation_allowed)
    {
        resources
            .validate_target_authorization(active)
            .map_err(|_| ())?;
    }
    let report = configurations.apply(command, mutation_allowed);
    let mut safety_changed = false;

    if report.state != RealtimeConfigurationState::Rejected {
        match action {
            CoreConfigurationAction::Activate => {
                if report.state != RealtimeConfigurationState::Active || report.active_authorized {
                    return Err(());
                }
                let active = configurations.active_configuration().ok_or(())?;
                let target_configuration = resources
                    .prepare_target_configuration(active)
                    .map_err(|_| ())?;
                motion.configure(active).map_err(|_| ())?;
                let monitor = resources
                    .configure_safety_inputs(active.profile(), nominal_scan_period_cycles)
                    .map_err(|_| ())?;
                resources
                    .commit_target_configuration(target_configuration)
                    .map_err(|_| ())?;
                *safety_inputs = monitor;
                *safety_input_status = safety_inputs
                    .as_ref()
                    .map_or(SafetyInputStatus::unconfigured(), |monitor| {
                        monitor.status(now)
                    });
                jobs.set_active_config(Digest::ZERO);
                safety
                    .apply(
                        SafetyEvent::Configure,
                        Conditions {
                            safe_outputs_established: true,
                            configuration_valid: true,
                            ..Conditions::default()
                        },
                    )
                    .map_err(|_| ())?;
                safety_changed = true;
            }
            CoreConfigurationAction::Authorize => {
                let authorized = configurations.authorized_identity().ok_or(())?;
                if !report.active_authorized || report.active_digest != authorized.digest {
                    return Err(());
                }
                jobs.set_active_config(authorized.digest);
            }
            CoreConfigurationAction::Clear => {
                if report.state != RealtimeConfigurationState::Cleared {
                    return Err(());
                }
                resources.clear_target_configuration();
                resources.clear_safety_inputs();
                motion.clear();
                *safety_inputs = None;
                *safety_input_status = SafetyInputStatus::unconfigured();
                jobs.set_active_config(Digest::ZERO);
                safety
                    .apply(SafetyEvent::Unconfigure, Conditions::default())
                    .map_err(|_| ())?;
                safety_changed = true;
            }
            CoreConfigurationAction::Begin
            | CoreConfigurationAction::Data
            | CoreConfigurationAction::Finish
            | CoreConfigurationAction::Abort => {}
        }
    }
    publish_configuration_report(endpoint, report_sequence, now, report)?;
    Ok(safety_changed)
}

fn publish_configuration_report(
    endpoint: &mut DefaultRealtimeEndpoint,
    report_sequence: &mut u32,
    now: DeviceCycle,
    report: RealtimeConfigurationReport,
) -> Result<(), ()> {
    let payload = report.encode().map_err(|_| ())?;
    let sequence = next_nonzero(*report_sequence);
    let frame = IntercoreFrame::new(
        FrameKind::Configuration,
        sequence,
        now,
        report.active_digest,
        &payload,
    )
    .map_err(|_| ())?;
    if endpoint.try_publish_telemetry(frame).is_ok() {
        *report_sequence = sequence;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_graph_command(
    graphs: &mut RealtimeGraphExecutor,
    configurations: &RealtimeConfigurationService<'static, { selected::CONFIGURATION_BINDINGS }>,
    jobs: &RealtimeJobService,
    safety: &SafetyMachine,
    endpoint: &mut DefaultRealtimeEndpoint,
    frame: &IntercoreFrame<{ alumina_runtime::COMMAND_PAYLOAD_BYTES }>,
    now: DeviceCycle,
    report_sequence: &mut u32,
) -> Result<(), ()> {
    frame.validate(FrameKind::Graph).map_err(|_| ())?;
    let payload = frame.payload().map_err(|_| ())?;
    let config_digest = configurations
        .authorized_identity()
        .map_or(Digest::ZERO, |identity| identity.digest);
    if config_digest.is_zero() || frame.header().config_digest != config_digest {
        return Err(());
    }
    if let Ok(command) = CoreGraphCommand::decode(payload) {
        let authority = GraphRuntimeAuthority {
            device_id: graphs.device_id(),
            capability_digest: selected::PACKAGE.board.capability_digest,
            config_digest,
            implementation_digest: command.implementation_digest,
        };
        let mutation_allowed =
            matches!(safety.state(), SafetyState::Safe | SafetyState::Configured)
                && !jobs.active()
                && endpoint.work_depth() == 0;
        let report = graphs.apply_lifecycle(command, authority, mutation_allowed)?;
        publish_graph_report(endpoint, report_sequence, now, config_digest, report)?;
        return publish_graph_execution_report(
            endpoint,
            report_sequence,
            now,
            config_digest,
            graphs.execution_report(),
        );
    }
    let command = CoreGraphExecutionCommand::decode(payload).map_err(|_| ())?;
    let execution_allowed =
        matches!(safety.state(), SafetyState::Safe | SafetyState::Configured) && !jobs.active();
    let report = graphs.apply_execution(command, now, execution_allowed)?;
    publish_graph_execution_report(endpoint, report_sequence, now, config_digest, report)
}

fn publish_graph_report(
    endpoint: &mut DefaultRealtimeEndpoint,
    report_sequence: &mut u32,
    now: DeviceCycle,
    config_digest: Digest,
    report: RealtimeGraphReport,
) -> Result<(), ()> {
    let payload = report.encode().map_err(|_| ())?;
    let sequence = next_nonzero(*report_sequence);
    let frame = IntercoreFrame::new(FrameKind::Graph, sequence, now, config_digest, &payload)
        .map_err(|_| ())?;
    if endpoint.try_publish_telemetry(frame).is_ok() {
        *report_sequence = sequence;
    }
    Ok(())
}

fn publish_graph_execution_report(
    endpoint: &mut DefaultRealtimeEndpoint,
    report_sequence: &mut u32,
    now: DeviceCycle,
    config_digest: Digest,
    report: RealtimeGraphExecutionReport,
) -> Result<(), ()> {
    let payload = report.encode().map_err(|_| ())?;
    let sequence = next_nonzero(*report_sequence);
    let frame = IntercoreFrame::new(FrameKind::Graph, sequence, now, config_digest, &payload)
        .map_err(|_| ())?;
    if endpoint.try_publish_telemetry(frame).is_ok() {
        *report_sequence = sequence;
    }
    Ok(())
}

fn publish_clock_report(
    endpoint: &mut DefaultRealtimeEndpoint,
    report_sequence: &mut u32,
    now: DeviceCycle,
    probe: DeadlineProbe,
) -> Result<(), ()> {
    let report = RealtimeClockReport {
        samples: probe.samples(),
        missed_deadlines: probe.misses(),
        maximum_lateness_cycles: probe.maximum_lateness_cycles(),
    };
    let payload = report.encode().map_err(|_| ())?;
    let sequence = next_nonzero(*report_sequence);
    let frame = IntercoreFrame::new(
        FrameKind::ClockSample,
        sequence,
        now,
        Digest::ZERO,
        &payload,
    )
    .map_err(|_| ())?;
    if endpoint.try_publish_telemetry(frame).is_ok() {
        *report_sequence = sequence;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish_safety_snapshot(
    endpoint: &mut DefaultRealtimeEndpoint,
    telemetry_sequence: &mut u32,
    observed: Instant,
    safety: SafetyMachine,
    transition_generation: u32,
    flags: u8,
    safety_inputs: SafetyInputStatus,
    maximum_lateness_cycles: u64,
) {
    let snapshot = SafetySnapshot {
        state: safety.state(),
        fault: safety.fault(),
        flags,
        transition_generation,
        safe_output_contract: selected::SAFE_OUTPUT_CONTRACT,
        maximum_lateness_cycles,
        safety_inputs,
    };
    let payload = match snapshot.encode() {
        Ok(payload) => payload,
        Err(_) => {
            endpoint.publish_fault(FaultCode::Identity.wire_value(), 2);
            return;
        }
    };
    *telemetry_sequence = next_nonzero(*telemetry_sequence);
    if let Ok(frame) = IntercoreFrame::new(
        FrameKind::Telemetry,
        *telemetry_sequence,
        DeviceCycle(observed.as_ticks()),
        Digest::ZERO,
        &payload,
    ) {
        let _ = endpoint.try_publish_telemetry(frame);
    }
}

async fn hold_safe_output_fault(endpoint: &mut DefaultRealtimeEndpoint, detail: u8) -> ! {
    let mut safety = SafetyMachine::new();
    let _ = safety.apply(
        SafetyEvent::Fault(FaultCode::SafeOutput),
        Conditions::default(),
    );
    let mut sequence = 0_u32;
    endpoint.publish_fault(FaultCode::SafeOutput.wire_value(), detail);
    loop {
        publish_safety_snapshot(
            endpoint,
            &mut sequence,
            Instant::now(),
            safety,
            1,
            0,
            SafetyInputStatus::unconfigured(),
            0,
        );
        Timer::after(Duration::from_millis(100)).await;
    }
}

const fn next_nonzero(value: u32) -> u32 {
    let next = value.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

const fn safety_snapshot_flags(safe_outputs_established: bool, realtime_job_active: bool) -> u8 {
    let mut flags = 0;
    if safe_outputs_established {
        flags |= SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED;
    }
    if realtime_job_active {
        flags |= SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE;
    }
    flags
}
