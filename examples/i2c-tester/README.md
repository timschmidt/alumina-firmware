# i2c-tester

Minimal `no_std` ESP32-S3 firmware that scans the T-Deck Pro's I2C0 bus and logs every responding seven-bit address.

The binary configures GPIO14 as SCL, GPIO13 as SDA, pulses the shared GPIO45 reset line, and probes addresses `0x08..=0x77` once per second. Reserved low and high address ranges are intentionally skipped.

## Build and run

Install the Espressif Rust tools described in the workspace [SETUP guide](../SETUP.md), then run:

```sh
source "$HOME/export-esp.sh"
cargo build --bin i2c-tester --release --locked
cargo flash --bin i2c-tester --release
```

The workspace runner uses `espflash flash --monitor`, so detected addresses appear in the monitor. A normal T-Deck Pro may report the battery charger, keyboard scanner, touch controller, and other populated I2C peripherals; compare results with [device.md](../device.md) and the board schematic.

The scanner uses an empty write as its address probe. Some devices or controllers may not support that probing style, so absence from the log is not definitive proof that no device is present.

## References

- [I2C-bus specification and user manual](https://www.nxp.com/docs/en/user-guide/UM10204.pdf)
- [`esp-hal` asynchronous I2C API](https://docs.espressif.com/projects/rust/esp-hal/1.0.0/esp32s3/esp_hal/i2c/master/)
- [LILYGO T-Deck Pro hardware](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- [ESP-IDF application image format](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/app_image_format.html)

## License

Licensed under [Apache-2.0](../LICENSE).
