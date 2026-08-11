# M3 canonical board-capability evidence

Date: 2026-08-10

Status: canonical format, portable tests, authenticated target wiring, strict
compile, and optimized link evidence. This does not claim configuration commit,
armability, physical visual reconciliation, or HIL.

## Implemented authority

`alumina-capability` defines one allocation-free, platform-independent
`ALMCAP01` V1 serialization for the complete typed `BoardPackage`.

- The 16-byte header fixes magic, exact version, zero reserved bytes, and total
  length. Strings are length-prefixed UTF-8 bytes, counts are `u32`, integers are
  little-endian, booleans are exact, resources have a fixed four-byte typed
  representation, enum numbers are explicit, and options have zero-filled
  absent forms. Rust layout and JSON ordering never enter the identity.
- The payload binds board/revision, chip/cores, qualification/armability,
  memory, every typed resource and alias, bus/device topology, flash regions,
  clocks, electrical constraints, interrupts, safe-output images, visual asset
  metadata/hotspots, and HIL requirements in declared order. Only the package's
  own digest field is excluded to avoid circular identity.
- Identity calculation first structurally validates the package, counts without
  allocation, then streams header and payload through SHA-256. Range reads
  recompute and compare the package's declared digest before re-encoding only
  the requested caller-buffer overlap. Offset/length arithmetic is checked.
- `xtask board check` now independently requires that recomputation to match the
  compiled digest. Its JSON output distinguishes declared/calculated digest,
  document bytes, and verification result.

Current exact documents are:

| Board | Bytes | Compiled and recomputed SHA-256 |
| --- | ---: | --- |
| MKS TinyBee V1.x | 3,251 | `000f151d9a404a94d82b311ab4033db23fe65e56c48d8a1d43bd773662c7c351` |
| T-Deck Pro | 2,605 | `617a1b62b7e7f68762a8950ebe582f47bfd20b66d8cd052363a4d841e08eec10` |

These identities conservatively change with qualification, armability, or
presentation metadata as well as electrical/execution facts. The format and
complete field order are normative in [`CAPABILITIES.md`](../CAPABILITIES.md).

## Authenticated range boundary

Public `/api/v1/identity` now advertises the capability digest and byte length.
Document bytes remain behind the existing HMAC/replay/rate-limited
`POST /api/v1/control` route:

- `CapabilitiesGet` takes one canonical 56-byte `ALMCPQ01` request containing
  offset, `1..=240` byte budget, and an expected digest (zero only for initial
  discovery). Its outer configuration digest must be zero.
- The response is a 64-byte `ALMCPR01` prefix plus exactly the declared range.
  It repeats complete length/digest and binds offset, chunk length, and exact
  completion. Prefix plus 240 bytes fits the existing 312-byte native response
  body without increasing service buffers.
- Firmware verifies the selected package identity before Wi-Fi starts. Each
  service range independently re-verifies it; unknown family/operation,
  malformed/reserved forms, stale digest, out-of-range offset, empty result, or
  internal package divergence returns a correlated failure rather than bytes.

The browser must require contiguous stable ranges and a final complete SHA-256
match before parsing or caching by identity. The response HMAC authenticates
each transport body; the document digest provides immutable content identity.

## Execution remains closed

Publishing a capability document grants no machine authority. Job admission now
requires all of:

1. the exact compiled nonzero capability digest;
2. a separately committed nonzero active-configuration digest known to both
   cores; and
3. an armable board package.

Both first boards remain `armable: false`, and both core-local job services start
with a zero active-configuration identity. `JobPrepare` therefore still returns
`Unsupported` before storage is opened. Core 1 repeats those checks if a command
ever reaches it. Capability discovery cannot acknowledge or execute a block.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask board check mks-tinybee-v1
cargo xtask board check t-deck-pro
cargo xtask capabilities --board mks-tinybee --json
cargo xtask capabilities --board t-deck-pro --json
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
git diff --check
```

The complete default workspace has 150 passing unit tests. Four focused tests
cover request/response canonicality and reserved bytes, repeatable real-board
documents, arbitrary 73-byte range reconstruction with independent final
SHA-256, compiled-digest verification, missing digest rejection, and exact-end
range behavior. Host and both target strict Clippy gates pass; both release
images link.

Final section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 718,892 | 11,880 | 250,256 | 53,444 |
| T-Deck Pro | 668,657 | 12,648 | 525,712 | 160,004 |

Relative to the firmware-prefetch checkpoint, canonical serialization,
boot verification, identity reporting, and range dispatch add 15,700 bytes of
TinyBee text and 15,184 bytes of T-Deck Pro text. Data grows by 8 and 24 bytes;
aggregate BSS does not grow. These are linked capacities, not stack high-water
or service/core deadline measurements.

No new registry package entered the lockfile: the new repository-owned crate is
`MIT OR Apache-2.0` and reuses the already admitted MIT/Apache-2.0 `sha2` graph.
The cargo-deny CI allowlist continues to exclude GPL-family implementation
dependencies.

## Claim boundary and next gate

No physical client fetched these ranges and no board was flashed. HIL must
measure capability hashing/serving under Wi-Fi load, response authentication,
timeouts/retries, stack/heap watermarks, and concurrent core-1 deadline latency.
Current board visuals remain empty pending licensed revision-specific photos.

The next gate is a canonical transactional machine-configuration document with
resource/capability validation, durable idle-only commit, boot recovery, and an
independent core-1 active-identity update. It must remain distinct from the
full immutable board document and must not make either board armable without its
physical qualification gates.
