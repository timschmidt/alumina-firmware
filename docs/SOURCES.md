# Research sources

Research captured on 2026-08-10 and extended through 2026-08-13. Upstream
dependencies and hardware repositories must be rechecked and pinned by commit
before implementation; this document is a planning evidence trail, not a
floating dependency specification.

## Local repositories

### Exact geometry and rendering

- [`hyperreal/Cargo.toml`](../../hyperreal/Cargo.toml),
  [`hyperreal/src`](../../hyperreal/src), and
  [`hyperreal/src/serde.rs`](../../hyperreal/src/serde.rs) — exact
  `Rational`/`Real` scalar model and structural serde representation whose
  transient caches/signals are excluded from canonical planner evidence.
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
- [`hyperpath/Cargo.toml`](../../hyperpath/Cargo.toml) and
  [`hyperpath/src`](../../hyperpath/src) — exact-aware path/toolpath carriers,
  retained provenance, PH curves, path-wide length/feed reports, exact
  squared-speed forward/reverse lookahead, independent constraint replay, and
  jerk-ramp schedules including the exact monotonic nonzero-boundary proposer
  and bounded component-local refinement to jerk-feasible node feeds, plus
  exact affine velocity/acceleration/jerk projection across arbitrary dense
  axes with independent Hypersolve row and bottleneck replay.
- [`hypersolve/Cargo.toml`](../../hypersolve/Cargo.toml) and
  [`hypersolve/src`](../../hypersolve/src) — symbolic constraints, exact direct
  solving, numerical proposal boundaries, exact replay, and interval/Krawczyk
  certification.
- [`csgrs/Cargo.toml`](../../csgrs/Cargo.toml) and
  [`csgrs/src`](../../csgrs/src) — current `TriangleMesh`, curve-region, solid,
  transform, extrusion/revolution/sweep, and graphics adapter APIs.
- [`hypergraphics/Cargo.toml`](../../hypergraphics/Cargo.toml) and
  [`hypergraphics/src`](../../hypergraphics/src) — exact scene/camera/projection
  boundary and GPU backend.
- [`hyperbrep`](../../hyperbrep), [`hyperphysics`](../../hyperphysics),
  [`hypersdf`](../../hypersdf), [`hypervoxel`](../../hypervoxel),
  [`hyperpack`](../../hyperpack), [`hyperparts`](../../hyperparts),
  [`hyperevolution`](../../hyperevolution), [`hypercircuit`](../../hypercircuit),
  and [`hyperdrc`](../../hyperdrc) — audited optional exact B-rep, simulation,
  implicit/voxel process, nesting, knowledge, optimization, circuit, and PCB DRC
  crates; planned only for concrete later workflows.

### Alumina and T-Deck applications

- [`alumina-interface/Cargo.toml`](../../alumina-interface/Cargo.toml),
  [`motion_schedule.rs`](../../alumina-interface/crates/alumina-interface-core/src/motion_schedule.rs),
  [`schedule_evidence.rs`](../../alumina-interface/crates/alumina-interface-core/src/schedule_evidence.rs),
  and [`machine_cam_ui.rs`](../../alumina-interface/src/machine_cam_ui.rs) —
  greenfield exact browser compiler, Hypergraphics presentation, canonical
  machine/cache evidence, exact output-quantum lowering, visible retained
  timer-factor policy, and `ALMEVD03` exact planner/lowering transcript replay.
- [`alumina-motion`](../crates/alumina-motion/src/lib.rs) — allocation-free
  production stepper validator and the fail-closed duration-pressure
  classification used only to decide whether an exact candidate may be rebuilt
  and completely replayed.
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
- [MKS TinyBee V1.0_003 schematic](https://github.com/makerbase-mks/MKS-TinyBee/blob/main/hardware/MKS%20TinyBee%20V1.0_003/MKS%20TinyBee%20V1.0_003%20SCH.pdf)
  — revision-labeled U1 74HC595 SER/SRCLK/RCLK/GND probe identities and the
  GPIO4-to-EXP1 LCD_RS buffer route used by the disconnected-load capture
  procedure. Only this hardware artifact was used for those physical facts;
  vendor firmware source is neither an implementation input nor imported.

FluidNC material is used only to research physical boards and timing/resource
ideas. The resolved plan does not import its YAML, protocol, WebUI, or GRBL
compatibility.

## MKS ESP32 FOC V1.0 and T-LoRa Pager

- [MKS ESP32 FOC V1.0 vendor branch](https://github.com/makerbase-motor/MKS-ESP32FOC/tree/MKS-ESP32-FOC-V1.0)
  — selected dual-motor FOC board and revision-specific repository.
- [V1.0 hardware directory](https://github.com/makerbase-motor/MKS-ESP32FOC/tree/MKS-ESP32-FOC-V1.0/Hardware)
  and [V1.0 user-manual directory](https://github.com/makerbase-motor/MKS-ESP32FOC/tree/MKS-ESP32-FOC-V1.0/User%20Manual)
  — schematic/PCB/manual evidence that must be pinned and reconciled with the
  received board.
- [ESP32-WROOM-32D/32U datasheet](https://www.espressif.com/sites/default/files/documentation/esp32-wroom-32d_esp32-wroom-32u_datasheet_en.pdf)
  — official module identity, memory, electrical, and strapping reference. The
  schematic identifies the normal ESP32-WROOM-32D module, whose standard flash
  capacity is 4 MiB; the received module must still be read and queried.
- [EG2133 V1.0 datasheet](https://www.egmicro.com/static/doc/%E5%8A%9F%E7%8E%87%E9%A9%B1%E5%8A%A8%E8%8A%AF%E7%89%87/%E5%A4%9A%E7%9B%B8%E5%8D%8A%E6%A1%A5/EG2133%E4%B8%AD%E5%8E%8B300V1.2A%E4%B8%89%E7%9B%B8%E5%8D%8A%E6%A1%A5%E9%A9%B1%E5%8A%A8%E8%8A%AF%E7%89%87%E6%95%B0%E6%8D%AE%E6%89%8BV1.0.pdf)
  — official gate-driver input/output truth table and input-bias reference. On
  this board each phase net drives an active-high HIN and active-low LIN-bar
  together: driven low selects the low-side MOSFET, driven high selects the
  high-side MOSFET, and only high impedance permits the internal opposing input
  biases to select both-off.
- [ams OSRAM AS5600 datasheet, v1-06](https://look.ams-osram.com/m/7059eac7531a86fd/original/AS5600-DS000365.pdf)
  — official read-only sensor-transport reference. It establishes fixed
  seven-bit address `0x36`, STATUS at `0x0B`, unscaled 12-bit RAW ANGLE at
  `0x0C`/`0x0D`, random/sequential reads, the special raw-angle address-pointer
  behavior, and I²C support through 1 MHz Fast-mode Plus. The MKS package keeps
  its more conservative 400 kHz connector limit. The independently authored
  driver exposes no configuration, range/zero, OTP, or burn operation.

Only the V1.0 hardware artifacts, manual, and official component datasheets
were consulted for the compile-only board target. Vendor test-code and
third-party FOC implementation sources were not inspected or used. In
particular, the schematic marks GPIO22 and GPIO12 unconnected, establishes no
independent inverter enable, and establishes no fitted SD/cache medium; example
code cannot override those revision-specific hardware facts.
- [LILYGO LilyGoLib](https://github.com/Xinyuan-LilyGO/LilyGoLib) and
  [current T-LoRa Pager hardware page](https://github.com/Xinyuan-LilyGO/LilyGoLib/blob/master/docs/hardware/lilygo-t-lora-pager.md)
  — current ESP32-S3/flash/PSRAM/device/pin source for the late board stub and
  later full inventory.

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
- [`edge-net`](https://github.com/ivmarkov/edge-net) — selected bounded `no_std`
  HTTP/DHCP and network-abstraction implementation. The M3 foundation pins
  `edge-dhcp` 0.6.0, `edge-http` 0.6.1, `edge-nal` 0.5.0, and
  `edge-nal-embassy` 0.6.0 because that adapter line targets `embassy-net` 0.7.
- [`esp-radio` 0.17 Wi-Fi documentation](https://docs.espressif.com/projects/rust/esp-radio/0.17.0/esp32/esp_radio/wifi/index.html)
  — allocator/scheduler requirements, radio initialization, controller/device
  ownership, and async AP/STA interfaces used by the first firmware adapter.
- [WHATWG Fetch Living Standard](https://fetch.spec.whatwg.org/) — normative
  origin serialization, CORS-preflight method/header checks, exposed response
  headers, credentials mode, redirects, and `Vary` behavior used by the browser
  transport boundary.
- [Chrome Private Network Access preflight guidance](https://developer.chrome.com/blog/private-network-access-preflight)
  — legacy browser private-network preflight request/response fields. Alumina
  validates the exact `true` opt-in when an older/experimental client presents
  it, but does not depend on that superseded rollout.
- [Chrome Local Network Access permission guidance](https://developer.chrome.com/blog/local-network-access)
  and [Chrome 142 release notes](https://developer.chrome.com/release-notes/142)
  — current permission-gated LAN fetch behavior, secure-context restriction,
  mixed-content relaxation, and `targetAddressSpace: "local"`. LNA replaced the
  paused PNA enforcement effort; ordinary cross-origin CORS remains independent.
- [`esp-hal` ESP32 peripheral documentation](https://docs.espressif.com/projects/rust/esp-hal/1.1.0/esp32/esp_hal/index.html)
  — current HAL modules and chip-specific availability for GPIO, DMA, I²C, I²S,
  LEDC, MCPWM, PCNT, RMT, SPI, timers, TWAI, UART, and related resources.
- [`esp-hal` repository](https://github.com/esp-rs/esp-hal) and
  [complete ESP32 API index](https://docs.espressif.com/projects/rust/esp-hal/1.1.0/esp32/esp_hal/all.html)
  — supported chip families, current peripheral surface, stability caveat, and
  source for a generated rather than handwritten capability matrix.
- [Espressif ESP32 I²S programming guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/peripherals/i2s.html)
  — primary description of two-slot standard mode, 8/16/24/32-bit slot widths,
  the one-BCLK PCM-short frame-sync pulse, BCLK/WS clock relationships, and DMA
  frame boundaries. Alumina uses these as functional facts, not ESP-IDF code.
- [ESP32 Technical Reference Manual](https://www.espressif.com/sites/default/files/documentation/esp32_technical_reference_manual_en.pdf),
  chapter 22 — primary register/FIFO/DMA and original-ESP32 I²S behavior
  reference. The implementation is pinned to `esp-hal` 1.0.0; its locally
  cached crate source is reviewed for the exact Rust API and chip workaround in
  use, while signal order and startup behavior remain HIL-gated.
- [`esp-hal` 1.0.0 PCM-short documentation](https://docs.espressif.com/projects/rust/esp-hal/1.0.0/esp32s2/esp_hal/i2s/master/index.html)
  — selected HAL API's PCM-short timing model and circular-DMA surface. The
  original ESP32 target has different conditional implementation paths, so the
  local locked source and physical capture, not this cross-chip diagram alone,
  determine target acceptance. The locally locked permissive source also
  establishes the exact `available`/`push` circular-TX API and per-descriptor
  EOF configuration used by the compile-only ownership adapter; neither fact is
  treated as evidence of FIFO drain or a physical WS/latch edge.
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

## Logic-analyzer fixture

- [Sipeed SLogic16U3 introduction](https://wiki.sipeed.com/hardware/en/logic_analyzer/slogic16u3/Introduction.html)
  — official 0–10 V digital input range, adjustable 0–6 V threshold, USB 3
  interface, and 800/400/200 MHz limits for 4/8/16 active channels.
- [SLogic16U3 hardware specification](https://wiki.sipeed.com/hardware/en/logic_analyzer/slogic16u3/Hardware_Specification.html)
  — official input/ground header roles, directional cable warning, adjacent
  ground guidance, and prohibition on treating analyzer VCC as a DUT input.
- [SLogic16U3 software guide](https://wiki.sipeed.com/hardware/en/logic_analyzer/slogic16u3/Software_User_Guide.html)
  — official threshold, active-channel/sample-rate, trigger, duration, and raw
  session workflow. Alumina retains the unedited session plus a VCD export;
  analyzer software is test tooling and is not linked, vendored, or distributed
  by the firmware repository.

## SD memory transport

- [SD Association simplified specifications](https://www.sdcard.org/downloads/pls/)
  and its public Physical Layer Simplified Specification — functional reference
  for SPI-mode entry, CMD8/ACMD41 initialization, OCR/CSD interpretation,
  response/data tokens, CRC, and block programming status. Alumina's driver is
  independently authored and copies no third-party implementation code. The
  specification site's access/use terms and possible product-license notices
  remain distinct from the repository's `MIT OR Apache-2.0` source license.

The first implementation intentionally supports conservative default-speed
single-block SD V2 operation only. Legacy-card initialization, SDUC, UHS,
multi-block transfer, erase, and vendor extensions require separate scope and
qualification.

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

The resolved project policy is stricter than the licenses require: both planner
and FOC implementations are clean-room, based on functional descriptions,
published mathematics, datasheets, and independently authored tests. SimpleFOC
source copying is not planned despite its MIT license; g2 source copying is
prohibited by the selected implementation path. The two-pass lookahead and
monotonic boundary-feed increments, exact component-local jerk refinement, and
lossless line-G1 enablement, and exact affine dense-axis projection were
implemented and tested without inspecting either implementation's source.

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

Alumina extends the host/MCU scheduled-work idea to direct browser-managed
per-MCU affine clock fits and immutable SD-cached job partitions. It does not
claim Klipper protocol, transport, or implementation compatibility.

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
