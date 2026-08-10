# M2 board-metadata and composition evidence

Date: 2026-08-10

Status: pre-HIL implementation slice. This extends the M2 compile foundation;
it does not close the physical safe-state, peripheral, or timing exit gate.

## Implemented claim

- `alumina-board` now models flash intervals, named clocks and admitted error
  bounds, electrical/routing constraints, interrupt ownership and qualified
  latency, serialized-engine safe images, licensed board visuals with normalized
  resource hotspots, and explicit revision-specific HIL requirements.
- Fitted devices carry typed auxiliary interrupt/reset/enable/data resources in
  addition to their bus route. Cross-domain observation is explicit: an SD
  service, for example, may receive an RT-owned card-detect state through the
  bounded core boundary, but does not acquire that GPIO.
- Validation rejects missing auxiliary resources, incomplete/out-of-bounds or
  overlapping flash regions, writes allowed while armed, unsupported zero-error
  clock claims, broken constraints and interrupt routes, invalid visual assets
  and polygons, and broken/duplicate HIL requirements.
- The capability exporter emits all of these records plus internal SRAM,
  real-time PSRAM policy, and the canonical capability digest. The strict v1
  JSON Schema accepts both generated documents.
- Firmware composition consumes every newly described chip peripheral into one
  fixed owner. Core 0 keeps Wi-Fi, serial/storage, shared buses, display/input,
  sensors, GPS/LoRa, and microphone resources. Core 1 keeps all TinyBee motion,
  thermal/safety inputs and the T-Deck vibration output.

## Board-package boundary

| Package | Resources | Aliases | Buses | Devices | Clocks | Constraints | IRQs | Safe images | HIL gates | Visuals | Armable |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| MKS TinyBee V1.x | 61 | 50 | 3 | 1 | 2 | 9 | 4 | 1 | 8 | 0 | no |
| LILYGO T-Deck Pro | 50 | 16 | 3 | 12 | 2 | 6 | 6 | 0 | 7 | 0 | no |

TinyBee now describes the established routed I²S bits (IO128–IO142 and
IO144–IO151), heaters/fans, beeper, thermistors, material input, servo,
encoder/LCD routes, both serial controllers, SD detect multiplexing, input-only
GPIOs, boot straps, and its whole-engine output restrictions. The inferred safe
I²S value remains `0x00001249`, is still unverified, and cannot arm.

T-Deck Pro now describes the fuel gauge, ambient-light sensor, IMU, microphone,
vibration output, keyboard LED, sensor power, boot input, and their supporting
interrupt/enable/I²S/DMA routes in addition to the seven imported bus devices.
Official V1.x hardware facts establish GPIO45 as CST328 touch reset and the EPD
reset line as unconnected. The EPD example and Patina integration were corrected
accordingly and recompiled.

## Reproduced checks

Portable toolchain: `rustc 1.88.0 (6b00bc388 2025-06-23)`. ESP toolchain:
`rustc 1.90.0-nightly (abf50ae2e 2025-09-16)` with
`xtensa-esp-elf-gcc 14.2.0`.

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run -q -p xtask -- board list
cargo run -q -p xtask -- board check mks-tinybee-v1
cargo run -q -p xtask -- board check t-deck-pro
cargo run -q -p xtask -- capabilities --board mks-tinybee --json \
  | jsonschema -i /dev/stdin schemas/capabilities-v1.schema.json
cargo run -q -p xtask -- capabilities --board t-deck-pro --json \
  | jsonschema -i /dev/stdin schemas/capabilities-v1.schema.json

cargo +esp check --target xtensa-esp32s3-none-elf --locked --lib \
  -p embedded-bus-async -p sx126x_async \
  -p t-deck-pro-battery-async -p t-deck-pro-epd-async \
  -p t-deck-pro-gps-async -p t-deck-pro-keyboard-async \
  -p t-deck-pro-lora-async -p t-deck-pro-touch-async
cargo +esp check --target xtensa-esp32s3-none-elf --locked --bins \
  -p i2c-tester -p patina
cargo +esp check --target xtensa-esp32s3-none-elf --locked --examples \
  -p t-deck-pro-epd-async -p t-deck-pro-lora-async

cargo run -q -p xtask -- check --board mks-tinybee
cargo run -q -p xtask -- check --board t-deck-pro
cargo run -q -p xtask -- build --board mks-tinybee --profile release
cargo run -q -p xtask -- build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
git diff --check
```

Results: all 39 portable unit tests, documentation tests, and strict host Clippy
passed. Both complete firmware targets checked, linked in release mode, and
passed strict ESP Clippy. All imported driver libraries, both imported binaries,
and the EPD/LoRa examples checked on ESP32-S3 after the routing correction. ELF
section totals are 10,393 bytes for TinyBee and 10,520 bytes for T-Deck Pro at
this composition-only stage.

## Licensing and claim boundary

No vendor photo, schematic, firmware, or other GPL repository asset was copied
by this slice. Hardware facts were transcribed into new typed records; the two
small changes to the already imported Apache-2.0 examples are recorded in the
import manifest. Each board has an asset intake policy preferring a new fixture
photograph under `CC0-1.0`, with a content digest, dimensions, attribution, and
reviewed hotspots. The capability visual arrays remain empty until such assets
exist.

Flash layouts are intentionally empty, capability digests remain zero, clock
error and interrupt-latency bounds remain unknown, safe output polarity is not
bench verified, and exact fixture revisions/options have not been reconciled.
No board was flashed and no load was energized. There is no claim of Wi-Fi,
peripheral, executor-affinity, deadline, reset/watchdog, E-stop/limit, SD, or
annotated-overlay behavior on hardware. Both packages remain non-armable until
their exported HIL gates are satisfied and immutable evidence is archived.
