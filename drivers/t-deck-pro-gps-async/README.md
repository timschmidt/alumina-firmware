# t-deck-pro-gps-async

Asynchronous `no_std` driver for the T-Deck Pro's u-blox MIA-M10Q GNSS module. `Gps` owns ESP32-S3 UART1 and the module-enable pin, sends UBX configuration commands, and parses NMEA 0183 sentences with the `nmea` crate.

## API

- `Gps::new` configures UART1 for 38,400 baud and drives the enable pin high.
- `set_power_mode` selects `Normal`, `Eco { update_rate_ms }`, or `SoftwareStandby` by sending UBX commands.
- `recovery` requests a UBX cold start.
- `read_message` reads until the parser has time, date, fix type, latitude, and longitude, then returns `GpsData`.
- `GpsData::has_fix` distinguishes a valid navigation fix from `FixType::NoFix` or missing fix data.
- `Time`, `Date`, and `FixType` provide allocation-free decoded fields; speed is reported in knots.

The current `Normal` configuration uses a two-second measurement interval. `Eco` sets the requested measurement interval but does not expose every u-blox power-management option. Errors are logged or collapsed to `()`.

## Usage

```rust,ignore
use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use t_deck_pro_gps_async::{Gps, PowerMode};

let tx: AnyPin = peripherals.GPIO43.degrade();
let rx: AnyPin = peripherals.GPIO44.degrade();
let enable = Output::new(peripherals.GPIO39, Level::High, OutputConfig::default());
let mut gps = Gps::new(peripherals.UART1, tx, rx, enable);

gps.set_power_mode(PowerMode::Eco {
    update_rate_ms: 10_000,
}).await?;

loop {
    let data = gps.read_message().await?;
    if data.has_fix() {
        // Use data.latitude, data.longitude, and data.fix_time.
    }
}
```

See [`simple_gps.rs`](examples/simple_gps.rs) for the complete ESP32-S3 startup and task wiring.

## References

- [MIA-M10Q integration manual](docs/mia-m10q-integration-manual.pdf)
- [Local MIA-M10Q notes](MIA-M10Q%20GPS.md) and [power-mode notes](GPS%20Power%20modes.md)
- [u-blox MIA-M10Q product resources](https://www.u-blox.com/en/product/mia-m10-series)
- [`nmea` crate documentation](https://docs.rs/nmea/0.5)
- [NMEA 0183 overview](https://gpsd.gitlab.io/gpsd/NMEA.html)
- [LILYGO T-Deck Pro GPS example](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)

## License

Licensed under [Apache-2.0](../LICENSE).
