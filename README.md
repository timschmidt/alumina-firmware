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

The first M3 foundation adds an explicit little-endian native protocol, bounded
storage operation bodies, SHA-256 content-addressed sequential uploads, atomic
publication checkpoints, and a deterministic reboot/cache/prefetch simulator.
The durable coordinator remains encapsulated by a core-0-owned backend; core 1
has no media handle. See the
[protocol/storage simulation evidence](docs/evidence/M3-PROTOCOL-STORAGE-SIM.md).

The first live-network foundation now initializes `esp-radio` on core 0 before
the real-time core starts, runs a protected WPA2 device AP, a fixed four-lease
DHCP service, and a two-connection bounded HTTP bootstrap on that same service
executor. Only exact greenfield read-only routes are admitted. A repository
development password exists for bench discovery, is reported as non-production,
and can never make an image production-armable. See the
[Wi-Fi/web compile evidence](docs/evidence/M3-WIFI-WEB-FOUNDATION.md).

Authenticated storage admission is also wired end to end: the public auth route
returns a fresh 128-bit boot challenge; browser/WASM requests and device
responses carry exact HMAC-SHA-256 proofs bound to a nonzero counter, method,
path, status/media, and SHA-256 body identity. Strict raw-header policy rejects
duplicates, transfer coding, noncanonical lengths, wrong media, replays, and a
global valid-request flood before the core-0 service owner decodes a native
frame. That admission milestone deliberately used an unavailable backend, so it
could not acknowledge volatile bytes as durable. See the historical
[authenticated-service evidence](docs/evidence/M3-AUTHENTICATED-SERVICE.md).

The cache backend itself is now concrete: `alumina-storage` implements a bounded
asynchronous 512-byte block-device contract, an explicitly provisioned raw SD
region, alternating SHA-256 anchors, and a hash-chained append-only record log.
Begin, chunk, abort, and publication are acknowledged only after ordered sync
barriers; reboot replay reconstructs exact upload state without trusting a
filesystem. The authenticated service dispatch is generic over that backend,
and `alumina-sim` exposes snapshots, torn writes, power cuts, and corruption for
the same implementation. That milestone kept board firmware explicitly
`unavailable` until a physical SD-SPI adapter could exist. See the historical
[durable-cache evidence](docs/evidence/M3-DURABLE-CACHE-MEDIA.md).

The first physical transport is now composed on both boards. A clean-room,
allocation-free SD SPI driver enters identification at 400 kHz, negotiates a
modern SD V2 card, enables command/data CRC, derives capacity from CSD, and
provides bounded single-block read/write/sync at 10 MHz. TinyBee owns SPI2 on
GPIO 18/23/19 with CS 5; T-Deck Pro owns GPIO 36/33/47 with CS 48 and keeps the
shared EPD/LoRa devices inactive. Boot identifies a card on core 0 and reports
`detached`, or `faulted` on failure. See the historical
[SD SPI transport evidence](docs/evidence/M3-SD-SPI-TRANSPORT.md).

Cache placement is now explicit and persistent. Boot reads only fixed raw blocks
2046–2047; foreign bytes mean unprovisioned, recognizable damaged Alumina bytes
mean faulted, and a valid hashed locator selects one exact region whose anchor
identity and complete log must replay. `StorageProvision` binds destructive
formatting to the authenticated request's observed card size, old generation and
media ID, exact new interval, fresh ID, and recovery intent. A 1 MiB front guard
keeps the locators outside conventional primary partition metadata and their
fixed placement avoids GPT's card-tail backup sectors. Core 1 now establishes a
board-specific hazardous-output contract before Wi-Fi: T-Deck Pro retains GPIO2
as high impedance, while TinyBee first latches a complete 24-bit all-safe
shift-register image and retains GPIO2 plus its four digital safety-input routes
as high impedance. Core 0 admits
storage mutation only while 100 ms safety publications remain identity-matched
and no more than 500 ms old. This makes explicit provisioning reachable in the
compiled images without making a bench, armability, or implicit-format claim.
See the [safe-boot evidence](docs/evidence/M3-SAFE-BOOT-OBSERVATION.md) and
[cache-provisioning evidence](docs/evidence/M3-CACHE-PROVISIONING.md).

Published cache objects can now be reopened without a heap-backed directory.
The core-0 reader linearly revalidates the committed log, binds an opaque cursor
to one exact typed object and manifest, verifies every sequential chunk before
copying it into fixed caller memory, and withholds the last chunk until the
aggregate object, manifest, and publication record all agree. Lookup misses are
benign; media divergence or post-open corruption latches an integrity fault.
This is storage readback, not executable job admission; the later machine-block
and job-prefetch layers add those independent gates. See the
[published-object reader evidence](docs/evidence/M3-PUBLISHED-OBJECT-READER.md).

The next boundary is also concrete: `alumina-machine-ir` now defines canonical
512-byte motion blocks for up to eight axes, with exact relative-tick intervals,
integer lattice displacement, repeated stream/capability/configuration
identities, zero padding, SHA-256 block identity, and a previous-block digest
chain. An incremental assembler handles arbitrary storage chunk splits. Core 0
and core 1 use separate stateful validators, and a dedicated inline work ring
transfers non-cloneable owned blocks under exact credits. See the
[machine-block boundary evidence](docs/evidence/M3-MACHINE-BLOCK-BOUNDARY.md).

`alumina-job` now owns the portable prepare/prefetch/admission lifecycle. Core 0
opens one exact typed publication, reads at most one verified SD chunk per
bounded step, retains ownership across a full work ring, and never marks a
partition complete until every block has transferred. Core 1 independently
validates one owned block at a time and advances completion only through an
exact acknowledgement token. Cached `StreamTick` values remain distinct from
absolute `DeviceCycle` values until a future deterministic commit installs an
epoch. The simulator drives these actors through a provisioned cache with
700-byte chunks, forces backpressure, and proves identical terminal stream
facts. Firmware now compiles the crate for both boards; authenticated job
control now routes canonical prepare/cancel/status frames through core 0, drives
bounded verified prefetch, installs the independent core-1 validator, and
publishes correlated status. Core 1 retains the first admitted block without
acknowledging or executing it. `alumina-capability` now encodes every typed board
fact as an allocation-free canonical `ALMCAP01` document. Both first packages
compile its independently recomputed SHA-256, firmware verifies it before Wi-Fi,
identity advertises it, and authenticated `CapabilitiesGet` returns bounded
digest-stable ranges. Preparation still returns `Unsupported` on both current
images because neither board package is armable; configuration commit and
hardware qualification remain separate closed gates. See the
[portable lifecycle evidence](docs/evidence/M3-JOB-PREFETCH-LIFECYCLE.md) and
[firmware wiring evidence](docs/evidence/M3-FIRMWARE-JOB-PREFETCH.md), plus the
[capability format](docs/CAPABILITIES.md) and
[canonical-capability evidence](docs/evidence/M3-CANONICAL-CAPABILITIES.md).

`alumina-config` defines canonical content-addressed resource bindings and
reduced exact machine facts, validates them against the immutable board package,
and streams a real published SD object through independent validators on both
cores. Authenticated firmware operations now order durable prepare, unauthorized
core-1 activation, durable commit, and exact authorization; boot recovery
revalidates committed bytes and discards orphan prepares. Only the resulting
durably authorized digest reaches either job actor. See the
[configuration format](docs/CONFIGURATION.md),
[portable configuration evidence](docs/evidence/M4-CONFIGURATION-IR.md), and
[firmware lifecycle evidence](docs/evidence/M4-FIRMWARE-CONFIGURATION-LIFECYCLE.md).

The first portable M6 execution slice now binds cached ownership to an exact
integer step-event trace: whole blocks are preflighted without work proportional
to step count, retained until terminal tick/position correlation, and never
acknowledged after a fault. Configuration-derived safety inputs have stable
core-1 slots with polarity, pull, exact assert/release debounce, finite sampling
watchdogs, arming masks, and typed E-stop/interlock/limit/driver/probe reactions.
TinyBee now realizes GPIO 33/32/22/35 as a transactional nominal one-millisecond
core-1 polling bank; every fault first reapplies the full board-safe image and
invalidates admitted job ownership. The exact input masks and next watchdog
deadline cross to core 0 in the canonical safety snapshot. Core 1 also binds
the exact scheduled epoch and descriptor starting lattice position to cached
motion, and releases a block only after every complete output image has a
target-confirmed physical commit. TinyBee's blocking writer is an unqualified
compile/HIL staging path; T-Deck Pro has no motion backend. This is target wiring,
not measured GPIO, electrical, response-time, or motion-output
qualification, so both packages remain non-armable. See the
[stepper evidence](docs/evidence/M6-EXACT-STEPPER-CORE.md) and
[portable safety-input evidence](docs/evidence/M6-SAFETY-INPUT-CORE.md), plus the
[target safety-input evidence](docs/evidence/M6-TARGET-SAFETY-INPUTS.md) and
[target motion-commit evidence](docs/evidence/M6-TARGET-MOTION-COMMIT.md). The
portable scheduled backend further separates future generation, immutable
timeline acceptance, and physical latch observation on an exact output lattice;
its motion-to-PCM-to-wire simulator retains ownership through output-free dwell
and terminal disable. See the
[scheduled-output evidence](docs/evidence/M6-SCHEDULED-OUTPUT-HORIZON.md). The
firmware target has not adopted that backend yet, so armability remains closed.

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
[expanded board-metadata evidence](docs/evidence/M2-BOARD-METADATA.md). The
network foundation can be built with `ALUMINA_AP_PASSWORD` supplied outside the
repository, but build-provisioned credentials still do not satisfy the planned
unique device-stored production credential gate.

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
  retain their original notices and provenance. Permissive MIT/Apache-compatible
  dependencies may be accepted after normal review; GPL-family implementation
  dependencies are excluded.
- Only dual-core ESP32 targets are in scope. MKS TinyBee and LILYGO T-Deck Pro
  are the first hardware targets; MKS ESP32 FOC V1.0 follows for servo control.
  The current T-LoRa Pager receives a late board stub before full support.
- Core 0 owns Wi-Fi, the web server, SD/cache service, T-Deck peripherals,
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
- [Native protocol](docs/PROTOCOL.md) — exact frame/message bytes, operation
  families, storage bodies, and hard version behavior.
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
