# aluminafw

`aluminafw` is the greenfield Embassy firmware platform planned for Alumina
machines, instruments, controllers, and embedded operator interfaces. It combines
the async driver structure of `t-deck-async-drivers-rs` with the useful embedded
web-serving behavior demonstrated by `alumina-firmware`, while putting all
network and UI work on one ESP32 core and all real-time work on the other.

Implementation is underway from the researched delivery plan. The first
portable crates define exact protocol identities, integer machine-job
validation, board-resource ownership, and a fail-closed safety state machine.
There are no deployed clients and no compatibility requirement: the old Alumina
firmware and interface are functional references, not APIs to preserve.

## Developer checks

The repository pins Rust 1.88. Run the portable checks from its root:

```console
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee-v1
cargo xtask board check t-deck-pro
```

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
