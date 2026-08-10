# M3 machine-block ownership boundary evidence

Date: 2026-08-10

Status: host-tested and ESP release-linked canonical work-block and ownership
queue foundation. This does not claim firmware job preparation, arming,
execution, pulse generation, a qualified queue horizon, physical SD behavior,
or HIL timing.

## Implemented claim

- A machine-job partition is now a nonempty concatenation of canonical
  512-byte `ExecutionBlock` values. The immutable storage object's byte length
  determines the exact block count and its SHA-256 identity commits the whole
  concatenation; blocks do not embed a circular partition-content digest.
- Each block binds `ALMBLK01`, exact machine-IR version, motion kind, axis width,
  contiguous sequence, record count/length, inclusive/exclusive stream ticks,
  nonzero stream ID, board-capability digest, active-configuration digest, and
  previous-block digest. Eight reserved header bytes and unused payload bytes
  must be zero. SHA-256 over bytes `0..480` occupies bytes `480..512`.
- A motion record stores exact `u64` duration, zero flags/reserved fields, and
  one signed `i64` lattice displacement per axis. The fixed 320-byte payload
  admits up to eight 3-axis or four 8-axis records. Durations must be nonzero,
  contiguous by construction, and sum exactly to the block interval.
- `ExecutionBlock` is exactly 512 bytes and intentionally implements neither
  `Copy` nor `Clone`. Its fields are private. Construction and decode enforce
  canonical structure and the block digest before ownership can be transferred.
  A golden fixture fixes the complete block hash for browser/native parity.
- `PartitionAssembler` accepts arbitrary verified byte slices, retains at most
  one partial block, and emits at most one owned block per call. It requires the
  published object length to be a nonzero exact 512-byte multiple and refuses
  incomplete or excess data. The storage chunk layout is not the RT schema.
- `MotionStreamValidator` checks each block against prepared identities,
  exact-next sequence, previous digest, relative-tick continuity, block/segment timing
  limits, per-axis displacement limits, and cumulative cross-block `i64`
  position. Its state advances only after complete validation. Core 0 and core 1
  instantiate separate validators over the same moved bytes.
- `StreamTick` is a distinct relative-time type. It cannot be confused with the
  absolute `DeviceCycle` supplied by a later deterministic commit; mapping to
  hardware time uses checked epoch addition and rejects overflow.
- `alumina-runtime` adds a dedicated inline `WorkQueue<N>` to the existing
  command, urgent, fault, and telemetry paths. The queue's free slots are the
  core-0 producer credits. A failed nonblocking send returns the still-owned
  block; a receive moves ownership into core 1. No pointer, reference, SD
  address, allocator object, or peripheral handle crosses the boundary.
- The provisional default work depth is eight, reserving 4,096 payload bytes.
  The complete default command/work/telemetry payload rings occupy 12,480 bytes;
  together with the declared 32,768-byte application-core stack, the static
  runtime budget accounts for 45,248 bytes. Depth eight is not yet evidence of
  sufficient time horizon.
- `alumina-sim` runs the real append-only `CacheMedia` and published-object
  reader with deliberately unaligned 700-byte upload chunks, assembles three
  execution blocks, independently validates them on both sides of a two-credit
  work queue, and obtains identical terminal cycle, position, and chain digest.
  A separately uploaded object whose storage hash is valid but whose embedded
  block bytes were changed fails machine-IR digest validation before enqueue.

The V1 block intentionally carries short integer-displacement segments rather
than source geometry or G-code. The authoritative browser will perform exact
path/constraint work and emit certified short schedules; firmware will later
interpolate them into hardware events and calculate only bounded local
hold/stop behavior.

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

The default workspace has 139 passing unit tests. `alumina-machine-ir` has nine,
`alumina-runtime` has eight, and `alumina-sim` has eleven. Host and both ESP
strict Clippy gates pass; both current board images check and link in release
mode.

Final release section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 654,056 | 11,768 | 250,368 | 59,332 |
| T-Deck Pro | 604,461 | 12,520 | 525,840 | 165,860 |

The aggregate BSS remains fixed by each linker layout, while the inline work
queue and its bookkeeping reduce the residual linker `.stack` section by about
4.1 KiB compared with the preceding reader checkpoint. These linked numbers are
not runtime watermarks. Actual core-0/core-1 stack high-water, channel-copy
critical-section duration, queue timing horizon, and interrupt latency remain
mandatory HIL measurements.

No new third-party package entered the lockfile. `sha2` was already resolved and
is `MIT OR Apache-2.0`; new crate edges only reuse it and internal Alumina crates.
Offline scans of the default, TinyBee, and T-Deck Pro graphs found no GPL, AGPL,
LGPL, SSPL, or missing-license entry.

## Claim boundary and next evidence

No board was connected or flashed. Firmware statically reserves the work ring
but does not yet open a publication, assemble blocks, install a prepared stream,
or consume work on core 1. The current deadline task remains a synthetic probe;
no block drives GPIO, I2S, RMT, PWM, current loops, or process outputs.

Next, add a bounded job-prepare lifecycle that names the exact published
partition, stream ID, capability/configuration digests, first cycle, block count,
and validation limits. A sole core-0 prefetch actor will own `ProvisionedCache`,
the reader, assembler, and service validator, and will fill credits only for the
prepared job. Core 1 will own the second validator and publish queue-horizon and
low-water/fault telemetry. Cancellation, stale identities, malformed blocks,
reader faults, and underrun must revoke work without extending output. Only
after that portable state machine passes simulator faults should either board
wire it to hardware-timed motion.

That portable successor is now implemented and recorded in
[`M3-JOB-PREFETCH-LIFECYCLE.md`](M3-JOB-PREFETCH-LIFECYCLE.md). This file retains
the narrower claim boundary of the commit it documents.
