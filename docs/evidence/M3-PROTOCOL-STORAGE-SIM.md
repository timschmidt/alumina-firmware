# M3 protocol, storage, and simulator foundation evidence

Date: 2026-08-10

Status: host/compile foundation only. This does not close M3 Wi-Fi, web, physical
SD, authentication, update, flood, or hardware-isolation gates.

This records the first semantic storage model at its commit. The later
[`M3-DURABLE-CACHE-MEDIA.md`](M3-DURABLE-CACHE-MEDIA.md) adds the concrete raw
block format and supersedes the claim boundary and current test/image totals.

## Implemented claim

- `alumina-protocol` manually encodes/decodes a 56-byte little-endian frame
  prefix and 16-byte operation prefix. It assigns 48 exact operations across 13
  frame families and rejects unknown version/family/operation/status/direction,
  nonzero reserved fields, invalid event/request direction, ambiguous
  correlation, and nonexact nested lengths.
- `alumina-storage` is `no_std` and uses RustCrypto SHA-256 without default `std`
  features. It defines typed executable/non-executable objects, fixed sequential
  chunks, a canonical `ACMF` V1 manifest hash, resumable constant-size upload
  checkpoints, verified-chunk and publish tokens, and explicit backend
  durability ordering.
- Upload plans, chunk prefixes, progress, and finalize requests have audited
  96/52/32/8-byte encodings. Golden vectors cover frame, operation, SHA-256, and
  canonical manifest bytes.
- Every durable mutation rejects boot/armed/energized or active-real-time state.
  Core-0 firmware owns `StorageServiceState`; core 1 has no storage coordinator
  or filesystem token.
- `alumina-sim` provides a rebootable in-memory SD image, orphan tracking,
  content scrub/readback, injected corruption, five exact power-cut boundaries,
  and deterministic concurrent prefetch/execution with a bounded queue and
  service-stall intervals.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run -q -p xtask -- check --board mks-tinybee
cargo run -q -p xtask -- check --board t-deck-pro
cargo run -q -p xtask -- build --board mks-tinybee --profile release
cargo run -q -p xtask -- build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
git diff --check
```

Results: all 62 portable unit tests and documentation tests pass with strict
host Clippy. The long-cache fixture uploads and atomically publishes 4,096 bytes
as 1,024 independently addressed chunks, then executes all 1,024 scheduled
blocks through a queue whose observed depth never exceeds four. A service stall
after two prefetched blocks faults at the third block's exact start cycle rather
than extending output. Corruption is detected by scrub and prefetch.
Both TinyBee and T-Deck Pro firmware targets check, link in release mode, and
pass strict ESP Clippy with the core-0 service coordinator. Because no hashing or
backend route is called yet, optimized ELF section totals remain 10,393 and
10,520 bytes respectively.

Power-loss tests cover: blob before journal, journal entry before checkpoint,
publish-pending before staging, staging before atomic visibility, and visible
manifest before journal cleanup. Reboot leaves a resumable transaction, an
unreferenced GC candidate, or the complete published object; no partial manifest
is readable.

## Claim boundary

The durable image is a behavioral model, not an implemented FAT/SD driver. No
HTTP server, WebSocket parser, ESP Wi-Fi AP/STA flow, credentials, request
authentication/rate limiting, physical SD write/rename/fsync mapping, update
signature, or real board I/O was exercised. SHA-256 currently uses the portable
software implementation; ESP accelerator integration is not claimed. The
firmware cache coordinator remains in boot-denied state because physical safe
outputs are not established. M3 remains open until the selected network/server
and SD backend reproduce these semantics under hardware load and fault tests.
