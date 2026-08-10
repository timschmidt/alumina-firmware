//! Hardware smoke test using `embedded-test`.
//!
//! Run it with an attached ESP32-S3 using
//! `cargo test -p patina --test hello_test` from the workspace root.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use defmt::assert_eq;

    #[init]
    fn init() {
        let peripherals = esp_hal::init(esp_hal::Config::default());

        let timg1 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG1);
        esp_rtos::start(timg1.timer0);

        rtt_target::rtt_init_defmt!();
    }

    #[test]
    async fn executor_and_timer_are_available() {
        defmt::info!("Checking the Embassy executor and timer");

        embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
        assert_eq!(1 + 1, 2);
    }
}
