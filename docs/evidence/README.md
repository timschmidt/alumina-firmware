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
