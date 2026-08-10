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
mod network;
pub mod service;

use alumina_protocol::{DeviceCycle, Digest, FrameKind};
use alumina_runtime::{
    APP_CORE_STACK_WORDS, DeadlineProbe, DefaultBoundary, DefaultRealtimeEndpoint,
    DefaultServiceEndpoint, IntercoreFrame, RuntimeBudget, UrgentKind,
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
use service::{ServiceBridge, StorageServiceState, UnavailableStorageBackend, init_service_bridge};

static BOUNDARY: StaticCell<DefaultBoundary> = StaticCell::new();
static APP_CORE_STACK: StaticCell<Stack<APP_CORE_STACK_WORDS>> = StaticCell::new();
static APP_CORE_EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();

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

    let service_bridge = init_service_bridge();
    let network = network::start(spawner, split.service.take_wifi(), service_bridge).await;

    let boundary = BOUNDARY.init(DefaultBoundary::new());
    let (service_endpoint, realtime_endpoint) = boundary.split();
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

#[embassy_executor::task]
async fn service_task(
    resources: selected::ServiceResources,
    mut endpoint: DefaultServiceEndpoint,
    network: network::NetworkControl,
    service_bridge: &'static ServiceBridge,
) {
    if Cpu::current() != Cpu::ProCpu {
        panic!("service executor started on the wrong core");
    }

    // Service admission and every future media/backend handle live only in the
    // core-0 task future. Core 1 receives verified owned blocks, never SD.
    let mut storage = StorageServiceState::new();
    let mut storage_backend = UnavailableStorageBackend;
    let mut sequence = 0_u32;
    let mut last_fault_generation = 0_u16;
    loop {
        sequence = sequence.wrapping_add(1);
        if let Ok(frame) = IntercoreFrame::new(
            FrameKind::Command,
            sequence,
            DeviceCycle(Instant::now().as_ticks()),
            Digest::ZERO,
            &[0],
        ) {
            let _ = endpoint.try_send_command(frame);
        }

        while let Ok(frame) = endpoint.try_receive_telemetry() {
            if frame.validate(FrameKind::Telemetry).is_err() {
                endpoint.publish_urgent(UrgentKind::EmergencyStop, 1);
            }
        }
        if let Some(fault) = endpoint.fault_after(last_fault_generation) {
            last_fault_generation = fault.generation;
            error!("RT fault code={} detail={}", fault.code, fault.detail);
        }

        while let Some(request) = service_bridge.try_receive() {
            let response = storage
                .dispatch(
                    &mut storage_backend,
                    request.request(),
                    DeviceCycle(Instant::now().as_ticks()),
                )
                .await;
            service_bridge.respond(&request, response);
        }

        let _keep_service_state_core_local = (
            &resources,
            &storage,
            &storage_backend,
            network.supervisor(),
            network.credential_source(),
        );
        Timer::after(Duration::from_millis(100)).await;
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

    let period = Duration::from_millis(1);
    let mut expected = Instant::now() + period;
    let mut probe = DeadlineProbe::default();
    let mut telemetry_sequence = 0_u32;
    let mut divider = 0_u8;
    let mut urgent_generation = 0_u16;

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
                endpoint.publish_fault(2, urgent.detail);
            }
        }
        while let Ok(command) = endpoint.try_receive_command() {
            if command.validate(FrameKind::Command).is_err() {
                endpoint.publish_fault(3, 0);
            }
        }

        divider = divider.wrapping_add(1);
        if divider == 100 {
            divider = 0;
            telemetry_sequence = telemetry_sequence.wrapping_add(1);
            let maximum = probe.maximum_lateness_cycles().to_le_bytes();
            if let Ok(frame) = IntercoreFrame::new(
                FrameKind::Telemetry,
                telemetry_sequence,
                DeviceCycle(observed.as_ticks()),
                Digest::ZERO,
                &maximum,
            ) {
                let _ = endpoint.try_publish_telemetry(frame);
            }
        }

        let _keep_tokens_core_local = &resources;
    }
}
