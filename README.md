# aluminafw

`aluminafw` is the greenfield Embassy firmware platform planned for Alumina
machines, instruments, controllers, and embedded operator interfaces. It combines
the async driver structure of `t-deck-async-drivers-rs` with the useful embedded
web-serving behavior demonstrated by `alumina-firmware`, while putting all
network and UI work on one ESP32 core and all real-time work on the other.

Implementation is underway from the researched delivery plan. The portable
crates now define exact protocol identities, integer machine-job validation,
board-resource ownership, a fail-closed safety state machine, and bounded
cross-core channels. TinyBee and T-Deck Pro have chip-specific composition roots
that consume the HAL peripheral singleton once, partition owned tokens, and run
one Embassy executor on each application core. Their exported packages now also
carry clocks, electrical constraints, interrupts, safe images, fitted-device
auxiliaries, licensed-photo overlays, and explicit HIL promotion gates. There are
no deployed clients and no compatibility requirement: the old Alumina firmware
and interface are functional references, not APIs to preserve.

## Developer checks

The repository pins Rust 1.88. Run the portable checks from its root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee-v1
cargo xtask board check t-deck-pro
cargo xtask capabilities --board mks-tinybee --json
```

The portable commands intentionally operate on the workspace's default members.
The imported ESP32-S3 crates require the Espressif Xtensa toolchain and an
explicit target:

```console
cargo +esp check --target xtensa-esp32s3-none-elf --locked --lib \
  -p embedded-bus-async -p sx126x_async \
  -p t-deck-pro-battery-async -p t-deck-pro-epd-async \
  -p t-deck-pro-gps-async -p t-deck-pro-keyboard-async \
  -p t-deck-pro-lora-async -p t-deck-pro-touch-async
cargo +esp check --target xtensa-esp32s3-none-elf --locked --bins \
  -p i2c-tester -p patina
cargo +esp check --target xtensa-esp32s3-none-elf --locked --examples \
  -p t-deck-pro-epd-async -p t-deck-pro-lora-async
```

The board-aware commands select exactly one chip family and discover the espup
Xtensa linker bundle when it is not already on `PATH`:

```console
cargo xtask check --board mks-tinybee
cargo xtask check --board t-deck-pro
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
```

Both current packages remain intentionally non-armable. “Compiles” means the
typed package and complete release image build for the declared chip; it is not
bench, safe-state, peripheral-smoke, or timing qualification. See the
[dual-core compile evidence](docs/evidence/M2-DUAL-CORE-RUNTIME.md) and the
[expanded board-metadata evidence](docs/evidence/M2-BOARD-METADATA.md).

## Imported T-Deck support

The complete audited T-Deck snapshot is registered as workspace members under
`drivers/` and `examples/`: shared async I²C/SPI buses, SX126x, battery/charger,
e-paper, GPS, keyboard, LoRa, touch, the I²C tester, and the Patina integration
firmware. Datasheets and upstream root metadata are retained under
`imports/t-deck-async-drivers-rs/upstream-root/`. See the
[machine-readable import record](imports/t-deck-async-drivers-rs.toml) and
[M1 evidence](docs/evidence/M1-TDECK-IMPORT.md).

## Committed direction

- New code is dual-licensed `MIT OR Apache-2.0`; copied Apache-2.0 T-Deck drivers
  retain their original notices and provenance. Either MIT or Apache-2.0
  dependencies may be accepted after normal review.
- Only dual-core ESP32 targets are in scope. MKS TinyBee and LILYGO T-Deck Pro
  are the first hardware targets; MKS ESP32 FOC V1.0 follows for servo control.
  The current T-LoRa Pager receives a late board stub before full support.
- Core 0 owns Wi-Fi, the web server, SD/file service, T-Deck peripherals,
  telemetry presentation, and idle work. Core 1 owns safety, motion, FOC,
  deterministic I/O, and hardware-timed queues.
- Every board starts as its own Wi-Fi AP and serves the matching UI. The UI can
  scan and join infrastructure Wi-Fi; a shared WLAN is the only planned
  first-generation transport for coordinating multiple MCUs.
- The browser/WASM application is the authoritative CAD/CAM and machine-job
  compiler. CSGRS and the Hyper stack, especially Hypercurve, Hyperpath, and
  Hypersolve, preserve exactness until a named conversion to the configured
  motor/count/timer lattice.
- Firmware is deliberately thin: board drivers, safety, protocols, clock sync,
  SD-backed immutable job caches, bounded queues, real-time interpolation, and
  motor control. It does not parse source geometry or raw G-code.
- Each MCU in a synchronized machine caches and validates its own command-stream
  partition, then starts from an agreed future local cycle count derived from
  the UI's clock model. No CAN, USB, or serial multi-MCU transport is planned.
- `alumina-interface` may be substantially redesigned. It will use
  Hypergraphics rather than its hand renderer and grow into an exact CAM,
  LabVIEW-style graph, board-aware logic analyzer, oscilloscope, and debugger.

## Planning set

- [Delivery plan](docs/PLAN.md) — milestones, dependencies, gates, risks, and
  the first end-to-end workflow.
- [Decisions](docs/DECISIONS.md) — resolved product, licensing, board, safety,
  network, and compatibility policy.
- [Architecture](docs/ARCHITECTURE.md) — dual-core runtime, resources, protocol,
  safety, motion, FOC, and exact-to-integer execution boundary.
- [Hyper integration](docs/HYPER-INTEGRATION.md) — precise roles for Hyperpath,
  Hypersolve, CSGRS, Hypergraphics, and optional Hyper crates.
- [Distributed jobs and storage](docs/DISTRIBUTED-JOBS.md) — Wi-Fi clock models,
  per-MCU SD caches, prepare/commit start, and failure semantics.
- [Repository audit](docs/REPOSITORY-AUDIT.md) — findings from all requested
  local repositories and the reusable T-Deck driver inventory.
- [Boards and hardware](docs/BOARD-MATRIX.md) — TinyBee, T-Deck Pro, MKS ESP32
  FOC V1.0, T-LoRa Pager, and the board-package/capability contract.
- [Peripheral and protocol coverage](docs/PERIPHERAL-COVERAGE.md) — generated
  resource coverage and staged support for wider ESP32 hardware.
- [Interface roadmap](docs/INTERFACE-ROADMAP.md) — authoritative WASM CAM,
  Hypergraphics, graph programming, annotated-board diagnostics, and plotting.
- [Verification strategy](docs/VERIFICATION.md) — simulation, exactness tests,
  HIL timing, synchronized-job, safety, and release evidence.
- [Research sources](docs/SOURCES.md) — local and upstream evidence captured on
  2026-08-10.

## First end-to-end workflow

The first demonstrator is an exact Hypercurve/Hyperpath 2D contour compiled by
the browser into a certified jerk-limited XYZ command stream, replayed first in
`alumina-sim`, uploaded as an immutable job to TinyBee SD, and executed as a
three-axis pen/air-cut trace. The UI overlays live resource state on an annotated
photo of the actual TinyBee and displays logic-analyzer-style step, direction,
limit, queue, and clock traces. A captured hardware trace must agree with the
simulator and the job's integer event record before powered cutting, laser,
heater, or plasma loads enter scope.
