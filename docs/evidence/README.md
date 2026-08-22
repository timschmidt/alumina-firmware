# Implementation evidence

This directory records reproducible milestone claims. A passing command is not
a hardware qualification: board promotion still follows the evidence ladder in
[`VERIFICATION.md`](../VERIFICATION.md).

- [`M0-PORTABLE-FOUNDATION.md`](M0-PORTABLE-FOUNDATION.md) — host-buildable
  protocol, job, board, safety, schema, and repository foundations.
- [`M1-TDECK-IMPORT.md`](M1-TDECK-IMPORT.md) — exact T-Deck source import,
  integration delta, and ESP32-S3 compile evidence.
- [`M2-DUAL-CORE-RUNTIME.md`](M2-DUAL-CORE-RUNTIME.md) — chip-specific
  peripheral ownership, two Embassy executors, bounded cross-core paths, and
  linked-image evidence.
- [`M2-BOARD-METADATA.md`](M2-BOARD-METADATA.md) — expanded board capabilities,
  electrical/timing/visual/HIL contracts, corrected T-Deck reset routing, and
  renewed host/ESP compile evidence.
- [`M2-TINYBEE-FLASH-VARIANTS.md`](M2-TINYBEE-FLASH-VARIANTS.md) — 8 MiB
  primary and separately identified 4 MiB TinyBee packages, distinct canonical
  capabilities, board-qualified artifacts, and explicit current image-fit
  evidence.
- [`M3-PROTOCOL-STORAGE-SIM.md`](M3-PROTOCOL-STORAGE-SIM.md) — exact native wire
  foundations, resumable content-addressed transaction model, rebootable cache,
  and bounded service/RT prefetch simulation.
- [`M3-WIFI-WEB-FOUNDATION.md`](M3-WIFI-WEB-FOUNDATION.md) — core-0 radio/AP,
  static IPv4, bounded DHCP/HTTP bootstrap, credential policy, and linked-image
  evidence for both first boards.
- [`M3-AUTHENTICATED-SERVICE.md`](M3-AUTHENTICATED-SERVICE.md) — canonical
  request/response HMAC, replay/rate/header admission, cancellation-safe core-0
  dispatch, and fail-closed native storage endpoints.
- [`M3-DURABLE-CACHE-MEDIA.md`](M3-DURABLE-CACHE-MEDIA.md) — asynchronous raw
  cache media, alternating anchors, hash-chained durable records, authenticated
  backend dispatch, and deterministic torn-write simulation.
- [`M3-SD-SPI-TRANSPORT.md`](M3-SD-SPI-TRANSPORT.md) — clean-room bounded SD
  SPI protocol, CRC/address/capacity handling, real cache-media integration, and
  TinyBee/T-Deck Pro core-0 composition.
- [`M3-CACHE-PROVISIONING.md`](M3-CACHE-PROVISIONING.md) — explicit hashed
  region locators, canonical destructive authorization, boot mount/recovery,
  authenticated status/dispatch, and power-cut simulation.
- [`M3-SAFE-BOOT-OBSERVATION.md`](M3-SAFE-BOOT-OBSERVATION.md) — complete
  TinyBee static safe image, T-Deck high-impedance hazard state, pre-Wi-Fi boot
  gate, and freshness-bound core-1 safety authority for storage admission.
- [`M3-PUBLISHED-OBJECT-READER.md`](M3-PUBLISHED-OBJECT-READER.md) — immutable
  typed publication lookup and chunk/aggregate revalidation through fixed
  caller-owned memory.
- [`M3-MACHINE-BLOCK-BOUNDARY.md`](M3-MACHINE-BLOCK-BOUNDARY.md) — canonical
  owned execution blocks, independent dual-core validation, and credited work
  transfer.
- [`M3-JOB-PREFETCH-LIFECYCLE.md`](M3-JOB-PREFETCH-LIFECYCLE.md) — bounded
  provisioned-cache prefetch, backpressure retention, cancellation, and
  independent core-1 admission actors.
- [`M3-FIRMWARE-JOB-PREFETCH.md`](M3-FIRMWARE-JOB-PREFETCH.md) — authenticated
  canonical job control, live core-0/core-1 actor ownership, fail-closed first
  block retention, storage-mutation exclusion, and linked target evidence.
- [`M3-CANONICAL-CAPABILITIES.md`](M3-CANONICAL-CAPABILITIES.md) — canonical
  complete board-package bytes/digests, bounded authenticated range retrieval,
  boot/xtask verification, and capability-versus-configuration separation.
- [`M4-CONFIGURATION-IR.md`](M4-CONFIGURATION-IR.md) — canonical resource and
  exact-machine facts, real published-SD streaming, and identical independent
  core-0/core-1 candidate validation with activation still closed.
- [`M4-DURABLE-CONFIGURATION-SELECTION.md`](M4-DURABLE-CONFIGURATION-SELECTION.md)
  — power-cut-safe active selection and explicit clear/abort replay.
- [`M4-FIRMWARE-CONFIGURATION-LIFECYCLE.md`](M4-FIRMWARE-CONFIGURATION-LIFECYCLE.md)
  — authenticated dual-core validate/activate/durable-commit/authorize wiring.
- [`M5-INTERFACE-EXACT-BASELINE.md`](M5-INTERFACE-EXACT-BASELINE.md) — greenfield
  exact/UI value boundaries, current sibling CSGRS/Hyper source enforcement,
  Hypergraphics ownership, canonical client/simulator bytes, and native/WASM
  production-build evidence.
- [`M5-EXACT-CAM-COMPILER.md`](M5-EXACT-CAM-COMPILER.md) — certified
  Hypercurve path/region presentation, lossless supported-family Hyperpath
  promotion, exact curve-to-machine-lattice compilation, canonical machine IR,
  conservative error evidence, and renewed native/WASM artifact checks.
- [`M5-UI-CNC-GEOMETRY-IMPORT.md`](M5-UI-CNC-GEOMETRY-IMPORT.md) — bounded
  exact selected-semantics CNC line/arc import, non-canonical source provenance,
  full transactional machine/cache/evidence admission, and native/WASM browser
  evidence with physical and WLAN claims kept closed.
- [`M6-EXACT-STEPPER-CORE.md`](M6-EXACT-STEPPER-CORE.md) — exact centered
  integer step interpolation, configuration-derived electrical timing,
  complete shifted-image mapping, canonical status, and cached-block simulation.
- [`M6-SAFETY-INPUT-CORE.md`](M6-SAFETY-INPUT-CORE.md) — configuration-derived
  safety-input slots, exact debounce and sample watchdogs, arming masks, and
  typed conservative reactions at the portable checkpoint.
- [`M6-TARGET-SAFETY-INPUTS.md`](M6-TARGET-SAFETY-INPUTS.md) — transactional
  TinyBee GPIO sampling, canonical input telemetry, and local safe-stop/job
  invalidation wiring, with all physical timing/electrical claims still closed.
- [`M6-TARGET-MOTION-COMMIT.md`](M6-TARGET-MOTION-COMMIT.md) — descriptor-bound
  target step execution, two-phase complete-image physical acknowledgement,
  qualified arm/start gates, and fail-closed terminal disable coordination.
- [`M6-I2S-PCM-SHORT-MODEL.md`](M6-I2S-PCM-SHORT-MODEL.md) — exact continuous
  PCM-short frame grid, full-image one-frame pipeline, bounded sparse-to-dense
  planning, and independent bit-level latch/starvation simulation.
- [`M6-SCHEDULED-OUTPUT-HORIZON.md`](M6-SCHEDULED-OUTPUT-HORIZON.md) — exact
  output lattice, bounded future-image ownership, staged/latched commit order,
  and end-to-end motion-to-wire simulation through terminal disable.
- [`M6-CIRCULAR-DMA-HORIZON.md`](M6-CIRCULAR-DMA-HORIZON.md) — allocation-free
  circular-ring release/refill ownership, two-phase dense-frame acceptance,
  independent physical-latch authority, and final-disable lead simulation.
- [`M6-CROSS-BLOCK-PREFILL.md`](M6-CROSS-BLOCK-PREFILL.md) — strict two-block
  admission, per-block physical-commit barriers, prestart successor ownership,
  and continuous circular-DMA/wire simulation across a block boundary.
- [`M6-TINYBEE-PCM-SHORT-SAFE-HARNESS.md`](M6-TINYBEE-PCM-SHORT-SAFE-HARNESS.md)
  — isolated build-only safe-image TinyBee capture artifact, bounded refill/stop
  procedure, and explicit disconnected-load/physical-review gate.
- [`M6-TINYBEE-SLOGIC-CAPTURE-CONTRACT.md`](M6-TINYBEE-SLOGIC-CAPTURE-CONTRACT.md)
  — analyzer-only phase/result marker, exact SLogic16U3 probe/photo contract,
  bounded VCD reconstruction, and digest-bound strict run-record replay.
- [`M7-DISTRIBUTED-CLOCKS-JOBS.md`](M7-DISTRIBUTED-CLOCKS-JOBS.md) — exact
  causal clock estimation, boot-bound cached schedules, three-stage
  install/confirm/start authority, adversarial two-MCU simulation, and closed
  hardware-output gates.
- [`M7-GLOBAL-JOB-MANIFEST.md`](M7-GLOBAL-JOB-MANIFEST.md) — browser/native
  construction of replayed per-MCU cache objects and the shared canonical
  multi-MCU manifest, with deterministic WASM production-bundle evidence.
- [`M7-BROWSER-CACHE-DELIVERY.md`](M7-BROWSER-CACHE-DELIVERY.md) — exact
  origin-bound browser/firmware authentication, retry-safe SD publication
  reconciliation, ordered participant delivery, and renewed WASM/ESP artifacts.
- [`M7-BROWSER-CLOCK-COORDINATOR.md`](M7-BROWSER-CLOCK-COORDINATOR.md) —
  worker-capable conservative heartbeat acquisition, boot-scoped clock models,
  exact prepare/install/confirm-or-abort coordination, and retry reconciliation.
- [`M7-BROWSER-AUTH-HTTP-SIM.md`](M7-BROWSER-AUTH-HTTP-SIM.md) — production
  worker-to-host authenticated HTTP/CORS clock traffic, finite-outage and reboot
  recovery, bounded-delay admission, and conservative excessive-delay rejection.
- [`M7-OBSERVED-START-REPLAY.md`](M7-OBSERVED-START-REPLAY.md) — canonical typed
  first-output evidence, retained tolerance faults, exact device-cycle-to-browser
  inversion, authenticated monotonic reconciliation, and two-MCU simulated replay.
- [`M7-PRESTART-HARDWARE-PRIMING.md`](M7-PRESTART-HARDWARE-PRIMING.md) — schedule
  wire version 2, abort-guard hardware priming, explicit prestart horizon
  acknowledgement, scheduled firmware ownership, and compile-only TinyBee
  PCM-short HAL composition with physical claims kept closed.
- [`M8-PORTABLE-FOC-FOUNDATION.md`](M8-PORTABLE-FOC-FOUNDATION.md) — exact Q2.30
  point/interval arithmetic, certified transforms and modulation, widened
  dq-current PI control, digest-bound commands, and deterministic functional
  plant replay with every hardware claim kept closed.
- [`M8-PORTABLE-CASCADED-SERVO.md`](M8-PORTABLE-CASCADED-SERVO.md) — exact
  Q31.32 position intervals, integer nested loop grids, transactional
  position/velocity-to-q-current cascade, first-cause limits, and deterministic
  ideal-current mechanical replay without target or configuration attachment.
- [`M8-PORTABLE-ENCODER-ESTIMATOR.md`](M8-PORTABLE-ENCODER-ESTIMATOR.md) —
  explicit multi-turn seeding, exact unique-window absolute-count unwrapping,
  outward position/velocity observations with required estimator widening,
  first-cause precision gates, and deterministic repeated-wrap truth replay
  without target attachment.
- [`M8-CANONICAL-SERVO-ENCODER-CONFIGURATION-V6.md`](M8-CANONICAL-SERVO-ENCODER-CONFIGURATION-V6.md)
  — greenfield fixed-width servo/encoder records, exact machine-scalar and
  conservative uncertainty cross-checks, full-digest outer-loop lowering, and
  coordinated browser schema rollover without a compatibility path.
- [`M8-MKS-FOC-SAFE-TARGET.md`](M8-MKS-FOC-SAFE-TARGET.md) — reconciled V1.0
  schematic facts, typed resources and canonical capability identity, explicit
  absent enable/storage capabilities, six-phase-high-impedance boot composition,
  and classic-ESP32 linked-image evidence with all energization paths closed.
- [`M8-FOC-SHUTDOWN-CONTRACT.md`](M8-FOC-SHUTDOWN-CONTRACT.md) — canonical
  historical configuration V2 shutdown strategies, immutable qualification and topology
  gates, core-1 FOC profiles, and renewed multi-target linked-image evidence.
- [`M8-EXACT-ELECTRICAL-ANGLE.md`](M8-EXACT-ELECTRICAL-ANGLE.md) — exact binary
  turns, independently certified fixed-point sine/cosine, rational sensor and
  alignment uncertainty, and digest-bound canonical rotor observations.
- [`M8-AS5600-CLOSED-OWNERSHIP.md`](M8-AS5600-CLOSED-OWNERSHIP.md) — read-only
  AS5600 wire semantics, dual MKS encoder type states, and sealed ADC/MCPWM
  ownership with no scheduled or energizing path.
- [`M8-CURRENT-SAMPLING-CONTRACT.md`](M8-CURRENT-SAMPLING-CONTRACT.md) — outward
  raw-ADC calibration, bounded two-shunt reconstruction, and replayable
  PWM/ADC synchronization evidence.
- [`M8-FOC-CONFIGURATION-V3.md`](M8-FOC-CONFIGURATION-V3.md) — canonical stored
  controller/rotor/current/timing records, exact cross-record validation, and
  SHA-256-bound real-time FOC lowering.
- [`M8-CLASSIC-ESP32-ADC1-OWNER.md`](M8-CLASSIC-ESP32-ADC1-OWNER.md) — ordered
  software-started ADC1 commissioning ownership for all four MKS current routes,
  with synchronization and torque-control claims kept closed.
- [`M8-EXACT-MCPWM-COMPARE.md`](M8-EXACT-MCPWM-COMPARE.md) — exact
  duty-interval to center-aligned compare lowering, fail-closed timer-zero
  staging, and stopped pin-disconnected MKS MCPWM ownership.
- [`M9-CANONICAL-GRAPH-DOCUMENT-V1.md`](M9-CANONICAL-GRAPH-DOCUMENT-V1.md) —
  bounded exact unit/type/value registries, opaque versioned structural nodes,
  explicit domains/clocks, typed wires, and canonical digest-verified graph
  replay with hostile-input coverage.
- [`M9-AUDITED-GRAPH-SEMANTICS.md`](M9-AUDITED-GRAPH-SEMANTICS.md) — exact
  context-bound node schemas, complete current-tick feedthrough, explicit
  read-before-write state, declared-state bounds, and deterministic
  port-level combinational-cycle witnesses.
- [`M9-CANONICAL-TYPE-STORAGE.md`](M9-CANONICAL-TYPE-STORAGE.md) — checked
  maximum canonical bytes for exact/composite/runtime-payload types and
  rejection of state declarations smaller than their complete value domain.
- [`M9-BOUNDED-GRAPH-CHANNELS.md`](M9-BOUNDED-GRAPH-CHANNELS.md) — required and
  optional input delivery, explicit synchronous/event/stream queue policy,
  cross-domain scalar rejection, and exact bounded channel-memory reports.
- [`M9-EXACT-GRAPH-RATES.md`](M9-EXACT-GRAPH-RATES.md) — exact rational clock
  resolution, explicit latest-at-or-before Stream transitions, smallest
  repeating schedules, minimum queue capacity, and bounded retained samples.
- [`M9-DETERMINISTIC-GRAPH-SIMULATION.md`](M9-DETERMINISTIC-GRAPH-SIMULATION.md)
  — fixed HostExact Stream/rate simulation, exact source-first scheduling,
  canonical implementation identity, and independently replayed `ALGT` traces.
- [`M9-EXACT-CONTROL-GRAPH.md`](M9-EXACT-CONTROL-GRAPH.md) — composable exact
  arithmetic, explicit unit-delay state, fail-safe permit gating, and a
  deterministic visible multi-rate PID/interlock fixture with `ALSI` V2
  context binding.
- [`M9-EXACT-INTERLOCK-PRIMITIVES.md`](M9-EXACT-INTERLOCK-PRIMITIVES.md) —
  exact inclusive-range and Boolean-conjunction HostExact primitives,
  independently exercised fail-closed semantics, a 13-kind editor palette,
  and renewed graph/workspace/component identities.
- [`M9-MIXED-SIGNAL-CONTROL-TRACE.md`](M9-MIXED-SIGNAL-CONTROL-TRACE.md) — six
  canonical reference probes and component outputs, certified exact-rational
  plots, aligned Boolean logic-analyzer lanes, and shared exact cursor evidence.
- [`M9-INTERLOCK-CAUSE-TRACE.md`](M9-INTERLOCK-CAUSE-TRACE.md) — independently
  timed external/range/combined Boolean lanes, exact stop-cause attribution,
  seven stable probes/outputs, and no-op sidecar-identity preservation.
- [`M9-EXACT-REPLAY-PROBE-TRIGGER.md`](M9-EXACT-REPLAY-PROBE-TRIGGER.md) —
  canonical `ALGP` V2 Boolean edge triggers, bounded exact pre/post replay
  resolution, transactional authoring, and a qualified trigger-window UI.
- [`M9-EXACT-GRAPH-PROBE-PAIR-PERSISTENCE.md`](M9-EXACT-GRAPH-PROBE-PAIR-PERSISTENCE.md)
  — one bounded origin-local ALGW/ALGP pair, all-or-nothing bound replay,
  exact no-op persistence, and native/browser `.algp` exchange.
- [`M9-EXACT-GRAPH-PROBE-PAIR-HISTORY.md`](M9-EXACT-GRAPH-PROBE-PAIR-HISTORY.md)
  — bounded ephemeral ALGW/ALGP pair snapshots, transactional bound replay,
  shared graph/probe/trigger/import navigation, and paired eviction.
- [`M9-BOUNDED-GRAPH-PROBE-METADATA.md`](M9-BOUNDED-GRAPH-PROBE-METADATA.md)
  — transactional canonical probe names, retention ceilings, event strides,
  trigger-window validation, pair history, and visible bounded editors.
- [`M9-BOUNDED-GRAPH-PROBE-PROJECTION.md`](M9-BOUNDED-GRAPH-PROBE-PROJECTION.md)
  — aggregate-bounded exact replay projection, per-probe stride/retention,
  exact root-time trigger windows, and direct mixed-signal plot consumption.
- [`M9-EXACT-MULTIRATE-PROBE-AXIS.md`](M9-EXACT-MULTIRATE-PROBE-AXIS.md) —
  exact rational shared-root axis/cursor, same-root mixed-rate probes,
  external-source provenance, and explicit retained sample-and-hold.
- [`M9-EXACT-PROBE-TYPE-PANES.md`](M9-EXACT-PROBE-TYPE-PANES.md) — stable
  registered-type analog panes, independent certified-enclosure scales, one
  exact root-time cursor, and an explicit bounded display policy.
- [`M9-EXACT-PHYSICAL-SCALAR-PROBES.md`](M9-EXACT-PHYSICAL-SCALAR-PROBES.md) —
  exact rational/interval/lattice scalar plots, registered quantum/unit
  retention, outward display enclosures, and closed non-scalar conversion.
- [`M9-EXACT-STATE-EVENT-PROBES.md`](M9-EXACT-STATE-EVENT-PROBES.md) — exact
  canonical non-scalar identity, bounded categorical state/event lanes,
  byte-exact change markers, and same-time sequence multiplicity.
- [`M9-EXACT-COMPOSITE-LITERAL-EDITOR.md`](M9-EXACT-COMPOSITE-LITERAL-EDITOR.md)
  — bounded schema-directed scalar/composite text, exact round trips,
  transactional UI integration, and closed handle/runtime authority.
- [`M9-CAPABILITY-RESOURCE-SELECTOR.md`](M9-CAPABILITY-RESOURCE-SELECTOR.md)
  — exact capability-derived resource selection, stable node identity,
  transactional rollback, and closed raw-handle/deployment authority.
- [`M9-COMPOSITE-CAPABILITY-RESOURCE-SELECTION.md`](M9-COMPOSITE-CAPABILITY-RESOURCE-SELECTION.md)
  — bounded schema-aware nested physical-resource selection, whole-workspace
  uniqueness, exact sibling preservation, and optimized reset/reload evidence
  with all live and deployment authority closed.
- [`M9-GRAPH-LABEL-DOMAIN-EDITOR.md`](M9-GRAPH-LABEL-DOMAIN-EDITOR.md) —
  bounded canonical node labels, schema-audited concrete execution choices,
  pair history/persistence, closed raw-device authority, and optimized
  localhost browser interaction evidence.
- [`M9-CACHE-DERIVED-GRAPH-JOB-HANDLES.md`](M9-CACHE-DERIVED-GRAPH-JOB-HANDLES.md)
  — exact canonical-job/cache-ready joins, inert catalog-bound `JobHandle`
  selection, atomic three-artifact browser persistence, and optimized
  edit/reload evidence with execution authority closed.
- [`M9-COMPOSITE-CACHED-JOB-SELECTION.md`](M9-COMPOSITE-CACHED-JOB-SELECTION.md)
  — bounded schema-aware value paths, exact nested `JobHandle` replacement,
  composite reference-set authoring, and optimized undo/redo/reload evidence
  with all execution authority closed.
- [`M9-EXACT-CONTROL-INSPECTOR.md`](M9-EXACT-CONTROL-INSPECTOR.md) — one shared
  fallible exact-control fixture, deterministic bounded semantic layout,
  explicit feedback/state inspection, exact-cursor traces, and optimized
  native/WASM browser-render evidence.
- [`M9-CANONICAL-GRAPH-WORKSPACE.md`](M9-CANONICAL-GRAPH-WORKSPACE.md) —
  canonical `ALGW` envelope, presentation-only integer placement, monotonic
  identities, transactional typed-wire edits, semantic blockers, and
  graph-bound reference-trace detachment.
- [`M9-GRAPH-PALETTE-PARAMETERS.md`](M9-GRAPH-PALETTE-PARAMETERS.md) — audited
  11-kind HostExact palette, monotonic node lifecycle, atomic incident-wire
  deletion, bounded exact scalar parameter editing, and empty-draft recovery.
- [`M9-GRAPH-WORKSPACE-HISTORY-PERSISTENCE.md`](M9-GRAPH-WORKSPACE-HISTORY-PERSISTENCE.md)
  — bounded canonical snapshot history, replay-backed undo/redo, origin-local
  current-workspace persistence, and exact native/browser `.algw` exchange.
- [`M9-GRAPH-COMPONENT-FRONT-PANEL.md`](M9-GRAPH-COMPONENT-FRONT-PANEL.md) —
  canonical `ALGC` authoring packages, typed public connector mappings, exact
  front-panel bindings, transactional workspace replacement, and a visible
  PID/interlock component panel.
- [`M9-GRAPH-HIERARCHY-FLATTENING.md`](M9-GRAPH-HIERARCHY-FLATTENING.md) —
  canonical digest-bound `ALGH` libraries/instances, derived collapsed port
  shapes, deterministic connector rewiring with fresh monotonic identities,
  and ordinary audited `ALGW` output.
- [`M9-BOUNDED-RECURSIVE-GRAPH-HIERARCHY.md`](M9-BOUNDED-RECURSIVE-GRAPH-HIERARCHY.md)
  — `ALGH` V2 scoped component dependencies, cycle/depth/expansion bounds,
  deterministic recursive flattening, and stable occurrence source paths.
- [`M9-EXACT-HIERARCHY-SOURCE-MAP.md`](M9-EXACT-HIERARCHY-SOURCE-MAP.md) —
  total node/wire root-or-occurrence provenance, canonical bounded `ALGM` V1
  replay by fresh hierarchy flattening, UI correlation, and exact browser
  sidecar download evidence.
- [`M9-CANONICAL-GRAPH-AUTHORING-SESSION.md`](M9-CANONICAL-GRAPH-AUTHORING-SESSION.md)
  — one bounded `ALGS` V1 identity over control/probe/cached-job/hierarchy
  authoring state, exact cross-artifact replay, atomic browser persistence and
  `.algs` exchange, and explicit retirement of the provisional bundle format.
- [`M9-UNIFIED-GRAPH-AUTHORING-HISTORY.md`](M9-UNIFIED-GRAPH-AUTHORING-HISTORY.md)
  — one bounded ephemeral timeline of complete canonical `ALGS` snapshots,
  atomic mixed graph/probe/cached-job/hierarchy undo/redo, exact replay plus UI
  catalog admission, and retirement of split pair/job histories.
- [`M9-DIRECT-ROOT-INSTANCE-AUTHORING.md`](M9-DIRECT-ROOT-INSTANCE-AUTHORING.md)
  — exact embedded-library selection, transactional root occurrence
  add/delete, monotonic hierarchy identities, selected-component digest remap,
  and complete-session native/WASM/browser qualification.
- [`M9-EXACT-COMPONENT-LIBRARY-EXCHANGE.md`](M9-EXACT-COMPONENT-LIBRARY-EXCHANGE.md)
  — bounded standalone `ALGC` leaf import/export, whole-library semantic
  admission, exact duplicate no-op, reference-safe removal, and complete-ALGS
  native/WASM/browser history and persistence qualification.
- [`M9-CANONICAL-NESTED-COMPONENT-PACKAGE-EXCHANGE.md`](M9-CANONICAL-NESTED-COMPONENT-PACKAGE-EXCHANGE.md)
  — canonical `ALCP` transitive definition/binding closures, additive
  collision-safe single-revision merge, exact duplicate no-op, and complete-
  session native/WASM/browser history and persistence qualification.
- [`M9-EXACT-PROMOTED-COMPONENT-PARAMETERS.md`](M9-EXACT-PROMOTED-COMPONENT-PARAMETERS.md)
  — `ALGH` V3 stable panel-item-derived parameter surfaces, recursive
  promotion, distinct exact per-occurrence values, definition-evolution
  policy, deterministic flattening, and optimized no-op/rejection/history/
  reload evidence.
- [`M9-EXACT-ROOT-HIERARCHY-CANVAS.md`](M9-EXACT-ROOT-HIERARCHY-CANVAS.md) —
  exact bound-root placement, typed monotonic root wiring, complete
  `ALGH`/`ALGM`/`ALGS` admission, and optimized pointer-drag undo/redo/reload
  evidence.
- [`M9-EXACT-FRONT-PANEL-AUTHORING.md`](M9-EXACT-FRONT-PANEL-AUTHORING.md) —
  monotonic bound-item creation, exact metadata/layout update and removal,
  authored-panel retention, and optimized pointer-drag history/persistence
  evidence.
- [`M9-STABLE-COMPONENT-CONNECTOR-AUTHORING.md`](M9-STABLE-COMPONENT-CONNECTOR-AUTHORING.md)
  — transactional public-input/public-output CRUD, monotonic stable identities,
  recursive ID-based placeholder/wire/parent refresh, and optimized complete-
  session history/persistence evidence.
- [`M9-SELECTED-LIBRARY-CONNECTOR-AUTHORING.md`](M9-SELECTED-LIBRARY-CONNECTOR-AUTHORING.md)
  — full recursive identity reports, selected-dependency connector editing,
  control-authority isolation, and optimized history/reload evidence.
- [`M9-SELECTED-COMPONENT-DEFINITION-CANVAS.md`](M9-SELECTED-COMPONENT-DEFINITION-CANVAS.md)
  — separate selected-dependency `ALGW` authoring, recursive authority-safe
  replacement, monotonic typed structure, and optimized history/reload
  evidence.
- [`M9-SCOPED-CHILD-OCCURRENCE-AUTHORING.md`](M9-SCOPED-CHILD-OCCURRENCE-AUTHORING.md)
  — atomic parent-placeholder/scoped-binding creation and deletion, public-
  binding vetoes, monotonic child identities, and exact optimized
  history/reload evidence.
- [`M9-EXACT-SCOPED-CHILD-OCCURRENCE-REBINDING.md`](M9-EXACT-SCOPED-CHILD-OCCURRENCE-REBINDING.md)
  — exact one-occurrence target replacement, stable-ID endpoint retention,
  same-shape parent preservation, recursive shape refresh, atomic typed
  rejection, and optimized no-op/history/reload evidence.
- [`M9-EXACT-FLATTENED-SOURCE-NAVIGATION.md`](M9-EXACT-FLATTENED-SOURCE-NAVIGATION.md)
  — exact final-node/wire origin revalidation, root/private/authoritative
  destination focus, stale-origin reconciliation, and transient optimized
  browser navigation evidence.
- [`M9-EXACT-COMPONENT-WIRE-FOCUS.md`](M9-EXACT-COMPONENT-WIRE-FOCUS.md)
  — exact transient component-local wire selection, distinct authoritative and
  private canvas highlighting, retained target-node inspection, stale-digest
  reconciliation, and optimized browser evidence.
- [`M9-EXACT-COMPONENT-IDENTITY-EVOLUTION.md`](M9-EXACT-COMPONENT-IDENTITY-EVOLUTION.md)
  — monotonic stable-name/behavior-version authoring, recursive no-alias digest
  replacement, exact authority isolation, atomic invalid/no-op behavior, and
  optimized history/reload evidence.
- [`M9-GENERAL-COMPONENT-LIBRARY-CREATION.md`](M9-GENERAL-COMPONENT-LIBRARY-CREATION.md)
  — deterministic named revision-1 empty `ALGC` construction, exact
  schema/clock inheritance, stable-name conflict rejection, duplicate no-op,
  immediate definition editing, and optimized history/reload evidence.
- [`M9-CAPABILITY-CATALOG-DIAGNOSTIC-PROBES.md`](M9-CAPABILITY-CATALOG-DIAGNOSTIC-PROBES.md)
  — caller-authenticated capability/registry intersection into concrete
  TinyBee resource nodes, canonical bounded `ALGP` output probes, separate
  offline target drafting, and visible closed-access evidence.
- [`M9-BOARD-CAPABILITY-EXPLORER.md`](M9-BOARD-CAPABILITY-EXPLORER.md) — complete
  bounded allocation-free `ALMCAP02` board decoding, board-name-independent
  owned UI state, explicit descriptive-versus-graph authority, searchable
  TinyBee resource/hazard/owner views, and an honest missing-photo/HIL gate.
- [`M9-OFFLINE-DIAGNOSTIC-EXPLORER.md`](M9-OFFLINE-DIAGNOSTIC-EXPLORER.md) —
  canonical bounded overview and triggered digital-edge records, deterministic
  TinyBee fixture, capability-reconciled resource cross-linking, exact-cycle UI
  trace, and explicit simulator/no-authority gates.
- [`M9-AUTHENTICATED-DIAGNOSTIC-TRANSPORT.md`](M9-AUTHENTICATED-DIAGNOSTIC-TRANSPORT.md)
  — exact context/digest-bound telemetry and capture lifecycles, fixed core-0
  state, typed client reconciliation, and localhost HTTP/HMAC range recovery
  without physical Wi-Fi or board contact.
- [`M9-FIXED-GRAPH-IR.md`](M9-FIXED-GRAPH-IR.md) — fixed 4 KiB portable graph
  package, allocation-free independent admission, complete implementation
  identity, and browser lowering into bounded Service/Realtime arenas.
- [`M9-FIXED-GRAPH-RUNTIME.md`](M9-FIXED-GRAPH-RUNTIME.md) — transactional
  const-generic admission, source-first start priming, split-core fixed-opcode
  execution, first-cause faults, and direct browser-compiler/runtime replay.
- [`M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md`](M9-AUTHENTICATED-GRAPH-DEPLOYMENT.md)
  — immutable SD publication, authenticated browser lifecycle, independent
  dual-core admission/authorization, live Embassy ownership, target links, and
  explicit boot-ephemeral/non-executing boundaries.
- [`M9-SPLIT-CORE-GRAPH-EXECUTION.md`](M9-SPLIT-CORE-GRAPH-EXECUTION.md) —
  permanent core-local actors, authenticated exact start/stop epochs,
  pinned-task release scheduling, retained first-cause faults, browser
  reconciliation, and explicit resource-free/timing-unqualified boundaries.
- [`M9-DURABLE-GRAPH-SELECTION.md`](M9-DURABLE-GRAPH-SELECTION.md) — typed
  prepare/commit/abort journal, configuration-first dual-core boot replay,
  exhaustive modeled power-cut recovery, and renewed ESP/WASM artifacts.
- [`M9-CAPABILITY-BOUND-GRAPH-INPUT.md`](M9-CAPABILITY-BOUND-GRAPH-INPUT.md) —
  capability-derived split arenas and exact opcode/resource palettes,
  `ALGRIR02` stable safety-input reads, independent dual-core admission,
  fail-closed freshness semantics, and renewed ESP/WASM artifacts.
- [`M9-EXECUTABLE-COMPOSITE-RESOURCE-CONSUMER.md`](M9-EXECUTABLE-COMPOSITE-RESOURCE-CONSUMER.md)
  — ordered two-resource graph execution, capability-derived browser lowering,
  atomic ALGW/ALGR rebinding, portable actor replay, and optimized localhost
  evidence with every live and physical authority kept closed.
- [`M9-TINYBEE-GRAPH-INPUT-TIMING-HARNESS.md`](M9-TINYBEE-GRAPH-INPUT-TIMING-HARNESS.md)
  — production-sized dual-core graph-input timing under real Wi-Fi/web tasks,
  exact disconnected-load SLogic wiring, streaming VCD reanalysis, and strict
  digest-bound run evidence; disconnected-board commissioning observations now
  cover radio-startup suspension and priority-executor recovery while all
  analyzer/HTTP-loaded qualification claims remain closed.
- [`M10-EXACT-SCHEDULE-PREFLIGHT.md`](M10-EXACT-SCHEDULE-PREFLIGHT.md) —
  Configuration V5 machine/time/output facts, exact physical error budgeting,
  Hyperpath/Hypersolve exact-stop jerk scheduling, certified firmware-V1
  interpolation, production stepper preflight, cached event replay, and
  deterministic source/partition evidence with physical claims kept closed.
- [`M10-CERTIFIED-CUBIC-MOTION.md`](M10-CERTIFIED-CUBIC-MOTION.md) — native
  exact cubic source retention, bounded pointwise source-to-motion reduction,
  exact diagonal feed carriers, stop-at-every-chord scheduling, domain-separated
  `ALMEVD02` replay, and native/WASM browser evidence with physical claims kept
  closed.
- [`M10-EXACT-TWO-PASS-LOOKAHEAD.md`](M10-EXACT-TWO-PASS-LOOKAHEAD.md) —
  clean-room exact squared-speed forward/reverse node planning, explicit caller
  and retained-radius limits, independent Hypersolve replay, conservative
  all-zero Alumina integration, and renewed native/WASM/loopback evidence with
  every physical claim kept closed.
- [`M10-EXACT-MONOTONIC-JERK.md`](M10-EXACT-MONOTONIC-JERK.md) — exact
  two-phase acceleration/deceleration/constant-feed transitions with nonzero
  boundary feeds, separate construction and generic kinematic replay, dormant
  all-zero-policy integration, and renewed native/WASM/loopback evidence with
  every physical claim kept closed.
- [`M10-EXACT-JERK-FEASIBLE-G1.md`](M10-EXACT-JERK-FEASIBLE-G1.md) — exact
  stop-separated component refinement to jerk-feasible node feeds, active
  lossless line-to-line G1 continuations, conservative curvature/cubic stop
  policy, production executor lowering, and renewed native/WASM/loopback
  evidence with every physical claim kept closed.
- [`M10-EXACT-AFFINE-AXIS-PROJECTION.md`](M10-EXACT-AFFINE-AXIS-PROJECTION.md)
  — arbitrary dense-axis exact affine velocity/acceleration/jerk projection,
  independent Hypersolve row and bottleneck replay, active Cartesian-line CAM
  integration, conservative curved-route fallback, and the retained-open
  timer/output-quantum boundary.
- [`M10-EXACT-TIMER-LATTICE-HEADROOM.md`](M10-EXACT-TIMER-LATTICE-HEADROOM.md)
  — exact one-sided output-quantum interval ceiling, caller-bounded rational
  dilation search, unchanged production-preflight replay, smallest-factor
  predecessor proof, and renewed native/WASM/loopback evidence with every
  physical claim kept closed.
- [`M10-CANONICAL-PLANNER-EVIDENCE-V3.md`](M10-CANONICAL-PLANNER-EVIDENCE-V3.md)
  — canonical exact planner/lowering policy and certification subtranscripts,
  cache-invariant structural `Real` serialization, policy-distinguishing replay,
  bounded encoding, and renewed native/WASM/loopback evidence with every
  physical claim kept closed.
- [`M10-SHARED-MCU-TIMER-RETIMING.md`](M10-SHARED-MCU-TIMER-RETIMING.md) — one
  jointly minimal exact same-grid timer factor, complete all-participant search
  replay, selected-stream/partition reconstruction, canonical
  `ALMSYN01`/`ALMSRT01` evidence, derived global-job timing identity, and
  native/WASM/loopback evidence with every physical claim kept closed.
- [`M10-DIRECT-FINITE-DIFFERENCE-IR.md`](M10-DIRECT-FINITE-DIFFERENCE-IR.md) —
  greenfield kind-bound Q31.32 third-order records, logarithmic sparse
  electrical admission, allocation-free dense execution, exact cached-token
  ownership, and immutable-partition simulation with every physical claim kept
  closed.
- [`M10-EXACT-CACHED-SERVO-STREAM.md`](M10-EXACT-CACHED-SERVO-STREAM.md) —
  certified browser Q31.32/Q2.30 servo recurrence projection, typed FOC-profile
  admission, allocation-free two-block setpoint ownership, terminal hold,
  complete-axis simulation, frozen moving-Hyper verification, and renewed ESP
  artifacts with every peripheral and energization claim closed.
- [`M10-PERMANENT-SERVO-LIFECYCLE.md`](M10-PERMANENT-SERVO-LIFECYCLE.md) —
  independently derived dual-core typed admission, fixed-memory distributed
  servo lifecycle, transactional complete-axis simulation join, static-memory
  correction, and renewed ESP artifacts with every physical gate closed.
- [`M10-MULTI-AXIS-SERVO-FOC-BANK.md`](M10-MULTI-AXIS-SERVO-FOC-BANK.md) —
  fixed-capacity all-axis candidate/commit installation, two-axis permanent
  cached-job replay, later-axis fault isolation, ordered safe invalidation, and
  renewed closed-gate ESP artifacts.
- [`M10-CANONICAL-DUAL-MKS-SERVO-CONFIGURATION.md`](M10-CANONICAL-DUAL-MKS-SERVO-CONFIGURATION.md)
  — one canonical dual-stage MKS document driving both cached-servo admission
  and complete simulator lowering while target qualification stays closed.
- [`M10-SIMULTANEOUS-PWM-COMMIT-BARRIER.md`](M10-SIMULTANEOUS-PWM-COMMIT-BARRIER.md)
  — allocation-free complete-vector latch correlation and sealed all-axis
  physical completions without partial logical publication.
- [`M10-TRANSACTIONAL-SERVO-BANK-ACTIVATION.md`](M10-TRANSACTIONAL-SERVO-BANK-ACTIVATION.md)
  — one opaque complete-bank initial candidate and configuration-bound
  sequence-zero activation with no independently live axis join.
- [`M10-FAIL-CLOSED-PWM-TARGET-OWNER.md`](M10-FAIL-CLOSED-PWM-TARGET-OWNER.md)
  — unique aggregate backend ownership, automatic all-stage safe handling,
  recoverable unsafe-fault state, and canonical dual-axis simulator replay.
- [`M10-CLOSED-MKS-PWM-BACKEND.md`](M10-CLOSED-MKS-PWM-BACKEND.md) — a linked
  MKS dual-stage aggregate backend whose only successful operation is the
  complete closed safe transaction, plus safe-first permanent mailbox rejection.
- [`M10-CORE1-MKS-TARGET-SELECTION.md`](M10-CORE1-MKS-TARGET-SELECTION.md) —
  pure selected-board preparation and retained exact dual-MKS target facts in
  the permanent core-1 configuration lifecycle, with every peripheral and
  authorization gate closed.
- [`M10-SELECTED-BOARD-AUTHORIZATION.md`](M10-SELECTED-BOARD-AUTHORIZATION.md)
  — pure configuration authorization preflight, full retained MKS fact replay,
  and target-bound arm readiness with every physical output gate still closed.
- [`M10-BROWSER-DIRECT-FINITE-DIFFERENCE.md`](M10-BROWSER-DIRECT-FINITE-DIFFERENCE.md)
  — exact browser/WASM affine lowering, interval-certified Q31.32 Newton
  differences, adaptive monotonic splitting, immutable direct cache packaging,
  and replayable `ALMDFE01` evidence under an isolated moving-Hyper source
  graph.
- [`M10-SCHEDULED-DIRECT-PCM.md`](M10-SCHEDULED-DIRECT-PCM.md) — allocation-free
  same-cycle complete-image composition, explicit successor/tail ownership,
  independent cross-block physical prefixes, and bit-level PCM-short latch
  simulation with every target and energization claim closed.
- [`M10-TARGET-DIRECT-DISPATCH.md`](M10-TARGET-DIRECT-DISPATCH.md) —
  descriptor-bound fixed-memory ordinary/direct selection in the permanent
  core-1 actor, explicit open-boundary and terminal-tail ownership, and renewed
  host/ESP build evidence with all peripheral and energization claims closed.
- [`M10-STATIC-SAFE-STREAM-HANDOFF.md`](M10-STATIC-SAFE-STREAM-HANDOFF.md) —
  fixed static-safe/prefill/start-observation/stream/stop/reclaim ownership,
  observed-safe motion gating, bit-level replay, and compile-only TinyBee HIL
  adoption with every production and physical claim closed.
- [`M10-PCM-SOFTWARE-ATTESTATION.md`](M10-PCM-SOFTWARE-ATTESTATION.md) — stable
  post-stop numeric lifecycle attestation, bounded RTT-log parsing, exact
  software-horizon replay, and schema-v2 correlation with the independent
  TinyBee VCD marker result.
- [`M10-BOUNDED-DMA-REFILL.md`](M10-BOUNDED-DMA-REFILL.md) — allocation-free
  fixed-budget preview/push/accept transactions, exact partial progress and
  retained credit, fail-closed uncertain-write recovery, and TinyBee safe-HIL
  adoption without a production or physical claim.
- [`M10-REFILL-WAKE-SUPERVISOR.md`](M10-REFILL-WAKE-SUPERVISOR.md) — exact
  grid/ring-bound refill policy, bracketed target-call deadlines,
  interrupt-or-fallback scheduling, first-cause retention, and bit-level timely
  versus delayed-wake simulation with every target attachment closed.
- [`M10-RUNTIME-STACK-WATERMARKS.md`](M10-RUNTIME-STACK-WATERMARKS.md) — bounded
  partial-boot executor canary epochs, incremental convergence, fixed passive
  health wire facts, target-only unsafe isolation, and renewed ESP artifacts.
- [`M10-RUNTIME-HEALTH-CLIENT.md`](M10-RUNTIME-HEALTH-CLIENT.md) — independent
  AHLT/ASWM validation, monotonic boot-scoped evidence, exact queue/headroom
  facts, and native plus window/worker WASM fetch adapters.
- [`M10-RUNTIME-HEALTH-WORKER-UI.md`](M10-RUNTIME-HEALTH-WORKER-UI.md) — strict
  schema-v2 worker polling, health-specific error retention, exact live-device
  queue/stack rendering, and signed localhost browser loss/recovery evidence.
- [`M10-AUTHENTICATED-CAPABILITY-WORKER-UI.md`](M10-AUTHENTICATED-CAPABILITY-WORKER-UI.md)
  — one shared firmware/simulator range service, retry-safe authenticated
  browser assembly, strict schema-v3 one-time document transfer, connected-board
  explorer facts, and isolated localhost capability-loss recovery.
- [`M10-CAPABILITY-BOUND-WAVEFORM-WORKER-UI.md`](M10-CAPABILITY-BOUND-WAVEFORM-WORKER-UI.md)
  — strict public identity reconciliation, worker-owned capability-selected
  input capture, an opt-in deterministic simulator provider, retry-safe
  retained-record release, schema-v4 exact trace admission/rendering, and
  repeated loopback Chromium lifecycle evidence.
- [`M10-AUTHENTICATED-LIVE-TELEMETRY.md`](M10-AUTHENTICATED-LIVE-TELEMETRY.md)
  — canonical retry-safe authenticated event polling, client-evidence-only
  acknowledgement, schema-v5 capability-bound live input status and sampled
  logic lanes, plus fresh and same-boot replacement-worker Chromium evidence.
- [`M10-BROWSER-CACHED-JOB-E2E.md`](M10-BROWSER-CACHED-JOB-E2E.md) — schema-V6
  exact CAM handoff, canonical active configuration, authenticated immutable
  cache delivery, deferred deterministic two-MCU start, simulated latch
  completion, and optimized loopback Chromium evidence.
- [`M10-REPEATED-CACHED-JOBS.md`](M10-REPEATED-CACHED-JOBS.md) — transactional
  terminal-job replacement, exact-retry preservation, immutable cache reuse,
  and two consecutive synchronized attempts on unchanged simulated MCU boots.
- [`M10-BROWSER-CACHED-JOB-RECOVERY.md`](M10-BROWSER-CACHED-JOB-RECOVERY.md)
  — operation-specific successful-response loss after storage and schedule
  mutations, read-only browser reconciliation, and no-fault Chromium regression.
- [`M10-BROWSER-CACHED-JOB-CONFIRM-RECOVERY.md`](M10-BROWSER-CACHED-JOB-CONFIRM-RECOVERY.md)
  — successful confirmation-response loss on both participants, exact
  confirmation-state progression, and identification of the then-open
  fresh-owner reattachment boundary.
- [`M10-BROWSER-CACHED-JOB-REATTACHMENT.md`](M10-BROWSER-CACHED-JOB-REATTACHMENT.md)
  — exact terminal descriptor identity retained in schedule status, read-only
  all-participant discovery, bounded replacement-worker reconciliation, and
  optimized same-actor Chromium evidence without new start authority.
- [`M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md`](M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md)
  — successful applied-abort response loss on both participants from installed
  and confirmed states, mandatory per-participant read-only reconciliation
  before the next mutation, exact aborted terminal state, and a no-fault
  Chromium regression.
- [`M10-BROWSER-CACHED-JOB-ABORT-REQUEST-RECOVERY.md`](M10-BROWSER-CACHED-JOB-ABORT-REQUEST-RECOVERY.md)
  — pre-application abort-request loss, mandatory unchanged-state status
  reconciliation, exact retry, and all-participant aborted completion.
- [`M10-BROWSER-CACHED-JOB-ABORT-GUARD-OUTAGE.md`](M10-BROWSER-CACHED-JOB-ABORT-GUARD-OUTAGE.md)
  — bounded repeated abort-mutation loss through the point of no return and an
  explicit completed-after-stop terminal rather than a fabricated abort.
- [`M10-BROWSER-CACHED-JOB-ABORT-SPLIT-OUTAGE.md`](M10-BROWSER-CACHED-JOB-ABORT-SPLIT-OUTAGE.md)
  — asymmetric applied/lost abort mutations, exact aborted/complete terminal
  facts, and an indeterminate-machine UI result.
- [`M10-BROWSER-CACHED-JOB-INSTALLING-STOP.md`](M10-BROWSER-CACHED-JOB-INSTALLING-STOP.md)
  — exact abort of an installed participant, cancellation of a never-installed
  participant without a fabricated cycle, and same-boot reuse by a second job.
- [`M10-BROWSER-CACHED-JOB-ABORT-DUPLICATE.md`](M10-BROWSER-CACHED-JOB-ABORT-DUPLICATE.md)
  — byte-identical replay of an applied authenticated abort, replay-window 401
  rejection before second native dispatch, and reopened-session status
  reconciliation to exact all-participant abort.
- [`M10-BROWSER-CACHED-JOB-ABORT-STATUS-OUTAGE.md`](M10-BROWSER-CACHED-JOB-ABORT-STATUS-OUTAGE.md)
  — bounded post-confirmation loss of every canonical schedule operation on
  both actors, autonomous local completion, repeated ambiguity retention, and
  a clean all-participant reconciliation sweep before truthful terminal state.
- [`M10-BROWSER-CACHED-JOB-ABORT-STALE-RESPONSE.md`](M10-BROWSER-CACHED-JOB-ABORT-STALE-RESPONSE.md)
  — one-shot substitution of valid old signed responses for later applied
  schedule operations, exact counter rejection, spent-session recovery, and a
  clean all-participant sweep before exact abort.
- [`M10-BROWSER-CACHED-JOB-CONFIRMED-SAFETY-FAULT.md`](M10-BROWSER-CACHED-JOB-CONFIRMED-SAFETY-FAULT.md)
  — one modeled post-confirmation local safety stop, canonical fault retention,
  continued abort cleanup of the still-confirmed peer, exact faulted/aborted
  terminal evidence, and a fresh no-fault Chromium control.
- [`M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION.md`](M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION.md)
  — ordinary authenticated status discovery of that modeled fault, automatic
  peer cleanup without an operator stop request, exact faulted/aborted terminal
  evidence, and a fresh no-fault Chromium control.
- [`M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION-RECOVERY.md`](M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION-RECOVERY.md)
  — loss of the remaining peer's successful applied-abort response during
  automatic cleanup, retained fault/ambiguity facts, mandatory read-only status
  reconciliation, and exact faulted/aborted recovery.
- [`M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION-REQUEST-RECOVERY.md`](M10-BROWSER-CACHED-JOB-SAFETY-FAULT-PROPAGATION-REQUEST-RECOVERY.md)
  — loss of the remaining peer's abort request before authentication or
  application, unchanged-state status reconciliation, exact retry, and
  faulted/aborted recovery.
- [`M10-ATTENDED-LEASE-RENEWAL.md`](M10-ATTENDED-LEASE-RENEWAL.md) — exact
  immutable-commit/mutable-lease authority, all-participant browser renewal,
  applied-response and pre-application request-loss reconciliation,
  exact-duration execution, sustained-loss expiry, and autonomous control.
- [`M10-BROWSER-WORKER-LEASE-EXPIRY-REATTACHMENT.md`](M10-BROWSER-WORKER-LEASE-EXPIRY-REATTACHMENT.md)
  — production-worker termination after exact all-running state, finite local
  lease expiry, complete fresh-owner status reconciliation, and explicit loss
  of browser-only authority without fabricated continuation.
- [`M10-TINYBEE-REALTIME-INPUT-TELEMETRY.md`](M10-TINYBEE-REALTIME-INPUT-TELEMETRY.md)
  — bounded canonical core-1 input snapshots, freshness- and mapping-checked
  core-0 translation into existing authenticated overview events, exact
  TinyBee-only memory budgets, and compile-only multi-target release evidence.
- [`M10-CAPABILITY-DERIVED-DIAGNOSTIC-OVERVIEW.md`](M10-CAPABILITY-DERIVED-DIAGNOSTIC-OVERVIEW.md)
  — greenfield capability V3 passive-observation authority, descriptor-derived
  TinyBee telemetry budgets/timing, graph-independent board-explorer admission,
  and optimized loopback browser evidence with every physical claim closed.
- [`M10-CAPABILITY-DERIVED-DIGITAL-CAPTURE.md`](M10-CAPABILITY-DERIVED-DIGITAL-CAPTURE.md)
  — greenfield capability V4 acquisition authority, a distinct deterministic
  simulator identity, image-derived worker/UI waveform admission, and optimized
  loopback browser evidence with every physical claim closed.
- [`M10-T-LORA-PAGER-COMPILE-STUB.md`](M10-T-LORA-PAGER-COMPILE-STUB.md) — the
  pinned-MIT current Pager resource inventory, compile-selected closed dual-core
  image, exact capability/artifact identities, and explicit peripheral,
  storage, visual, hardware, and moving-Hyper boundaries.
- [`M10-CAPABILITY-BOUND-VISUAL-ASSETS.md`](M10-CAPABILITY-BOUND-VISUAL-ASSETS.md)
  — immutable capability-bound raster ranges, schema-V13 worker acquisition,
  bounded PNG/hotspot rendering, synthetic simulator qualification, and an
  explicit licensed-physical-photo gate.
