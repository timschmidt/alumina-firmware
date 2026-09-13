#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames)]
#![allow(
    dead_code,
    reason = "the isolated radio HIL binary retains the production board split but intentionally leaves unrelated service and real-time APIs inert"
)]

extern crate alloc;

#[cfg(not(feature = "hil-mks-tinybee-wifi-radio-safe"))]
compile_error!("this binary requires `hil-mks-tinybee-wifi-radio-safe`");

#[path = "hardware/mod.rs"]
mod hardware;
#[path = "storage.rs"]
mod storage;

use alloc::string::String;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::{
    AuthenticationMethod, Config as WifiConfig, ControllerConfig, ap::AccessPointConfig,
    sta::StationConfig,
};
// Force-link the UART-backed Defmt logger used by ESP dependencies even though
// the fixture's stable evidence is emitted as allocation-free UART text.
use esp_println_uart as _;

use hardware::mks_tinybee;

const AP_SSID: &str = "Alumina-radio-safe";
const AP_PASSPHRASE: &str = "alumina-development";
const RECLAIMED_HEAP_BYTES: usize = 96 * 1_024;
const AP_ONLY_OBSERVATION: Duration = Duration::from_secs(15);

esp_bootloader_esp_idf::esp_app_desc!();

/// Disconnected-load radio reduction. The complete TinyBee shifted safe image
/// is established before the scheduler or radio starts. It then holds a single,
/// stable SSID through an AP-only to AP+STA transition. No storage, motion,
/// process-output, web, or second-core actor is constructed.
#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let mks_tinybee::SplitResources {
        runtime,
        mut service,
        realtime,
    } = mks_tinybee::split(peripherals);
    service.write_diagnostic_boot_marker();
    esp_println_uart::println!("ALUMINA_WIFI_HIL stage=boot");

    let safe_outputs = match realtime.establish_safe_outputs() {
        Ok(outputs) => outputs,
        Err(_) => halt("safe-output-establishment"),
    };
    esp_println_uart::println!("ALUMINA_WIFI_HIL stage=outputs-safe");

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_HEAP_BYTES);
    let timer_group0 = TimerGroup::new(runtime.timer_group0);
    let software_interrupt = SoftwareInterruptControl::new(runtime.software_interrupt);
    esp_rtos::start(timer_group0.timer0, software_interrupt.software_interrupt0);
    let _retained_runtime = (runtime.cpu_control, software_interrupt.software_interrupt1);

    let ap_only = access_point(AP_SSID);
    let radio_config =
        ControllerConfig::default().with_initial_config(WifiConfig::AccessPoint(ap_only));
    let (mut controller, _interfaces) =
        match esp_radio::wifi::new(service.take_wifi(), radio_config) {
            Ok(parts) => parts,
            Err(_) => halt("radio-initialization"),
        };
    esp_println_uart::println!(
        "ALUMINA_WIFI_HIL stage=ap-only ssid={} channel=6 seconds={}",
        AP_SSID,
        AP_ONLY_OBSERVATION.as_secs()
    );
    Timer::after(AP_ONLY_OBSERVATION).await;

    if controller
        .set_config(&WifiConfig::AccessPointStation(
            StationConfig::default(),
            access_point(AP_SSID),
        ))
        .is_err()
    {
        halt("ap-station-configuration");
    }
    esp_println_uart::println!("ALUMINA_WIFI_HIL stage=ap-station ssid={}", AP_SSID);

    // Preserve every singleton owner for the complete observation. The safe
    // output object is never converted into a streaming or direct-output API.
    let _retained_owners = (&safe_outputs, &_retained_runtime, &controller);
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

fn access_point(ssid: &str) -> AccessPointConfig {
    AccessPointConfig::default()
        .with_ssid(String::from(ssid))
        .with_password(String::from(AP_PASSPHRASE))
        .with_auth_method(AuthenticationMethod::Wpa2Personal)
        .with_channel(6)
        .with_max_connections(1)
}

fn halt(stage: &str) -> ! {
    esp_println_uart::println!("ALUMINA_WIFI_HIL_ABORT stage={}", stage);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    esp_println_uart::println!("ALUMINA_WIFI_HIL_PANIC {}", info);
    loop {
        core::hint::spin_loop();
    }
}
