# LILYGO T-Deck Pro hardware

This workspace targets the e-paper LILYGO T-Deck Pro family built around the ESP32-S3FN16R8. LILYGO publishes multiple hardware revisions and audio/modem variants; confirm the board revision against the vendor schematic before relying on a pin or peripheral that is not exercised by this repository.

## Core hardware

- ESP32-S3 dual-core Xtensa LX7, with 16 MiB flash and 8 MiB PSRAM
- 3.1-inch GDEQ031T10 e-paper panel with a UC8253 controller and 240 × 320 logical framebuffer
- CST328 touch controller and TCA8418 keyboard
- BQ25896 charger, with a BQ27220 fuel gauge present on current vendor revisions
- SX1262 LoRa radio and u-blox MIA-M10Q GNSS receiver
- BHI260AP IMU, LTR-553ALS light sensor, microSD, microphone, and vibration motor
- Variant-dependent PCM5102A audio or A7682E cellular module

The [vendor repository](https://github.com/Xinyuan-LilyGO/T-Deck-Pro) is authoritative for revision differences, schematics, and component addresses.

## Shared buses and ownership

I2C0 uses GPIO14 for SCL and GPIO13 for SDA. The battery charger, keyboard, touch controller, IMU, light sensor, and fuel gauge share that bus. Applications should use a single controller with per-device adapters such as this workspace's `RwLockI2cDevice`.

The display, LoRa radio, and microSD card share GPIO36 (SCK), GPIO33 (MOSI), and GPIO47 (MISO where required), but have independent chip-select lines. Do not drive multiple chip selects simultaneously. Vendor pin definitions assign GPIO45 to touch reset and list no dedicated e-paper reset; the current examples also pass GPIO45 to the display driver, so verify that reset assumption against the board revision in hand.

## Pin assignments

| Peripheral | Signal | GPIO |
| --- | --- | ---: |
| Shared I2C | SCL / SDA | 14 / 13 |
| Touch | interrupt / reset | 12 / 45 |
| Keyboard | interrupt / backlight | 15 / 42 |
| E-paper | SCK / MOSI / CS / DC / BUSY | 36 / 33 / 34 / 35 / 37 |
| LoRa | SCK / MOSI / MISO / CS / BUSY / reset / DIO1 | 36 / 33 / 47 / 3 / 6 / 4 / 5 |
| microSD | SCK / MOSI / MISO / CS | 36 / 33 / 47 / 48 |
| GPS | ESP32 RX / ESP32 TX / PPS / enable | 44 / 43 / 1 / 39 |
| IMU | interrupt / 1.8 V enable | 21 / 38 |
| Light sensor | interrupt | 16 |
| Microphone | data / clock | 17 / 18 |
| Vibration motor | control | 2 |
| LoRa power | enable | 46 |
| Modem variant | RI / DTR / reset / RX / TX / power key | 7 / 8 / 9 / 10 / 11 / 40 |
| Audio variant | I2S BCLK / data / LRCLK | 7 / 8 / 9 |

Signal names follow the vendor's ESP32-side definitions. For UART connections, pair the ESP32 transmit signal with the module receive signal and vice versa.

## Workspace support

| Component | Workspace package | Main API |
| --- | --- | --- |
| Shared I2C/SPI | [`embedded-bus-async`](embedded-bus-async/README.md) | `RwLockI2cDevice`, `RwLockDevice` |
| BQ25896 | [`t-deck-pro-battery-async`](t-deck-pro-battery-async/README.md) | `BatteryService` |
| UC8253/GDEQ031T10 | [`t-deck-pro-epd-async`](t-deck-pro-epd-async/README.md) | `EInkDisplay` |
| MIA-M10Q | [`t-deck-pro-gps-async`](t-deck-pro-gps-async/README.md) | `Gps` |
| TCA8418 | [`t-deck-pro-keyboard-async`](t-deck-pro-keyboard-async/README.md) | `KeyboardController` |
| SX1262 | [`sx126x_async`](sx126x-async-rs/README.md), [`t-deck-pro-lora-async`](t-deck-pro-lora-async/README.md) | `SX126x`, `LoraRadio` |
| CST328 | [`t-deck-pro-touch-async`](t-deck-pro-touch-async/README.md) | `TouchController` |

The IMU, light sensor, fuel gauge, storage, audio/modem, and motor do not yet have dedicated drivers in this workspace.

## References

- [LILYGO T-Deck Pro repository, revision table, source pin definitions, and schematics](https://github.com/Xinyuan-LilyGO/T-Deck-Pro)
- Local component notes and datasheets linked from each driver README
- Root-level local documents: [`S62F_Lora.pdf`](S62F_Lora.pdf), [`T-HeadJack V1.0.pdf`](T-HeadJack%20V1.0.pdf), [`XL9535_C561273.pdf`](XL9535_C561273.pdf), and [`pcm5102a.pdf`](pcm5102a.pdf)
