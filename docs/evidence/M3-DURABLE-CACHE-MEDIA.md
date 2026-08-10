# M3 durable cache-media evidence

Date: 2026-08-10

Status: host-tested and ESP release-linked media/service foundation. This does
not claim a physical SD-SPI transport, card initialization, executable readback,
compaction, AP+STA operation, browser interoperability, or HIL durability.

This records the raw-media milestone at its commit. The later
[`M3-SD-SPI-TRANSPORT.md`](M3-SD-SPI-TRANSPORT.md) adds physical card protocol
and board composition and supersedes the adapter/status and current image/test
totals below.

## Implemented claim

- `alumina-storage::media` is a bounded `no_std` asynchronous 512-byte block
  backend. A caller supplies an exact device subregion, cache limits, and a
  nonzero media identity. Mount never formats or expands that region; formatting
  is a separate, explicitly destructive provisioning operation.
- Cache media V1 reserves two anchor sectors and stores an append-only sequence
  of `Begin`, `Chunk`, `Publish`, and `Abort` records. Each fixed header binds
  kind, sequence, length, upload identity, and the preceding record digest.
  Zero-padded payload sectors and the header are SHA-256 hashed; a separate
  commit sector binds their digest and physical start.
- A mutation writes header/payload, synchronizes, writes its commit sector,
  synchronizes, writes the older alternating anchor with a new generation and
  committed tail, then synchronizes again. It returns success only after the
  final barrier. Any device write/sync error faults the live instance and forces
  remount before retry.
- Mount validates both hashed anchors, region geometry, frozen cache limits,
  every reserved byte, record sizing/padding/commit, monotonic sequence, digest
  chain, upload transition, chunk SHA-256, aggregate object identity, canonical
  manifest identity, publication reference, and exact tail. Bytes beyond the
  selected committed tail are ignored and can be overwritten. One torn peer
  anchor falls back to the prior complete generation and is reported degraded.
- The V1 record workspace admits a maximum 1,024-byte chunk without allocation.
  Firmware policy admits one sequential upload, objects through 64 MiB, and up
  to 65,536 chunks, additionally bounded by remaining region sectors.
- `alumina-service` now dispatches authenticated native status, begin, chunk,
  and finalize requests through an async `StorageBackend`. The concrete
  `CacheMedia<D>` implementation maps storage/media failures to protocol status,
  returns exact 32-byte progress or 72-byte backend status bodies, and derives
  mutation availability from both mounted state and core-1 safety/job facts.
  Malformed lengths and chunk digest mismatches are rejected before backend I/O.
- The two current firmware compositions instantiate an explicit
  `UnavailableStorageBackend`. Consequently they cannot acknowledge a mutation
  before a physical board adapter exists; the same core-0 service path can own a
  mounted `CacheMedia` without exposing a media handle to core 1.
- `alumina-sim` exposes the same raw block contract through cloneable exact
  snapshots, a persistent-operation counter, one-shot cuts on write or sync,
  configurable torn-sector prefixes, power restoration, and byte corruption.
  Its maximum-size chunk fixture cuts every header/payload/barrier/commit/anchor
  position and observes only the old or complete new committed progress.

The authoritative job cache is deliberately not FAT or another general-purpose
filesystem. A separate human-readable exchange partition can be evaluated later,
but it will not define executable visibility or weaken the raw cache's commit
rules.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask check --board mks-tinybee
cargo xtask check --board t-deck-pro
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo tree --locked --offline --prefix none --format '{p}\t{l}'
git diff --check
```

The complete default workspace has 92 passing unit tests. Of these,
`alumina-storage` has 21 tests, including exhaustive cut sweeps around begin,
chunk, publication, interrupted format, torn-anchor fallback, committed-byte
corruption, region isolation, and fail-closed mutation. `alumina-sim` has nine
tests, including the reusable maximum-record torn-write sweep and bounded
prefetch/starvation behavior. Host and both ESP strict Clippy gates pass; both
board images check and link in release mode.

Final release section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 573,116 | 10,176 | 251,968 | 73,492 |
| T-Deck Pro | 524,553 | 10,992 | 527,364 | 179,948 |

The aggregate BSS includes linker-reserved stack and the intentional 65,536-byte
reclaimed radio heap. These are linked capacity baselines, not runtime
watermarks or deadline evidence.

The dependency lock gained no third-party package in this slice. Runtime
`heapless` and test-only `embassy-futures` were already locked and are
`MIT OR Apache-2.0`. The offline default graph contains only permissive licenses;
the existing deny policy excludes GPL-family implementation dependencies.

## Claim boundary and next evidence

No device was connected or flashed. The media implementation currently has no
board SD-SPI adapter, card detect/hot-removal policy, card initialization,
capacity-derived region provisioning, hardware write-busy timeout, or physical
power-cut evidence. Firmware therefore reports `unavailable` and cannot format
or mutate a card. Formatting also has no network endpoint, intentionally.

Publication is replayable and counted, but executable object enumeration,
random chunk readback, audit export, scrub, garbage collection, deletion, and
compaction are not implemented on raw media yet. A full append-only region
fails closed with `Capacity`; it is never silently reformatted. Distributed job
prepare/start remains out of scope for this slice.

Next, implement and host-test the bounded SD SPI-mode transport, compose it from
each board's declared bus/chip-select/card-detect resources, mount only an
explicit provisioned region, and keep firmware non-armable on absent,
unformatted, changed, or corrupt media. HIL must then cut power at every barrier,
exercise removal/busy/CRC/timeout faults, measure concurrent Wi-Fi and prefetch
load, and confirm core-1 deadline isolation before cached execution is admitted.
