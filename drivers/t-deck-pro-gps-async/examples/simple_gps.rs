#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, with_timeout};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pin};
use esp_hal::i2c::master::I2c;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use log::{error, info, trace};
use t_deck_pro_gps_async::{Gps, PowerMode};
use t_deck_pro_keyboard_async::keyboard::{KeyState, KeyboardController};

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("{info}");
    loop {}
}

extern crate alloc;

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Trace);

    info!("Logger initialized");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    info!("Peripherals initialized");

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(timer0.alarm0);
    let keyboard_scl = peripherals.GPIO14;
    let keyboard_sda = peripherals.GPIO13;
    let keyboard_int = Input::new(peripherals.GPIO15, InputConfig::default());

    let config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100));

    let shared_rst = Output::new(peripherals.GPIO45, Level::High, OutputConfig::default());
    let keyboard_i2c = I2c::new(peripherals.I2C0, config)
        .unwrap()
        .with_sda(keyboard_sda)
        .with_scl(keyboard_scl)
        .into_async();
    let mut keyboard_controller =
        KeyboardController::new(keyboard_i2c, keyboard_int, Some(shared_rst));
    match keyboard_controller.init().await {
        Ok(_) => log::debug!("Keyboard controller initialized."),
        Err(_) => log::warn!("Error initializing keyboard controller."),
    };

    let tx = peripherals.GPIO43.degrade();
    let rx = peripherals.GPIO44.degrade();
    let enable_pin = Output::new(peripherals.GPIO39, Level::High, OutputConfig::default());

    info!("Initializing GPS");
    let mut gps = Gps::new(peripherals.UART1, tx, rx, enable_pin);

    info!("Entering main loop to read GPS messages...");
    loop {
        let key_event_fut = keyboard_controller.read_key_events();
        let gps_read_fut = gps.read_message();

        match embassy_futures::select::select(key_event_fut, gps_read_fut).await {
            embassy_futures::select::Either::First(key_events) => {
                if let Ok(events) = key_events {
                    for event in events {
                        if event.state == KeyState::Up {
                            info!("Key pressed: {:?}", event.key);
                            match event.key {
                                'q' => {
                                    info!("Attempting to recover GPS (cold start)...");
                                    if with_timeout(Duration::from_secs(1), gps.recovery())
                                        .await
                                        .is_err()
                                    {
                                        error!("GPS recovery failed.");
                                    } else {
                                        info!("GPS recovery command sent successfully.");
                                    }
                                }
                                'w' => {
                                    info!("Setting GPS to Normal mode...");
                                    if with_timeout(
                                        Duration::from_secs(1),
                                        gps.set_power_mode(PowerMode::Normal),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        error!("Failed to set power mode to Normal.");
                                    } else {
                                        info!("Successfully set power mode to Normal.");
                                    }
                                }
                                'e' => {
                                    info!("Setting GPS to Power Save mode static hold...");
                                    if with_timeout(
                                        Duration::from_secs(1),
                                        gps.set_power_mode(PowerMode::Eco {
                                            update_rate_ms: Duration::from_secs(30).as_millis()
                                                as _,
                                        }),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        error!("Failed to set power mode to Power Save.");
                                    } else {
                                        info!("Successfully set power mode to Power Save.");
                                    }
                                }
                                'r' => {
                                    info!("Setting GPS to Software Standby mode...");
                                    if with_timeout(
                                        Duration::from_secs(1),
                                        gps.set_power_mode(PowerMode::SoftwareStandby),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        error!("Failed to set power mode to Standby.");
                                    } else {
                                        info!("Successfully set power mode to Standby.");
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            embassy_futures::select::Either::Second(gps_data_result) => match gps_data_result {
                Ok(gps_data) => {
                    info!("Received new GPS data: {gps_data:?}");
                }
                Err(_) => trace!("GPS read did not produce a complete message"),
            },
        }
    }
}
