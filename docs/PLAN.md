# Aluminafw delivery plan

Research and decision snapshot: 2026-08-10.

## Mission

Build a greenfield `no_std`, Embassy-based platform for dual-core ESP32 motion,
automation, instrumentation, and embedded operator-interface boards. Copy every
driver currently present in `t-deck-async-drivers-rs` with provenance, retain
the embedded-web functionality demonstrated by `alumina-firmware`, and redesign
`alumina-interface` as the authoritative exact CAD/CAM, configuration, control,
diagnostic, and graphical-programming environment.

The coordinated products are:

1. `aluminafw`: thin embedded drivers, dual-core runtime, safety, Wi-Fi/web
   service, native protocol, SD job cache, bounded command queues, real-time
   interpolation, step/FOC control, and telemetry.
2. `alumina-interface`: browser/WASM exact modeling and CAM, machine/job
   compiler, multi-MCU coordinator, Hypergraphics UI, annotated-board
   diagnostics, plots, simulation, and typed dataflow authoring.
3. CSGRS and the Hyper stack: authoritative exact geometry, constraints, path
   scheduling, certified approximation, graphics conversion, and optional
   physical/process models.

## Explicit non-goals

- No compatibility with `alumina-firmware`, the old `alumina-interface`, GRBL,
  FluidNC configuration/protocol, Klipper protocol, g2 protocol, or SimpleFOC
  APIs. They are research/behavior references only.
- No firmware-side parser for G-code, CAD, meshes, graph documents, or raw
  geometry. G-code import, where useful, terminates in exact UI geometry.
- No single-core ESP32 build, degraded profile, or cooperative Wi-Fi/motion
  scheduling.
- No USB, serial, CAN/TWAI, or other alternative Alumina multi-MCU transport in
  the first architecture. Wi-Fi is the coordination transport.
- No claim that a local-LAN device is safe for direct Internet exposure.
- No arbitrary user code, WASM, allocation, or unbounded graph interpreter on
  the real-time core.

## Release invariants

- Core 0 owns Wi-Fi, AP/STA management, HTTP/WebSocket, web assets, SD,
  configuration parsing, T-Deck peripherals, telemetry presentation, and idle
  tasks. Core 1 owns all motion, torque, process-energy timing, endstops,
  emergency response, and deterministic sampled I/O.
- Cross-core traffic is fixed-capacity and allocation-free. Urgent safety,
  ordinary commands/job blocks, and telemetry use independent bounded paths.
- All real-time interrupt code/data and active queues reside in internal memory.
  Flash/NVS/update writes are impossible while armed or running.
- A board package contains physical facts and safe states. Stored runtime machine
  configuration contains attached motor/driver/mechanical/process facts. Neither
  may silently infer the other.
- The browser/WASM compiler is authoritative. Exact values remain exact until a
  named, deterministic, configurable-precision conversion to integer steps,
  encoder counts, PWM values, ADC units, and timer ticks.
- Numerical proposals cannot make combinatorial or acceptance decisions without
  exact/certified replay. Undecided precision is an explicit result.
- Firmware independently validates every bounded property it can: exact schema,
  hashes, configuration identity, integer ranges, timing monotonicity, local
  rates, resource ownership, queue fit, and safe output duration.
- Network loss cannot create unbounded energy. A cached autonomous job is allowed
  only when its complete duration, interlocks, and fault policy are local.
- Every MCU in a distributed job has a complete immutable local SD partition and
  a measured clock mapping before it can prepare or commit.
- Physical E-stop and safety-interlock design does not depend on Wi-Fi atomicity.

## First end-to-end workflow

The first workflow is fixed so early work converges on one evidence chain:

1. Author an exact 2D contour containing a line, circular arc, and Bezier.
2. Fetch a TinyBee capability snapshot and a Cartesian XYZ pen/air-cut machine
   configuration containing exact steps/mm and bounded calibration facts.
3. Build the path with Hypercurve/Hyperpath, certify lookahead and jerk-limited
   timing, allocate an explicit error budget, and quantize to integer I²S step
   events/ticks in browser/WASM.
4. Replay identical bytes in `alumina-sim`; inject Wi-Fi loss, limit, queue, SD,
   and clock faults and compare against the certificate and safety policy.
5. Upload the immutable manifest/stream to TinyBee SD, validate, arm, and execute
   with a pen or disconnected/air-cut machine.
6. Show every relevant connector/resource on an annotated TinyBee photograph and
   correlate UI scope/logic traces, firmware telemetry, and logic-analyzer
   captures of step, direction, limits, and job start.

No heater, spindle, laser, plasma, or cutting load is enabled by this workflow.
Those enter only after the same trace/safety gates pass with harmless loads.

## Delivery sequence

Milestones are dependency/evidence gates rather than calendar promises.

### M0 — Governance, clean-room boundaries, and reproducible scaffold

Work:

- Add standard `LICENSE-MIT` and `LICENSE-APACHE` texts; set new crate manifests
  to `MIT OR Apache-2.0` and preserve copied-file licensing independently.
- Add `THIRD_PARTY.toml`, SPDX/header policy, source/behavioral-reference ledger,
  SBOM and dependency-license CI, and a clean-room contribution checklist.
- Record ADRs for core ownership, board/machine split, native protocol, exact
  boundary, SD cache, multi-MCU clocks/start, safety, security, and updates.
- Pin a mutually compatible Rust/ESP toolchain across `esp-hal`, `esp-rtos`,
  `esp-hal-embassy`, `esp-radio`, Embassy, networking, HTTP, and serialization.
- Create the workspace in `ARCHITECTURE.md`, `xtask`, host-test features, schema
  generation, memory-budget checks, and reproducible UI-asset embedding.
- Capture black-box functional fixtures from the old Alumina web serving/UI and
  published g2/SimpleFOC behavior. Fixtures describe results, not source layout.

Exit gate:

- A clean checkout runs host CI and compiles minimal T-Deck Pro and TinyBee
  dual-core images with documented commands.
- Every source/import/reference has a license/provenance disposition; no signing
  key, Wi-Fi secret, or private credential is present.
- The clean-room requirements/test authoring and implementation roles/process are
  documented before motion or FOC implementation starts.

### M1 — Complete T-Deck driver import and service-peripheral parity

Work:

- Copy the complete current inventory with source revision, copyright headers,
  Apache-2.0 notice, changes, tests, and examples:
  `embedded-bus-async`, `sx126x-async-rs`, `t-deck-pro-battery-async`,
  `t-deck-pro-epd-async`, `t-deck-pro-gps-async`,
  `t-deck-pro-keyboard-async`, `t-deck-pro-lora-async`,
  `t-deck-pro-touch-async`, `i2c-tester`, and the Patina behavior fixture.
- Generalize only the SPI chip-select HAL coupling necessary to reuse the bus;
  do not gratuitously rewrite working protocols.
- Keep `Rc`/shared async bus ownership and all intentionally `!Send` handles on
  core 0. Represent shared I²C/SPI topology in the board package.
- Convert Patina's task/model/display-coalescing pattern into bounded service
  actors without making the application a production dependency.

Exit gate:

- All available host/compile tests pass and T-Deck Pro reproduces battery, EPD,
  GPS, touch, keyboard, LoRa, and shared-bus smoke behavior.
- A source-to-destination inventory proves that no present T-Deck driver was
  omitted and no unimplemented fitted peripheral is falsely claimed.

### M2 — Board packages and strict dual-core Embassy runtime

Work:

- Implement exactly-one `board-*` selection through `xtask`; reject any board
  whose application-core count is below two.
- Define revisioned board metadata: chip/memory/partitions, pins and virtual
  resources, buses/devices, DMA/interrupts, strapping/electrical constraints,
  clock sources, safe states, image/hotspot assets, HIL suite, and qualification.
- Create small Rust composition roots that consume `esp_hal::Peripherals` once
  and return disjoint `ServiceResources` and `RealtimeResources`.
- Start one pinned Embassy executor per core using the supported runtime. Add
  fixed SPSC job/command and telemetry channels plus an urgent safety mailbox.
- Bring up T-Deck Pro as the service slice and TinyBee as the real-time slice.
  Validate the official TinyBee I²S output map and all-safe startup image before
  connecting motors or heaters.
- Export canonical board capabilities and an image hotspot map for simulator/UI.

Exit gate:

- Core 0 can saturate synthetic service work while a core-1 deadline probe stays
  within an initial measured envelope.
- Duplicate/impossible pins, DMA, buses, clock ownership, unsafe boot states, and
  single-core targets fail before arming.
- Logic-analyzer traces identify every TinyBee shifted bit and safe reset image.

The TinyBee flash-identity follow-up now makes the observed 8 MiB module the
primary `mks-tinybee` build and retains a separately identified 4 MiB variant.
Both compile through the same physical-routing composition, export distinct
canonical capability digests, and leave board-qualified ELFs; runtime probing
cannot substitute them. The 4 MiB image currently fits, but its physical
fixture and final partition/web/update budgets remain open. See the
[flash-variant evidence](evidence/M2-TINYBEE-FLASH-VARIANTS.md).

### M3 — Native protocol, Wi-Fi/web service, simulator, and SD cache

Work:

- Use `esp-radio`, `embassy-net`, and a reviewed bounded `no_std` HTTP/WebSocket
  server. Qualify the chosen server rather than assuming a pre-1.0 implementation
  is robust.
- Boot into a protected device AP by default. Serve the exact matching compressed
  UI and provide scan/join/leave/recovery flows for infrastructure Wi-Fi.
- Define one greenfield protocol for identity, capabilities, configuration,
  network setup, clock samples, storage, jobs, commands, health/faults, telemetry,
  and signed updates. Exact version mismatch is a hard rejection/update flow.
- Use JSON only for bounded human-facing discovery/configuration and fixed binary
  frames for command, job-block, clock, waveform, and telemetry streams.
- Implement authenticated content-addressed SD chunks, resumable uploads,
  atomic manifests, integrity scans, capacity/health, audit export, and idle-only
  mutation. Use an explicitly provisioned raw cache region with alternating
  anchors and synchronized append records; any optional exchange filesystem is
  separate and non-authoritative. Core 0 prefetches verified blocks; core 1 never
  reads storage.
- Build `alumina-sim` with virtual time, resource engines, I²S images, safety
  state, queue/storage/network faults, trace/replay, and recorded board fixtures.
- Add signed/recoverable firmware+UI update packaging, per-device credentials,
  request size/rate limits, origin policy, and no direct-Internet claim.

Exit gate:

- A fresh device AP serves the UI, scans/joins a WLAN, and remains recoverable
  after wrong credentials or interrupted configuration.
- The simulator and TinyBee accept, cache, validate, stream, and discard a
  synthetic long job with bounded RAM and power-loss-safe storage semantics.
- Network fuzz/flood and SD stalls cannot overflow RT queues or extend a bounded
  output; writes/updates are rejected while armed.

### M4 — Native machine/resource model and board-aware diagnostic UI

Work:

- Define stable typed resource IDs and capabilities for GPIO, shifted I²S bits,
  ADC, PWM/MCPWM/LEDC, timer, RMT, PCNT, I²C/SPI/UART, storage, radio/network,
  board devices, safety inputs, axes, and process outputs. An integer “pin” may
  be an alias, never the type system.
- Implement transactional declare/validate/construct/commit configuration with
  ownership, conflicts, frequency/resolution, clock, DMA/interrupt, safe-state,
  maximum-duration, memory, and configuration-digest checks.
- Store/report step angle, microstep choices/current, mechanics/gearing,
  encoders, calibration and uncertainty, timer/event limits, PWM/ADC/current
  sense, motor/power-stage, travel/dynamic/process, and safety facts required by
  browser path planning.
- Provide scheduled digital/PWM/sample/serial/timer operations on the MCU cycle
  clock with cancellation, deadlines, fixed batches, and watchdogs.
- Replace old interface device panels with generated capability forms. Add a
  licensed annotated-photo view whose hotspots link connectors, pins, buses,
  devices, and virtual resources to live value, owner, state, safety, and plots.
- Separate a low-rate all-resource status overview from bounded triggered digital
  edge capture and analog waveform acquisition. Direct output tests require a
  diagnostic lease, timeout, safe range, and disarmed mode where appropriate.

Exit gate:

- One simulated schema generates configuration and diagnostic UI for GPIO,
  TinyBee I²S, timers/PWM, ADC, serial, storage, safety, and an axis without
  board-name branches.
- Invalid electrical/timing/resource combinations reject atomically with a
  hotspot-linked diagnostic.
- Live TinyBee/T-Deck state overlays match measured pins/devices and stay bounded
  under maximum overview and capture rates.

### M5 — Exact interface, Hypergraphics, and authoritative WASM CAM

Work:

- Move `alumina-interface` from its legacy mesh/sketch APIs to an explicitly
  recorded set of the current sibling CSGRS/Hyper working trees. Never use a
  published CSGRS package as a fallback merely because its manifest version
  matches. Use `TriangleMesh`, `CurveRegion2`/Hypercurve paths, exact `Real`
  transforms, Hyperpath, and Hypersolve.
- Extend Hypergraphics (or one narrow adapter) to own exact mesh/curve scenes,
  camera/projection, grids, axes, overlays, selection, picking, and checked GPU
  conversion. Delete interface-owned vertex/normal/edge/camera rendering paths.
- Implement a UI-only CNC G-code importer that parses supported decimal/modal
  geometry exactly into Hypercurve/Hyperpath; unsupported semantics fail before
  CAM. It is an optional importer, never canonical job or firmware input.
- Model exact machine/process constraints from the connected capability/config
  record. Compose Hyperpath length/feed/lookahead/jerk reports and Hypersolve
  exact/certified solves; extend these crates or a thin CAM layer where machine
  constraint projection is absent.
- Allocate configurable error among curve reduction, motor/count lattice,
  timing, calibration, and following/control budgets. Refine only to the
  precision required to certify the policy.
- Emit reproducible global manifests and canonical per-MCU integer/fixed-point
  streams with source, compiler, capability, configuration, schedule, and error
  evidence. Keep GPU floats structurally unable to enter CAM.
- Run identical compiler fixtures in browser/WASM and native tests for
  reproducibility, but the shipped browser/WASM application remains the
  authoritative user workflow.

Exit gate:

- Line/arc/Bezier/NURBS and pathological exact cases have conservative source →
  reduced path → command lattice → timer error reports.
- Identical source/config/policy produces byte-identical job bytes and digests.
- No old CSGRS type, hand renderer, silent float tolerance, or renderer-to-CAM
  path remains.

Implementation checkpoint: the legacy application has been replaced by a
greenfield exact core, canonical protocol/simulator client, and Hypergraphics
native/WASM shell. The build rejects registry substitutes for the current
sibling CSGRS/Hyper stack and structurally separates exact, measured, canonical
machine, and display values. Hypergraphics now owns certified exact curve/path
and role-preserving region presentation. A separate compiler certifies a
line/arc/Bezier fixture through motion-specific curve chords, exact path length,
machine-step and timer lattices, and the real canonical firmware IR. General
Bezier metric promotion, complete machine constraints/error budgets,
lookahead/jerk, browser transport, and release pinning remain open. Canonical
packaging now independently replays real firmware blocks, publishes immutable
per-MCU storage objects, and binds owned partitions into the shared sorted
global manifest with exact rational duration agreement. Fixture identities are
still sentinels and automatic global resource partitioning remains open. See the
[M5/I0 evidence](evidence/M5-INTERFACE-EXACT-BASELINE.md), [M5/I1-I3
evidence](evidence/M5-EXACT-CAM-COMPILER.md), and [M5/M7 packaging
evidence](evidence/M7-GLOBAL-JOB-MANIFEST.md).

### M6 — Safety kernel, clean-room stepper control, and first workflow

Work:

- Implement `Boot -> Safe -> Configured -> Armed -> Running -> Hold -> Fault`
  with latched faults, local E-stop/endstop/probe, bounded reset, enable chains,
  driver/process watchdogs, and configuration/job identity.
- From clean-room requirements, implement the Synthetos-style behavior needed by
  Alumina: N-axis constraint projection, forward/reverse lookahead validation,
  junction limits, third-order jerk schedules, short linearly varying-velocity
  segments, hold/resume, and deterministic terminal state. Reuse/extend exact
  Hyperpath mathematics in the UI; do not copy g2 code.
- Keep firmware's role bounded: validate/consume precomputed schedule segments,
  interpolate integer events, generate pulses, report horizons, and calculate a
  local safe hold/stop when an asynchronous safety event requires it.
- Implement direct GPIO/RMT and TinyBee I²S stream/static backends as hardware
  warrants, including pulse width, direction setup/hold, enable sequencing,
  aggregate update rate, shared heater/fan image, and TMC UART/SPI support.
- Execute the selected exact-contour → simulator → TinyBee SD → pen/air-cut
  workflow and produce trace/certificate correlation in the UI.

Implementation checkpoint: the allocation-free exact step-event executor,
configuration-derived role/polarity/timing profile, full TinyBee-style shifted
image mapper, fixed canonical execution report, and cached-block simulator trace
are present. Cached-block admission now preflights every segment in bounded
record/axis work, preserves live state on rejection, retains unique ownership
through exact execution, and permits acknowledgement only after independently
correlated terminal progress. Exact count/position, half-tick interpolation,
overflow, configured rate, pulse/setup/hold, normal disable, deadline-fault,
malformed-report, ownership, and image-integrity tests pass. The hardware
serializer, browser planner, and physical workflow remain closed; both first
board packages remain non-armable. Core 1 now composes the exact cached executor
with the active configuration, descriptor-bound absolute lattice origin,
scheduled epoch, interlock-qualified arm/start state, and a two-phase complete-
image transaction. A block is acknowledged only after its exact generated
output prefix is physically committed, and normal disable waits for the exact
enable-hold cycle.
TinyBee's blocking static writer is only a compile/HIL staging path and cannot
qualify arming; T-Deck Pro has no motion output. The portable safety-input
layer retains configuration-derived resource/polarity/pull/debounce/watchdog
facts, exact stable transitions, and arming masks. TinyBee now binds its four
digital routes to a transactional core-1 GPIO bank, scans at a nominal 1 ms,
publishes exact status masks/deadlines, and synchronously reapplies the safe
image before invalidating job ownership on a fault. T-Deck Pro exposes an empty
machine-safety bank. Hold degrades to Stop until constrained deceleration is
qualified. None of this closes electrical, response-time, safe-output, or HIL
evidence. A portable PCM-short layer now encodes each complete image into a
duplicated two-slot frame, rejects fractional device-cycle/frame relationships,
requires exact boundary alignment and one-frame lead, and expands sparse updates
into a continuous fixed-capacity horizon. The simulator reconstructs every
image from all 64 modeled wire bits and latches sequence, timing, contract, and
horizon-starvation faults. Target DMA ownership, physical latch observation,
safe stop/reclaim, and HIL qualification remain open; this software checkpoint
does not alter either board's armability.
The scheduled checkpoint adds an exact backend output lattice and a bounded
scheduled-image owner that separates future generation, hardware-timeline
acceptance, and ordered latch observation. It stops cleanly at ring capacity,
retains the admitted block through every physical image and any output-free
terminal dwell, and routes normal disable through the same commit path. A host
integration drives its images through the PCM-short timeline and independent
64-bit wire observer at a four-cycle fixture quantum. Firmware now instantiates
that future-image owner and requires an explicit `Priming → Primed` hardware
horizon acknowledgement between the distributed abort guard and local start.
Both board adapters reject every streaming-horizon call, and TinyBee's
safe-prefilled PCM-short circular-DMA composition remains compile-only and
unreachable. Target frame materialization, a qualified cycle/frame epoch, safe
peripheral reclaim, interrupt-backed commit observation, and logic-analyzer
evidence remain open; both boards remain non-armable.
The follow-on circular-DMA checkpoint now owns exact released frame credits and
uses a preview/push/accept transaction so a failed or partial target write never
extends the sealed horizon. Sparse motion tags become materialized only in their
exact dense frame and can retire only after a separate monotonic physical-latch
observation. The final normal-disable image is planned while the final block and
its output tokens remain retained, and the scheduled owner enforces one complete
image per strictly increasing output-grid boundary. A four-frame host ring
simulation exercises steady release/refill and proves final disable is visible
before block acknowledgement. The TinyBee HAL surface uses one four-byte DMA
descriptor per modeled frame, but remains compile-only and unreachable. FIFO/WS
phase, refill interrupt/wake policy, static-to-stream handoff, safe reclaim, and
all physical claims remain open.
The cross-block follow-on changes admission from one outstanding token to a
strict two-block FIFO. Multi-block schedule installation and arming now require
both initial blocks; core 1 carries that window through prestart priming and
refills it only when the oldest independent physical barrier returns. The
scheduled owner records a generated-update count and exact terminal cycle per
block, so successor images may remain queued when the predecessor is released.
Portable motion tests and a four-frame circular-DMA/wire simulation prove the
successor is planned before predecessor release, every dense frame remains
continuous, and acknowledgements remain ordered. The bounded window makes
minimum cached block duration versus refill lead a compiler/qualification fact;
insufficient horizon fails closed. Target interrupt timing and physical capture
remain open.
An isolated release-only `mks-tinybee-pcm-short-safe` HIL artifact now turns the
first physical gate into a reproducible procedure. It establishes the blocking
safe image before timekeeping or I²S setup, retains all service/second-core
tokens inert, streams and refills only that same safe image, stops DMA, rewrites
two safe samples, and parks. The `xtask` command builds but cannot flash. This
adds no capture result: disconnected loads, raw logic-analyzer evidence, phase
review, and explicit promotion remain required.
The follow-on capture contract adds an analyzer-only GPIO4 phase/result marker,
an exact SLogic16U3 four-channel probe and photo procedure, a bounded streaming
VCD decoder, and a strict run record whose decoded measurements must replay the
SHA-256-bound analysis report. It targets the separately identified primary
8 MiB TinyBee package while preserving the 4 MiB build variant. No capture or
qualification is implied, and production motion still cannot select this HIL
path.

Exit gate:

- Captured pulses meet board/machine timing and count requirements at the
  declared maximum coordinated rates under Wi-Fi/web/SD load.
- Lookahead, jerk, hold/resume, limits, probing, underrun, timer wrap, and fault
  tests agree across exact reference, simulator, firmware trace, and logic
  analyzer within declared integer timing bounds.
- Any invalid job, stale config, underrun, limit/E-stop, or missed deadline takes
  the documented local safe path.

### M7 — Wi-Fi multi-MCU clocks and cached synchronized jobs

Work:

- Implement timestamped heartbeat responses with boot ID, receive/transmit cycle
  samples, counter width/source, queue horizon, and quality flags.
- In a browser worker, unwrap counters and robustly fit one affine UI↔MCU clock
  map with drift and uncertainty per device. Reject stale, asymmetric, or
  background-throttled samples.
- Partition a global job into immutable per-MCU streams with shared epochs and
  sync markers. Require every participant to upload, hash, locally validate, and
  report its exact stored partition before prepare.
- Implement nonce/digest-bound prepare, future local-cycle selection, commit,
  acknowledgement, abort guard, arm lease, idempotent retries, and observed-start
  reconciliation from `DISTRIBUTED-JOBS.md`.
- Define attended versus cached-autonomous network-loss behavior. Exclude live
  cross-MCU feedback and runtime repartitioning initially.
- Qualify in simulation, then with harmless GPIO pulses on two physical
  dual-core boards before distributed axes or process energy.

Implementation checkpoint: fixed heartbeat/RT-deadline wire formats, an exact
causal host estimator, boot-bound prepare receipts, participant-bound
install/confirm/abort state, dual-core firmware routing, and adversarial two-MCU
simulation are present. Commit/reference wire version 2 adds board-qualified
prime lead, one-shot local hardware priming at the abort guard, an explicit
`Primed` acknowledgement, and fail-closed missed-start behavior. Schedule
report version 3 adds immutable typed first-output observations and a dedicated
retained tolerance fault. The target binds that
future horizon and confirmed local start to the exact scheduled step executor
behind configuration, interlock, deadline, cached-work, package, and
physical-output qualification gates. The interface now produces the canonical
global manifest and owned participant cache packages consumed by this protocol.
Origin-bound authenticated browser upload and retry-safe per-participant cache
reconciliation are now implemented. A worker-capable conservative browser clock
adapter and headless prepare/install/confirm-or-abort coordinator now enforce
boot identity, exact deadlines, all-installed-before-confirm, finite leases,
precommit cancellation, and ambiguous-mutation status reconciliation. Worker
lifecycle/UI integration and production-worker authenticated browser/HTTP
qualification now cover nominal sampling, response loss, a finite outage,
reboot, bounded delay, and conservative excessive-delay rejection. Live
cache/schedule ownership, broader packet-stress and background-throttling cases,
HIL runs, a qualified I²S/DMA backend, and the physical exit gate remain open;
current board packages are non-armable. Firmware-to-browser observed-start
reconciliation is implemented in the portable path: exact affine inversion
maps each authenticated cycle observation to a conservative browser-time
interval, refuses missing/regressed/foreign evidence, and the deterministic
two-MCU simulator proves its known edges are contained. See the
[observed-start replay evidence](evidence/M7-OBSERVED-START-REPLAY.md).

Exit gate:

- Starts meet a published cross-MCU edge tolerance under nominal and saturated
  Wi-Fi, with uncertainty predicted conservatively before commit.
- Lost/reordered/duplicated messages, browser suspension, AP failure, MCU reboot,
  wrong/full/corrupt SD, clock drift, and one participant fault prevent start or
  produce the documented safe local outcome.
- No distributed safety claim relies on Wi-Fi simultaneous stop; required
  physical interlock topology is recorded in the machine configuration.

### M8 — Clean-room FOC and servo control on MKS ESP32 FOC V1.0

Work:

- Reconcile vendor branch, V1.0 schematic/manual, component values, PWM/enables,
  current channels, dual AS5600 buses, faults, ratings, and safe startup against
  the received board and bench measurements.
- Define separate `MotorModel`, `PowerStage`, `RotorSensor`, `CurrentSense`,
  `Modulator`, torque/motion controller, and safety traits for BLDC/PMSM and later
  two-phase servo steppers.
- Clean-room implement Clarke/Park transforms, sine/SVPWM, electrical alignment,
  voltage/estimated-current/DC-current/dq-current torque modes, and cascaded
  velocity/position control. Use published mathematics and independent tests,
  not copied SimpleFOC implementation.
- Synchronize MCPWM and ADC; run current/torque in an explicitly budgeted ISR or
  interrupt executor, with slower velocity/position/telemetry domains. Validate
  complete parameter snapshots before atomic RT swap.
- Report the motor, encoder, current-sense, PWM/ADC, loop-rate, calibration,
  uncertainty, and qualified limit contract so UI CAM can account for servo
  resolution, acceleration, following, and torque limits.
- Add overcurrent/voltage/temperature/speed/following/sensor faults and a
  hardware-measured shutdown path. Qualify one motor at low voltage/current
  before dual-motor operation.

Portable checkpoint: `alumina-foc` now provides allocation-free exact Q2.30
point and outward-interval arithmetic, wide-intermediate Clarke/Park transforms,
certified rotations, non-clipping min/max modulation, anti-windup dq-current PI
control, voltage-circle-bounded digest snapshots/commands, and hardware ownership
traits.
The deterministic simulator replays a dimensionless one-pole dq plant and
separates controller evidence from physical motor claims. The MKS board adapter,
physical electrical alignment, other torque modes, cascaded motion loops,
safety monitors, synchronized MCPWM/ADC, WCET, and all bench qualification
remain open.
See the [portable FOC evidence](evidence/M8-PORTABLE-FOC-FOUNDATION.md).

Safe-target checkpoint: a typed MKS ESP32 FOC V1.0 package and classic-ESP32
firmware feature now compile and link. Schematic reconciliation replaces the
earlier example-derived enable assumption: GPIO22/GPIO12 are unconnected, no
independent inverter enable or fitted cache medium is established, and neither
is advertised. Core 1 owns both MCPWM units, ADC1, both encoder buses, and six
phase inputs. The EG2133 truth table shows that driven low and driven high each
select a bridge device, so its only implemented transition instead makes all
six pins no-pull inputs before the first await. All motion/FOC operations
reject, storage is explicitly unavailable, and the target remains non-armable.
See the [safe-target evidence](evidence/M8-MKS-FOC-SAFE-TARGET.md).

The completed Configuration V2 checkpoint removed the obsolete `FocEnable`
selector and added a
canonical axis-local shutdown contract for dedicated enable, dedicated disable,
or phase high impedance. Core 1 retains the exact strategy, stage/control
resources, polarity, qualified evidence, and transition-cycle bound. Validation
requires a `Qualified` stage topology and matching board safe values, so the
current MKS `Described` stages still reject. See the
[shutdown-contract evidence](evidence/M8-FOC-SHUTDOWN-CONTRACT.md).

The exact-angle checkpoint now uses a wrapping `u32` turn lattice, independently
certified outward sine/cosine series, configurable component/norm ULP gates, and
exact count/pole-pair/direction/alignment uncertainty. Rotor samples bind the
configuration digest and pole pairs and replay their canonical rotation before
controller use. See the
[exact-angle evidence](evidence/M8-EXACT-ELECTRICAL-ANGLE.md).

The AS5600/closed-ownership checkpoint now adds an independently authored,
read-only async AS5600 driver. RAW ANGLE remains an exact 12-bit count; defined
STATUS flags and reserved bits are separate, and a diagnostic observation
brackets rather than conflates three I²C transactions. The MKS target keeps
both mode-selectable connector buses dormant at safe boot, then offers an
explicit unscheduled type-state transition to two independent 400 kHz AS5600
owners. MCPWM0/1 plus their high-impedance phase pins and ADC1 plus its four
uncalibrated inputs are sealed into types with no energizing/sampling trait or
raw-token extractor. See the
[AS5600/closed-ownership evidence](evidence/M8-AS5600-CLOSED-OWNERSHIP.md).

The current-sampling checkpoint now adds exact raw-ADC calibration intervals,
validated two-shunt phase reconstruction, retained zero-sequence uncertainty,
and replayable PWM/ADC timing witnesses. Digest identity, integer PWM period,
trigger jitter, acquisition aperture, channel skew, conversion latency, and
nearest-switching-edge guards must agree before a sample can bind to a FOC
snapshot. Its synchronized producer is still portable software: no target
creates the stamp and it makes no timing or current-measurement claim. See the
[current-sampling evidence](evidence/M8-CURRENT-SAMPLING-CONTRACT.md).

Configuration V3 now gives those rotor/current/timing facts canonical stored
records. It adds fixed runtime, direct/quadrature controller, rotor, two-channel
ADC calibration, and PWM/ADC synchronization records; exact cross-record checks
bind duplicated scalar authorities; and only a complete independently hashed
profile lowers into digest-bound FOC snapshots. The MKS capability identity now
names one compile-supported AS5600 endpoint on each independent encoder bus.
The real `Described` stages still reject and no peripheral is activated. See the
[configuration V3 evidence](evidence/M8-FOC-CONFIGURATION-V3.md).

The classic-ESP32 ADC1 ownership checkpoint now adds a compile-only,
software-started diagnostic path. A type-state transition consumes ADC1 and all
four routed current inputs, retains explicit per-route attenuation at the
HAL-default 12-bit resolution, and exposes a bounded channel-0/channel-1 polling
sequence. It records request and conversion-completion cycles but cannot create
a sample-aperture or PWM-edge witness and does not implement `CurrentSense`.
The transition is unscheduled and untested on hardware. See the
[ADC1-owner evidence](evidence/M8-CLASSIC-ESP32-ADC1-OWNER.md).

The exact MCPWM compare checkpoint now binds device-cycle, PWM-period, and
center-aligned counter clocks; lowers each conservative Q2.30 duty interval by
midpoint/ties-to-even; retains its whole interval-to-lattice error; enforces
minimum high/low pulses; and derives both compare edges in counter ticks. A
portable complete-image owner accepts one future image and faults on identity,
grid, timer-zero sequence, or cycle-overflow disagreement. On MKS, an
unscheduled type-state transition validates both HAL clock trees before it
configures, stops, and zeroes timer 0 in both MCPWM units. No operator or GPIO
is attached, no compare is written, and all phase pins remain inputs. See the
[MCPWM compare evidence](evidence/M8-EXACT-MCPWM-COMPARE.md).

Configuration V4 now joins both programmed ADC attenuations and the complete
MCPWM source/counter clocks, raw dividers, timer peak, minimum-pulse domain, and
quantization policy to canonical stored bytes. Full-stream lowering constructs
the digest-bound current proof and `PwmCompareContract` together. The MKS target
then requires its compiled capability identity and exact stage/phase/ADC routing
before it can construct a private stopped-MCPWM selection. The current
`Described` stage still closes this route before target selection. See the
[FOC hardware configuration V4 evidence](evidence/M8-FOC-HARDWARE-CONFIGURATION-V4.md).
Physical electrical alignment, calibrated analog error, truthful device-cycle
edge stamps, measured sensor latency, and any nonzero duty remain separate
reviewed work.

The first configuration-derived hardware-loop simulation now validates a
lowered V4 axis as one indivisible bundle, primes neutral duty at an exact
timer-zero, constructs a synchronized raw-ADC/current/rotor observation from
the active integer compare image, executes dq current control, lowers the next
interval-valued SVPWM image, and commits it at the next exact boundary. It
supports one current update per PWM period and rejects fractional
counter-to-device-cycle edges rather than inferring timestamps. Every command,
token, boundary, raw-sample, precision, overflow, and clock-domain rejection
terminally closes the virtual owner. Deterministic replay and an explicit
1,000,000-versus-1,200,000 Q2.30 ULP policy case make accumulated numerical
uncertainty visible. See the
[configured hardware-loop evidence](evidence/M8-CONFIGURED-FOC-HARDWARE-LOOP.md).
It remains portable simulation: no HAL peripheral, electrical plant, target
task, or energizing operation is attached.

Exit gate:

- PWM/ADC phase, offset/gain, electrical angle, loop WCET/jitter, current ripple,
  following error, and shutdown latency meet named conservative profiles.
- Network, web, SD, telemetry, and UI stalls cannot perturb the inner loop.
- Only measured modes are advertised; vendor current/power ratings are not
  inherited as Alumina qualifications.

### M9 — General typed graphical control and instrumentation

Work:

- Replace the recursive ad hoc graph value model with versioned exact values,
  units, records, arrays, options/results, events, bounded streams/waveforms,
  resource/job handles, clocks, and explicit state.
- Add typed nodes/wires, subgraphs, reusable components, front panels, cases,
  loops/state machines, delay/feedback, bounded queues, rate transitions,
  backpressure, fault flow, probes, and deterministic serialization/diffs.
- Partition execution into `HostExact`, `Service`, and whitelisted `Realtime`
  domains. Compile fixed memory, resource claims, rates, WCET, and safe failure
  into audited graph IR; fixed firmware safety always has authority.
- Generate peripheral/protocol nodes from the connected capability ledger,
  eventually covering all implemented ESP32 resources without a hard-coded
  palette or false support claims.
- Extend the diagnostic UI into an integrated board explorer, logic analyzer,
  oscilloscope, event/state plotter, XY/spectrum tools, triggers, cursors,
  capture/replay, exact path correlation, experiment records, and export.

Implementation checkpoint: `alumina-interface-core` now owns the first
greenfield structural graph document. It provides bounded exact unit/type/value
registries, explicit clocks and HostExact/Service/Realtime placement, opaque
versioned nodes, typed ports/wires, and canonical digest-verified `ALGR` V1
replay under an independent admission policy. A separate registry binds audited
node shapes to that exact type/clock context, declares complete feedthrough and
read-before-write state, bounds declared state, and produces exact deterministic
combinational-cycle witnesses. Unknown nodes still round-trip but cannot pass
semantic analysis. Checked recursive analysis now proves maximum canonical
typed-value bytes and rejects undersized state declarations. Every input now
also declares required/optional and synchronous/bounded Event/Stream delivery,
an explicit full-queue policy, and a checked allocation report including source
tick/sequence envelopes; synchronous wires cannot cross concrete execution
ownership. Cross-clock Stream feedthrough now requires an audited exact
latest-at-or-before transition: rational clock analysis proves one shared root,
the smallest repeating schedule, minimum queue capacity, and bounded held
sample storage. A separate fixed HostExact implementation registry and bounded
simulator now execute only external Stream sources, that audited transition,
and Stream sinks using exact root time. Canonical `ALGT` traces bind graph and
implementation identities and replay by independent simulation. The first
portable deployed boundary is now a fixed 4 KiB `ALGRIR01` package with
allocation-free independent admission, whitelisted Boolean Service/Realtime
opcodes, integer device-cycle schedules, declared WCET plus executor reserve,
and contiguous state/channel/bridge arenas. A third interface registry binds
the complete audited semantics and fixed implementations, then lowers one
single-device graph into those independently replayed bytes. General node,
state, or Event execution remains open, but the fixed package now has a portable
firmware runtime: transactional exact-identity admission into const-generic
arenas, safety-gated Service tick-zero priming, unique Service/Realtime endpoint
ownership, canonical queue execution, exact-cycle release admission, and a
shared first-cause fault latch. Authenticated upload/core transfer, live task
composition, and active/candidate lifecycle are now present: the browser
publishes a typed immutable 4 KiB SD object, an authenticated coordinator has
both cores independently validate and select its exact identities, and fixed
staging/active storage plus dual-core authorization are linked in every current
board image. The headless browser/WASM client reconciles lost responses and
rejects foreign identity. Durable selection/boot recovery, composition of the
selected bytes into live fixed executors, measured executor timing, resource
opcodes, capability nodes, and graph UI remain open.
See the
[canonical document](evidence/M9-CANONICAL-GRAPH-DOCUMENT-V1.md),
[audited semantic](evidence/M9-AUDITED-GRAPH-SEMANTICS.md), and
[type-storage](evidence/M9-CANONICAL-TYPE-STORAGE.md) and
[bounded-channel](evidence/M9-BOUNDED-GRAPH-CHANNELS.md), plus the
[exact-rate](evidence/M9-EXACT-GRAPH-RATES.md) and
[deterministic-simulation](evidence/M9-DETERMINISTIC-GRAPH-SIMULATION.md), and
[fixed graph-IR](evidence/M9-FIXED-GRAPH-IR.md) and
[portable graph-runtime](evidence/M9-FIXED-GRAPH-RUNTIME.md), and
[authenticated deployment](evidence/M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md)
evidence.

Exit gate:

- A saved multi-rate producer/consumer/PID/interlock graph simulates, validates,
  deploys, captures, and replays with bounded memory and explicit clocks.
- Unknown nodes round-trip, invalid units/resources/rates fail at their wire or
  board hotspot, and no arbitrary code enters RT.
- Plots remain bounded and preserve fault/trigger evidence under maximum
  qualified telemetry load.

### M10 — T-LoRa Pager stub, broader boards, and production hardening

Work:

- Add a compile-only metadata/resource/image stub for the current ESP32-S3
  T-LoRa Pager, then inventory and implement its devices after first-target and
  FOC evidence is stable.
- Add selected dual-core FluidNC-associated PCBs as new physical board packages,
  not configuration/protocol compatibility targets.
- Add relay, industrial-I/O, and laboratory profiles only with electrical range,
  isolation, startup, watchdog, calibration, uncertainty, and fail-safe behavior.
- Expand peripheral/protocol implementations from actual hardware use cases;
  CAN/Modbus/Ethernet/etc. do not become alternate Alumina sync transports
  without a separately approved architecture change.
- Run long-duration load, brownout/power-loss, thermal, storage wear/corruption,
  fuzz, update rollback, security, clock drift, motion/FOC, and recovery suites.
- Publish signed artifacts, SBOM, reproducible builds, board photos/hotspots,
  schemas, capability snapshots, known limits, and qualification evidence.

Exit gate:

- Every supported board has a CI build, safe-state proof, capability snapshot,
  HIL smoke suite, measured memory/timing envelope, and honest qualification.
- Releases distinguish `described`, `compiles`, `bench`, `motion-qualified`, and
  `production-qualified`; resemblance to an upstream board is never evidence.

## Dependency and parallelization rules

- M0 precedes source import. Driver import and interface dependency migration may
  proceed in parallel after its license/toolchain gates.
- M2 core ownership precedes live networking, storage prefetch, and any
  real-time output. M3 simulator/protocol may begin against host fixtures earlier.
- M4 capability/configuration facts precede authoritative machine precision and
  M5 job compilation.
- M5 and the core-1 queue/step backend may develop in parallel against the same
  machine-IR fixtures; M6 is where both evidence chains meet.
- M7 requires the M3 cache/clock protocol and M6 single-MCU safe execution.
- M8 requires the M2 runtime, M4 resource/config model, M6 safety kernel, and a
  physically reviewed MKS FOC board; it does not depend on distributed motion.
- M9 editor/serialization/plot work may start earlier, but firmware graph
  deployment depends on resource, safety, and clock semantics.

## Major risks and controls

| Risk | Consequence | Planned control |
| --- | --- | --- |
| ESP flash/cache stalls both cores | Lost step/FOC deadlines | IRAM/internal-DRAM hot path, hardware buffering, armed write prohibition, measured load tests |
| Wi-Fi timing is mistaken for real time | Desynchronized or partial multi-MCU start | Cached streams, measured affine clocks/uncertainty, future hardware starts, abort guard, local safety |
| Browser suspension or loss | Queue starvation or uncontrolled energy | Complete caches, worker clock-quality checks, attended/autonomous policy, local duration/interlocks |
| SD latency/corruption | RT underrun or wrong job | Content hashes, atomic manifests, fixed prefetch credits, low-water safe stop, fault injection |
| “Pin” hides I²S/PWM/ADC facts | Invalid TinyBee or FOC operation | Typed resources and board-specific engine/electrical constraints |
| Exact computation becomes unbounded | UI hang or unusable compile | Configurable precision, explicit undecided result, budgets/cancellation, cached exact facts |
| Approximate proposal becomes a decision | Geometry/motion correctness loss | Hypersolve/Hyperlimit exact or interval-certified replay gate |
| Servo power is inferred from GPIO | Electrical damage | Named power-stage profile, schematic reconciliation, low-energy staged HIL |
| Vendor ratings are treated as qualification | Thermal/current damage | Independent measurement and conservative published profile |
| Browser graph is nondeterministic | Unrepeatable control | Explicit clocks/state/queues, fixed RT opcode set, static reports, trace replay |
| Broad peripheral scope never converges | No usable release | Board-driven waves and generated `unimplemented`/qualification ledger |
| Reference-code license contamination | Distribution restrictions | Clean-room process, provenance ledger, SPDX/license CI, independent tests |
| Annotated photos become stale/misleading | Wiring/configuration mistakes | Revision/hash/provenance, normalized hotspots, board-HIL reconciliation |

## Definition of done for any capability

A board, peripheral, protocol, job opcode, motion/FOC mode, graph node, plot
channel, or endpoint is supported only when it has:

- a versioned schema/capability record and exact owner domain;
- documented electrical, rate, resolution, memory, timing, queue, and safety
  limits, including uncertainty where measured;
- deterministic configuration, digest, and failure/recovery semantics;
- simulator/host tests and malformed/boundary/property cases;
- compile and HIL evidence on every claimed board/revision;
- observable health, fault, timing, and high-water telemetry;
- documentation and annotated physical mapping where relevant;
- license/provenance/security review and SBOM coverage; and
- a qualification label no stronger than its archived evidence.
