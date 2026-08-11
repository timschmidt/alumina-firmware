# M3 cached-job prefetch lifecycle evidence

Date: 2026-08-10

Status: portable state-machine and target-compile evidence. This does not claim
an authenticated firmware job endpoint, distributed commit, motion output, or
physical hardware qualification.

This records the portable lifecycle milestone at its commit. The later
[`M3-FIRMWARE-JOB-PREFETCH.md`](M3-FIRMWARE-JOB-PREFETCH.md) instantiates these
actors in both target images and supersedes the claim boundary and image/test
totals below; deterministic commit and output remain closed.

## Implemented boundary

`alumina-job` now joins the immutable publication reader, canonical machine
blocks, and credited cross-core work ring without giving either core authority
it should not possess.

- `JobDescriptor` binds a nonzero boot-local prepare correlation, the exact
  typed `PublishedObject` and manifest, stream identity, board-capability and
  active-configuration digests, compile-time axis width, object-derived block
  count, first relative tick, and nonzero machine limits. A full V1 partition
  must start at `StreamTick(0)` and be a nonempty exact multiple of 512 bytes.
- `StreamTick` and `DeviceCycle` are distinct types. Cached bytes contain only
  relative ticks; checked `StreamTick::at_epoch` conversion can occur only after
  a later deterministic commit supplies an absolute local epoch. Addition
  overflow is rejected.
- `ServicePrefetch` is the sole owner of the open `PublishedReader`, 1024-byte
  caller buffer, partial `PartitionAssembler`, service-side
  `MotionStreamValidator`, and at most one complete unsent block. Each `step`
  reads no more than one SD chunk, yields on missing ring credit, and retains the
  exact block rather than cloning, dropping, or rereading it.
- The service actor declares completion only after the publication cursor,
  assembler, validator, and sent-block count all independently reach the same
  exact terminal boundary. A media or machine-IR error transitions it to
  `Faulted` and clears partial service-side bytes. Cancellation clears unread
  and unsent service state.
- `RealtimeJob` owns a second validator with no storage access. It admits at
  most one block at a time and returns an opaque owned `AdmittedBlock` to the
  future hardware engine. Completion advances only when that same prepare ID,
  sequence, block digest, and cumulative progress are acknowledged. Validation
  or token divergence faults closed. Cancelled/faulted jobs can drain queued
  blocks before another prepare.
- The runtime queue remains the ownership boundary: free slots are producer
  credits, queued slots belong to neither actor, and core 1 receives the only
  owned value that may become executable.

## End-to-end simulation

The `alumina-sim` integration test builds three chained 512-byte motion blocks,
uploads them as 700, 700, and 136-byte chunks, explicitly provisions a raw cache
region through the locator protocol, and opens the resulting exact
publication. This intentionally makes storage and execution boundaries
unaligned.

Two service steps fill a two-block work ring. A third step returns
`Backpressured` without another media read or loss of ownership. Core 1 then
takes and acknowledges blocks through its independent validator while core 0
resumes. Both actors finish with three blocks, terminal `StreamTick(300)`, and
the exact lattice displacement `[6, -3, 0]`; the reader reports exactly three
storage chunks. The existing corruption test independently proves that a
storage-valid object with a damaged execution-block digest never reaches the
work queue.

Unit tests additionally reject wrong object kinds, zero identities, mismatched
axis widths, invalid partition lengths/counts, nonzero initial stream ticks,
wrong block order, acknowledgement-token divergence, and epoch overflow. They
exercise cancellation followed by bounded ring draining.

## Reproducible checks

Run from the repository root:

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

The default workspace has 143 passing unit tests. `alumina-job` has four,
`alumina-machine-ir` has nine, `alumina-runtime` has eight, and `alumina-sim`
has eleven. Host and both ESP strict Clippy gates pass, and both current board
images check and link in release mode.

Final release section totals are unchanged because the firmware dependency is
compiled but its actors are not instantiated yet:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 654,056 | 11,768 | 250,368 | 59,332 |
| T-Deck Pro | 604,461 | 12,520 | 525,840 | 165,860 |

These linked values are not runtime watermarks. Stack high-water, queue
critical-section duration, SD/Wi-Fi contention, and interrupt latency remain
mandatory HIL measurements.

No third-party package entered the lockfile. `alumina-job` is repository-owned
and dual-licensed `MIT OR Apache-2.0`; it depends only on other Alumina crates
and already-approved Embassy primitives. Offline scans of the default (89
entries), TinyBee (469), and T-Deck Pro (481) dependency graphs found no GPL,
AGPL, LGPL, SSPL, or missing-license entry.

## Claim boundary and next evidence

No board was connected or flashed. The firmware now target-compiles
`alumina-job`, but its core-0 task does not yet instantiate `ServicePrefetch`,
its core-1 task does not yet instantiate `RealtimeJob`, and no authenticated
`JobPrepare` request can install a descriptor. The deadline loop remains a
synthetic probe, and no admitted block drives GPIO, I2S, RMT, PWM, FOC, or a
process output.

Next, define canonical prepare/cancel/status inter-core and Wi-Fi bodies, make
the firmware service task own the prefetch actor alongside `ProvisionedCache`,
make the real-time task own the independent admission actor, and publish exact
queue horizon and terminal/fault state. Commit/start remains a separate later
gate: it must bind a fresh boot identity and nonce, absolute future
`DeviceCycle`, finite lease, safety state, and locally complete prefetched
horizon before any hardware scheduler can arm.
