# T-Deck Pro async drivers

Asynchronous, `no_std` Rust drivers and firmware experiments for the [LILYGO T-Deck Pro](https://github.com/Xinyuan-LilyGO/T-Deck-Pro), built with `embedded-hal-async`, Embassy, and `esp-hal` for the ESP32-S3.

The drivers cover the current e-paper T-Deck Pro hardware and are still evolving. Most return compact `Result<_, ()>` errors and assume the board wiring documented in [device.md](device.md); applications should log failures and choose retry or recovery policies appropriate for their hardware.

## Workspace crates

| Package | Main API | Purpose |
| --- | --- | --- |
| [`embedded-bus-async`](embedded-bus-async/README.md) | `RwLockI2cDevice`, `RwLockDevice` | Share one I2C or SPI controller between cooperative async tasks |
| [`sx126x_async`](sx126x-async-rs/README.md) | `SX126x`, `Config`, `PacketParams`, `ModParams` | Low-level, HAL-generic SX1261/SX1262 radio commands |
| [`t-deck-pro-battery-async`](t-deck-pro-battery-async/README.md) | `BatteryService`, `BatteryData`, `FaultStatus` | BQ25896 charger status and ADC measurements |
| [`t-deck-pro-epd-async`](t-deck-pro-epd-async/README.md) | `EInkDisplay` | UC8253/GDEQ031T10 framebuffer, full refresh, and partial refresh |
| [`t-deck-pro-gps-async`](t-deck-pro-gps-async/README.md) | `Gps`, `GpsData`, `PowerMode` | MIA-M10Q UART, UBX power control, and NMEA parsing |
| [`t-deck-pro-keyboard-async`](t-deck-pro-keyboard-async/README.md) | `KeyboardController`, `KeyEvent` | TCA8418 interrupt FIFO and T-Deck key mapping |
| [`t-deck-pro-lora-async`](t-deck-pro-lora-async/README.md) | `LoraRadio`, `LoraConfig` | Board-specific SX1262 initialization, transmit, and receive |
| [`t-deck-pro-touch-async`](t-deck-pro-touch-async/README.md) | `TouchController`, `TouchPoint` | CST328 interrupt-driven multitouch reads |
| [`i2c-tester`](i2c-tester/README.md) | `i2c-tester` binary | Probe the board's usable seven-bit I2C address range |
| [`patina`](patina/README.md) | `patina` binary | Integrate the bus, battery, keyboard, touch, and display drivers |

The device drivers are the reusable foundation. `i2c-tester` and `patina` are firmware applications rather than libraries.

## Build and flash

Follow [SETUP.md](SETUP.md) to install the Espressif Rust toolchain and `espflash`, then load the environment created by `espup`:

```sh
source "$HOME/export-esp.sh"
cargo check --workspace --lib --bins --examples --locked
```

The root [`.cargo/config.toml`](.cargo/config.toml) selects `xtensa-esp32s3-none-elf`. Build a specific firmware image or example from the workspace root:

```sh
cargo build -p i2c-tester --bin i2c-tester --release --locked
cargo build -p t-deck-pro-epd-async --example simple_example --release --locked
```

Flash and monitor a selected target with `espflash`; for example:

```sh
cargo flash -p t-deck-pro-epd-async --example simple_example --release
```

Hardware-backed `embedded-test` tests in `patina` require a connected board. Ordinary `cargo test` cannot run Xtensa `no_std` binaries on the host.

## Integration model

The T-Deck Pro multiplexes peripherals across buses and, in some cases, reset or SPI pins. Create one Embassy `RwLock` per physical bus, clone an `Rc` handle into one wrapper per device, and keep all users on the same cooperative executor. `Rc` deliberately makes these wrappers single-threaded; they are not `Send` and must not cross executor threads.

The `patina` firmware demonstrates the intended composition:

1. Pulse the shared reset line before transferring it to the display driver.
2. Share I2C0 among battery, keyboard, and touch services.
3. Give each peripheral its own task and publish state changes through bounded channels.
4. Coalesce updates in a central model so the e-paper display is not refreshed for every input sample.

Consult [device.md](device.md) before changing pin assignments. In particular, the display, LoRa radio, and SD card share SPI signals and require independent chip-select handling.

## References

- [LILYGO T-Deck Pro hardware and examples](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [Rust on ESP Book](https://docs.esp-rs.org/book/) and [`esp-hal` documentation](https://docs.espressif.com/projects/rust/esp-hal/1.0.0/esp32s3/esp_hal/)
- [Embassy documentation](https://embassy.dev/) and [`embassy-sync`](https://docs.rs/embassy-sync/0.7)
- [`embedded-hal` 1.0](https://docs.rs/embedded-hal/1) and [`embedded-hal-async` 1.0](https://docs.rs/embedded-hal-async/1)
- [ESP-IDF application image format](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/app_image_format.html)
- Local component references: [SX1262 module](t-deck-pro-lora-async/S62F_SX1262.md), [MIA-M10Q GPS](t-deck-pro-gps-async/MIA-M10Q%20GPS.md), [GPS power modes](t-deck-pro-gps-async/GPS%20Power%20modes.md), [TCA8418 keyboard](t-deck-pro-keyboard-async/TCA8418%20keypad%20scan%20IC.md), [UC8253 e-paper](t-deck-pro-epd-async/UC8253%20EPD.md), and the component datasheets stored under each crate's `docs/` directory

## License

Unless a member crate states otherwise, this workspace is licensed under [Apache-2.0](LICENSE). `sx126x_async` retains its upstream dual MIT/Apache-2.0 license declaration.
