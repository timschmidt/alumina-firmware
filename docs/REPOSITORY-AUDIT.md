# Local repository audit

Audit date: 2026-08-10. The review covered manifests, public APIs, application
structure, board definitions, render paths, network/HTTP routes, planner code,
driver crates, and working-tree state. No requested source repository was
modified.

## Exact geometry stack

| Repository | Local version | Relevant capability | Aluminafw/interface implication |
| --- | ---: | --- | --- |
| [`hyperreal`](../../hyperreal) | 0.13.1 | Exact `Rational` and expression-valued `Real`, checked comparison/conversion, optional serialization | Canonical exact scalar at the CAD/CAM side; do not place arbitrary-precision evaluation in firmware ISRs |
| [`hyperlattice`](../../hyperlattice) | 0.6.1 | Exact points, vectors, matrices, transformations | Use for exact work/tool/machine frames before integer quantization |
| [`hyperlimit`](../../hyperlimit) | 0.4.1 | Exact/refined limits and predicates | Preserve predicate decisions in geometry and CAM; keep tolerance policy explicit |
| [`hypertri`](../../hypertri) | 0.4.1 | Exact triangulation/CDT and N-dimensional support | Use native exact output instead of legacy float triangulation paths |
| [`hypermesh`](../../hypermesh) | 0.1.0 | Exact triangle-mesh topology/booleans and checked graphics-buffer conversion | Native mesh type for current CSGRS and the Hypergraphics bridge |
| [`hypercurve`](../../hypercurve) | 0.3.1 | Exact lines, arcs, Beziers, NURBS, curve paths/regions, derivatives, parameter domains, finite projection with explicit chord error | Core of exact CAM and certified machine-resolution curve reduction |
| [`csgrs`](../../csgrs) | 0.23.0 | Native `TriangleMesh` and `CurveRegion2`, solid/curve adapters, exact bounds/transforms, graphics mesh adapters | Replace the interface’s removed `Mesh`/`Sketch` API and obsolete feature set |
| [`hypergraphics`](../../hypergraphics) | 0.1.0 | Exact scene/render boundary, `ExactMesh`, `ExactVertex`, `ExactCamera`, checked `Projection64`, GPU backend types | Own all visualization conversion; render floats remain a one-way boundary |

### Stack conclusion

The local stack is already arranged around exact native geometry and checked
conversion boundaries. The firmware should not depend directly on this `std` and
arbitrary-precision stack. `alumina-interface`/host CAM should compile exact
geometry into a versioned integer machine IR; firmware should validate and
execute that IR with bounded integer/fixed-point arithmetic.

Hypercurve supplies the necessary concept for finite curve projection: an
explicit chord-error bound. It does not by itself define machine calibration,
timer quantization, step-event proof, or a serialized job format. Those belong in
`alumina-machine-ir` and the interface CAM layer.

## `alumina-interface`

Reviewed files:

- [`Cargo.toml`](../../alumina-interface/Cargo.toml)
- [`src/lib.rs`](../../alumina-interface/src/lib.rs)
- [`src/design_graph.rs`](../../alumina-interface/src/design_graph.rs)
- [`README.md`](../../alumina-interface/README.md)

### Reusable pieces

- Existing eframe/egui/egui-wgpu browser/desktop application and its design,
  control, and diagnostics surfaces.
- Early Hypergraphics backend integration and checked `Projection64` boundary.
- A graph canvas, typed port concept, CAD node evaluator, device discovery panel,
  queue controls, and initial pin-history plot.
- Trunk build path and assets already suitable for embedding in firmware after
  reproducible packaging is added.

### Required migration

- The manifest still targets CSGRS 0.20.1 with obsolete features, while the
  workspace contains CSGRS 0.23.0 and current Hyper crates.
- Code imports removed `csgrs::mesh::Mesh`, `csgrs::sketch::Sketch`, and legacy
  CSG traits. Migrate to `TriangleMesh`, `CurveRegion2`/`CurvePath2`, current
  solid/curve APIs, and exact `Real` transformations.
- The application invokes the Hypergraphics backend but still hand-builds
  interleaved vertices, normals, edges, point spheres, grids, and camera matrices.
  Extend Hypergraphics adapters/scene primitives, then remove the interface-owned
  renderer and redundant `nalgebra`/`geo` conversion paths.
- The current graph value model is limited to `f64`, `Vec3<f64>`, text, legacy
  sketch, and legacy mesh values. It needs exact geometry values, unit-bearing
  numerics, records, arrays, streams, events, resource handles, state, and clock
  domains.
- Evaluation is recursive and essentially pure/acyclic. Graph persistence is
  explicitly incomplete. General control requires versioned serialization,
  cycles with explicit state/delay, bounded channels, deterministic scheduling,
  subgraphs, and migration.
- Device control is tied to polling `/pins`, reading `/device`, and posting text
  to `/queue`. Replace this with capability discovery plus binary scheduled
  command and telemetry streams while retaining a temporary compatibility
  adapter.
- The plot is a small historical pin plot. It needs timestamped ring buffers,
  decimation, triggers, cursors, units, multi-rate channels, XY/frequency views,
  capture/replay, and export.

The detailed sequence is in `INTERFACE-ROADMAP.md`.

## `alumina-firmware`

Reviewed files include:

- [`Cargo.toml`](../../alumina-firmware/Cargo.toml)
- [`src/main.rs`](../../alumina-firmware/src/main.rs)
- [`src/wifi.rs`](../../alumina-firmware/src/wifi.rs)
- [`src/commandbuffer.rs`](../../alumina-firmware/src/commandbuffer.rs)
- [`src/planner.rs`](../../alumina-firmware/src/planner.rs)
- [`src/devices`](../../alumina-firmware/src/devices)
- [`build.rs`](../../alumina-firmware/build.rs)

### Reusable behavior

- Builds and embeds interface HTML, gzipped JavaScript, Brotli-compressed WASM,
  favicon, and device imagery.
- Demonstrates an onboard Wi-Fi access point and embedded HTTP service.
- Supplies behavior to preserve during migration: `/device`, `/device/image`,
  `/time`, `/files`, `/queue`, `/pins`, static web assets, command intake, and
  simple device metadata.
- Contains seed board definitions for TinyBee, ESP32Drive, ESP32-CAM, and XProV5,
  plus local TinyBee schematic/reference documents.

### Why it should be replaced rather than incrementally converted

- It is an ESP-IDF `std` application using `esp-idf-hal`, `esp-idf-svc`, and
  `esp-idf-sys`; the target is Embassy/no_std with `esp-hal`, `esp-rtos`, and
  `esp-radio`.
- Board modules are largely constants/metadata. HTTP commands still manipulate
  fixed GPIO behavior rather than allocating typed resources from a selected
  board.
- The queue parser accepts text commands directly into application behavior. The
  real-time core must never parse these formats.
- The planner is a rudimentary `f32` trapezoidal model with fixed scale, no
  multi-block lookahead, no jerk constraints, and no machine-resolution proof.
- The stepper abstraction counts logical steps but does not supply a hardware
  pulse/timer/DMA engine.
- TinyBee virtual outputs are described as PCF8575-style I/O in comments/code,
  but the hardware/FluidNC profile uses an I²S shift-register output chain.
- The repository currently contains `private.pem`. Aluminafw ignores private-key
  extensions and requires build-time/per-device secret injection; it must never
  inherit a shared private signing key.

The old firmware remains a route/asset/board behavior reference through the M3
migration gate, then becomes a maintained legacy branch or is archived according
to the user’s compatibility decision.

## `t-deck-async-drivers-rs`

Reviewed:

- [`Cargo.toml`](../../t-deck-async-drivers-rs/Cargo.toml)
- [`embedded-bus-async`](../../t-deck-async-drivers-rs/embedded-bus-async)
- [`sx126x-async-rs`](../../t-deck-async-drivers-rs/sx126x-async-rs)
- every `t-deck-pro-*-async` crate
- [`patina`](../../t-deck-async-drivers-rs/patina)
- [`i2c-tester`](../../t-deck-async-drivers-rs/i2c-tester)

### Driver inventory and destination

| Source crate | Device/function | Proposed destination |
| --- | --- | --- |
| `embedded-bus-async` | Shared async I²C/SPI wrappers | `drivers/embedded-bus-async` |
| `sx126x-async-rs` | Generic asynchronous SX126x protocol | `drivers/sx126x-async-rs` |
| `t-deck-pro-battery-async` | BQ25896 charger/power management | `drivers/t-deck-pro-battery-async` |
| `t-deck-pro-epd-async` | UC8253/GDEQ031T10 E-paper | `drivers/t-deck-pro-epd-async` |
| `t-deck-pro-gps-async` | u-blox MIA-M10Q GPS | `drivers/t-deck-pro-gps-async` |
| `t-deck-pro-keyboard-async` | TCA8418 keyboard scanner | `drivers/t-deck-pro-keyboard-async` |
| `t-deck-pro-lora-async` | T-Deck SX1262 integration | `drivers/t-deck-pro-lora-async` |
| `t-deck-pro-touch-async` | CST328 touch controller | `drivers/t-deck-pro-touch-async` |
| `i2c-tester` | Hardware diagnostic utility | `tests/hil` or `examples/i2c-tester` |
| `patina` | Integrated Embassy application/reference | behavioral fixture and T-Deck example |

All source driver crates are Apache-2.0 in the inspected workspace. Import them
with their license/header history and an explicit revision/provenance record.
Do not silently relabel imported files under a project-wide dual license.

### Structural findings

- The workspace is already `no_std`, Embassy, `esp-hal`, `esp-rtos`, and
  `esp-radio` oriented, making it the correct baseline rather than the IDF
  firmware.
- Patina currently uses one Embassy executor. Its device-task and centralized
  model/display-coalescing pattern maps well to the service core.
- Shared bus handles use `Rc<RwLock<CriticalSectionRawMutex, _>>` and are
  intentionally local/non-`Send`. Keep the whole bus and its clients on one core.
- The SPI wrapper’s chip-select output is coupled to an ESP HAL type; generalize
  only that boundary when importing so the bus utility remains reusable.
- Display, LoRa, and SD share a physical SPI topology on T-Deck; battery,
  keyboard, and touch share I²C topology. The board package must declare this
  rather than allowing independent resource allocation.
- The request to copy “all drivers” covers the drivers actually present above.
  The local repository does not currently provide drivers for every fitted or
  possible T-Deck function such as audio, storage, IMU/light sensing, modem, or
  vibration; those are separate future capability items after hardware
  reconciliation.

## Cross-repository conclusions

1. Use `t-deck-async-drivers-rs`, not `alumina-firmware`, as the runtime/toolchain
   baseline.
2. Preserve `alumina-firmware` behavior through API/asset migration tests, not by
   retaining ESP-IDF service dependencies.
3. Update `alumina-interface` to the exact stack before defining machine IR, so
   old float/mesh types never become protocol commitments.
4. Add a narrow Hypergraphics adapter from current CSGRS/Hypermesh native types.
   This may live in Hypergraphics behind an optional feature or in a small
   interface adapter crate, but conversion policy must be owned once.
5. Keep exact geometry on host/WASM and define one checked exact-to-integer
   machine boundary. Do not round-trip through graphics buffers.
6. Model TinyBee output bits and other expanders as distinct typed resources.
7. Separate step/direction and FOC power hardware behind a common trajectory
   contract; TinyBee alone does not supply a SimpleFOC-style inverter/current
   sensing path.

## Workspace hygiene observed

The audit found pre-existing user work in `hypercurve` and untracked fuzz/output
artifacts in `hyperlimit`. They were left untouched. The new planning repository
does not depend on cleaning, staging, or rewriting any sibling repository.
