# Alumina interface roadmap

## Outcome

Evolve `alumina-interface` into four coherent layers:

1. an exact CAD/CAM authoring environment built on current CSGRS and Hyper crates;
2. a Hypergraphics visualization layer with one checked exact-to-GPU boundary;
3. a capability-driven firmware configuration, control, and diagnostics client;
4. a typed graphical dataflow environment for host, service-core, and real-time
   execution, with serious streaming plots and replay.

This should be a staged migration inside the existing interface repository, not
a big-bang UI rewrite. Each stage must keep the app buildable and produce a
serialized schema migration where persistent data changes.

## Stage I0 — Baseline and dependency alignment

Work:

- Record current WASM/native builds, screenshots, representative graph behavior,
  and existing firmware endpoint fixtures.
- Replace CSGRS 0.20.1 and obsolete feature flags with the current local CSGRS
  0.23.0 plus compatible `hyperreal`, `hyperlattice`, `hyperlimit`, `hypertri`,
  `hypermesh`, `hypercurve`, and `hypergraphics` revisions.
- Pin the compatible set in one workspace/dependency policy. Do not upgrade egui,
  wgpu, CSGRS, and the entire Hyper stack in an inseparable commit.
- Add CI for native tests/check, wasm check, Trunk production build, and a small
  headless serialization/migration suite.
- Define feature boundaries so geometry/editor tests do not require a GPU or live
  device.

Exit:

- Current UI builds against the pinned toolchain with temporary adapters clearly
  marked; baseline fixtures remain viewable.

## Stage I1 — Current exact geometry API

### Type migrations

| Current interface concept | Target |
| --- | --- |
| `csgrs::mesh::Mesh<()>` | `csgrs::TriangleMesh` / native Hypermesh type |
| `csgrs::sketch::Sketch<()>` | `CurveRegion2`, `CurvePath2`, or an explicitly open curve type |
| legacy CSG trait calls | current `solid` functions and `SolidExt` checked operations |
| geometry-domain `f64` scalar | exact `hyperreal::Real`; measured values use unit-tagged numeric types |
| `geo::LineString` bridge | native Hypercurve/CSGRS curve carrier |
| ad hoc transform matrices | exact Hyperlattice transforms for modeling/CAM |

Use current operations such as exact primitives, `try_union`, `try_difference`,
`try_intersection`, `try_transform`, extrusion/revolution/sweep, exact bounds,
and native-triangle visitation. Propagate errors through node diagnostics instead
of substituting an empty mesh or renderer tolerance.

### Graph value split

Do not expand the current `DValue` enum indefinitely. Introduce schema-stable
families:

- exact scalars, rational values, points, vectors, matrices, intervals/limits;
- exact open paths, closed paths/regions, triangle meshes, solids, and toolpaths;
- unit-bearing measured/commanded scalars and vectors;
- booleans, integers, strings, bytes, arrays, options, results, records, and
  enums;
- timestamps, durations, clocks, events, bounded streams, and waveforms;
- firmware resources, device handles, configuration handles, jobs, and faults;
- opaque versioned extension values only with a registered codec/migration.

Exact CAD values and runtime measurements should not share a bare `f64` variant.
The type checker must force an explicit quantize/measure/convert node.

Exit:

- Representative solid and curve graphs evaluate with current exact types.
- Old graph fixtures either migrate deterministically or report a precise manual
  migration requirement.
- No old CSGRS type or feature remains in the manifest/source.

## Stage I2 — Hypergraphics owns visualization

The interface currently calls a Hypergraphics backend but still implements much
of a renderer. Complete the boundary rather than merely renaming it.

### Hypergraphics additions

Add or expose, preferably in Hypergraphics with narrowly optional dependencies:

- checked adapters from `TriangleMesh`/Hypermesh to `ExactMesh`;
- scene instances/transforms and material/style records;
- exact line/path/region overlays and triangulated face visualization;
- edge/wireframe, vertex markers, normals, selections, probes, and annotations;
- exact or semantically typed grid, axes, work envelope, origin, and toolpath
  primitives;
- `ExactCamera`, viewport, clipping, projection, picking rays, and screen/world
  conversion;
- backend-owned staging/caching keyed by exact-geometry/style revisions; and
- structured conversion failures for non-finite/out-of-range GPU values.

If adding CSGRS directly to Hypergraphics would create an undesirable dependency,
put a small `alumina-hypergraphics-adapter` crate between native exact types and
Hypergraphics. Conversion must still be centralized and tested once.

### Interface removals

Remove interface-owned:

- interleaved vertex/normal/index buffer construction;
- manual normal and edge extraction already representable by the scene API;
- icosahedron spheres used to draw every point;
- hand-built grids, axes, work envelope, and selection geometry;
- hand-maintained model/view/projection multiplication; and
- redundant `nalgebra`/`geo` conversions where they exist only for rendering.

Rendering floats are allowed only after `ExactCamera` and exact scene geometry
reach Hypergraphics’ checked projection/backend boundary. Picking returns a
checked exact/semantic selection or explicitly bounded approximation; it never
feeds a float rendering position directly into CAM.

Exit:

- `alumina-interface` contains scene composition and interaction code, not a
  mesh renderer.
- Visual regressions cover solids, curves, grid/axes, edges, normals, selection,
  clipping, and extreme exact coordinates.
- GPU conversion errors are visible diagnostics, never silent NaNs.

## Stage I3 — Versioned capability-driven firmware client

Replace board assumptions and endpoint-specific UI with one client model shared
by control, graph nodes, and plots.

### Connection lifecycle

1. Fetch identity, protocol versions, security state, and capability digest.
2. Negotiate a compatible protocol/schema version.
3. Fetch or retrieve cached capabilities by digest.
4. Fetch active configuration/state and reconcile it with local drafts.
5. Open one binary telemetry/command WebSocket with credits/backpressure.
6. Synchronize the device monotonic clock and maintain uncertainty.
7. Restore subscriptions at bounded rates; never default to “all channels at
   maximum rate.”

### UI model

Generate panels and node palette entries from capabilities:

- resources grouped by physical/virtual namespace and ownership domain;
- supported modes, units, ranges, update/sample rates, frequency/resolution
  tradeoffs, aliases, and electrical/safety warnings;
- allocation conflicts and alternative mappings;
- clock domains and scheduling accuracy;
- safe state, current owner, armed/running/fault state, and watchdog duration;
- qualification/support level for the selected board capability.

Configuration editing is transactional: draft, validate locally, request firmware
validation, display structured diagnostics, commit, and confirm matching digest.
Direct “toggle pin 17” controls exist only in a safe diagnostics mode and still
use leases, capability checks, and output timeouts.

### Compatibility

Keep a small old-firmware adapter for `/device`, `/pins`, `/queue`, and `/time`.
It exposes a visibly limited legacy capability set and cannot emulate scheduling,
safe resource leasing, or exact machine jobs. Remove it after the agreed window.

Exit:

- No board name appears in ordinary control-panel logic.
- One simulated capability fixture can generate UI for GPIO, I²S output, serial,
  timer/PWM, axis, and telemetry resources.
- Stale config/capability digests block mutation with a clear reconciliation flow.

## Stage I4 — Typed stateful dataflow core

The current recursive CAD evaluator remains useful for pure subgraphs, but a
LabVIEW-like environment needs explicit semantics for time, state, concurrency,
and failure.

### Graph document

Define a versioned document containing:

- stable graph/node/port/wire IDs independent of visual ordering;
- registered node kind plus node schema version;
- typed parameters and default values;
- port type/unit/shape and optional capability constraints;
- node placement, grouping, comments, front-panel controls, and probe state;
- subgraph/component definitions and instances;
- deployment domain and clock/sample-rate intent;
- resource claims and configuration digest constraints; and
- migrations from every released schema version.

Serialization must be canonical enough for digests and diff tooling while keeping
human-readable export available. Unknown node types round-trip as unresolved
placeholders rather than being deleted.

### Execution semantics

- A pure node runs when all required inputs are available and memoizes exact CAD
  values by content/revision identity.
- Cycles require explicit state, delay, feedback, queue, or loop structures; a
  combinational cycle is a compile error.
- Synchronous wires transfer values within one logical tick.
- Stream/channel wires declare capacity, overflow policy, ordering, stop, timeout,
  timestamp, and backpressure behavior.
- Event wires distinguish an edge from a sampled boolean level.
- Every timed loop names a clock, rate/period, phase/deadline, overrun policy, and
  permissible jitter.
- Parallel nodes without data dependencies may run concurrently in the host
  domain; firmware compilation uses a static or bounded deterministic schedule.
- Error/fault flow is typed and visible. It is not a hidden global exception.

### Core node families

- values, structures, arrays, enums, options/results, units, conversion;
- arithmetic, logic, comparison, filters, statistics, interpolation;
- exact CAD/CSG, curves, transforms, CAM, machine quantization;
- state, delay, integrator, PID/PID+feedforward, state-space, trajectory;
- For/While/Timed Loop, Case/Match, sequence/dependency, state machine;
- bounded queue, stream, latest-value channel, event, trigger, merge/split;
- subgraphs, reusable components, parameters, front-panel controls/indicators;
- probes, assertions, limits, safety interlocks, watchdog, fault latch;
- resource lease/configure/read/write/schedule nodes generated from firmware
  capabilities; and
- protocol nodes for UART/RS-485/Modbus, I²C, SPI, TWAI/CAN, USB, Ethernet,
  Wi-Fi/TCP/UDP/HTTP, BLE, ESP-NOW, LoRa, GPS, SD, and supported devices.

Do not ship all protocol implementations at once. The palette reflects only the
connected board/firmware’s advertised, qualified capabilities; unavailable node
kinds remain useful for simulation but fail deployment validation.

### Compilation/deployment domains

| Domain | Allowed characteristics | Examples |
| --- | --- | --- |
| `HostExact` | dynamic allocation, arbitrary precision, UI interaction | CAD/CSG, CAM, optimization, report generation |
| `Service` | bounded async I/O but not hard real-time | web requests, files, GPS/LoRa, logging, EPD, slow Modbus |
| `Realtime` | whitelisted opcodes, fixed state/queues, explicit clocks and WCET | ADC sampling, filters, PID, scheduled outputs, safety logic |

The graph compiler performs type/unit checking, capability/resource allocation,
domain partitioning, cycle/state analysis, rate-transition insertion or error,
memory budgeting, bounded queue analysis, and timing-budget analysis. It emits:

- host execution plan;
- optional service/realtime graph IR;
- cross-domain bridge definitions;
- resource/configuration transaction;
- static memory and timing report; and
- canonical graph/deployment/configuration digests.

The firmware IR is deliberately not arbitrary Rust, WASM, JavaScript, or an
unbounded bytecode VM. Each opcode has fixed validation, state size, domain,
execution bound, and failure semantics.

Exit:

- Pure CAD graphs and stateful control graphs coexist but cannot accidentally
  exchange an unquantized renderer float.
- A producer/consumer graph with two rates, a bounded channel, PID, safety limit,
  and scheduled output simulates, compiles, deploys, records, and replays.
- The editor explains broken types, units, capabilities, cycles, rates, memory,
  and deadlines at the offending wire/node.

## Stage I5 — Plotting, probes, and experiment workflow

Plots should be a first-class dataflow instrument rather than an ever-growing
vector of polled pin values.

### Data path

- Device telemetry frames contain channel ID, device-clock timestamp, sequence,
  quality/status flags, and typed values.
- Clock synchronization maintains a host/device mapping plus uncertainty.
- Per-channel fixed/ring storage has an explicit memory/time horizon.
- Rendering uses min/max envelope decimation for time-series fidelity; optional
  LTTB or domain-specific decimation is selected explicitly.
- Acquisition and rendering rates are independent. Hidden panels do not retain
  full-rate unbounded data.
- Fault/event edges and trigger neighborhoods are preserved even when ordinary
  samples are decimated.

### Views and tools

- multi-channel time plot with physical units and independent/linked axes;
- XY/phase, histogram, spectrum/FFT, Bode/frequency-response, state/event, and
  digital-logic views;
- edge/level/window/software triggers with pre/post-trigger capture;
- cursors, deltas, statistics, annotations, and exact job/path correlation;
- commanded position, integer step position, encoder position, following error,
  current/voltage/torque, planner horizon, and deadline/jitter channels;
- start/stop/single capture and deterministic replay into graph inputs;
- export with schema, units, clock mapping, firmware/board/config/job digests;
- probe placement directly on wires, with data-domain-aware display; and
- experiment records containing graph, parameters, machine config, commands,
  telemetry, assets, and results.

Exit:

- Sustained plotting stays within configured browser/device memory and telemetry
  bandwidth.
- Trigger captures preserve fault causality and can be replayed without live
  hardware.
- A job report can trace an exact CAD path segment to machine-IR instructions and
  commanded/measured position samples.

## Stage I6 — Usability and ecosystem

- Searchable capability-aware palette, quick insert, keyboard wiring, alignment,
  groups/frames, minimap, and scalable large-graph rendering.
- Context help generated from node schemas and board/device documentation.
- Reusable component packages with version ranges, migrations, signatures, and
  permission/resource declarations.
- Front-panel dashboards bound to graph ports, with separate edit/run modes.
- Graph diff, review, dependency lockfile, deployment manifest, and reproducible
  experiment bundles.
- Simulation devices and recorded capability fixtures so authoring is not tied
  to live hardware.
- Clear separation of “simulated,” “connected,” “configured,” “armed,” and
  “running” states throughout the UI.

## Interface-specific acceptance criteria

- Current CSGRS/Hyper versions compile natively and for WASM.
- Hypergraphics owns all GPU buffer generation and projection; hand-renderer
  helpers have been removed.
- Graph files are versioned, migratable, diffable, and never silently lose an
  unknown node/value.
- Every device mutation is capability checked, configuration-digest checked,
  authorized, bounded, and observable.
- Exact CAD/CAM never passes through a renderer float; machine quantization is a
  named graph/compiler operation with an error report.
- Realtime-deployed graphs have fixed memory, explicit clocks, bounded queues,
  static resource claims, and a validation/timing report.
- Plots remain responsive and bounded under the maximum supported telemetry
  profile and retain fault/event evidence.
