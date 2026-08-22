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
available to both window and worker scopes. The shipped browser now creates and
supervises an explicit module worker that owns independent HMAC sessions, clock
models, bounded histories, retry cadence, and redacted live diagnostic panels.
Authenticated browser/HTTP simulation now covers nominal traffic, response loss,
a finite outage, reboot, bounded delay, and conservative excessive-delay
rejection. The same worker now acquires immutable capabilities through bounded
digest-stable ranges, repeats the exact range after ambiguous loss, revalidates
the complete canonical document on both sides of schema v3, and exposes a
board-name-independent live explorer. Complete public device/security/machine-
membership discovery, broader network/storage fault injection, WLAN
provisioning, and physical connections remain open. See the
[M7 browser cache-delivery evidence](evidence/M7-BROWSER-CACHE-DELIVERY.md) and
[clock/coordinator evidence](evidence/M7-BROWSER-CLOCK-COORDINATOR.md), plus the
[authenticated browser/HTTP evidence](evidence/M7-BROWSER-AUTH-HTTP-SIM.md) and
[authenticated capability worker/UI
evidence](evidence/M10-AUTHENTICATED-CAPABILITY-WORKER-UI.md).

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
- Use Hyperpath path-wide length, exact forward/reverse acceleration
  reachability, corner limits, and jerk-ramp reports as the starting motion
  model. Extend the exact stack or a narrow CAM crate for machine-axis
  projection, process constraints, kinematics, and holds.
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

Implementation checkpoint: the window-free compiler and visible Machine/CAM
workspace now derive an exact two-axis profile, usable travel, complete
resolution/error budget, and timer/output lattice from canonical Configuration
V5 and board capabilities. Retained Hypercurve lines and explicit arcs enter
Hyperpath losslessly. A retained polynomial cubic now enters a distinct,
bounded source-to-motion compiler: exact degree-elevated-chord predicates and
Hypercurve de Casteljau splits produce an exact line path under the allocated
positional bound. Hyperpath now combines explicit caller, global, tangent, and
retained-radius ceilings in exact squared-speed forward/reverse passes, then
independently replays the result with Hypersolve. A bounded exact pass then
lowers each stop-separated positive component by uniform halving until all of
its spans have replayed monotonic jerk transitions. The current policy supplies
positive internal ceilings only to lossless source-line pairs with an exact G1
join; curvature-bearing joins, corners, reversals, and every cubic chord retain
zero ceilings. For an all-line route, exact retained unit-direction components
feed Hyperpath's arbitrary dense-axis affine velocity/acceleration/jerk
projection, with every span/axis row and bottleneck independently replayed.
Curved routes deliberately retain conservative direction-independent limits
and no affine projection. Phase selection reads the actual nodes: zero/zero
retains the four-phase rest-to-rest profile and positive spans consume
Hyperpath's retained two-phase transition. Caller-bounded interpolation is then
lowered to the sibling `alumina-machine-ir` type. Each exact ideal interval is
ceiled to the configured output quantum, and a caller-bounded rational-factor
search retains factor-one and immediate-predecessor failures before accepting
the smallest stream which passes unchanged production stepper preflight.
Immutable cache partitioning, independent event simulation, and canonical
`ALMEVD03` source, metric, approximation, planner, and lowering evidence replay
must all succeed transactionally. V3 independently commits exact caller policy,
every retained affine/lookahead/jerk certification row, timer-search decisions,
scheduled points/segments, and production preflight. Its policy-variation tests
prove that byte-identical machine output does not collapse distinct planner or
timer policy into one evidence identity.

An optional UI-only CNC adapter now parses a deliberately selected connected XY
line/explicit-IJ-arc subset directly into exact rationals and native Hypercurve
objects. Explicit units, plane, endpoint mode, and arc-centre mode are required;
every process, ambiguous, or unsupported word fails closed. Raw-source identity
and per-curve modal provenance remain separate from exact-geometry/job identity,
and the direct Hypercurve fixture remains the default. Equivalent text and a
comment-only variant reproduce identical geometry, partition, and evidence,
while process words and an out-of-travel path leave the prior workspace
unchanged. Quadratic/rational Bezier, spline/NURBS scheduling, native or
nonzero-feed cubic carriers, nonzero-radius blends, broader kinematics,
tool/work transforms, process constraints, automatic resource partitioning,
authenticated physical delivery, and repeatable release pinning remain open.
See the [initial M5/I1-I3 evidence](evidence/M5-EXACT-CAM-COMPILER.md),
[exact scheduling evidence](evidence/M10-EXACT-SCHEDULE-PREFLIGHT.md),
[certified cubic-motion evidence](evidence/M10-CERTIFIED-CUBIC-MOTION.md),
[exact two-pass evidence](evidence/M10-EXACT-TWO-PASS-LOOKAHEAD.md),
[exact monotonic-jerk evidence](evidence/M10-EXACT-MONOTONIC-JERK.md),
[exact jerk-feasible G1 evidence](evidence/M10-EXACT-JERK-FEASIBLE-G1.md),
[exact affine-axis projection
evidence](evidence/M10-EXACT-AFFINE-AXIS-PROJECTION.md),
[exact timer-lattice
evidence](evidence/M10-EXACT-TIMER-LATTICE-HEADROOM.md),
[canonical planner/lowering V3
evidence](evidence/M10-CANONICAL-PLANNER-EVIDENCE-V3.md),
[shared-MCU exact retiming
evidence](evidence/M10-SHARED-MCU-TIMER-RETIMING.md),
[selected CNC import evidence](evidence/M5-UI-CNC-GEOMETRY-IMPORT.md), and
[M5/M7 packaging evidence](evidence/M7-GLOBAL-JOB-MANIFEST.md).

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

Implementation checkpoint: `ALMOVW01` and `ALMDIG01` provide the first bounded
allocation-free overview and triggered digital-edge record boundary. Canonical
authenticated subscription/configuration/status/event/chunk/range bodies now
bind complete device/boot/capability/config/clock context and exact SHA-256
identities. The fixed core-0 owner and typed client prove idempotent mutation,
latest-only loss accounting, retained-record recovery, ambiguous-response retry,
and complete validation through in-memory and localhost HTTP/HMAC simulations.
The production browser worker now reconciles public device identity with signed
capability/boot/clock facts, selects only capability-admitted stable Boolean
inputs, explicitly requests diagnostic arm, downloads exact retained ranges,
and revalidates the complete record in the rendering realm. The live panel
cross-links the resulting four-lane deterministic TinyBee trace to the same
resource aliases and integer cycle cursor. The standalone host simulator opts
into a dynamic immediate-capture provider. TinyBee now compile-composes a
bounded semantic overview provider over its four configured core-1 input slots;
its capture provider remains `Unsupported`, and T-Deck Pro/MKS ESP32 FOC retain
no provider storage. This remains simulator evidence for capture and
compile/link evidence for target overview, with no physical acquisition,
lease, machine arm, command, or output authority. Live WebSocket delivery,
analog waveforms, physical analyzer comparison, and overload HIL remain open. See
[`capability-bound waveform worker/UI`](evidence/M10-CAPABILITY-BOUND-WAVEFORM-WORKER-UI.md)
and [`TinyBee real-time input telemetry`](evidence/M10-TINYBEE-REALTIME-INPUT-TELEMETRY.md).

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
  Attended versus cached-autonomous network-loss policy is chosen explicitly;
  schema V10 carries the manifest policy into validated snapshots, and exact
  configuration bit 1 gates autonomous staging and both target-core admissions.
- Show physical safety-chain coverage separately from Wi-Fi state. Never present
  a successful packet as an E-stop guarantee.

Implementation checkpoint: canonical participant packages, the identical
global manifest, and retry-safe authenticated delivery state now reach the
browser boundary. Schedule-derived packages now pass one jointly minimal exact
same-grid timer-factor search before publication: every candidate production-
replays every MCU, selected streams replay against exact point carriers, and
`ALMSYN01` binds derivations, search outcomes, final IR, and partition
identities before the compiler derives manifest duration/synchronization facts.
Mixed clock/event grids still fail closed. Conservative heartbeat acquisition
works from a window or worker scope, and the headless coordinator implements
boot-bound prepare/install/confirm-or-abort, precommit cancellation, exact deadline
classification, and read-only transition reconciliation. Worker
creation/supervision, live multi-device clock-session ownership, redacted
history panels, and authenticated Chromium-to-host-MCU HTTP clock tests are now
implemented. Those tests cover response loss, a finite outage, reboot, bounded
delay, and conservative excessive-delay rejection. The headless coordinator now
also consumes immutable authenticated first-output observations, exactly
inverts their boot-scoped cycle intervals into browser monotonic time, preserves
their simulator/peripheral/software authority, and displays conservative
participant spread and shared-epoch error. Its two-device simulation proves
known edges are contained and rejects missing or regressed evidence. Live
cache/job driving and the complete worker lifecycle now carry two consecutive
attempts on two unchanged simulator boots: the rendering realm retains the first
terminal evidence, clears its bounded owner, then stages a distinct prepare ID
which reuses the immutable cache and selects a new future epoch. See the
[repeated cached-job evidence](evidence/M10-REPEATED-CACHED-JOBS.md).
Operation-specific successful-response loss now qualifies live browser
reconciliation after one applied storage chunk and one applied schedule commit,
followed by a clean no-fault run. See the [browser cached-job recovery
evidence](evidence/M10-BROWSER-CACHED-JOB-RECOVERY.md).
Successful response loss after applied pre-guard aborts on both participants is
qualified from installed and confirmed states with mandatory
status-before-next-mutation progression; see the
[browser abort-recovery
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md). The separate
pre-application selector now qualifies one-shot initial abort-request loss from
confirmed state: authenticated status observes unchanged authority before the
exact mutation is retried. See the [browser abort-request recovery
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-REQUEST-RECOVERY.md).
A bounded repeated loss of that abort mutation is now qualified from confirmed
state through the guard while status reads remain available. No abort is
applied; current worker schema V10 retains the accepted stop and terminates as
`completed_after_stop_request` after exact all-participant completion. See the
[browser abort-guard outage
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-GUARD-OUTAGE.md). A second
qualification lets one actor apply abort while the other loses abort mutations
through its guard. Current schema V10 retains the exact `aborted`/`complete`
split as terminal `split_after_stop_request`; see the [browser abort-split outage
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-SPLIT-OUTAGE.md).
An installation-phase stop now uses emitted commit authority rather than the
browser's complete planned set: it aborts an installed actor, cancels a ready
never-installed actor, retains the exact cycle/null distinction, and then
completes a second job on the same sessions and boots. See the [browser
installing-stop
evidence](evidence/M10-BROWSER-CACHED-JOB-INSTALLING-STOP.md).
A one-shot post-application duplicate is now qualified separately. Each actor
applies one authenticated `JobAbort`, rejects its byte-identical replay with
HTTP 401 before second native dispatch, and the worker preserves ambiguity
while reopening authenticated session authority before status proves the exact
terminal state. See the [browser abort-duplicate
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-DUPLICATE.md).
A bounded complete schedule-operation outage is now qualified after global
confirmation. Both actors discard exactly 24 schedule requests before
application while unrelated diagnostic traffic remains live. The worker
retains last-authoritative facts and requires a wholly successful
all-participant status sweep before any later schedule mutation; completion is
accepted only from exact local reports. See the [browser abort/status-outage
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-STATUS-OUTAGE.md). A one-shot
stale-response substitution is now qualified independently. Each simulator
retains a valid signed confirmation response, applies the next schedule
operation, and returns the old response; exact counter mismatch handling rejects
it without consuming pending state. The worker then spends the ambiguous
request, reopens session authority, and requires another whole-participant
status sweep. The run covers substitution for both `JobAbort` and the
reconciliation `JobStatus`; see the [browser abort/stale-response
evidence](evidence/M10-BROWSER-CACHED-JOB-ABORT-STALE-RESPONSE.md). Cached-job
fetch ownership is serial, so arbitrary concurrent packet or response
reordering is not claimed. One named post-confirmation safety-fault expectation
now retains a canonical local `SafetyStop`, skips a repeat abort to that
terminal actor, aborts the remaining confirmed peer, and admits only exact
global `faulted` with `faulted`/`aborted` participants; see the [browser
confirmed safety-fault
evidence](evidence/M10-BROWSER-CACHED-JOB-CONFIRMED-SAFETY-FAULT.md). The
safe-output transaction is modeled, so no physical safety claim follows.
The named automatic-propagation expectation now sends no operator stop command:
an ordinary authenticated status report exposes the same modeled fault, the
coordinator aborts the remaining confirmed peer, and the browser admits only
zero-error global `faulted` with exact `faulted`/`aborted` participants. See the
[automatic safety-fault propagation
evidence](evidence/M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION.md).
The paired recovery expectation drops the remaining peer's successful applied
abort response, retains exact `faulted`/`confirmed` ambiguity, and requires an
authenticated read-only status report before terminal `faulted`/`aborted`. See
the [automatic safety-fault propagation recovery
evidence](evidence/M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION-RECOVERY.md).
The request-recovery expectation instead drops the peer's first abort before
authentication/application, requires status to retain `confirmed`, and permits
`aborted` only after the exact retry. See the [automatic safety-fault
propagation request-recovery
evidence](evidence/M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION-REQUEST-RECOVERY.md).
Attended-policy controls, indefinite schedule or total endpoint outage,
authentication/bootstrap loss, broader reordering/substitution, duplication
outside the exact one-shot cases, other fault families or terminal mixtures,
broader network/storage faults,
background-throttling qualification, and physical qualification remain open.
Fresh-owner same-attempt terminal reattachment now
uses an all-participant read-only status round and exact retained descriptor
tokens; it exposes `retained_complete`, preserves local start cycles, carries no
old UI epoch, and grants no new start authority. See the [browser cached-job
reattachment evidence](evidence/M10-BROWSER-CACHED-JOB-REATTACHMENT.md).
Successful confirmation-response loss on both participants is qualified in the
[browser confirmation-recovery
evidence](evidence/M10-BROWSER-CACHED-JOB-CONFIRM-RECOVERY.md). Observed-edge authority
is recorded separately in the
[observed-start replay evidence](evidence/M7-OBSERVED-START-REPLAY.md).

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

Implementation checkpoint: the window-free core now retains bounded exact
units, rational/interval/canonical/composite values, runtime event/stream types,
resource/job handles, explicit clocks and execution domains, opaque versioned
nodes, typed parameters/ports/wires, and a canonical SHA-256-identified `ALGR`
V1 document. Replay enforces caller-owned admission limits and exact
decode/re-encode equality. A context-bound audited-node registry now adds exact
shape/domain admission, complete feedthrough, explicit read-before-write state,
declared-state bounds, and iterative port-level cycle witnesses. It also proves
maximum canonical bytes for every literal/runtime payload and
rejects undersized state declarations. Audited inputs now distinguish required
and optional synchronous slots from bounded Event/Stream queues, retain an
explicit full policy, reject synchronous cross-domain sharing, and report
checked per-input/aggregate canonical memory ceilings. Audited cross-clock
Stream dependencies now use exact rational clock resolution and one explicit
latest-at-or-before transition, with shared-root, smallest-pattern,
minimum-queue, and held-sample proofs. A separate fixed HostExact registry now
simulates external Stream sources, that transition, Stream sinks, exact
arithmetic/clamping, explicit unit delays, and fail-safe permit gating in exact
root time; canonical traces replay by independently regenerating every byte. A
shared fallible PID/interlock fixture now feeds tests and the native/WASM
control workspace. Its separate canonical `ALGW` envelope embeds the unchanged
graph, presentation-only integer placement, monotonic identity cursors, and
workspace revision. Its bounded semantic layout exposes typed ports, exact
parameters, explicit state/feedback, and exact cursor values behind certified
display projections. Placement and typed wire connect/disconnect edits are
transactional; topology edits detach the graph-bound reference trace, while
invalid/cyclic candidates preserve the prior draft. The third registry now
binds complete audited semantics to fixed implementation, domain, clock, and
WCET descriptors. It lowers the initial Boolean
Service-to-Realtime subset into an independently replayed 4 KiB `ALGRIR02`
package with integer device-cycle schedules and fixed state/channel/bridge
arenas. Production lowering now derives those arenas and the exact opcode and
typed resource/class/access palettes from the authenticated target capability
document. A portable firmware runtime transactionally admits those exact
bytes/identities and every node capability into const-generic storage, primes
Service tick zero, splits unique Service/Realtime owners, executes five fixed
opcodes at exact release cycles, and shares only the bounded bridge and
first-cause fault latch. The resource opcodes can read only known, fresh,
debounced TinyBee safety-input semantics; the paired form reads both ordered
selectors every release, and either form fails closed when unavailable.
The headless browser/WASM client now publishes that package through the
resumable SD cache and drives authenticated install/status/activate operations.
Firmware independently replays it on core 0 and core 1, retains distinct
candidate/active images, and withholds active bytes until both cores authorize
the exact identities. Permanent pinned-core actors now execute exact future run
epochs, and a power-cut-tested selector journal recovers the committed package
through configuration-first independent boot admission. General node/state/Event
execution, measured executor timing, physical input HIL, graph outputs, broader
resource opcodes, child-occurrence rebinding, front-panel runtime inputs, live
device telemetry,
and device-trigger capture plots remain open. Replay-only probe triggers are
separately implemented below. The canonical placement/wiring surface and a
13-kind audited palette now support node create/delete and exact scalar
parameter replacement.
Bounded complete canonical `ALGS` snapshots now provide replay-backed
undo/redo across graph, probe, trigger, sidecar-import, cached-job, selected
component, component-library, direct root-instance, hierarchy, and source-map
state. Navigation replays every nested artifact, then reruns UI semantic and cached-job catalog
admission before changing either authoring state or history stacks.
Origin-local browser persistence now stores one canonical bounded `ALGS` V1
over the current control ALGW, bound ALGP,
catalog-bound cached-job ALGW, selected ALGC, complete ALGH, and regenerated
ALGM. It commits no restored state until every nested replay, cross-artifact
identity, audited graph, and catalog-membership check succeeds. The retired
three-section value is not a compatibility input. Native/browser `.algs`,
`.algw`, `.algp`, `.algc`, and `.algm` exchange crosses the corresponding
bounded replay boundary; ALGP import can change only the sidecar bound to the
current workspace.
A separate bounded canonical `ALGC` envelope now embeds the unchanged workspace,
maps public connector terminals to exact internal endpoints, and binds integer
front-panel controls/indicators to public terminals or retained exact
parameters. The first visible PID/interlock component has eight exact controls
and seven replay-only indicators: four exact-rational values and three Boolean
interlock states. Invalidating a binding detaches the panel
without weakening the workspace draft. Canonical `ALGH` V2 now binds root and
component-scoped instances to exact `ALGC` digests, derives each typed collapsed
port shape, rejects dependency cycles, bounds depth and expanded occurrences,
and recursively flattens the dependency DAG into an ordinary workspace with
fresh monotonic node/wire identities. The visible proof expands a wrapper and
its PID leaf at depth two to 21 audited nodes and 25 wires, retaining stable
source paths `[1]` and `[1, 1]`. Broader identity-bearing parameter editors,
parameter promotion/overrides, runtime panel value injection/execution, and
responsive/grouped layout policies remain open. A dedicated library panel now
selects exact dependencies admitted by `ALGH`, exports the selected canonical
`ALGC`, imports a bounded semantically audited standalone leaf, removes only an
unreferenced non-authoritative dependency, adds dependencies as root
occurrences with monotonic root node identities, and deletes selected root
occurrences plus incident root wires. It can also construct a named version-1
empty `ALGC` from the current control schema/clocks with all node, wire,
connector, and panel identity cursors initialized to one, then select it
immediately for definition editing. Invalid, overlong, or
conflicting names reject atomically; requesting the same canonical empty
component is a selection-only no-op. Exact duplicate import is likewise a
no-op. The UI regenerates the complete `ALGH`/flattened `ALGW`/`ALGM` branch,
reruns semantic admission, and commits
through unified complete-`ALGS` history and persistence only after every check
passes. Compatible selected-component edits remap old digest bindings while
preserving root state and unrelated dependency encodings. The same exact
selector now drives a separate structural canvas for one non-authoritative
dependency's embedded `ALGW`. It supports the audited palette, monotonic node
and typed-wire editing, exact metadata and integer placement, recursive digest
selection, and complete-session history/persistence. `ALGH`-owned child
placeholders remain visible and wireable but cannot be deleted through the
ordinary-node surface, and the selected control authority remains read-only.
An existing library dependency can now be instantiated inside that selected
definition: one candidate allocates the parent-local placeholder and adds its
scoped `ALGH` binding, while the dedicated delete action removes that
placeholder, incident wires, and binding together. Public connector/panel
bindings veto removal; cycles and indirect control-authority replacement reject
before commit; deletion never rewinds the parent node cursor. The selected
placeholder can instead be rebound to another exact dependency already in the
library. Stable child connector IDs preserve compatible parent-local wires and
public endpoints while node identity, label, placement, and allocation cursors
remain exact. Same-shape replacement changes only the scoped `ALGH` binding at
the parent boundary; a public-shape change recursively replaces affected
parent identities. Both paths freshly flatten and regenerate `ALGM`; selecting
the already bound child is an exact no-op, and missing live connectors, cycles,
limits, or an indirect control-authority rewrite reject atomically. The selected
dependency's stable name and declared behavior version can now be changed as
one canonical transaction. Versions use canonical nonzero decimal `u32` text
and may stay unchanged or increase, never regress; stable-name collisions and
invalid metadata reject, while exact no-ops return before history. Accepted
metadata evolution uses the complete recursive replacement report to rewrite
all affected digest
bindings, regenerate `ALGH`/`ALGM`, preserve unchanged authority exactly, and
clear stale source focus without retaining an alias. Nested binding
import/exchange, parameter
promotion/overrides, coordinated descendant/control-authority replacement,
and collaboration/conflict handling remain open.
A separate canonical `ALGM` V1 sidecar now maps every final node and wire
to exactly one root or component-occurrence origin and binds both the complete
source `ALGH` and flattened `ALGW`. Its bounded replay freshly flattens the
caller-supplied hierarchy and regenerates every byte before returning
provenance. The selected-node inspector and exact trace cursor expose stable
occurrence-to-final endpoint correlation, while `.algm` import/export remains
non-mutating UI audit metadata with no firmware authority. A visible
flattened-source browser now lists every final node and wire with that exact
origin. Opening one rechecks the current final-to-origin mapping and resolves
component paths through current scoped `ALGH` bindings before selecting the
root, private-library, or authoritative-control source. Component wires select
and distinctly highlight their exact local wire while retaining the target
node in the inspector. Destination selection, highlighting, and one-shot
scrolling are transient; they do not change `ALGS`, history, or persistence,
and stale remapped origins clear. A capability-derived
catalog now intersects the complete
caller-authenticated graph-executor document with reviewed deployment bindings
and constructs only the four TinyBee stable Boolean input handles. A separate
offline Realtime draft consumes two selected handles in one reviewed
conjunction node, feeds a required sink, and re-lowers every accepted selector
edit into independently decoded fixed firmware bytes. A public bounded
native/WASM replay boundary now
installs those exact bytes independently into the same portable fixed-memory
Service and Realtime actor types used by firmware, binds the target's static
opcode/resource palette, performs the actor lifecycle, and retains actual
provider-call order plus completed reports or the first terminal fault. Its
complete bounded canonical `ALGRREP1` artifact commits the exact target,
implementation, package, run, limits, canonicalized caller inputs, actual
reads, and outcomes. Raw import checks the hard byte ceiling before hashing,
admits exact identities/canonical inputs under caller-narrowed limits, rebuilds
fresh firmware actors, and requires every generated byte to match. The UI
exports the current success/fault `.algrrep` files and imports them only as a
non-mutating verification operation.
The visible TinyBee transaction commits neither ALGW nor ALGR until four
conjunction releases and one unavailable-input fault replay match exactly.
This is offline evidence and grants no session, GPIO, deployment, start, or
safety authority. ADC, UART, timer, shifted output, storage, raw GPIO, and all
other unadmitted access remain closed.
Canonical `ALGP` V2 sidecars now bind bounded probes to exact workspace outputs
and retain one Boolean-stream rising, falling, or either-edge trigger with a
bounded pre/post retained-sample window. Trigger edits are transactional,
removing the source atomically clears the trigger, and exact resolution uses a
bounded waiting ring after the declared stride. It reports graph clock,
tick/sequence, available window endpoints/counts, and completeness only after
the simulation graph identity matches the bound workspace.
Probe and trigger identity changes now participate in browser persistence,
while canonical no-op edits do not cause redundant writes. A malformed stored
sidecar or one bound to another valid ALGW rejects the complete pair rather
than partially restoring the graph. A graph edit that removes an observed
endpoint visibly installs an empty canonical sidecar bound to the revised
workspace instead of retaining unbound probe intent. Canonical probe names,
host-retention ceilings, and event-decimation strides are now editable with
stable probe/source/type identity. Complete candidate validation rejects
duplicate/malformed names, invalid bounds, and any active trigger window that
no longer fits; exact reapplication is a no-op.
The reference plot selects four exact-rational outputs and the independently
resampled external permit, measurement-range predicate, and conjunction. It
displays certified analog enclosures and causally ordered high/low lanes in the
exact probe-5 falling-edge window from ticks 1–5, with the trigger and cursor at
tick 3. No live telemetry, physical acquisition, or device-trigger authority is
implied.
The complete capability document now also has a bounded
allocation-free descriptive decoder and a board-name-independent owned explorer
model. The visible TinyBee view exposes all 62 typed resources, 51 aliases,
owners, safe states, hazards and supporting-section counts while keeping its
four graph-readable inputs visibly narrower. Searchable graph-closed/hazardous
views grant no operation. No photo or hotspot is drawn because the exact package
publishes no licensed visual; its physical-reconciliation HIL gate remains
open. The same explorer is now admitted from the live worker's complete
authenticated capability document rather than only the offline reference
fixture. Capability-bound one-shot digital capture is now live only through
the opt-in host simulator; live telemetry, physical acquisition, and
annotated-photo rendering remain separate open gates. See the
[`canonical document`](evidence/M9-CANONICAL-GRAPH-DOCUMENT-V1.md) and
[`audited semantic`](evidence/M9-AUDITED-GRAPH-SEMANTICS.md), plus the
[`type-storage`](evidence/M9-CANONICAL-TYPE-STORAGE.md) and
[`bounded-channel`](evidence/M9-BOUNDED-GRAPH-CHANNELS.md), and
[`exact-rate`](evidence/M9-EXACT-GRAPH-RATES.md) and
[`deterministic-simulation`](evidence/M9-DETERMINISTIC-GRAPH-SIMULATION.md), and
[`mixed-signal control trace`](evidence/M9-MIXED-SIGNAL-CONTROL-TRACE.md), and
[`interlock cause trace`](evidence/M9-INTERLOCK-CAUSE-TRACE.md), and
[`exact replay probe trigger`](evidence/M9-EXACT-REPLAY-PROBE-TRIGGER.md), and
[`exact graph/probe pair persistence`](evidence/M9-EXACT-GRAPH-PROBE-PAIR-PERSISTENCE.md), and
[`exact graph/probe pair history`](evidence/M9-EXACT-GRAPH-PROBE-PAIR-HISTORY.md), and
[`bounded graph-probe metadata`](evidence/M9-BOUNDED-GRAPH-PROBE-METADATA.md), and
[`bounded graph-probe projection`](evidence/M9-BOUNDED-GRAPH-PROBE-PROJECTION.md), and
[`exact multi-rate probe axis`](evidence/M9-EXACT-MULTIRATE-PROBE-AXIS.md), and
[`exact-control inspector`](evidence/M9-EXACT-CONTROL-INSPECTOR.md), and
[`canonical graph workspace`](evidence/M9-CANONICAL-GRAPH-WORKSPACE.md), and
[`graph palette/parameters`](evidence/M9-GRAPH-PALETTE-PARAMETERS.md), and
[`graph history/persistence`](evidence/M9-GRAPH-WORKSPACE-HISTORY-PERSISTENCE.md),
and
[`graph components/front panels`](evidence/M9-GRAPH-COMPONENT-FRONT-PANEL.md), and
[`graph hierarchy flattening`](evidence/M9-GRAPH-HIERARCHY-FLATTENING.md), and
[`bounded recursive graph hierarchy`](evidence/M9-BOUNDED-RECURSIVE-GRAPH-HIERARCHY.md), and
[`exact hierarchy source map`](evidence/M9-EXACT-HIERARCHY-SOURCE-MAP.md), and
[`canonical graph authoring session`](evidence/M9-CANONICAL-GRAPH-AUTHORING-SESSION.md), and
[`unified graph authoring history`](evidence/M9-UNIFIED-GRAPH-AUTHORING-HISTORY.md), and
[`direct root-instance authoring`](evidence/M9-DIRECT-ROOT-INSTANCE-AUTHORING.md), and
[`exact component-library exchange`](evidence/M9-EXACT-COMPONENT-LIBRARY-EXCHANGE.md), and
[`selected-component definition canvas`](evidence/M9-SELECTED-COMPONENT-DEFINITION-CANVAS.md), and
[`scoped child-occurrence authoring`](evidence/M9-SCOPED-CHILD-OCCURRENCE-AUTHORING.md), and
[`exact scoped child-occurrence rebinding`](evidence/M9-EXACT-SCOPED-CHILD-OCCURRENCE-REBINDING.md), and
[`exact flattened-source navigation`](evidence/M9-EXACT-FLATTENED-SOURCE-NAVIGATION.md), and
[`exact component-wire focus`](evidence/M9-EXACT-COMPONENT-WIRE-FOCUS.md), and
[`exact component-identity evolution`](evidence/M9-EXACT-COMPONENT-IDENTITY-EVOLUTION.md), and
[`general component-library creation`](evidence/M9-GENERAL-COMPONENT-LIBRARY-CREATION.md), and
[`capability catalog/diagnostic probes`](evidence/M9-CAPABILITY-CATALOG-DIAGNOSTIC-PROBES.md),
and [`board capability explorer`](evidence/M9-BOARD-CAPABILITY-EXPLORER.md), and
[`authenticated diagnostic transport`](evidence/M9-AUTHENTICATED-DIAGNOSTIC-TRANSPORT.md), and
[`fixed graph-IR`](evidence/M9-FIXED-GRAPH-IR.md) and
[`portable graph-runtime`](evidence/M9-FIXED-GRAPH-RUNTIME.md), and
[`authenticated deployment`](evidence/M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md)
and [`durable selection`](evidence/M9-DURABLE-GRAPH-SELECTION.md), plus
[`capability-bound input`](evidence/M9-CAPABILITY-BOUND-GRAPH-INPUT.md),
[`executable composite input`](evidence/M9-EXECUTABLE-COMPOSITE-RESOURCE-CONSUMER.md),
and
[`browser firmware-actor replay`](evidence/M9-BROWSER-FIRMWARE-ACTOR-REPLAY.md)
and
[`canonical graph replay evidence`](evidence/M9-CANONICAL-GRAPH-REPLAY-EVIDENCE.md)
evidence.

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
