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
