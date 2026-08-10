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

- protocol encoding/decoding, version negotiation, malformed/truncated frames,
  maximum lengths, and stable discriminants;
- board metadata parsing, alias resolution, resource conflicts, active levels,
  engine limitations, and safe-state completeness;
- transactional configuration success, rollback, digest stability, duplicate
  ownership, invalid clocks/frequencies, and memory budget checks;
- safety state transition tables, impossible transitions, latching/reset, timeout,
  and fail-safe output actions;
- planner boundary conditions, lookahead, junctions, jerk/acceleration/velocity
  envelopes, feed hold/resume, homing/probing, and kinematics;
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
FluidNC importer, HTTP metadata, binary protocol, graph document, machine IR,
update manifest, and telemetry decoder.

### 2. Deterministic simulator and trace model

`alumina-sim` supplies virtual clocks and resources:

- GPIO, I²S output image, RMT, PWM, ADC, encoder, UART/bus, limits, and faults;
- configurable motor/plant models for open-loop steppers and initial servo loops;
- network arrival jitter, queue pressure, command duplication/loss, clock drift,
  and disconnects;
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
available, loopback/test loads, and a logic analyzer or capture MCU.

Common tests:

- boot/reset/brownout safe levels before and after firmware initialization;
- board identity/revision and capability digest;
- each GPIO direction/pull/active level and every virtual-expander bit;
- bus/device discovery, shared-bus arbitration, interrupt/reset/power pins;
- Wi-Fi AP/STA/provisioning, web assets, command/telemetry, disconnect/reconnect;
- clock synchronization and scheduled I/O ordering;
- watchdog, emergency stop, limit, fault latch/reset, and output maximum duration;
- update interruption/rollback and configuration power-loss recovery while idle;
  and
- long-running memory/queue/stack high-water marks.

T-Deck-specific tests cover every imported battery, keyboard, touch, EPD, GPS,
and LoRa driver plus coalesced display updates under network load.

TinyBee-specific tests capture every I²S output bit, safe image, motor pulse,
direction setup/hold, limit response, heater/fan watchdog behavior, and combined
output-stream arbitration.

### 6. Core-isolation and load tests

Measure rather than assume dual-core isolation. Test core 1 while core 0 performs:

- maximum supported Wi-Fi TCP/WebSocket traffic;
- repeated HTTP requests and compressed asset delivery;
- T-Deck EPD refresh plus input/GPS/LoRa activity;
- worst permitted telemetry encode/decimation and client reconnect behavior;
- SD/file reads; and
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

### 8. FOC/servo qualification

FOC begins with current-limited bench hardware, an emergency cutoff, and an
unloaded or safely restrained motor. Progression:

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

### 9. Interface, graphics, and dataflow tests

- native/WASM unit tests for exact graph evaluation, graph schema migration,
  unknown-node round trip, type/unit errors, and capability reconciliation;
- Hypergraphics visual/golden tests for exact mesh/curve adapters, grid/axes,
  selection, clipping, and large/exact coordinate conversion failures;
- browser integration tests against simulated firmware capabilities and recorded
  HTTP/WebSocket traces;
- property/fuzz tests for graph compiler validation and fixed-memory firmware IR;
- deterministic simulation/replay of multi-rate stateful graphs;
- telemetry plot stress with bounded browser memory, decimation, triggers, event
  retention, disconnect/reconnect, and export/import; and
- explicit tests proving that GPU `f32` buffers cannot satisfy CAM/machine-IR
  input types.

### 10. Security and recovery tests

- parser fuzzing, oversized/deep payload rejection, WebSocket fragmentation,
  slow-loris behavior, connection/command rate limits, and authentication state;
- same-origin/CORS and session/CSRF behavior for mutating routes;
- unique provisioning credentials, secret redaction, and no private-key material
  in repository/artifacts/logs;
- signed image/web manifest success and tamper/rollback rejection;
- power cut at each update/configuration commit stage;
- fail-safe response to Wi-Fi loss, client death, service-core reset, corrupt job,
  wrong digest, duplicate/stale sequence, and clock-sync loss; and
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
- bus timeout/stuck line and external-device reset;
- filesystem full/corrupt and update/config persistence interruption; and
- watchdog expiry in each state.

## Release artifact and evidence set

For each board/release publish or archive:

- source revision, locked dependencies, toolchain, build command, and SBOM;
- firmware/web/schema versions and cryptographic digests;
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
upstream FluidNC configuration exists.
