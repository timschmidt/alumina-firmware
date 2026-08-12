# Verification and release evidence

## Principle

Compile success proves only that a board profile is syntactically coherent.
Aluminafw controls motion and potentially hazardous energy, so every release claim
must be backed by layered evidence from pure math through hardware timing and
failure recovery.

Quantitative thresholds belong in versioned board/machine qualification profiles.
They should be chosen from motor, driver, encoder, power-stage, process, and
machine requirements, then measured. This plan deliberately does not invent a
universal jitter, loop-rate, or shutdown threshold before hardware is selected.

## Test layers

### 1. Portable crate tests

Run on the host for every change:

- protocol encoding/decoding, exact-version mismatch rejection, malformed/truncated frames,
  maximum lengths, and stable discriminants;
- board metadata parsing, alias resolution, resource conflicts, active levels,
  engine limitations, and safe-state completeness;
- transactional configuration success, rollback, digest stability, duplicate
  ownership, invalid clocks/frequencies, and memory budget checks;
- safety state transition tables, impossible transitions, latching/reset, timeout,
  and fail-safe output actions;
- planner boundary conditions, lookahead, junctions, jerk/acceleration/velocity
  envelopes, feed hold/resume, homing/probing, and kinematics;
- Hyperpath feed/lookahead/jerk report composition and Hypersolve proposal versus
  exact/interval-certified acceptance, including precision exhaustion;
- integer/fixed-point overflow, deterministic rounding, residual/error diffusion,
  timer wraparound, and queue watermark behavior;
- FOC transforms/control-law unit tests against analytically known vectors and a
  separately implemented high-precision reference model;
- graph IR validation, cycles/state, units/types, memory bounds, rates, domains,
  and invalid opcode/resource combinations; and
- machine-IR canonical serialization, hash, overflow, continuity, constraint,
  and configuration-identity validation.

Use property testing for invariants and targeted regression fixtures for every
reported defect. Fuzz every untrusted parser in host builds: JSON configuration,
UI-only CNC G-code geometry import, HTTP metadata, binary protocol, graph
document, machine IR, update manifest, SD manifest, and telemetry decoder.

### 2. Deterministic simulator and trace model

`alumina-sim` supplies virtual clocks and resources:

- GPIO, I²S output image, RMT, PWM, ADC, encoder, UART/bus, limits, and faults;
- configurable motor/plant models for open-loop steppers and initial servo loops;
- network arrival jitter, queue pressure, command duplication/loss, clock drift,
  and disconnects;
- multiple boot-scoped MCU cycle counters, heartbeat delay/asymmetry, affine
  clock fitting, cached partitions, prepare/commit/abort, and partial readiness;
- SD block latency, corruption, full media, power loss, and prefetch starvation;
- exact PCM-short frame grids, complete-image suffix reconstruction, one-frame
  latch delay, sparse-to-dense horizon fill, wrong phase/order, and starvation
  before any target I²S/DMA timing claim;
- exact event-grid quantization and the complete scheduled-output ownership
  chain: future generation, bounded timeline acceptance, dense frame emission,
  independently reconstructed physical latch, output-free terminal dwell, and
  separately acknowledged terminal disable;
- exact circular-DMA ring ownership: externally safe prefill, whole-frame
  release credits, preview/push/accept refill identity, sealed versus writable
  horizons, untracked-write/underrun faults, and final-disable preplanning while
  the unique block remains retained;
- configuration-derived FOC hardware-loop replay from active integer compare
  edges through synchronized raw ADC/current/rotor observations, dq control,
  interval SVPWM, complete-image staging, and the next exact timer-zero, with
  explicit counter/device clock grids and compare-precision rejection;
- bounded cross-block ownership: two independently validated tokens, successor
  prefill before predecessor release, per-block commit-count/terminal-cycle
  barriers, strict acknowledgement order, and a gap-free dense wire trace;
- flash-stall and delayed-service events without pretending to prove hardware
  timing; and
- trace/replay of every command, state transition, scheduled event, output, sample,
  and telemetry frame.

The simulator is the fast oracle for interface and graph testing. It must use the
same protocol/IR validators as firmware but an independently structured execution
reference where differential testing is intended.

### 3. Exact CAD/CAM and machine-IR verification

Golden cases include:

- lines/arcs/Bezier/NURBS, rational transforms, degenerate/near-touching curves,
  cusps, very small/large coordinates, and mixed exact expressions;
- adaptive subdivisions around extrema, curvature changes, and discontinuities;
- multi-axis transforms and kinematics near singular or limit boundaries;
- quantization to asymmetric step lattices and low/high timer frequencies;
- events coincident with curve endpoints, pauses, tool transitions, and holds;
- intentional integer-width and time-range overflow; and
- reproducibility across native and WASM host builds where supported.

Global-job cases additionally permute participant discovery order, duplicate
device/stream identities, vary exact local timer ratios, corrupt named partition
and evidence digests, split storage chunks across arbitrary byte boundaries,
and replay every canonical manifest through the independent firmware decoder and
storage coordinator.

For each accepted job, independently compute or bound:

- maximum geometric deviation from exact path to reduced path;
- reduced path to commanded step lattice;
- temporal quantization and constraint error;
- endpoint/continuity error; and
- total declared envelope using a conservative composition rule.

The certificate is testable evidence, not a substitute for firmware validation.
Randomized property tests compare the integer executor trace to a high-precision
host reference. Byte-identical input/config/policy must produce byte-identical IR.

### 4. Per-board compile matrix

Every described board has a CI job that:

- selects exactly one board/chip target through `xtask`;
- validates metadata and Rust resource construction agreement;
- builds debug/diagnostic and production images;
- checks flash/internal-RAM/PSRAM/IRAM section budgets and reports changes;
- verifies embedded interface manifest/digests;
- scans dependencies/licenses and emits an SBOM; and
- fails when a required HIL capability has no registered test.

Compile matrices are separate because ESP chip features and targets are mutually
exclusive. A synthetic “all boards in one binary” build is not useful.

### 5. Hardware-in-the-loop smoke suite

Each physical board fixture provides controllable power, serial/JTAG where
available, loopback/test loads, and a logic analyzer or capture MCU. Initial
manual fixtures may use the available SLogic16U3, webcam, 10 MHz DSO,
multimeter, USB connection, TinyBee, and T-Deck Pro; automate power/capture as
the suite stabilizes. Add the received MKS ESP32 FOC V1.0 with a current-limited
supply and physically safe motor fixture.

Common tests:

- boot/reset/brownout safe levels before and after firmware initialization;
- board identity/revision and capability digest;
- each GPIO direction/pull/active level and every virtual-expander bit;
- bus/device discovery, shared-bus arbitration, interrupt/reset/power pins;
- Wi-Fi AP/STA/provisioning, web assets, command/telemetry, disconnect/reconnect;
- clock synchronization and scheduled I/O ordering;
- annotated-photo hotspot identity against the actual board revision;
- SD resumable upload, digest validation, atomic publish, bounded prefetch, and
  recovery from full/corrupt/interrupted media;
- watchdog, emergency stop, limit, fault latch/reset, and output maximum duration;
- update interruption/rollback and configuration power-loss recovery while idle;
  and
- long-running memory/queue/stack high-water marks.

T-Deck-specific tests cover every imported battery, keyboard, touch, EPD, GPS,
and LoRa driver plus coalesced display updates under network load.

TinyBee-specific tests capture every I²S output bit, safe image, motor pulse,
direction setup/hold, limit response, heater/fan watchdog behavior, and combined
output-stream arbitration. Capture must identify initial peripheral-start clocks,
the first and steady-state WS/latch phase, BCLK/data setup and hold, DMA refill
boundaries, the relationship between software completion and the physical latch,
and safe behavior for starvation, stop, reset, and static-to-stream handoff.
The initial safe-only capture fixture and mandatory disconnected-load/run-record
procedure are specified in [`HIL.md`](HIL.md); its build or software completion
is not itself a HIL pass.

### 6. Core-isolation and load tests

Measure rather than assume dual-core isolation. Test core 1 while core 0 performs:

- maximum supported Wi-Fi TCP/WebSocket traffic;
- repeated HTTP requests and compressed asset delivery;
- T-Deck EPD refresh plus input/GPS/LoRa activity;
- worst permitted telemetry encode/decimation and client reconnect behavior;
- SD/file reads; and
- long verified SD-job prefetch and block-boundary handoff;
- rejected flash/NVS/update requests during motion.

Separate idle-only tests perform actual flash write/erase/OTA and confirm that
the system cannot enter or remain armed. Diagnostic builds verify that every
claimed IRAM-safe interrupt path and transitively accessed datum is in internal
memory. Production trace measurements capture deadline latency distribution,
worst observed latency, pulse/loop jitter, queue horizon, underruns, and safe-stop
response.

Test for hours/days at maximum qualified rates, with intentional packet floods,
slow clients, malformed frames, full queues, telemetry backpressure, and service
task panics/resets. No service failure may directly energize or extend a
real-time output.

### 7. Advanced stepper qualification

Use logic-analyzer/capture hardware to verify:

- exact step counts and direction for single and coordinated N-axis traces;
- minimum/maximum pulse width, direction setup/hold, enable timing, and maximum
  sustainable aggregate rate;
- direct GPIO, RMT, and I²S backend equivalence at the logical trace level;
- jerk/acceleration/velocity envelope and junction behavior;
- queue low-water response and constrained stop before underrun;
- feed hold/resume, cancel, hard stop, limit, probe, and timer wraparound;
- Cartesian plus each qualified kinematics plug-in; and
- Wi-Fi/service saturation during all above cases.

Compare the captured event trace to the simulator and machine-IR reference using
integer timestamps with an explicitly allowed board timing tolerance.

### 8. Wi-Fi multi-MCU and cached-job qualification

Begin with two simulated MCUs, then harmless GPIO start pulses on two physical
dual-core boards. For every run archive UI send/receive timestamps, raw device
cycle samples, accepted/rejected heartbeat set, affine rate/offset/uncertainty,
chosen local start cycles, scheduled and captured start edges, participant boot
and partition digests, and network/load conditions.

Sweep and inject:

- normal, congested, high-delay, asymmetric, reordered, duplicated, and lost
  Wi-Fi packets plus AP restart and browser-worker suspension;
- oscillator drift, cycle wrap, boot-ID change, stale clock fits, deliberately
  underestimated sync tolerance, and start-tick quantization;
- one participant missing, faulted, rebooting, wrong configuration, late to
  prepare/commit, or unable to abort before the guard boundary;
- wrong/missing/corrupt/full SD partitions and power loss during chunk upload or
  manifest publication; and
- attended versus cached-autonomous Wi-Fi loss before prepare, after commit, at
  start, during execution, and at completion.

The measured edge spread must fall inside the uncertainty predicted before
commit plus a board-qualified execution bound. Any run outside it is a clock
model/qualification failure, not a larger undocumented tolerance. Verify that no
Wi-Fi result is represented as a physical E-stop guarantee and that the machine's
hardwired safety chain remains effective with all network equipment removed.

### 9. FOC/servo qualification

FOC begins on reconciled MKS ESP32 FOC V1.0 with current-limited bench power, an
emergency cutoff, and one unloaded or safely restrained motor. Progression:

The current [portable FOC checkpoint](evidence/M8-PORTABLE-FOC-FOUNDATION.md)
proves fixed-point interval arithmetic, transforms/modulation, dq PI state, and
deterministic functional-plant replay only. It satisfies none of the physical
progression steps or measurements below.

1. Validate PWM polarity, dead time, disable path, ADC triggers, phase-current
   offsets/gain, bus voltage, and sensor direction with no active torque.
2. Low-voltage open-loop electrical rotation and sensor alignment.
3. Voltage-mode closed loop with conservative limits.
4. Current-mode dq loop after current-sense synchronization is proven.
5. Velocity and cascaded position loops, then trajectory following.
6. Load/dynamometer and thermal testing within selected hardware ratings.

Record:

- PWM and ADC phase/timing, loop execution time and missed-deadline count;
- current reconstruction error, dq tracking/ripple, velocity/position following;
- sensor dropout/plausibility, overcurrent/voltage/temperature, stall/runaway,
  and enable/fault shutdown latency;
- parameter swap behavior and invalid-tuning rejection; and
- concurrent stepper/servo/network load if a board claims it.

A profile advertises only voltage, estimated-current, DC-current, or FOC-current
modes actually qualified with its sensing and power stage.

### 10. Interface, graphics, and dataflow tests

The first development checkpoint and its exact source caveat are recorded in
[`evidence/M5-INTERFACE-EXACT-BASELINE.md`](evidence/M5-INTERFACE-EXACT-BASELINE.md).
It proves the native/WASM type and production-artifact baseline; it does not
replace the browser, visual, exact-CAM, or hardware evidence below.

The next development checkpoint is recorded in
[`evidence/M5-EXACT-CAM-COMPILER.md`](evidence/M5-EXACT-CAM-COMPILER.md). It
proves certified exact curve/region presentation and the first deterministic
curve-to-canonical-lattice compiler fixture. It remains narrower than the M5
exit gate and makes no browser-workflow, complete machine-error, or hardware
claim.

- native/WASM unit tests for exact graph evaluation, forward migration among new
  released schemas (not the old interface), unknown-node round trip, type/unit
  errors, and capability reconciliation;
- Hypergraphics visual/golden tests for exact mesh/curve adapters, grid/axes,
  selection, clipping, and large/exact coordinate conversion failures;
- browser integration tests against simulated firmware capabilities and recorded
  HTTP/WebSocket traces;
- annotated-photo/hotspot golden tests, overview freshness/quality, cross-linking
  among board/config/graph/plot, and safe diagnostic leases/timeouts;
- SD job manager and multi-device clock/readiness/prepare/commit UI tests;
- property/fuzz tests for graph compiler validation and fixed-memory firmware IR;
- deterministic simulation/replay of multi-rate stateful graphs;
- telemetry plot stress with bounded browser memory, decimation, triggers, event
  retention, disconnect/reconnect, and export/import; and
- explicit tests proving that GPU `f32` buffers cannot satisfy CAM/machine-IR
  input types.

### 11. Security and recovery tests

- parser fuzzing, oversized/deep payload rejection, WebSocket fragmentation,
  slow-loris behavior, connection/command rate limits, and authentication state;
- default-AP credential setup, scan/join failure, saved-credential corruption,
  AP recovery, multi-device origin/credential isolation, and LAN/VPN-only threat
  assumptions;
- same-origin/CORS and session/CSRF behavior for mutating routes;
- unique provisioning credentials, secret redaction, and no private-key material
  in repository/artifacts/logs;
- signed image/web manifest success and tamper/rollback rejection;
- power cut at each update/configuration commit stage;
- policy-correct response to Wi-Fi loss, client death, service-core reset,
  corrupt job, wrong digest, duplicate/stale sequence, and clock-sync loss for
  both attended and explicitly cached-autonomous jobs; and
- dependency/advisory review with documented disposition.

## Fault-injection catalog

Maintain a table mapping each fault to injection method, expected state, output
action, telemetry event, reset requirements, and tested boards. Minimum catalog:

- emergency stop and each hard limit;
- driver/power-stage fault and overcurrent;
- encoder/sensor disconnect, impossible sample, and following error;
- over/undervoltage, overtemperature, brownout, and reset;
- command/telemetry queue overflow, motion underrun, missed real-time deadline;
- malformed/stale/wrong-config command or machine IR;
- lost network, clock uncertainty, service task failure;
- participant prepare/commit/abort loss, boot-ID change, or start disagreement;
- bus timeout/stuck line and external-device reset;
- cache-media full/corrupt/torn-anchor, optional filesystem faults, and
  update/config persistence interruption; and
- watchdog expiry in each state.

## Release artifact and evidence set

For each board/release publish or archive:

- source revision, locked dependencies, toolchain, build command, and SBOM;
- firmware/web/schema versions and cryptographic digests;
- global job and every per-MCU partition digest, clock-fit/start evidence, and
  observed distributed synchronization error where applicable;
- board revision/schematic provenance and capability snapshot;
- flash/RAM/IRAM/stack/queue budget report;
- simulator/property/fuzz summaries;
- HIL fixture revision and smoke logs;
- deadline, jitter, step-rate or control-loop measurements;
- safe-state/fault/update recovery results;
- supported capability and known-limit matrix; and
- signed binaries plus reproducibility comparison.

## Promotion gates

| Promotion | Required evidence |
| --- | --- |
| `described -> compiles` | metadata validation, CI build, memory report, provenance |
| `compiles -> bench` | boot/safe-state and named peripheral HIL smoke suite |
| `bench -> motion-qualified` | timing/load/fault traces against declared machine profile |
| `motion-qualified -> production-qualified` | sustained stress, recovery, update/security, hardware/release review |

No board is promoted because it resembles another ESP32 board or because an
upstream FluidNC configuration exists; those files are research evidence only.
