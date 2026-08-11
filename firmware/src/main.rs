#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "dropping a HAL token must never be replaced by leaking peripheral ownership"
)]
#![deny(clippy::large_stack_frames)]

#[cfg(not(any(feature = "board-mks-tinybee", feature = "board-t-deck-pro")))]
compile_error!("select exactly one board feature through `cargo xtask build --board <id>`");
#[cfg(all(feature = "board-mks-tinybee", feature = "board-t-deck-pro"))]
compile_error!("multiple board features selected; Alumina images contain exactly one board");

mod hardware;
mod job;
mod network;
pub mod service;
mod storage;

use alumina_protocol::{DeviceCycle, Digest, FrameKind};
use alumina_runtime::{
    APP_CORE_STACK_WORDS, DeadlineProbe, DefaultBoundary, DefaultRealtimeEndpoint,
    DefaultServiceEndpoint, IntercoreFrame, RuntimeBudget, UrgentKind,
};
use alumina_safety::{
    Conditions, Event as SafetyEvent, FaultCode, SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE,
    SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED, SafetyMachine, SafetyObservationPolicy, SafetyObserver,
    SafetySnapshot, SafetyState,
};
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::system::{Cpu, Stack};
use esp_hal::timer::timg::TimerGroup;
use panic_rtt_target as _;
use static_cell::StaticCell;

use hardware::selected;
use job::{JobService, RealtimeJobService};
use service::{ServiceBridge, StorageServiceState, init_service_bridge};

static BOUNDARY: StaticCell<DefaultBoundary> = StaticCell::new();
static APP_CORE_STACK: StaticCell<Stack<APP_CORE_STACK_WORDS>> = StaticCell::new();
static APP_CORE_EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();

const SAFETY_OBSERVATION_MAX_AGE_CYCLES: u64 = Duration::from_millis(500).as_ticks();

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
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1_024);
    esp_alloc::heap_allocator!(size: 36 * 1_024);
    let mut split = selected::split(peripherals);

    let timer_group0 = TimerGroup::new(split.runtime.timer_group0);
    let software_interrupt = SoftwareInterruptControl::new(split.runtime.software_interrupt);
    esp_rtos::start(timer_group0.timer0);

    let boundary = BOUNDARY.init(DefaultBoundary::new());
    let (mut service_endpoint, realtime_endpoint) = boundary.split();
    let app_stack = APP_CORE_STACK.init(Stack::new());

    esp_rtos::start_second_core(
        split.runtime.cpu_control,
        software_interrupt.software_interrupt0,
        software_interrupt.software_interrupt1,
        app_stack,
        move || {
            let app_executor = APP_CORE_EXECUTOR.init(esp_rtos::embassy::Executor::new());
            app_executor.run(move |realtime_spawner| {
                realtime_spawner.must_spawn(realtime_task(split.realtime, realtime_endpoint));
            });
        },
    );

    // Hazardous outputs are established by their sole core-1 owner. Wi-Fi is
    // intentionally not initialized until a fresh contract-bound `Safe`
    // snapshot crosses the owned inter-core boundary.
    await_initial_safe_snapshot(&mut service_endpoint).await;
    let service_bridge = init_service_bridge();
    let network = network::start(spawner, split.service.take_wifi(), service_bridge).await;

    spawner.must_spawn(service_task(
        split.service,
        service_endpoint,
        network,
        service_bridge,
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
    let mut jobs = JobService::new();
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
                            alumina_job::RealtimeJobReport::decode(payload)
                                .is_ok_and(|report| jobs.observe_realtime(report).is_ok())
                        })
                }
                _ => false,
            };
            if !valid {
                storage.invalidate_safety_observation();
                endpoint.publish_urgent(UrgentKind::EmergencyStop, 1);
            }
        }
        if let Some(fault) = endpoint.fault_after(last_fault_generation) {
            last_fault_generation = fault.generation;
            storage.invalidate_safety_observation();
            error!("RT fault code={} detail={}", fault.code, fault.detail);
        }

        while let Some(request) = service_bridge.try_receive() {
            storage.set_service_job_active(jobs.excludes_storage_mutation(&endpoint));
            let now = DeviceCycle(Instant::now().as_ticks());
            let response = if JobService::handles(request.request()) {
                jobs.dispatch(&mut storage_backend, &mut endpoint, request.request(), now)
                    .await
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
            network.supervisor(),
            network.credential_source(),
        );
        Timer::after(Duration::from_millis(10)).await;
    }
}

#[embassy_executor::task]
async fn realtime_task(
    resources: selected::RealtimeResources,
    mut endpoint: DefaultRealtimeEndpoint,
) {
    if Cpu::current() != Cpu::AppCpu {
        endpoint.publish_fault(1, 0);
        panic!("realtime executor started on the wrong core");
    }

    let resources = match resources.establish_safe_outputs() {
        Ok(resources) => resources,
        Err(_) => {
            hold_safe_output_fault(&mut endpoint, 0).await;
        }
    };
    let mut safety = SafetyMachine::new();
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
    let mut last_job_active = false;

    publish_safety_snapshot(
        &mut endpoint,
        &mut telemetry_sequence,
        Instant::now(),
        safety,
        transition_generation,
        SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED,
        0,
    );

    loop {
        Timer::at(expected).await;
        let observed = Instant::now();
        probe.observe(
            DeviceCycle(expected.as_ticks()),
            DeviceCycle(observed.as_ticks()),
            100,
        );
        expected += period;

        if let Some(urgent) = endpoint.urgent_after(urgent_generation) {
            urgent_generation = urgent.generation;
            if urgent.code == UrgentKind::EmergencyStop as u8 {
                let _ = safety.apply(
                    SafetyEvent::Fault(FaultCode::EmergencyStop),
                    Conditions::default(),
                );
                transition_generation = next_nonzero(transition_generation);
                endpoint.publish_fault(2, urgent.detail);
            }
        }
        while let Ok(command) = endpoint.try_receive_command() {
            let valid = match command.header().kind {
                FrameKind::Job => jobs
                    .apply_command(&mut endpoint, &command, DeviceCycle(observed.as_ticks()))
                    .is_ok(),
                FrameKind::Command => command.validate(FrameKind::Command).is_ok(),
                _ => false,
            };
            if !valid {
                let _ = safety.apply(
                    SafetyEvent::Fault(FaultCode::Identity),
                    Conditions::default(),
                );
                transition_generation = next_nonzero(transition_generation);
                endpoint.publish_fault(3, 0);
            }
        }
        if jobs
            .preadmit(&mut endpoint, DeviceCycle(observed.as_ticks()))
            .is_err()
        {
            let _ = safety.apply(
                SafetyEvent::Fault(FaultCode::Identity),
                Conditions::default(),
            );
            transition_generation = next_nonzero(transition_generation);
            endpoint.publish_fault(4, 0);
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
                safety_snapshot_flags(true, job_active),
                probe.maximum_lateness_cycles(),
            );
        }

        divider = divider.wrapping_add(1);
        if divider == 100 {
            divider = 0;
            publish_safety_snapshot(
                &mut endpoint,
                &mut telemetry_sequence,
                observed,
                safety,
                transition_generation,
                safety_snapshot_flags(true, job_active),
                probe.maximum_lateness_cycles(),
            );
            if jobs.has_job() {
                let _ = jobs.publish_report(&mut endpoint, DeviceCycle(observed.as_ticks()));
            }
        }

        let _keep_tokens_core_local = &resources;
    }
}

fn publish_safety_snapshot(
    endpoint: &mut DefaultRealtimeEndpoint,
    telemetry_sequence: &mut u32,
    observed: Instant,
    safety: SafetyMachine,
    transition_generation: u32,
    flags: u8,
    maximum_lateness_cycles: u64,
) {
    let snapshot = SafetySnapshot {
        state: safety.state(),
        fault: safety.fault(),
        flags,
        transition_generation,
        safe_output_contract: selected::SAFE_OUTPUT_CONTRACT,
        maximum_lateness_cycles,
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
        publish_safety_snapshot(endpoint, &mut sequence, Instant::now(), safety, 1, 0, 0);
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
