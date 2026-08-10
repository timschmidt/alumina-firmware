# t-deck-pro-battery-async

Asynchronous `no_std` driver for the T-Deck Pro's BQ25896 charger and power-path controller. The driver uses any seven-bit `embedded-hal-async::i2c::I2c` implementation and communicates at address `0x6B`.

## API

- `BatteryService::new` takes ownership of an I2C handle.
- `enable_adc` and `disable_adc` control continuous ADC conversion.
- `measure` enables the ADC when necessary, waits one second for its first conversion, and returns `BatteryData`.
- `BatteryData` reports battery voltage, VBUS voltage, charge current, thermistor percentage, charge state, input state, power-good state, and `FaultStatus`.
- `ChargingStatus`, `VbusStatus`, `ChargeFault`, and `NtcFault` decode the BQ25896 status fields.

The current API logs the underlying I2C error and returns `Result<_, ()>`. Disable the ADC after a sampling burst when power consumption matters.

## Usage

```rust,ignore
use t_deck_pro_battery_async::{BatteryService, ChargingStatus};

let mut battery = BatteryService::new(i2c);
let sample = battery.measure().await?;

if sample.charging_status == ChargingStatus::ChargeDone {
    // Charging has completed.
}

battery.disable_adc().await?;
```

On T-Deck Pro, I2C0 is shared with the keyboard and touch controller. Use [`embedded-bus-async`](../embedded-bus-async/README.md) or another compatible bus manager when these drivers run together. The complete ESP32-S3 wiring is shown in [`simple_battery.rs`](examples/simple_battery.rs).

## References

- [Texas Instruments BQ25896 datasheet](https://www.ti.com/lit/ds/symlink/bq25896.pdf)
- [LILYGO T-Deck Pro examples and schematic material](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [`embedded-hal-async` I2C trait](https://docs.rs/embedded-hal-async/1/embedded_hal_async/i2c/trait.I2c.html)
- [Rust on ESP Book](https://docs.esp-rs.org/book/)

## License

Licensed under [Apache-2.0](../LICENSE).
