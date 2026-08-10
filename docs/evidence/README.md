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
