# Alumina interface roadmap

## Outcome

Redesign `alumina-interface` into one browser/WASM application with five coherent
subsystems:

1. exact CAD/CAM and authoritative machine-job compilation on current CSGRS and
   the Hyper stack;
2. Hypergraphics-owned exact visualization with one checked GPU boundary;
3. native Alumina Wi-Fi device configuration, SD-job, clock, synchronized-job,
   control, and safety workflows;
4. an annotated-board logic-analyzer/oscilloscope environment for assembly,
   bring-up, and debugging; and
5. a typed, timed, stateful LabVIEW-style graph system spanning host, service,
   and whitelisted real-time domains.

Substantial redesign is authorized. There are no deployed graph files, clients,
routes, or UI workflows to preserve. Reuse sound egui/eframe/Trunk scaffolding
and useful interaction ideas, but build no compatibility adapter or schema
migration for the old firmware/interface.

## I0 — New baseline and domain boundaries

Work:

- Select and record one mutually compatible set from the current sibling CSGRS,
  Hyperreal, Hyperlattice, Hyperlimit, Hypertri, Hypermesh, Hypercurve,
  Hyperpath, Hyperphysics, Hypersolve, and Hypergraphics working trees. A
  manifest version such as CSGRS 0.23.0 is only a package label; no published
  CSGRS release may substitute for the workspace source. Add optional Hyper
  crates only for a concrete use.
- Establish native unit/property tests, WASM checks, Trunk production builds,
  headless protocol/compiler fixtures, and browser integration tests.
- Split crates/modules so exact geometry/CAM and protocol/simulation tests do not
  require a window, GPU, live device, or browser storage.
- Define four disjoint value boundaries: exact design/CAM values, bounded
  measured values with units/uncertainty, canonical firmware wire/IR values, and
  lossy display/GPU values. Only named checked conversions cross them.
- Remove old endpoint models, legacy graph serialization assumptions, and direct
  board-name conditionals before they can become new commitments.

Exit:

- A minimal app, exact library, native protocol client, simulator client, and
  Hypergraphics scene build natively and for WASM with independent tests.
- Type construction prevents an `f32` renderer value from satisfying an exact
  CAM or machine-job input.

Implementation checkpoint: the greenfield workspace, four disjoint value
domains, real protocol/machine-IR client fixtures, current sibling-stack source
gate, native CSGRS mesh, checked Hypergraphics adapter, and validated production
WASM bundle are implemented. The captured Hypercurve/Hyperphysics trees contain
concurrent development edits, so release pinning and browser integration remain
open. See the
[M5/I0 evidence](evidence/M5-INTERFACE-EXACT-BASELINE.md).

## I1 — Current exact geometry and Hypergraphics

### Geometry migration

| Old interface concept | New authority |
| --- | --- |
| `csgrs::mesh::Mesh<()>` | current `TriangleMesh` / native Hypermesh |
| `csgrs::sketch::Sketch<()>` | `CurveRegion2`, `CurvePath2`, or explicit open Hypercurve path |
| legacy CSG traits | current solid functions and checked extension APIs |
| geometry-domain `f64` | `hyperreal::Real`, with explicit finite approximation only at named boundaries |
| ad hoc `geo`/`nalgebra` bridges | native Hypercurve/Hyperlattice facts |
| custom path/feed math | Hyperpath carriers and certified reports plus thin machine-specific extensions |

Propagate exact-operation failures and undecided predicates as structured node or
compiler diagnostics. Never substitute empty geometry, zero, or a renderer
tolerance.

### Hypergraphics ownership

Add or expose in Hypergraphics, or one narrow
`alumina-hypergraphics-adapter`, the scene facilities the interface needs:

- checked adapters from CSGRS/Hypermesh and Hypercurve paths/regions;
- instances, exact transforms, material/style, layers, and revision-keyed cache;
- exact axes, grids, work envelopes, toolpaths, selections, probes, annotations,
  edge/wireframe, normals, and point markers;
- `ExactCamera`, viewport, clipping, projection, picking, and screen/world
  conversion; and
- explicit errors for non-finite or out-of-range GPU conversion.

Delete interface-owned vertex/normal/index construction, point icosahedra, grid
and axis mesh generation, camera matrix math, and renderer-only conversions.
Picking returns a semantic/exact object or a bounded approximation that cannot
silently become CAM geometry.

Exit:

- Representative solids, exact curves, regions, machine envelope, and toolpath
  scenes render through Hypergraphics with visual regressions.
- The interface composes scenes and interactions; it no longer implements a
  second mesh renderer.

Implementation checkpoint: Hypergraphics now owns certified presentation of an
exact line/arc/cubic path and curved material/hole region in addition to the
checked native mesh adapter. Exact sources and subdivision/role evidence remain
in the scene; neither line meshes nor their GPU values can become CAM input.
Visual regression coverage, filled regions, picking, envelopes, and interaction
remain open. See the
[M5/I1-I3 evidence](evidence/M5-EXACT-CAM-COMPILER.md).

## I2 — Native device, network, storage, and clock client

One client model is shared by configuration, board view, graph nodes, plots, and
jobs. It speaks only the new Alumina schema.

Connection lifecycle:

1. Load the exact matching embedded UI or reject a remotely loaded UI whose
   protocol/schema version differs.
2. Authenticate and fetch device ID, boot ID, board/revision, capability digest,
   security/update state, and machine membership.
3. Fetch capabilities by digest and current configuration/state by digest.
4. Open bounded binary telemetry/command streams and request explicit overview
   or capture subscriptions; never subscribe to every high-rate channel.
5. Begin timestamped cycle-counter heartbeats and maintain rate, offset, drift,
   delay, and uncertainty in a browser worker.
6. Reconcile immutable SD manifests/blobs and job readiness.

Configuration UI is generated from board and resource capabilities. It displays
physical/virtual namespace, aliases, modes, units, electrical limits, clock and
rate constraints, calibration/uncertainty, ownership/conflicts, safe state,
qualification, memory/queue budgets, and the resulting digest. Editing is
transactional: draft, local validation, firmware validation, commit, confirm.

Network setup begins on the device AP. The UI displays scan results, associates
with an infrastructure WLAN, verifies reachability, and preserves a recovery
path. A multi-MCU workspace connects directly to every participant over the same
trusted WLAN/VPN, maintains independent credentials/clock fits, and never treats
one device's state as proof of another's.

Storage UI supports resumable content-addressed upload, integrity progress,
capacity/health, immutable manifests, per-MCU partition state, audit download,
and idle-only deletion. Raw source geometry remains browser/project data; only
compiled job packages become executable device cache entries.

Implementation checkpoint: the headless/WASM client now performs exact
origin-bound HMAC V2 fetches, validates the fixed boot challenge and signed
native responses, reconciles ambiguous resumable uploads through publication
inspection, and orders each participant partition before the shared global
manifest. The production WASM build exercises the current sibling CSGRS/Hyper
and Alumina schemas. Conservative boot-scoped heartbeat acquisition is now
available to both window and worker scopes. Credential and device-discovery UX,
live-browser simulated HTTP tests, panels, WLAN provisioning, and worker
lifecycle supervision remain open. See the
[M7 browser cache-delivery evidence](evidence/M7-BROWSER-CACHE-DELIVERY.md) and
[clock/coordinator evidence](evidence/M7-BROWSER-CLOCK-COORDINATOR.md).

Exit:

- TinyBee, T-Deck Pro, and simulator panels require no board-name branches.
- Stale board/config/job/boot digests block mutation with precise diagnostics.
- Device AP provisioning, WLAN transition/recovery, storage power-loss fixtures,
  and clock fitting work in browser integration tests.

## I3 — Authoritative exact CAM and job compiler

### Exact source path

- Retain CSGRS/Hypercurve/Hyperpath source facts, transforms, parameters, tool
  facts, and provenance as exact `Real` values.
- Add a UI-only CNC G-code importer for selected geometry semantics. Parse
  decimal tokens exactly, resolve modal state explicitly, and lift supported
  lines/arcs into Hypercurve/Hyperpath. Ambiguous or unsupported commands fail;
  imported G-code is never canonical or sent to firmware.
- Use Hyperpath path-wide length, acceleration, corner-lookahead, and jerk-ramp
  reports as the starting motion model. Extend the exact stack or a narrow CAM
  crate for machine-axis projection, process constraints, kinematics, and holds.
- Use Hypersolve's exact direct routes and proposal→exact/interval certification
  discipline for suitable constraint problems. Expose precision exhaustion and
  undecided results rather than silently accepting approximate solutions.

### Capability-driven precision

The compiler consumes the connected board and stored machine configuration:

- step angle/microsteps/current, mechanical transmission and calibration;
- encoder/count and sensor resolution;
- PWM/ADC clocks, phase/dead-time/current-sense and qualified control rates;
- timer tick/event rates, queue/block limits, and measured jitter;
- travel, velocity, acceleration, jerk, following/process/safety limits; and
- exact transforms plus bounded physical uncertainty.

Given a user/process tolerance, allocate a conservative budget among curve
projection, integer command lattice, timer quantization, calibration uncertainty,
and expected following/control error. Refine exact values only enough to certify
the requested policy. Report each component and the limiting hardware fact.

### Compilation artifacts

Emit:

- one canonical global manifest with source/compiler/policy/machine/participant
  digests, coordinate epoch, synchronization markers, and global safety policy;
- one immutable per-MCU stream containing only integer/fixed-point local work;
- a certificate/evidence report connecting exact path spans to reduced spans,
  integer events, constraints, and error budgets;
- simulator expectations and correlation IDs for later command/measurement
  telemetry; and
- a deterministic partition and byte representation.

V1 instructions are intentionally small: state, jerk/finite-difference or
coordinated integer segments, synchronization, bounded input condition, bounded
output action, and job boundary. Add native arc/Bezier execution only after its
integer interpolator carries a verified error and timing envelope.

Exit:

- The selected line/arc/Bezier contour compiles identically across repeated WASM
  runs and matches native test fixtures.
- Golden/pathological tests prove or conservatively bound CAD→reduced path,
  reduced path→command lattice, and timing error.
- Firmware never appears as an alternative CAM compiler in the UI.

Implementation checkpoint: a window-free compiler independently certifies the
representative source path into exact motion chords, then proves nearest
machine-step and cumulative timer rounding before emitting the sibling
`alumina-machine-ir` segment type. It retains a conservative
source-curve-to-canonical-command-chord bound and keeps physical following error
outside that claim. It now independently replays canonical firmware blocks,
publishes immutable local cache objects, and binds owned participant packages
into the shared sorted multi-MCU manifest with exact rational duration checks.
Capability-driven policy, full kinematics and lookahead, production-derived
identities, browser determinism tests, and authenticated Wi-Fi/cache/schedule
integration remain open. See the [M5/I1-I3
evidence](evidence/M5-EXACT-CAM-COMPILER.md) and [M5/M7 packaging
evidence](evidence/M7-GLOBAL-JOB-MANIFEST.md).

## I4 — Annotated board explorer, logic analyzer, and oscilloscope

The diagnostic experience is organized around the actual object in front of the
user, not a flat pin table.

### Physical view

- Store licensed, revision-specific board photographs, optional reverse/angled
  views, dimensions, image digest, photographer/source/license, and normalized
  hotspot polygons in the board package.
- Map every connector, terminal, silkscreen name, GPIO, virtual I²S bit, bus,
  fitted device, power domain, safety input, and important test point to stable
  resource/device/net IDs.
- Overlay live value, direction/mode, allocation owner, execution domain, safe
  value, armed/fault state, sample age/quality, qualification, warnings, and
  capture activity. Selecting either photo, graph node, config field, or plot
  highlights the same resource everywhere.
- Provide assembly mode with connector pinout, configured machine function,
  expected idle reading, wiring notes, and safe guided tests. Hazardous outputs
  remain lease-, range-, timeout-, and arm-policy constrained.

### Acquisition model

“All I/O live” has two honest layers:

1. A bounded lower-rate overview of every readable/commanded resource, with age
   and whether a value is measured, latched, inferred, or last-commanded.
2. Explicit device-side captures using RMT/PCNT/timers/ADC/DMA or qualified
   software sampling: selected channels, sample/edge rate, clock, depth,
   trigger, pre/post window, decimation, and overflow/quality flags.

Views include digital timing, multi-channel time plots, XY/phase, histograms,
FFT/spectrum and later Bode tools, state/event lanes, commanded/step/encoder
position, following error, current/voltage/torque, queue horizon, deadlines, and
clock-sync uncertainty. Use min/max envelope decimation for time-series fidelity;
preserve trigger/fault neighborhoods. Cursors and annotations correlate exact
path spans, job instructions, board resources, and measurements.

Experiment records bundle board/config/job/graph digests, clock models, capture
settings, commands, telemetry, annotations, and exports. Webcam snapshots may be
attached as user evidence but are not silently treated as calibrated metrology.

Exit:

- TinyBee and T-Deck photographs provide complete reviewed hotspot coverage for
  every advertised connector/resource.
- Overview and high-rate capture remain within explicit device, network, and
  browser memory budgets and retain fault evidence under load.
- Logic-analyzer step/direction/start captures align with firmware/simulator
  integer traces and displayed clock uncertainty.

## I5 — Wi-Fi multi-MCU coordinator

- Maintain one counter-unwrapping/affine clock fit per boot ID in a browser
  worker; visualize delay samples, drift, uncertainty, freshness, and the reason
  a device is not schedulable.
- Compile a global job into immutable participant partitions and show upload,
  hash, local validation, resource/config match, prefetch, and readiness for
  every MCU.
- Implement the prepare/choose-future-time/commit/confirm-or-abort sequence from
  `DISTRIBUTED-JOBS.md`, including idempotent retries, leases, point-of-no-return,
  and observed start-edge reconciliation.
- Make partial readiness or version/config mismatch impossible to overlook.
  Attended versus cached-autonomous network-loss policy is chosen explicitly.
- Show physical safety-chain coverage separately from Wi-Fi state. Never present
  a successful packet as an E-stop guarantee.

Implementation checkpoint: canonical participant packages, the identical
global manifest, and retry-safe authenticated delivery state now reach the
browser boundary. Conservative heartbeat acquisition works from a window or
worker scope, and the headless coordinator implements boot-bound
prepare/install/confirm-or-abort, precommit cancellation, exact deadline
classification, and read-only transition reconciliation. Worker
creation/supervision, live multi-device sessions, operator panels, and physical
qualification remain open.

Exit:

- Two simulated then two physical dual-core devices start harmless cached GPIO
  traces within a measured/predicted bound under normal and loaded Wi-Fi.
- Browser throttling, network loss, reboot, stale clocks, corrupt/full storage,
  and failed participants prevent start or produce the declared safe local state.

## I6 — Typed stateful graphical programming

### Document and types

Create a new versioned graph document; no old schema compatibility is needed. It
contains stable graph/node/port/wire IDs, registered node/schema versions, typed
parameters/defaults, units/shapes, placement/groups/comments, front panels,
subgraphs/components, deployment domains, clocks/rates, resource claims, and
canonical digests. Unknown future nodes round-trip as unresolved placeholders.

Value families include exact scalars/geometry, bounded measured values and
uncertainty, booleans/integers/strings/bytes, arrays/options/results/records/enums,
timestamps/durations/clocks, events, bounded streams/waveforms, and typed
resource/config/job/fault handles. Exact values and measured/display floats
require explicit conversion nodes.

### Execution semantics

- Pure nodes run when inputs are available and memoize exact values by identity.
- Cycles require explicit delay/state/feedback/queue/loop structures;
  combinational cycles are errors.
- Synchronous wires, events, and bounded streams have distinct ordering,
  capacity, overflow, timeout, stop, and backpressure semantics.
- Timed loops name clock, period/rate, phase/deadline, jitter, and overrun policy.
- Error and fault flow is typed and visible.
- Host nodes may allocate and use arbitrary precision. Service nodes use bounded
  async resources. Realtime nodes are whitelisted, fixed-memory, statically
  scheduled/budgeted opcodes beneath non-bypassable safety logic.

Core node families cover values/structures/units, arithmetic and DSP, exact
CAD/CSG/curve/CAM, state/control/trajectory, loops/cases/state machines, bounded
channels/events, components/front panels, probes/assertions/interlocks, and
capability-generated ESP32 resource/protocol/device operations.

The compiler performs type/unit/capability checking, resource allocation, domain
partitioning, state/cycle analysis, rate-transition validation, fixed memory and
queue analysis, WCET/deadline analysis, and safety-policy checks. It emits host
plans, optional service/RT graph IR, bridge definitions, deployment/config
digests, and static reports—not arbitrary Rust, JS, or WASM on firmware.

Exit:

- A multi-rate producer/consumer graph with bounded queue, filter/PID, state,
  safety limit, scheduled output, probes, and front panel simulates, deploys,
  captures, and replays deterministically.
- Errors identify the exact node/wire/resource/photo hotspot and explain units,
  capability, rate, memory, or safety mismatch.

## I7 — Usability, review, and ecosystem

- Searchable capability-aware palette, quick insert, keyboard wiring, alignment,
  groups/frames, minimap, scalable large-graph rendering, edit/run modes, and
  context help generated from schemas and board/device evidence.
- Reusable component packages with locked dependencies, signatures, permissions,
  resource declarations, review status, and reproducible experiment bundles.
- Graph/job/config diff and review, simulator and recorded board fixtures,
  one-click trace-to-source/resource correlation, and visible separation of
  simulated, connected, configured, prepared, armed, running, held, and faulted.
- Accessibility and usability review with machine assembly/configuration tasks,
  beginning with the original RepRap developer review and then a wider group.

## Interface-wide acceptance criteria

- Current CSGRS/Hyper versions compile natively and for WASM; Hypergraphics owns
  every GPU buffer and projection boundary.
- The browser is the sole shipped CAM authority; firmware accepts only native
  compiled work and performs bounded validation/execution.
- No compatibility shim, old endpoint, parallel geometry type, or renderer float
  reaches the new protocol or machine-job model.
- Every mutation is authenticated, capability/config/boot digest checked,
  bounded, safety supervised, and observable.
- Multi-device clocks and starts always display uncertainty and archived evidence.
- Graph RT deployment has fixed memory, explicit clocks, bounded queues, static
  resources/WCET, and non-bypassable firmware safety.
- Board overview, captures, and plots remain responsive and bounded while
  preserving fault/trigger evidence and physical resource identity.
