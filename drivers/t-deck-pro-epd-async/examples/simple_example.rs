#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::fmt::Write;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;

use alloc::rc::Rc;
use embassy_executor::Spawner;
use embassy_sync::rwlock::RwLock;
use embassy_time::{Delay, Duration, Timer};
use embedded_bus_async::spi::RwLockDevice;
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use esp_println::println;
use log::info;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern crate alloc;

// Emit metadata required by ESP-IDF-compatible bootloaders.
esp_bootloader_esp_idf::esp_app_desc!();

use t_deck_pro_epd_async::EInkDisplay;

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    info!("Logger initialized");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    info!("Peripherals initialized");

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(timer0.alarm0);
    let epd_dc = Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
    let epd_busy = Input::new(peripherals.GPIO37, InputConfig::default());
    let epd_rst = Output::new(peripherals.GPIO45, Level::High, OutputConfig::default());

    let sclk = peripherals.GPIO36;
    let mosi = peripherals.GPIO33;
    let cs = Output::new(peripherals.GPIO34, Level::High, OutputConfig::default());

    Timer::after(Duration::from_secs(1)).await;

    let spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_khz(100))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(sclk)
    .with_mosi(mosi)
    .into_async();

    let spi_bus = Rc::new(RwLock::new(spi));
    let mut spi_device =
        RwLockDevice::new(spi_bus, cs, Delay).expect("failed to deassert EPD chip select");

    let mut display = EInkDisplay::new(epd_dc, epd_busy, Some(epd_rst), 240, 320, false);

    display
        .init(&mut spi_device)
        .await
        .expect("Failed to initialize the e-paper display");

    let mut counter = 0;
    info!("Entering main loop.");
    loop {
        display.clear(BinaryColor::Off).unwrap();
        let mut text_buf = heapless::String::<32>::new();
        write!(text_buf, "Counter: {counter}").unwrap();
        Text::new(
            &text_buf,
            Point::new(80, 150),
            MonoTextStyle::new(&FONT_10X20, BinaryColor::On),
        )
        .draw(&mut display)
        .unwrap();

        display
            .refresh_display(&mut spi_device)
            .await
            .expect("Failed to refresh the e-paper display");
        counter += 1;
        Timer::after(Duration::from_secs(5)).await;
    }
}
