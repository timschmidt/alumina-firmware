# M2 dual-core compile-foundation evidence

Date: 2026-08-10

Status: pre-HIL implementation slice. This evidence does not close the M2
hardware/timing exit gate.

## Implemented claim

- `alumina-runtime` provides owned fixed-capacity cross-core frames, an
  eight-entry ordered command path, a lossy 32-entry telemetry path, and
  independent atomic latest-value urgent/fault mailboxes. Its compile-time
  budget rejects zero-sized queues, an undersized second-core stack, insufficient
  real-time horizon, arithmetic overflow, and internal-memory overflow.
- The MKS TinyBee V1.x and LILYGO T-Deck Pro Rust packages validate chip, memory,
  two-core ownership, typed physical/virtual resources, aliases, routed buses,
  fitted devices, safe values, and shifted-output safe-image coverage.
- `xtask` reconciles each implemented TOML record against its Rust package and
  selects exactly one board feature, chip target, and Cargo profile. It finds
  the Xtensa GCC bundle installed under the `esp` rustup toolchain when the
  linker is not inherited on `PATH`.
- The firmware composition root consumes `esp_hal::Peripherals` once. Board
  modules return disjoint runtime, core-0 service, and core-1 real-time token
  sets. The runtime starts one Embassy executor per core through `esp-rtos`.
- Core 0 owns Wi-Fi, storage/serial buses, and all imported T-Deck device routes.
  Core 1 owns TinyBee I2S0, its DMA channel and routed pins, limit inputs, ADC,
  and the second timer group.
- Core 1 runs a one-millisecond deadline probe, handles urgent messages before
  ordinary commands, and never waits to publish ordinary telemetry. Core 0 may
  wait or drop ordinary work but can always publish the independent urgent
  signal.
- The async SPI sharing adapter no longer depends on an ESP HAL output type;
  any `embedded-hal` output pin is accepted, and real chip-select failures are
  propagated. Every adapted T-Deck target is recompiled below.
- `schemas/capabilities-v1.schema.json` fixes the current portable board export
  shape. Generated TinyBee and T-Deck documents validate against it.

## Package boundary

| Package | Typed resources | Aliases | Buses | Devices | Qualification | Armable |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| MKS TinyBee V1.x | 43 | 34 | 1 | 1 | compiles | no |
| LILYGO T-Deck Pro | 35 | 10 | 3 | 7 | compiles | no |

The TinyBee safe I2S image is `0x00001249`: bits 0, 3, 6, 9, and 12 are set for
the five StepStick disable routes. This value is inferred from the official
FluidNC mapping and plain active-high disable-pin notation. It is marked
`bench_verified = false`, the capability digest remains zero, and the package
cannot arm. No motor, heater, fan, or other process load was connected or
enabled for this evidence.

## Reproduced checks

Portable toolchain: `rustc 1.88.0 (6b00bc388 2025-06-23)`. ESP toolchain:
`rustc 1.90.0-nightly (abf50ae2e 2025-09-16)` with
`xtensa-esp-elf-gcc 14.2.0`.

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee
cargo xtask board check t-deck-pro
cargo xtask board check mks-esp32-foc-v1
cargo xtask board check t-lora-pager-current
cargo xtask capabilities --board mks-tinybee --json
cargo xtask capabilities --board t-deck-pro --json
cargo run --quiet -p xtask -- capabilities --board mks-tinybee --json \
  | jsonschema schemas/capabilities-v1.schema.json
cargo run --quiet -p xtask -- capabilities --board t-deck-pro --json \
  | jsonschema schemas/capabilities-v1.schema.json

cargo +esp check --target xtensa-esp32s3-none-elf --locked --lib \
  -p embedded-bus-async -p sx126x_async \
  -p t-deck-pro-battery-async -p t-deck-pro-epd-async \
  -p t-deck-pro-gps-async -p t-deck-pro-keyboard-async \
  -p t-deck-pro-lora-async -p t-deck-pro-touch-async
cargo +esp check --target xtensa-esp32s3-none-elf --locked --bins \
  -p i2c-tester -p patina
cargo +esp check --target xtensa-esp32s3-none-elf --locked --examples \
  -p t-deck-pro-epd-async -p t-deck-pro-lora-async

cargo xtask check --board mks-tinybee
cargo xtask check --board t-deck-pro
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
git diff --check
```

Results: 35 portable unit tests passed, plus all documentation tests and strict
Clippy. All eight imported driver libraries, both imported binaries, and the
changed EPD/LoRa examples compiled for ESP32-S3. Both Alumina firmware images
passed strict ESP Clippy and linked in release mode. Their ELF section summaries
were 10,257 bytes for TinyBee and 10,312 bytes for T-Deck Pro before Wi-Fi,
storage, web, and physical drivers are instantiated.

Negative checks also passed: a direct firmware build with no board feature is
rejected, a board/target mismatch is rejected, planned boards have no firmware
composition root, and selecting both board features activates mutually exclusive
ESP HAL chip families and cannot compile.

## Claim boundary and next hardware gate

No board was flashed. There is no claim of executor affinity observed on a
physical MCU, safe reset behavior, shifted-bit polarity, T-Deck peripheral smoke
behavior, Wi-Fi/service saturation timing, interrupt affinity, watchdog action,
or queue behavior under physical contention. M2 remains open until the following
artifacts exist:

1. exact board-revision reconciliation and licensed annotated photos/hotspots;
2. a fail-safe TinyBee I2S startup/fault driver, followed by logic-analyzer traces
   for every routed shifted bit and the all-safe reset image;
3. T-Deck battery, EPD, GPS, touch, keyboard, LoRa, SD, and shared-bus smoke logs;
4. a core-0 saturation test with archived core-1 deadline distributions and a
   declared initial timing envelope; and
5. reset, watchdog, urgent-mailbox, malformed-command, limit, and E-stop HIL
   traces with harmless/disconnected loads.
