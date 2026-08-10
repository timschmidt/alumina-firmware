//! T-Deck Pro I2C scanner firmware.
//!
//! It probes the usable seven-bit address range once per second and logs ACKs.

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::i2c::master::I2c;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// Emit metadata required by ESP-IDF-compatible bootloaders.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Debug);
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(timer0.alarm0);

    let i2c_scl = peripherals.GPIO14;
    let i2c_sda = peripherals.GPIO13;

    let config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100));

    let mut rst = Output::new(peripherals.GPIO45, Level::High, OutputConfig::default());
    rst.set_low();
    Timer::after(Duration::from_millis(20)).await;
    rst.set_high();
    Timer::after(Duration::from_millis(300)).await;

    let mut i2c = I2c::new(peripherals.I2C0, config)
        .expect("Failed to create i2c.")
        .with_sda(i2c_sda)
        .with_scl(i2c_scl)
        .into_async();

    loop {
        log::debug!("i2c scan starting.");
        // Probe the nonreserved seven-bit address range.
        for address in 0x08..=0x77 {
            if i2c.write_async(address, &[]).await.is_ok() {
                log::debug!("   -> Found device at address 0x{address:02X}");
            }
        }
        log::debug!("i2c scan complete.");
        Timer::after(Duration::from_secs(1)).await;
    }
}
