# M3 published-object reader evidence

Date: 2026-08-10

Status: host-tested and ESP release-linked immutable readback foundation. This
does not claim a machine-IR decoder, a core-0-to-core-1 block pool, executable
job admission, network read/export, physical SD testing, or HIL timing.

## Implemented claim

- `alumina-storage::media::PublishedReader` locates the newest committed
  publication matching an exact `(StoredObject, manifest)` pair. `StoredObject`
  includes the object kind, so bytes published as opaque data cannot be opened
  as a machine-job partition merely by reusing a digest.
- Opening a reader performs a bounded linear scan through the selected anchor's
  committed tail. It rechecks every record header, zero padding, commit digest,
  sequence, previous-record digest, upload transition, chunk identity,
  aggregate object digest, manifest digest, publication-to-begin reference, and
  final anchor facts. It builds no heap-backed publication directory.
- The opaque cursor binds the media ID, region, opening generation and tail,
  exact upload plan, begin record, selected publication record, next record
  sequence, and digest chain. Later append-only commits are allowed; a cursor
  cannot be moved to a different, reformatted, truncated, or older medium.
- Each read uses caller-owned `[u8; 1024]` storage. It verifies the next record
  and commit, upload ID, contiguous index, exact derived length, and SHA-256
  chunk identity before copying bytes. Unused output bytes are zeroed. The
  cursor cannot skip, rewind, or synthesize completion.
- The last chunk is withheld until the reader also rechecks the originally
  selected publication record and verifies the complete object and canonical
  manifest hashes. Earlier chunks are independently verified and are suitable
  for a future bounded prefetch horizon; core 1 still receives none in this
  checkpoint.
- An exact lookup miss returns `PublishedNotFound`, maps to native protocol
  `NotFound`, and leaves the cache ready. A device or integrity error during
  lookup/read faults `CacheMedia`; `ProvisionedCache` preserves the more precise
  `media-integrity` manager fault rather than collapsing it to a generic device
  failure.
- The manager-level API survives provision, upload, publication, reboot,
  locator discovery, and remount without exposing the physical adapter. The
  reader accepts append-only growth and detects corruption introduced after it
  was opened.

The lookup cost is proportional to committed cache blocks, and one chunk read
uses fixed stack-resident record workspaces. This is a deliberate V1 tradeoff:
bounded RAM and an auditable immutable log take priority over constant-time
enumeration. Runtime stack high-water and SD latency remain physical evidence
requirements before arming.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask check --board mks-tinybee
cargo xtask check --board t-deck-pro
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo tree --locked --offline --prefix none --format '{p}|{l}'
git diff --check
```

The default workspace has 131 passing unit tests. `alumina-storage` has 34,
including read-before/remount equivalence, exact typed misses, append-after-open,
post-open corruption, provisioned-manager reboot readback, and integrity-fault
latching. Host and both ESP strict Clippy gates pass; both board images check
and link in release mode.

Final release section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 654,304 | 11,768 | 250,368 | 63,476 |
| T-Deck Pro | 604,017 | 12,520 | 525,840 | 170,012 |

These are linked capacity observations, not runtime stack watermarks or
deadline evidence. This slice adds no dependency. Offline scans of the default,
TinyBee, and T-Deck Pro resolved graphs found no GPL, AGPL, LGPL, SSPL, or
missing-license entry. New implementation remains `MIT OR Apache-2.0`.

## Claim boundary and next evidence

No device was connected or flashed. The firmware does not yet invoke the reader,
parse a cached machine-job partition, enumerate publications, stream audit
bytes over HTTP, scrub in the background, compact a full log, or delete data.
The reader verifies storage chunks, not execution-block semantics. Its 1,024-byte
storage boundary is not itself the real-time queue format.

Next, define a canonical machine-IR byte stream and a fixed owned execution-block
envelope. Core 0 must incrementally decode that stream across arbitrary storage
chunk boundaries, validate capability/configuration identity, block sequence,
time continuity, and a block digest chain, then transfer only complete owned
buffers through explicit credits. Core 1 must independently validate each
envelope before extending its execution horizon. Simulator fault injection and
TinyBee Wi-Fi-plus-SD timing then determine the admitted pool depth; neither the
current eight-frame command queue nor an assumed SD rate is evidence for it.
