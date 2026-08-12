#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "HIL-owned peripheral tokens must stop or drop normally"
)]
#![deny(clippy::large_stack_frames)]
#![allow(
    dead_code,
    reason = "the isolated HIL binary reuses production board, network, and service modules but intentionally leaves unrelated APIs inert"
)]

#[cfg(not(feature = "hil-mks-tinybee-graph-input-timing-safe"))]
compile_error!("this binary requires `hil-mks-tinybee-graph-input-timing-safe`");

#[path = "graph_platform.rs"]
mod graph_platform;
#[path = "hardware/mod.rs"]
mod hardware;
#[path = "network.rs"]
mod network;
#[path = "service.rs"]
mod service;
#[path = "storage.rs"]
mod storage;

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use alumina_board::{OwnerDomain, ResourceId};
use alumina_config::{
    BindingFlags, BindingRole, ConfigurationFlags, ConfigurationHeader, ConfigurationRecord,
    ConfigurationStreamValidator, RealtimeConfigurationProfile, ResourceBinding, SignalPolarity,
};
use alumina_graph_ir::{
    BOOLEAN_STREAM_ITEM_BYTES, GraphIrChannel, GraphIrChannelOwner, GraphIrDomain,
    GraphIrFullPolicy, GraphIrHeader, GraphIrNode, GraphIrOpcode, GraphIrPackage, GraphIrSchedule,
    encode_graph_resource_parameter, graph_ir_content_digest,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest};
use alumina_runtime::APP_CORE_STACK_BYTES;
use alumina_runtime::graph::{GraphRunIdentity, GraphRuntimeAuthority};
use alumina_safety::{FaultCode, MAX_SAFETY_INPUTS, SafetyInputMonitor, SafetyInputReaction};
use alumina_storage::{ContentHasher, sha256};
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, TICK_HZ, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Output;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::system::{Cpu, Stack};
use esp_hal::timer::timg::TimerGroup;
use panic_rtt_target as _;
use static_cell::StaticCell;

use graph_platform::{GRAPH_RUNTIME_LIMITS, GraphBridge, RealtimeGraphActor, ServiceGraphActor};
use hardware::mks_tinybee;

const RELEASE_PERIOD_CYCLES: u64 = TICK_HZ / 1_000;
const NODE_WCET_CYCLES: u64 = RELEASE_PERIOD_CYCLES / 20;
const EXECUTOR_RESERVE_CYCLES: u64 = RELEASE_PERIOD_CYCLES / 5;
const TOTAL_WCET_CYCLES: u64 = 2 * NODE_WCET_CYCLES;
const INPUT_DEBOUNCE_CYCLES: u32 = (2 * RELEASE_PERIOD_CYCLES) as u32;
const INPUT_MAXIMUM_GAP_CYCLES: u32 = (10 * RELEASE_PERIOD_CYCLES) as u32;
const GRAPH_CLOCK_ID: u32 = u32::from_le_bytes(*b"HIL1");
const GRAPH_TRANSACTION_ID: u64 = 1;
const GRAPH_RUN_ID: u64 = 1;
const GRAPH_START_LEAD: Duration = Duration::from_millis(250);
const NETWORK_SETTLE: Duration = Duration::from_secs(1);
const GENERAL_HEAP_BYTES: usize = 4 * 1_024;

const _: () = {
    assert!(TICK_HZ >= 1_000 && TICK_HZ.is_multiple_of(1_000));
    assert!(RELEASE_PERIOD_CYCLES <= u32::MAX as u64 / 10);
    assert!(NODE_WCET_CYCLES != 0);
    assert!(TOTAL_WCET_CYCLES + EXECUTOR_RESERVE_CYCLES < RELEASE_PERIOD_CYCLES);
};

static GRAPH_BRIDGE: StaticCell<GraphBridge> = StaticCell::new();
static SERVICE_ACTOR: StaticCell<ServiceGraphActor> = StaticCell::new();
static REALTIME_ACTOR: StaticCell<RealtimeGraphActor> = StaticCell::new();
static REALTIME_PROFILE: StaticCell<RealtimeConfigurationProfile> = StaticCell::new();
static RUN_SIGNAL: StaticCell<Signal<CriticalSectionRawMutex, GraphRunIdentity>> =
    StaticCell::new();
static APP_CORE_STACK: StaticCell<Stack<APP_CORE_STACK_BYTES>> = StaticCell::new();
static APP_CORE_EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();

static BOOT_SAFE_READY: AtomicBool = AtomicBool::new(false);
static NETWORK_INITIALIZED: AtomicBool = AtomicBool::new(false);
static REALTIME_READY: AtomicBool = AtomicBool::new(false);
static REALTIME_ACTIVATED: AtomicBool = AtomicBool::new(false);
static RUN_CONFIRMED: AtomicBool = AtomicBool::new(false);
static RELEASE_COUNT: AtomicU32 = AtomicU32::new(0);
static INPUT_TRANSITION_COUNT: AtomicU32 = AtomicU32::new(0);
static NETWORK_STARTUP_MAXIMUM_SAMPLE_GAP: AtomicU32 = AtomicU32::new(0);
static NETWORK_STARTUP_WATCHDOG_OBSERVED: AtomicBool = AtomicBool::new(false);
static MAXIMUM_DISPATCH_LATENESS: AtomicU32 = AtomicU32::new(0);
static SINK_ACTIVE: AtomicBool = AtomicBool::new(false);
static HIL_FAULT: AtomicU32 = AtomicU32::new(0);

esp_bootloader_esp_idf::esp_app_desc!();

/// Safe graph-input timing fixture. It initializes the production AP/web
/// tasks on core 0 and the production-sized fixed graph actor on core 1, but
/// never initializes storage, motion streaming, or a process-output API.
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();
    esp_println_uart::println!("ALUMINA_HIL_BOOT stage=entry");

    if let Err(reason) = mks_tinybee::PACKAGE.validate() {
        esp_println_uart::println!("ALUMINA_HIL_ABORT code=100 stage=board-package");
        error!(
            "HIL_ABORT board package invalid: {:?}",
            defmt::Debug2Format(&reason)
        );
        park().await
    }

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let device_id = DeviceId::from_esp_base_mac(esp_hal::efuse::Efuse::read_base_mac_address());
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1_024);
    esp_alloc::heap_allocator!(size: GENERAL_HEAP_BYTES);
    let mut split = mks_tinybee::split(peripherals);

    let timer_group0 = TimerGroup::new(split.runtime.timer_group0);
    let software_interrupt = SoftwareInterruptControl::new(split.runtime.software_interrupt);
    esp_rtos::start(timer_group0.timer0);

    let wifi = split.service.take_wifi();
    let (timing_marker, sink_marker) = split.service.into_graph_hil_markers();
    let (config_digest, realtime_profile) = build_configuration();
    let realtime_profile = REALTIME_PROFILE.init(realtime_profile);
    let package = build_graph_package(device_id, config_digest);
    let content_digest = graph_ir_content_digest(package.bytes());
    let package_digest = package.digest();
    let authority = GraphRuntimeAuthority {
        device_id,
        capability_digest: mks_tinybee::PACKAGE.board.capability_digest,
        config_digest,
        implementation_digest: package.header().implementation_digest,
    };

    let bridge: &'static GraphBridge = GRAPH_BRIDGE.init(GraphBridge::new());
    let service_actor = SERVICE_ACTOR.init(ServiceGraphActor::new(bridge));
    let realtime_actor = REALTIME_ACTOR.init(RealtimeGraphActor::new(bridge));
    install_actor(
        service_actor,
        package.bytes(),
        content_digest,
        package_digest,
        authority,
    );
    install_realtime_actor(
        realtime_actor,
        package.bytes(),
        content_digest,
        package_digest,
        authority,
    );
    esp_println_uart::println!("ALUMINA_HIL_BOOT stage=graph-installed");

    let run_signal: &'static Signal<CriticalSectionRawMutex, GraphRunIdentity> =
        RUN_SIGNAL.init(Signal::new());
    let app_stack = APP_CORE_STACK.init(Stack::new());
    esp_rtos::start_second_core(
        split.runtime.cpu_control,
        software_interrupt.software_interrupt0,
        software_interrupt.software_interrupt1,
        app_stack,
        move || {
            let executor = APP_CORE_EXECUTOR.init(esp_rtos::embassy::Executor::new());
            executor.run(move |realtime_spawner| {
                realtime_spawner.must_spawn(realtime_task(
                    split.realtime,
                    realtime_actor,
                    realtime_profile,
                    run_signal,
                    timing_marker,
                    sink_marker,
                ));
            });
        },
    );

    wait_for_flag_or_fault(&BOOT_SAFE_READY).await;
    esp_println_uart::println!("ALUMINA_HIL_BOOT stage=boot-safe-ready");
    info!(
        "HIL_STATIC_SAFE board={} image=0x{:06x}; DISCONNECT ALL MOTOR AND PROCESS LOADS",
        env!("ALUMINA_BOARD_ID"),
        board_mks_tinybee::DESCRIBED_SAFE_I2S_IMAGE
    );

    let service_bridge = service::init_service_bridge();
    esp_println_uart::println!("ALUMINA_HIL_BOOT stage=network-starting");
    let network = network::start(spawner, wifi, service_bridge, device_id).await;
    esp_println_uart::println!("ALUMINA_HIL_BOOT stage=network-started");
    NETWORK_INITIALIZED.store(true, Ordering::Release);
    wait_for_flag_or_fault(&REALTIME_READY).await;
    esp_println_uart::println!("ALUMINA_HIL_BOOT stage=realtime-ready");
    esp_println_uart::println!(
        "ALUMINA_HIL_WIFI_STARTUP max_sample_gap={} watchdog_observed={}",
        NETWORK_STARTUP_MAXIMUM_SAMPLE_GAP.load(Ordering::Acquire),
        NETWORK_STARTUP_WATCHDOG_OBSERVED.load(Ordering::Acquire)
    );
    Timer::after(NETWORK_SETTLE).await;
    let fault = HIL_FAULT.load(Ordering::Acquire);
    if fault != 0 {
        esp_println_uart::println!(
            "ALUMINA_HIL_ABORT code={} stage=network-steady-state",
            fault
        );
        error!("HIL_ABORT network steady-state fault={}", fault);
        park().await
    }

    let start_cycle = DeviceCycle((Instant::now() + GRAPH_START_LEAD).as_ticks());
    let run = GraphRunIdentity {
        transaction_id: GRAPH_TRANSACTION_ID,
        run_id: GRAPH_RUN_ID,
        content_digest,
        package_digest,
        start_cycle,
    };
    if service_actor.prepare_start(run, true).is_err() {
        HIL_FAULT.store(101, Ordering::Release);
        esp_println_uart::println!("ALUMINA_HIL_ABORT code=101 stage=service-prime");
        error!("HIL_ABORT service graph prime failed");
        park().await
    }
    run_signal.signal(run);
    wait_for_flag_or_fault(&REALTIME_ACTIVATED).await;
    if service_actor.observe_realtime_started(run).is_err() {
        HIL_FAULT.store(102, Ordering::Release);
        esp_println_uart::println!("ALUMINA_HIL_ABORT code=102 stage=start-observation");
        error!("HIL_ABORT service graph start observation failed");
        park().await
    }
    RUN_CONFIRMED.store(true, Ordering::Release);
    esp_println_uart::println!(
        "ALUMINA_HIL_RUNNING ssid=Alumina-{} address=192.168.4.1",
        env!("ALUMINA_BOARD_ID")
    );

    info!(
        "HIL_GRAPH_RUNNING start={} period={} wcet={} reserve={} config={:?} package={:?}",
        start_cycle.0,
        RELEASE_PERIOD_CYCLES,
        TOTAL_WCET_CYCLES,
        EXECUTOR_RESERVE_CYCLES,
        defmt::Debug2Format(&config_digest),
        defmt::Debug2Format(&package_digest)
    );
    info!(
        "HIL_NETWORK ssid=Alumina-{} address=192.168.4.1; use /api/v1/health for core-0 load",
        env!("ALUMINA_BOARD_ID")
    );

    loop {
        Timer::after(Duration::from_secs(5)).await;
        let fault = HIL_FAULT.load(Ordering::Acquire);
        info!(
            "HIL_STATUS releases={} input_transitions={} sink_active={} max_dispatch_late={} fault={} ap_ready={}",
            RELEASE_COUNT.load(Ordering::Relaxed),
            INPUT_TRANSITION_COUNT.load(Ordering::Relaxed),
            SINK_ACTIVE.load(Ordering::Relaxed),
            MAXIMUM_DISPATCH_LATENESS.load(Ordering::Relaxed),
            fault,
            network.supervisor().access_point_expected()
        );
        if fault != 0 {
            error!("HIL_RESULT failed closed; no graph timing qualification granted");
        }
    }
}

fn build_configuration() -> (Digest, RealtimeConfigurationProfile) {
    let header = ConfigurationHeader {
        capability_digest: mks_tinybee::PACKAGE.board.capability_digest,
        record_count: 1,
        realtime_record_count: 1,
        flags: ConfigurationFlags(ConfigurationFlags::LAB_CONTROL),
    };
    let record = ConfigurationRecord::Binding(ResourceBinding {
        instance: 0,
        role: BindingRole::SafetyInterlock,
        resource: ResourceId::Gpio(33),
        owner: OwnerDomain::Realtime,
        polarity: SignalPolarity::ActiveLow,
        flags: BindingFlags(BindingFlags::PULL_UP | BindingFlags::REQUIRED_INTERLOCK),
        minimum_active_cycles: INPUT_DEBOUNCE_CYCLES,
        minimum_inactive_cycles: INPUT_DEBOUNCE_CYCLES,
        maximum_frequency_hz: 0,
        watchdog_cycles: INPUT_MAXIMUM_GAP_CYCLES,
    });
    let header_bytes = header
        .encode()
        .unwrap_or_else(|_| panic!("HIL configuration header is invalid"));
    let record_bytes = record
        .encode()
        .unwrap_or_else(|_| panic!("HIL configuration record is invalid"));
    let mut hasher = ContentHasher::new();
    hasher.update(&header_bytes);
    hasher.update(&record_bytes);
    let digest = hasher.finalize().digest;
    let total_bytes = header
        .total_bytes()
        .unwrap_or_else(|_| panic!("HIL configuration length is invalid"));
    let mut validator =
        ConfigurationStreamValidator::<1>::new(mks_tinybee::PACKAGE, digest, total_bytes)
            .unwrap_or_else(|_| panic!("HIL configuration validator did not initialize"));
    validator
        .push(&header_bytes)
        .unwrap_or_else(|_| panic!("HIL configuration header was not admitted"));
    validator
        .push(&record_bytes)
        .unwrap_or_else(|_| panic!("HIL configuration record was not admitted"));
    let (identity, profile) = validator
        .finish_with_profile()
        .unwrap_or_else(|_| panic!("HIL configuration did not complete"));
    if identity.digest != digest || profile.safety_input_count() != 1 {
        panic!("HIL configuration identity/profile mismatch");
    }
    (digest, profile)
}

fn build_graph_package(device_id: DeviceId, config_digest: Digest) -> GraphIrPackage {
    let graph_digest = sha256(b"alumina-hil/tinybee/gpio33-stable-boolean-input-to-sink/v1").digest;
    let implementation_digest =
        sha256(b"alumina-hil/fixed-graph-ir-v2-production-arenas/timing-contract/v1").digest;
    GraphIrPackage::encode(
        GraphIrHeader {
            device_id,
            graph_digest,
            implementation_digest,
            capability_digest: mks_tinybee::PACKAGE.board.capability_digest,
            config_digest,
            service_schedule: GraphIrSchedule::EMPTY,
            realtime_schedule: GraphIrSchedule {
                clock_id: GRAPH_CLOCK_ID,
                period_cycles: RELEASE_PERIOD_CYCLES,
                total_wcet_cycles: TOTAL_WCET_CYCLES,
                executor_reserve_cycles: EXECUTOR_RESERVE_CYCLES,
                node_count: 2,
            },
            total_state_bytes: 0,
            service_state_bytes: 0,
            realtime_state_bytes: 0,
            channel_storage_bytes: BOOLEAN_STREAM_ITEM_BYTES,
            bridge_storage_bytes: 0,
        },
        &[
            GraphIrNode {
                graph_node_id: 1,
                domain: GraphIrDomain::Realtime,
                opcode: GraphIrOpcode::StableBooleanInput,
                schedule_clock_id: GRAPH_CLOCK_ID,
                period_cycles: RELEASE_PERIOD_CYCLES,
                wcet_cycles: NODE_WCET_CYCLES,
                state_offset: 0,
                state_bytes: 0,
                parameter: encode_graph_resource_parameter(ResourceId::Gpio(33)),
            },
            GraphIrNode {
                graph_node_id: 2,
                domain: GraphIrDomain::Realtime,
                opcode: GraphIrOpcode::BooleanStreamSink,
                schedule_clock_id: GRAPH_CLOCK_ID,
                period_cycles: RELEASE_PERIOD_CYCLES,
                wcet_cycles: NODE_WCET_CYCLES,
                state_offset: 0,
                state_bytes: 0,
                parameter: 0,
            },
        ],
        &[GraphIrChannel {
            graph_wire_id: 1,
            source_node: 0,
            target_node: 1,
            owner: GraphIrChannelOwner::Realtime,
            full_policy: GraphIrFullPolicy::Fault,
            capacity: 1,
            item_bytes: BOOLEAN_STREAM_ITEM_BYTES,
            storage_offset: 0,
            storage_bytes: BOOLEAN_STREAM_ITEM_BYTES,
        }],
    )
    .unwrap_or_else(|_| panic!("HIL graph package is invalid"))
}

fn install_actor(
    actor: &mut ServiceGraphActor,
    bytes: &[u8],
    content_digest: Digest,
    package_digest: Digest,
    authority: GraphRuntimeAuthority,
) {
    actor
        .install(
            bytes,
            GRAPH_TRANSACTION_ID,
            content_digest,
            package_digest,
            authority,
            GRAPH_RUNTIME_LIMITS,
            true,
        )
        .unwrap_or_else(|_| panic!("service HIL graph admission failed"));
}

fn install_realtime_actor(
    actor: &mut RealtimeGraphActor,
    bytes: &[u8],
    content_digest: Digest,
    package_digest: Digest,
    authority: GraphRuntimeAuthority,
) {
    actor
        .install(
            bytes,
            GRAPH_TRANSACTION_ID,
            content_digest,
            package_digest,
            authority,
            GRAPH_RUNTIME_LIMITS,
            true,
        )
        .unwrap_or_else(|_| panic!("realtime HIL graph admission failed"));
}

#[embassy_executor::task]
async fn realtime_task(
    resources: mks_tinybee::RealtimeResources,
    actor: &'static mut RealtimeGraphActor,
    profile: &'static RealtimeConfigurationProfile,
    run_signal: &'static Signal<CriticalSectionRawMutex, GraphRunIdentity>,
    mut timing_marker: Output<'static>,
    mut sink_marker: Output<'static>,
) {
    esp_println_uart::println!("ALUMINA_HIL_RT_INIT stage=entry");
    if Cpu::current() != Cpu::AppCpu {
        realtime_fault(&mut timing_marker, &mut sink_marker, 1).await;
    }
    let mut resources = match resources.establish_safe_outputs() {
        Ok(resources) => resources,
        Err(_) => realtime_fault(&mut timing_marker, &mut sink_marker, 2).await,
    };
    esp_println_uart::println!("ALUMINA_HIL_RT_INIT stage=safe-outputs");
    let period = Duration::from_ticks(RELEASE_PERIOD_CYCLES);
    let mut monitor = match resources.configure_safety_inputs(profile, RELEASE_PERIOD_CYCLES) {
        Ok(Some(monitor)) => monitor,
        Ok(None) | Err(_) => realtime_fault(&mut timing_marker, &mut sink_marker, 3).await,
    };
    esp_println_uart::println!("ALUMINA_HIL_RT_INIT stage=input-configured");

    // Establish a known debounced state before core 0 may initialize Wi-Fi.
    // Outputs are already at the complete board safe image and neither graph
    // execution nor arm authority exists during this commissioning boundary.
    let mut next_scan = Instant::now();
    let mut first_sample = true;
    let mut previous_sample = loop {
        let now = wait_for_next_sample(&mut next_scan, period).await;
        if sample_input(&resources, &mut monitor, now).is_err() {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 4).await;
        }
        if first_sample {
            esp_println_uart::println!("ALUMINA_HIL_RT_INIT stage=first-input-sample");
            first_sample = false;
        }
        if monitor
            .stable_active_by_resource(ResourceId::Gpio(33), now)
            .is_some()
        {
            break now;
        }
    };
    esp_println_uart::println!("ALUMINA_HIL_RT_INIT stage=boot-input-debounced");
    BOOT_SAFE_READY.store(true, Ordering::Release);

    // ESP radio initialization may briefly suspend the other core. This
    // explicitly unarmed boot phase measures and retains that discontinuity;
    // it never promotes a stale monitor into operational authority.
    let mut startup_monitor_valid = true;
    while !NETWORK_INITIALIZED.load(Ordering::Acquire) {
        let now = wait_for_next_sample(&mut next_scan, period).await;
        retain_network_startup_sample_gap(now.0.saturating_sub(previous_sample.0));
        previous_sample = now;
        if startup_monitor_valid {
            match sample_input_allow_watchdog(&resources, &mut monitor, now) {
                Ok(true) => {}
                Ok(false) => {
                    NETWORK_STARTUP_WATCHDOG_OBSERVED.store(true, Ordering::Release);
                    startup_monitor_valid = false;
                }
                Err(()) => {
                    realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 15)
                        .await;
                }
            }
        }
    }
    let operational_boundary = DeviceCycle(Instant::now().as_ticks());
    let boundary_gap = operational_boundary.0.saturating_sub(previous_sample.0);
    retain_network_startup_sample_gap(boundary_gap);
    if boundary_gap > u64::from(INPUT_MAXIMUM_GAP_CYCLES) {
        NETWORK_STARTUP_WATCHDOG_OBSERVED.store(true, Ordering::Release);
    }

    // Reconstruct the monitor after radio initialization. Only this fresh,
    // fully debounced monitor may authorize graph activation, and every later
    // missed 10 ms deadline fails closed.
    let mut monitor = match resources.configure_safety_inputs(profile, RELEASE_PERIOD_CYCLES) {
        Ok(Some(monitor)) => monitor,
        Ok(None) | Err(_) => {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 16).await
        }
    };
    next_scan = Instant::now();
    loop {
        let now = wait_for_next_sample(&mut next_scan, period).await;
        if sample_input(&resources, &mut monitor, now).is_err() {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 17).await;
        }
        if monitor
            .stable_active_by_resource(ResourceId::Gpio(33), now)
            .is_some()
        {
            break;
        }
    }
    REALTIME_READY.store(true, Ordering::Release);

    // Network settling and graph priming happen on core 0. Keep the fresh
    // finite input watchdog alive while waiting; readiness never freezes an
    // old sample.
    let run = loop {
        if let Some(run) = run_signal.try_take() {
            break run;
        }
        let now = wait_for_next_sample(&mut next_scan, period).await;
        if sample_input(&resources, &mut monitor, now).is_err()
            || monitor
                .stable_active_by_resource(ResourceId::Gpio(33), now)
                .is_none()
        {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 5).await;
        }
    };
    next_scan = Instant::now();
    if actor.prepare_start(run, true).is_err() || actor.activate(run).is_err() {
        realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 6).await;
    }
    REALTIME_ACTIVATED.store(true, Ordering::Release);
    while !RUN_CONFIRMED.load(Ordering::Acquire) {
        if HIL_FAULT.load(Ordering::Acquire) != 0 {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 7).await;
        }
        let now = wait_for_next_sample(&mut next_scan, period).await;
        if sample_input(&resources, &mut monitor, now).is_err() {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 8).await;
        }
    }

    // The package starts at a future common epoch. Continue sampling until the
    // first graph release so its resource read cannot inherit a stale value.
    let start = Instant::from_ticks(run.start_cycle.0);
    while next_scan + period < start {
        let now = wait_for_next_sample(&mut next_scan, period).await;
        if sample_input(&resources, &mut monitor, now).is_err() {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 9).await;
        }
    }

    let mut last_sink = false;
    loop {
        let window = match actor.next_release_window() {
            Ok(Some(window)) => window,
            Ok(None) | Err(_) => {
                realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 10).await
            }
        };
        Timer::at(Instant::from_ticks(window.scheduled_cycle.0)).await;
        let observed = DeviceCycle(Instant::now().as_ticks());
        let lateness = observed.0.saturating_sub(window.scheduled_cycle.0);
        retain_maximum_lateness(lateness);
        if sample_input(&resources, &mut monitor, observed).is_err() {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 11).await;
        }
        let release_cycle = if observed <= window.latest_dispatch_cycle {
            window.scheduled_cycle
        } else {
            observed
        };
        timing_marker.set_high();
        let report = actor.release(release_cycle, true, |resource| {
            monitor.stable_active_by_resource(resource, observed)
        });
        timing_marker.set_low();
        let report = match report {
            Ok(report) => report,
            Err(_) => {
                realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 12).await
            }
        };
        let Some(sink) = report.last_sink_value else {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 13).await;
        };
        if report.nodes_executed != 2 || report.sink_items != 1 {
            realtime_safe_fault(&mut resources, &mut timing_marker, &mut sink_marker, 14).await;
        }
        if sink != last_sink {
            if sink {
                sink_marker.set_high();
            } else {
                sink_marker.set_low();
            }
            last_sink = sink;
            SINK_ACTIVE.store(sink, Ordering::Release);
        }
        RELEASE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

fn sample_input(
    resources: &mks_tinybee::EstablishedRealtimeResources,
    monitor: &mut SafetyInputMonitor<MAX_SAFETY_INPUTS>,
    at: DeviceCycle,
) -> Result<(), ()> {
    sample_input_allow_watchdog(resources, monitor, at)
        .and_then(|healthy| healthy.then_some(()).ok_or(()))
}

fn sample_input_allow_watchdog(
    resources: &mks_tinybee::EstablishedRealtimeResources,
    monitor: &mut SafetyInputMonitor<MAX_SAFETY_INPUTS>,
    at: DeviceCycle,
) -> Result<bool, ()> {
    let scan = resources.scan_safety_inputs(monitor, at).map_err(|_| ())?;
    if matches!(
        scan.reaction,
        Some(SafetyInputReaction::Fault(FaultCode::Watchdog))
    ) {
        return Ok(false);
    }
    if scan.transitioned_mask != 0 {
        INPUT_TRANSITION_COUNT.fetch_add(scan.transitioned_mask.count_ones(), Ordering::Relaxed);
    }
    Ok(true)
}

async fn wait_for_next_sample(next_scan: &mut Instant, period: Duration) -> DeviceCycle {
    let target = *next_scan + period;
    Timer::at(target).await;
    let observed = Instant::now();
    *next_scan = if observed > target { observed } else { target };
    DeviceCycle(observed.as_ticks())
}

fn retain_network_startup_sample_gap(value: u64) {
    retain_atomic_maximum(&NETWORK_STARTUP_MAXIMUM_SAMPLE_GAP, value);
}

fn retain_maximum_lateness(value: u64) {
    retain_atomic_maximum(&MAXIMUM_DISPATCH_LATENESS, value);
}

fn retain_atomic_maximum(destination: &AtomicU32, value: u64) {
    let value = u32::try_from(value).unwrap_or(u32::MAX);
    let mut current = destination.load(Ordering::Relaxed);
    while value > current {
        match destination.compare_exchange_weak(
            current,
            value,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

async fn wait_for_flag_or_fault(flag: &AtomicBool) {
    while !flag.load(Ordering::Acquire) {
        let fault = HIL_FAULT.load(Ordering::Acquire);
        if fault != 0 {
            esp_println_uart::println!("ALUMINA_HIL_ABORT code={} stage=realtime-wait", fault);
            error!("HIL_ABORT realtime initialization fault={}", fault);
            park().await
        }
        Timer::after(Duration::from_millis(1)).await;
    }
}

async fn realtime_safe_fault(
    resources: &mut mks_tinybee::EstablishedRealtimeResources,
    timing_marker: &mut Output<'static>,
    sink_marker: &mut Output<'static>,
    code: u32,
) -> ! {
    let _ = resources.force_safe_outputs();
    realtime_fault(timing_marker, sink_marker, code).await
}

async fn realtime_fault(
    timing_marker: &mut Output<'static>,
    sink_marker: &mut Output<'static>,
    code: u32,
) -> ! {
    timing_marker.set_low();
    sink_marker.set_low();
    SINK_ACTIVE.store(false, Ordering::Release);
    HIL_FAULT.store(code, Ordering::Release);
    esp_println_uart::println!("ALUMINA_HIL_RT_FAULT code={}", code);
    error!("HIL_RT_FAULT code={}", code);
    park().await
}

async fn park() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
