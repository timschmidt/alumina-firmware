#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is unsafe for esp-hal values that borrow transfer buffers"
)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    time::Rate,
    timer::systimer::SystemTimer,
};
use esp_println::println;
use log::info;
use t_deck_pro_touch_async::touch::TouchController;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern crate alloc;

// Emit metadata required by ESP-IDF-compatible bootloaders.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    info!("Logger initialized");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    info!("Peripherals initialized");

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(timer0.alarm0);

    let shared_rst = Output::new(peripherals.GPIO45, Level::High, OutputConfig::default());

    let touch_scl = peripherals.GPIO14;
    let touch_sda = peripherals.GPIO13;
    let touch_int = Input::new(peripherals.GPIO12, InputConfig::default());

    let config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100));

    let touch_i2c = I2c::new(peripherals.I2C0, config)
        .unwrap()
        .with_sda(touch_sda)
        .with_scl(touch_scl)
        .into_async();
    let mut touch_controller = TouchController::new(touch_i2c, touch_int, Some(shared_rst));
    match touch_controller.init().await {
        Ok(_) => log::debug!("Touch controller initialized."),
        Err(_) => log::warn!("Error initializing touch controller."),
    };

    spawner.spawn(read_touch(touch_controller)).unwrap();

    info!("Drawing complete. Entering idle loop.");
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn read_touch(
    mut touch_controller: TouchController<
        'static,
        I2c<'static, Async>,
        esp_hal::i2c::master::Error,
    >,
) {
    loop {
        match touch_controller.read_touches().await {
            Ok(touches) => {
                info!("Touches detected {touches:?}");
            }
            Err(err) => {
                log::warn!("Error receiving touch: {err:?}");
            }
        }
        Timer::after(Duration::from_millis(20)).await;
    }
}
