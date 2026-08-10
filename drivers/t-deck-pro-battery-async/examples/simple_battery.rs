#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is unsafe for esp-hal values that borrow transfer buffers"
)]
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::{Async, clock::CpuClock, i2c::master::I2c, time::Rate, timer::systimer::SystemTimer};
use esp_println::println;
use log::{error, info};
use t_deck_pro_battery_async::BatteryService;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    info!("Logger initialized");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    info!("Peripherals initialized");

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(timer0.alarm0);

    let battery_scl = peripherals.GPIO14;
    let battery_sda = peripherals.GPIO13;
    let config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100));

    let battery_i2c = I2c::new(peripherals.I2C0, config)
        .expect("Could not initialize I2C")
        .with_sda(battery_sda)
        .with_scl(battery_scl)
        .into_async();

    let battery_service = BatteryService::new(battery_i2c);

    spawner
        .spawn(read_battery_task(battery_service))
        .expect("Failed to spawn read_battery_task");

    info!("Spawned task. Entering idle loop.");
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn read_battery_task(
    mut battery_service: BatteryService<I2c<'static, Async>, esp_hal::i2c::master::Error>,
) {
    loop {
        match battery_service.measure().await {
            Ok(data) => {
                info!("--- Battery Status ---");
                info!("Voltage: {:.3} V", data.voltage);
                info!("VBUS Voltage: {:.3} V", data.vbus_voltage);
                info!("Charge Current: {:.3} A", data.charge_current);
                info!("Battery Temp Percent: {:.1}%", data.battery_temp_percent);
                info!("Charge Status: {:?}", data.charging_status);
                info!("VBUS Status: {:?}", data.vbus_status);
                info!("Power Good: {}", data.power_good);
                info!("Faults: {:?}", data.faults);
                info!("----------------------");
            }
            Err(_) => {
                error!("Failed to measure battery data.");
            }
        }

        if battery_service.disable_adc().await.is_err() {
            error!("Failed to disable ADC");
        }

        Timer::after(Duration::from_secs(5)).await;
    }
}
