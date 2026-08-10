# Research sources

Research captured on 2026-08-10. Upstream dependencies and hardware repositories
must be rechecked and pinned by commit before implementation; this document is a
planning evidence trail, not a floating dependency specification.

## Local repositories

### Exact geometry and rendering

- [`hyperreal/Cargo.toml`](../../hyperreal/Cargo.toml) and
  [`hyperreal/src`](../../hyperreal/src) — exact `Rational`/`Real` scalar model.
- [`hyperlattice/Cargo.toml`](../../hyperlattice/Cargo.toml) and
  [`hyperlattice/src`](../../hyperlattice/src) — exact linear algebra and geometry.
- [`hyperlimit/Cargo.toml`](../../hyperlimit/Cargo.toml) and
  [`hyperlimit/src`](../../hyperlimit/src) — exact/refined limits and predicates.
- [`hypertri/Cargo.toml`](../../hypertri/Cargo.toml) and
  [`hypertri/src`](../../hypertri/src) — exact triangulation.
- [`hypermesh/Cargo.toml`](../../hypermesh/Cargo.toml) and
  [`hypermesh/src`](../../hypermesh/src) — native exact triangle mesh and checked
  graphics buffers.
- [`hypercurve/Cargo.toml`](../../hypercurve/Cargo.toml) and
  [`hypercurve/src`](../../hypercurve/src) — exact curve/path/region operations
  and finite projection with explicit chord-error control.
- [`csgrs/Cargo.toml`](../../csgrs/Cargo.toml) and
  [`csgrs/src`](../../csgrs/src) — current `TriangleMesh`, curve-region, solid,
  transform, extrusion/revolution/sweep, and graphics adapter APIs.
- [`hypergraphics/Cargo.toml`](../../hypergraphics/Cargo.toml) and
  [`hypergraphics/src`](../../hypergraphics/src) — exact scene/camera/projection
  boundary and GPU backend.

### Alumina and T-Deck applications

- [`alumina-interface/Cargo.toml`](../../alumina-interface/Cargo.toml),
  [`src/lib.rs`](../../alumina-interface/src/lib.rs), and
  [`src/design_graph.rs`](../../alumina-interface/src/design_graph.rs) — current
  old CSGRS API, partial Hypergraphics backend, hand-built render data, CAD graph,
  firmware controls, and plots.
- [`alumina-firmware/Cargo.toml`](../../alumina-firmware/Cargo.toml),
  [`src/main.rs`](../../alumina-firmware/src/main.rs),
  [`src/wifi.rs`](../../alumina-firmware/src/wifi.rs),
  [`src/planner.rs`](../../alumina-firmware/src/planner.rs), and
  [`src/devices`](../../alumina-firmware/src/devices) — ESP-IDF runtime, embedded
  web assets/routes, device constants, command queue, and current simple planner.
- [`t-deck-async-drivers-rs/Cargo.toml`](../../t-deck-async-drivers-rs/Cargo.toml),
  [`patina`](../../t-deck-async-drivers-rs/patina), and all local driver crates —
  Embassy/esp-hal baseline, device-task integration, bus ownership, and driver
  inventory.
- [`t-deck-async-drivers-rs/LICENSE`](../../t-deck-async-drivers-rs/LICENSE) —
  Apache-2.0 source-import obligation.

## MKS TinyBee and FluidNC

- [MKS TinyBee vendor repository](https://github.com/makerbase-mks/MKS-TinyBee) —
  board purpose, ESP32-WROOM-32U, memory, 12–24 V, motor/heater/fan/endstop/SD/Wi-Fi
  feature summary, schematics, and vendor pin definitions.
- [FluidNC](https://github.com/bdring/FluidNC) — current ESP32 CNC firmware,
  runtime machine configuration, hardware abstraction, WebUI, and upstream source.
- [Requested FluidNC existing-hardware page](http://wiki.fluidnc.com/en/hardware/existing_hardware)
  — the route was unavailable from the research environment during this audit, so
  the current GitHub repositories below were used for the actionable hardware and
  schema evidence.
- [FluidNC configuration collection](https://github.com/bdring/fluidnc-config-files)
  — current official and contributed board/machine profiles.
- [Official MKS TinyBee FluidNC profile](https://github.com/bdring/fluidnc-config-files/blob/main/official/MKS_TinyBee_1_XYZAB.yaml)
  — `I2S_STATIC`, I²S pins 25/27/26, `I2SO.0`–`.23` mapping, axes, limits, SPI/SD,
  thermistors, heaters, fans, and expansion notes.
- [FluidNC configuration specification](https://github.com/bdring/FluidNC/blob/main/tools/fluidnc-config-spec.md)
  and [JSON schema](https://github.com/bdring/FluidNC/blob/main/tools/fluidnc-config-schema.json)
  — current machine fields, GPIO/I²S/UART pin namespaces, buses, axes, and timing
  engine semantics used to scope an importer rather than clone the schema.
- [FluidNC issue 1295](https://github.com/bdring/FluidNC/issues/1295) — field report
  illustrating WebUI/flash activity and real-time behavior; treated as a risk
  signal, not the primary architectural proof.

## Embassy and ESP Rust runtime

- [Espressif Rust: asynchronous programming](https://docs.espressif.com/projects/rust/book/application-development/async.html)
  — Embassy integration model for ESP targets.
- [`esp-hal-embassy::Executor`](https://docs.espressif.com/projects/rust/esp-hal-embassy/0.8.0/esp32/esp_hal_embassy/struct.Executor.html)
  — multicore-safe executor, per-core execution, and no task stealing.
- [`esp-rtos` ESP32 documentation](https://docs.espressif.com/projects/rust/esp-rtos/0.3.0/esp32/esp_rtos/index.html)
  — runtime/second-core and Embassy support; final implementation pins a mutually
  compatible version set rather than assuming these documentation versions.
- [Embassy repository](https://github.com/embassy-rs/embassy) and
  [`embassy-net` documentation](https://docs.embassy.dev/embassy-net/git/default/index.html)
  — no_std async networking and TCP/IP stack.
- [Picoserve](https://github.com/sammhicks/picoserve) — async `no_std`, no-heap
  HTTP server with JSON, SSE, and WebSocket support; its own README notes pre-1.0
  changes and limited stress testing, hence the explicit review gate.
- [`esp-hal` ESP32 peripheral documentation](https://docs.espressif.com/projects/rust/esp-hal/1.1.0/esp32/esp_hal/index.html)
  — current HAL modules and chip-specific availability for GPIO, DMA, I²C, I²S,
  LEDC, MCPWM, PCNT, RMT, SPI, timers, TWAI, UART, and related resources.
- [`esp-hal` repository](https://github.com/esp-rs/esp-hal) and
  [complete ESP32 API index](https://docs.espressif.com/projects/rust/esp-hal/1.1.0/esp32/esp_hal/all.html)
  — supported chip families, current peripheral surface, stability caveat, and
  source for a generated rather than handwritten capability matrix.
- [Rust on ESP ancillary crates](https://docs.espressif.com/projects/rust/book/introduction/ancillary-crates.html)
  — `esp-radio` coverage of Wi-Fi, BLE, ESP-NOW, and low-level IEEE 802.15.4 plus
  its scheduler/background-runtime requirements and per-driver maturity caveat.

## ESP32 timing and memory

- [Espressif SPI flash concurrency constraints](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/peripherals/spi_flash/spi_flash_concurrency.html)
  — cache effects and cross-core constraints during flash operations.
- [Espressif interrupt allocation and IRAM-safe handlers](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/intr_alloc.html)
  — critical code and transitively accessed data must reside in internal IRAM/DRAM
  to remain serviceable while flash cache is unavailable.

These IDF documents describe the underlying ESP32 hardware/cache behavior even
though aluminafw will not use ESP-IDF services.

## Motion and motor-control references

- [Synthetos](https://synthetos.com/) and
  [g2core](https://github.com/synthetos/g2) — 9-axis motion, third-order
  jerk-controlled planning, junction handling, JSON interface, linearly changing
  velocity in nominally sub-millisecond execution segments, and board separation.
- [g2core license](https://github.com/synthetos/g2/blob/edge-preview/LICENSE) —
  GPLv2 with a BeRTOS-style exception for specifically marked components. The
  plan uses published behavior/math as a reference and calls for an independent
  implementation unless a later legal/provenance decision explicitly selects
  identified component files.
- [SimpleFOC documentation](https://docs.simplefoc.com/) and
  [Arduino-FOC source](https://github.com/simplefoc/Arduino-FOC) — modular BLDC
  and stepper FOC across motors, sensors, drivers, and MCUs.
- [SimpleFOC torque/FOC loop](https://docs.simplefoc.com/torque_control) — voltage,
  estimated-current, DC-current, and dq/FOC-current modes and high-rate inner-loop
  role.
- [SimpleFOC position loop](https://docs.simplefoc.com/angle_loop) — cascaded
  position/velocity/torque and advanced direct position/torque structures.
- [SimpleFOC MIT license](https://github.com/simplefoc/Arduino-FOC/blob/master/LICENSE)
  — relevant if implementation code, rather than concepts/tests, is reused.

## Lightweight MCU control reference

- [Klipper code overview](https://www.klipper3d.org/Code_Overview.html) — host/MCU
  division and scheduled microcontroller work.
- [Klipper MCU commands](https://www.klipper3d.org/MCU_Commands.html) — startup
  allocation/configuration/finalization, object IDs, scheduled digital/PWM/step
  operations, clock use, and output safety durations.
- [Klipper protocol](https://www.klipper3d.org/Protocol.html) — compact command and
  response dictionary, sequencing, CRC, scheduling examples, and bounded MCU
  processing goals.
- [Klipper GPLv3 license](https://github.com/Klipper3d/klipper/blob/master/COPYING)
  — the Alumina protocol/resource model borrows architectural ideas, not source.

## Graphical dataflow reference

- [NI LabVIEW block-diagram data flow](https://www.ni.com/docs/en-GB/bundle/labview/page/block-diagram-data-flow.html)
  — nodes execute from data availability and independent paths may run in
  parallel.
- [NI LabVIEW wires](https://www.ni.com/docs/en-AS/bundle/labview/page/using-wires-to-link-block-diagram-objects.html)
  — typed wires, required terminals, fan-out, and visible type mismatch.
- [NI LabVIEW channel wires](https://www.ni.com/en/support/documentation/supplemental/16/channel-wires.html)
  — explicit asynchronous producer/consumer channels, stream semantics, stop and
  timeout metadata, including real-time use.
- [NI LabVIEW loops and structures](https://www.ni.com/docs/en-US/bundle/labview/page/loops-and-other-structures.html)
  — For, While, Timed Loop, case, and sequence structures used to define the
  graph-editor roadmap without attempting UI or file-format compatibility.
