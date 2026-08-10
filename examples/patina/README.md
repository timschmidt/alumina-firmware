# Patina

`patina` is an experimental asynchronous firmware application for the LILYGO T-Deck Pro. It demonstrates how to combine this workspace's shared-bus adapters and battery, keyboard, touch, and e-paper drivers in one `no_std` Embassy application.

The firmware is an integration example, not yet a general-purpose user interface or production device image. Its network task is a placeholder and its display presents a compact peripheral-status screen.

## Architecture

Peripheral tasks publish bounded `AppEvent` messages to a central coordinator:

- `battery_task` samples `BatteryService` every 30 seconds and reports changed measurements.
- `keyboard_task` reads `KeyboardController` events and reports completed key actions.
- `touch_task` reads `TouchController` slots and reports the first active point.
- `network_task` currently publishes one “not started” status.
- `app_task` applies events to `AppModel`, coalesces updates, and emits `DisplayCmd::Render` commands.
- `epd_task` owns `EInkDisplay` and refreshes only when commanded.

The coordinator enforces a three-second minimum e-paper refresh interval. Key events request an immediate eligible refresh, touch events are debounced for 150 ms, network events are coalesced for 500 ms, and battery changes are delayed for up to three seconds.

I2C0 is shared by the BQ25896, TCA8418, and CST328 through `Rc<RwLock<_>>` and `RwLockI2cDevice`. SPI2 is wrapped in `RwLockDevice` for the display. These `Rc`-based adapters are single-executor tools and are intentionally not `Send`.

## Build and flash

Install and activate the Espressif Rust toolchain as described in the workspace [setup guide](../SETUP.md), then run from the workspace root:

```sh
cargo check -p patina --bin patina --locked
cargo flash -p patina --bin patina --release
```

The root Cargo configuration selects `xtensa-esp32s3-none-elf` and uses `espflash` as the runner. The firmware expects the pin assignments in [device.md](../device.md), including GPIO14/GPIO13 for I2C and the board's shared GPIO45 reset line.

## Hardware test

`tests/hello_test.rs` is an `embedded-test` smoke test and requires a connected ESP32-S3 board:

```sh
cargo test -p patina --test hello_test
```

It cannot execute as an ordinary host test because the workspace targets Xtensa `no_std` firmware.

## Current limitations

- Network initialization and application UI controls are not implemented.
- Peripheral initialization failures are logged, but keyboard and touch tasks still run so hardware recovery policy can be developed in one place.
- Display rendering uses a single full-frame status view; independent dirty-region updates are future work.
- The touch status view displays only the first active hardware slot.

## References

- [LILYGO T-Deck Pro hardware, pin definitions, and examples](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [Embassy executor](https://docs.embassy.dev/embassy-executor/) and [`embassy-sync`](https://docs.rs/embassy-sync/0.7)
- [`esp-hal` for ESP32-S3](https://docs.espressif.com/projects/rust/esp-hal/1.0.0/esp32s3/esp_hal/) and the [Rust on ESP Book](https://docs.esp-rs.org/book/)
- [`embedded-graphics`](https://docs.rs/embedded-graphics/0.8)
- Workspace drivers: [`embedded-bus-async`](../embedded-bus-async/README.md), [battery](../t-deck-pro-battery-async/README.md), [keyboard](../t-deck-pro-keyboard-async/README.md), [touch](../t-deck-pro-touch-async/README.md), and [e-paper](../t-deck-pro-epd-async/README.md)

## License

Licensed under [Apache-2.0](../LICENSE).
