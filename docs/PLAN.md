# Aluminafw delivery plan

Research snapshot: 2026-08-10.

## Mission and scope

Build a reusable `no_std` Embassy firmware platform for ESP32-based motion,
automation, instrumentation, and operator-interface boards. Preserve all
drivers currently present in `t-deck-async-drivers-rs`, retain the useful web
serving behavior of `alumina-firmware`, update `alumina-interface` to the latest
local exact-geometry stack and Hypergraphics, and establish an exact and
testable CAD-to-machine pipeline.

The plan treats the following as separate but coordinated products:

1. `aluminafw`: embedded runtime, board packages, drivers, safety, motion,
   protocol, web asset serving, simulator, and build tooling.
2. `alumina-interface`: exact CAD/CAM, Hypergraphics visualization, device
   discovery/control, dataflow authoring, plotting, and machine-IR generation.
3. The Hyper stack and CSGRS: exact modeling, predicates, meshing, curves, and
   checked graphics/CAM conversion boundaries.

## Architectural invariants

These are release requirements, not aspirations.

- The real-time executor owns every peripheral that can affect motion, motor
  torque, process energy, endstops, or emergency shutdown.
- Wi-Fi, HTTP, WebSocket, file access, logging, display/input devices, and idle
  work run on the service executor.
- Cross-core traffic uses bounded, allocation-free messages. There are no
  cross-core async bus mutexes and no unbounded command or telemetry queues.
- Real-time interrupt paths and all transitively accessed data live in internal
  RAM where required. Flash/NVS writes, OTA, and configuration commits are
  rejected while a real-time job is armed or running.
- A board package only describes and instantiates physical facts. Machine
  semantics live in a separately validated configuration.
- Configuration is transactional: declare resources, validate capabilities and
  conflicts, construct inactive objects, establish safe defaults, then commit
  once with a digest. Partial configuration never arms outputs.
- Floating-point render data is never a CAM or control input. The machine
  boundary is an explicit, deterministic conversion from exact values to integer
  device lattices.
- Network loss cannot leave an output indefinitely active. Every hazardous
  scheduled output has a safe default and maximum duration or is owned by an
  independent safety state machine.
- Production motion builds on single-core ESP32 variants either fail at compile
  time or are explicitly labeled a degraded profile; this policy is an open
  decision, not an implicit fallback.

## Delivery sequence

The milestones are ordered by dependency and evidence, not calendar dates.
Calendar estimates should be made after first-wave boards, hardware availability,
and protocol/licensing choices are confirmed.

### M0 — Decisions, baselines, and reproducible workspace

Work:

- Resolve the decisions in `OPEN-QUESTIONS.md`, especially licensing, first-wave
  boards, single-core policy, and compatibility requirements.
- Record ADRs for the core split, board model, resource/configuration protocol,
  machine IR, web stack, security, and licensing/provenance.
- Pin one compatible Rust/ESP toolchain set across `esp-hal`, `esp-rtos`,
  `esp-hal-embassy`, `esp-radio`, Embassy, and the chosen HTTP server.
- Capture reference builds and behavior for `alumina-firmware`,
  `alumina-interface`, and T-Deck Patina before changing them.
- Establish CI targets, formatting/lint policies, a dependency-license scan,
  SBOM generation, and reproducible web-asset embedding.

Exit gate:

- A clean checkout can run host tests and compile minimal T-Deck Pro and TinyBee
  images using documented commands.
- ADRs remove ambiguity about source copying and reference-code reuse.
- No firmware signing key, Wi-Fi secret, or device credential is stored in Git.

### M1 — Workspace and complete T-Deck driver import

Work:

- Create the workspace described in `ARCHITECTURE.md`.
- Import every driver crate currently present in `t-deck-async-drivers-rs` with
  source provenance, notices, copyright headers, and commit identity preserved.
- Import `embedded-bus-async` and `sx126x-async-rs`; preserve all existing unit,
  compile, and hardware examples.
- Generalize the SPI chip-select wrapper from an `esp-hal` output type to
  `embedded-hal`/`embedded-hal-async` digital traits where practical. Do not
  rewrite working device protocols merely to fit the new workspace.
- Keep the existing `Rc` shared-bus pattern local to the service core. Driver
  handles that are intentionally `!Send` must never cross to the real-time core.
- Add a machine-readable `THIRD_PARTY.toml` or equivalent provenance ledger.

Imported inventory:

- `embedded-bus-async`
- `sx126x-async-rs`
- `t-deck-pro-battery-async` (BQ25896)
- `t-deck-pro-epd-async` (UC8253/GDEQ031T10)
- `t-deck-pro-gps-async` (MIA-M10Q)
- `t-deck-pro-keyboard-async` (TCA8418)
- `t-deck-pro-lora-async` (SX1262 integration)
- `t-deck-pro-touch-async` (CST328)
- `i2c-tester` as a diagnostic example
- the Patina application as behavioral reference and, where useful, a board
  example rather than a production firmware dependency

Exit gate:

- Imported crates pass their available host/compile tests.
- T-Deck Pro initializes all imported devices and reproduces existing input,
  battery, EPD, GPS, touch, keyboard, and LoRa smoke tests.
- License/provenance review confirms that imported Apache-2.0 code remains
  properly marked regardless of the license chosen for new code.

### M2 — Board packages and dual-core Embassy skeleton

Work:

- Implement mutually exclusive `board-*` features selected through `xtask`,
  with exactly one ESP chip feature in each firmware build.
- Define board capabilities, resource aliases, bus topology, DMA channels,
  interrupt ownership, safe startup levels, memory/flash layout, and optional
  PSRAM in declarative metadata plus a small Rust composition root.
- Start one Embassy executor on each core with the ESP runtime’s supported
  second-core facility. Construct async peripherals on the core that will own
  their interrupts.
- Implement fixed-capacity SPSC command, urgent-safety, and telemetry channels.
- Add a build-time board metadata exporter consumed by the interface and tests.
- Bring up T-Deck Pro as the service-peripheral slice and TinyBee as the
  real-time-output slice.

Exit gate:

- A test task on core 0 can saturate networking/display work without missing a
  synthetic core-1 deadline under the agreed timing threshold.
- Cross-core APIs accept only fixed-size or fixed-capacity protocol values.
- Duplicate pins, impossible DMA allocation, unsafe startup defaults, and
  unsupported capabilities fail before arming.

### M3 — Service plane, Wi-Fi, and embedded interface

Work:

- Replace ESP-IDF services with `esp-radio`, `embassy-net`, and a reviewed
  `no_std` HTTP/WebSocket server. Picoserve is the leading candidate, subject to
  a stress and security review because it remains pre-1.0.
- Support provisioning plus AP, STA, and AP+STA modes where the chip permits.
- Serve versioned pre-compressed interface assets and an immutable manifest.
- Add `/api/v1` capability, configuration, job, telemetry, health, time, and
  update endpoints. Keep `/device`, `/pins`, and `/queue` as a temporary,
  feature-gated migration shim only.
- Send control and telemetry as compact binary frames over WebSocket; keep JSON
  for discovery, diagnostics, and human-authored configuration.
- Add authentication, origin checks, rate/size limits, secure provisioning,
  signed update verification, and per-device credentials. Do not claim Internet
  exposure is safe until threat-model tests pass.
- Serve immutable assets while motion is active only after cache/jitter tests.
  Never write or erase flash while armed.

Exit gate:

- The browser discovers board capabilities and streams telemetry without polling
  individual pins.
- Network fuzz/stress tests cannot overflow a real-time queue or extend a
  hazardous output beyond its configured watchdog.
- An interrupted update leaves a bootable signed image and safe outputs.

### M4 — Klipper-like resource configuration and scheduled I/O

Work:

- Establish and continuously generate the chip-aware implementation/qualification
  matrix in `PERIPHERAL-COVERAGE.md`; a known but unimplemented peripheral must be
  discoverable as unavailable rather than silently absent or falsely supported.
- Define stable typed resource IDs for GPIO, ADC, DAC where present, LEDC/MCPWM
  PWM, timers, RMT, PCNT, I²S, I²C/SPI devices, UART, TWAI/CAN, USB, Ethernet,
  Wi-Fi, BLE, ESP-NOW, SD, displays, touch/input, LoRa, and board-specific
  expanders.
- Add resource discovery, leasing, ownership, pin aliases, inversion/pulls,
  frequency/resolution constraints, DMA/interrupt requirements, and safe-state
  metadata.
- Implement an allocate/configure/finalize transaction with schema and config
  digests, inspired by Klipper’s small-MCU configuration pattern but written
  independently.
- Provide scheduled digital/PWM/ADC/serial/timer operations against a monotonic
  device clock, with cancellation, deadlines, bounded batches, and maximum output
  durations.
- Generate interface node definitions from the same capability schema.

Exit gate:

- The interface can configure and operate supported resources without board-
  specific endpoint code.
- Invalid aliases, conflicts, frequencies, clock domains, unsafe values, or
  oversized queues are rejected atomically with structured diagnostics.
- A simulator and hardware trace agree on scheduled event order and integer
  timestamps.

### M5 — Real-time safety kernel and advanced stepper motion

Work:

- Implement explicit `Boot -> Safe -> Configured -> Armed -> Running -> Hold ->
  Fault` states with latched faults and physically meaningful reset rules.
- Build an N-axis path planner with forward/reverse lookahead, coordinated axes,
  junction constraints, feed hold/resume, homing, probing, soft/hard limits, and
  kinematics plug-ins.
- Implement third-order jerk-limited S-curve profiles and short fixed-duration
  execution segments with linearly changing velocity, taking design inspiration
  from Synthetos/g2 behavior without copying licensed implementation code.
- Use deterministic integer/fixed-point execution. Backends emit direct GPIO,
  RMT/DMA, or I²S stream/static pulse data according to board capability.
- Add step-direction timing contracts, enable sequencing, direction setup/hold,
  pulse-width validation, and optional TMC UART/SPI configuration/telemetry.
- Separate the planner, segment generator, and pulse engine so their timing and
  correctness can be tested independently.

Exit gate:

- Logic-analyzer traces meet pulse-width, direction, synchronization, jitter,
  and queue-underrun limits at maximum supported multi-axis rates.
- Lookahead and feed-hold tests preserve velocity/acceleration/jerk constraints.
- Any underrun, limit event, watchdog expiry, or invalid segment transitions to a
  proven safe state.

### M6 — Exact CAD/CAM to machine-resolution execution

Work:

- Update the interface to current CSGRS and Hyper crates before building CAM.
- Keep solids, curve regions, transformations, and CAM paths in exact `Real`
  form on the browser/host side.
- Define rational machine transforms and device lattices: steps per unit,
  encoder counts per turn, timer frequency, permitted path error, and axis limits.
- Use Hypercurve projection/subdivision with an explicit chord-error budget to
  reduce exact curves to the machine’s spatial resolution. Couple geometry error
  and timing error rather than selecting an arbitrary display tolerance.
- Compile a canonical, versioned, hashed machine IR containing integer positions,
  timer ticks, constraints, tool events, coordinate-frame/config digests, and an
  error certificate. V1 may use certified small line segments; native exact
  line/arc/Bezier forward-difference opcodes follow only when their integer
  execution is independently verified.
- Validate bounds, continuity, hashes, overflow, timing monotonicity, config
  identity, and the error envelope again on firmware before arming.
- Return commanded lattice position and measured encoder position so plots can
  distinguish geometric, quantization, following, and control error.

Exit gate:

- Golden tests demonstrate a bounded maximum deviation from exact CAD curve to
  commanded machine lattice, including pathological rational/Bezier cases.
- Recompiling identical CAD, machine config, and tool policy yields byte-identical
  machine IR.
- No `f32` graphics buffer or camera transform can enter the CAM/motion API by
  type construction.

### M7 — SimpleFOC-style servo and field-oriented control

Work:

- Define separate motor, power-stage, rotor-sensor, current-sense, PWM/ADC sync,
  and control-law traits for BLDC, permanent-magnet synchronous, and two-phase
  stepper servo arrangements.
- Implement Clarke/Park transforms, space-vector or sine PWM, electrical angle
  alignment, voltage/estimated-current/DC-current/FOC-current torque modes as
  hardware allows, and cascaded torque/velocity/position loops.
- Run the current/torque loop from a timer/PWM-synchronized real-time context;
  run velocity and position loops at explicit lower rates. Budget every loop and
  keep tuning/telemetry off the critical path.
- Support encoder, Hall, magnetic SPI/I²C, analog/PWM, PCNT, and user-supplied
  sensors incrementally. Require calibrated current sensing for current-mode FOC.
- Add current, voltage, speed, temperature, following-error, and sensor-plausibility
  shutdowns. Parameter updates use double-buffered validated snapshots.
- Expose stepper and servo axes through a common trajectory contract while
  preserving distinct hardware and safety capabilities.

Exit gate:

- Start with a named supported inverter/servo development board; TinyBee
  step-stick sockets are not treated as FOC hardware.
- Processor-in-loop and dynamometer/bench traces meet loop rate, phase-current,
  following-error, overcurrent, and shutdown-latency criteria.
- A network or UI stall cannot affect the inner control loop.

### M8 — Hypergraphics interface and graphical control environment

Work:

- Complete the migration in `INTERFACE-ROADMAP.md`: latest CSGRS/Hyper APIs,
  exact scene adapters, Hypergraphics camera/projection, no hand-rolled mesh
  renderer, capability-driven device controls, and streaming plots.
- Split the present graph model into a pure exact CAD graph and a stateful timed
  control/dataflow graph connected through typed bridge nodes.
- Add units, structures/records, arrays/streams, events, state, sample clocks,
  scheduling domains, bounded channels, backpressure, subgraphs, reusable
  components, loops/state machines, probes, and versioned serialization.
- Compile an audited deterministic subset to firmware graph IR. Dynamic editing,
  visualization, and non-real-time nodes stay in the browser/service domain;
  arbitrary code and allocation are forbidden in the real-time domain.
- Generate peripheral/protocol nodes from capabilities rather than maintaining a
  monolithic hard-coded palette.

Exit gate:

- A saved graph can be reopened without semantic loss and reports broken wires,
  unit mismatch, unsupported device capabilities, and scheduling violations
  before deployment.
- A representative producer/consumer control graph runs deterministically on
  hardware with bounded memory and reproduces in simulation/replay.
- Plots can trigger, capture, decimate, export, and correlate commands with
  telemetry without perturbing the real-time loop.

### M9 — Additional boards and production hardening

Work:

- Add the user-selected FluidNC-compatible boards using the board-package
  contract and configuration importer/conversion tooling where useful.
- Add relay, industrial-I/O, and laboratory controller profiles only with their
  voltage, isolation, startup, watchdog, and fail-safe characteristics modeled.
- Run sustained network/motion/FOC stress, power-loss and brownout, thermal,
  EMC-relevant recovery, fuzzing, dependency review, and update rollback tests.
- Publish compatibility matrices, known timing limits, signed artifacts, SBOMs,
  configuration schemas, and reproducible build instructions.

Exit gate:

- Each supported board has a maintained CI build, capability snapshot, hardware
  smoke suite, safe-state test, and measured timing envelope.
- Release claims distinguish compile support, bench-tested support, and
  production-qualified support.

## Parallel workstreams and dependencies

After M0, several workstreams can proceed concurrently, but the dependencies
below are strict:

- Driver import and board metadata can proceed alongside interface API migration.
- Web serving depends on the service executor and protocol framing, not on the
  final motion planner.
- Advanced stepper execution depends on the resource allocator, real-time clock,
  safety state machine, and TinyBee I²S backend.
- FOC depends on a selected power-stage board, synchronized PWM/ADC support, and
  the safety kernel; it does not depend on TinyBee step output.
- Exact machine IR depends on current Hyper/CSGRS interface types and a stable
  firmware resource/config digest.
- Firmware graph deployment depends on the resource model and timing domains;
  its browser editor and serialization can begin earlier.

## Compatibility and migration policy

- `alumina-firmware` remains the behavioral reference until M3 reaches endpoint
  parity. It is not incrementally converted from ESP-IDF; `aluminafw` is a clean
  Embassy workspace with explicit migration shims.
- Legacy HTTP routes are read-only or narrowly translated at the service-core
  boundary and are removed after one advertised compatibility window.
- Existing board constants are treated as clues, not authoritative electrical
  specifications. Every profile is reconciled with a schematic or vendor board
  file before outputs are enabled.
- FluidNC YAML is an import source, not the internal schema. Unsupported pin
  modes, timing engines, or semantics produce explicit conversion errors.
- CSGRS and Hyper migrations are completed as coherent dependency updates; the
  interface does not carry parallel old/new geometry types.

## Major risks and controls

| Risk | Consequence | Planned control |
| --- | --- | --- |
| ESP flash/cache stalls both cores | Lost motion/FOC deadlines | Internal-RAM hot path, DMA engines, idle-only writes, stress measurement |
| Wi-Fi or browser floods commands | Queue overflow or unsafe latency | Bounded admission, rate/size limits, backpressure, watchdogs |
| “Pin” abstraction hides I²S/DMA limits | Invalid TinyBee behavior | Typed resources and capability validation; no magic integer pins |
| Exact host values exceed MCU cost | Missed deadlines or code bloat | Exact host/CAM, certified integer machine IR, fixed-point firmware |
| Servo hardware is assumed from GPIO | Electrical damage | Named power-stage profiles, sensing calibration, safe bring-up gates |
| “All ESP32 peripherals” becomes unbounded | Never-ending first release | Capability families, generated nodes, staged board-driven support |
| Reference-code license contamination | Distribution constraints | Clean-room implementation, provenance ledger, dependency/license CI |
| Board variants silently conflict | Unsafe startup or dead buses | Exactly-one profile, generated allocation checks, schematic review/HIL |
| Browser graph is nondeterministic | Unrepeatable control behavior | Explicit clocks/state/backpressure, validated firmware subset, replay |
| Web assets or telemetry exhaust RAM | RT starvation | Fixed budgets, immutable compressed assets, decimation, separate pools |

## Definition of done for every capability

A peripheral, protocol, board, motor mode, graph node, or endpoint is not
“supported” until it has:

- a versioned capability/schema representation;
- documented ownership, timing, memory, and safe-state behavior;
- conflict and invalid-configuration tests;
- simulator or host tests where possible;
- a board CI build and hardware smoke evidence where hardware is involved;
- observable health/fault telemetry;
- bounded queues and failure behavior;
- user-facing interface discovery and diagnostics;
- migration and compatibility notes; and
- license/provenance attribution.
