# M6 cross-block prefill evidence

Date: 2026-08-11

Status: portable strict two-block ownership, target-composed core-1 lifecycle,
and deterministic dense circular-DMA simulation. This is not ESP32 interrupt,
FIFO/WS, physical-latch, underrun, safe-reclaim, or machine qualification.
TinyBee and T-Deck Pro remain non-armable.

## Bounded admission is explicit

`RealtimeJob` now lends at most `REALTIME_BLOCK_WINDOW = 2` independently
validated blocks to the execution pipeline. The window is a strict FIFO of
private prepare/sequence/digest/progress facts. Core 1 may validate a successor
while the predecessor remains physically owned, but it may acknowledge only the
oldest exact token. An out-of-order acknowledgement, invalid successor, stream
validation failure, cancellation, or local hardware fault invalidates the whole
window. No invalidated token can later advance completed progress.

The existing fixed report carries the newest admitted prefix and last completed
prefix. Their block-count difference is the exact outstanding count and is
canonical only from one through two while state is `Admitted`; no compatibility
flag, shim, or negotiated legacy mode was added.

Firmware retains two pre-admitted blocks for every multi-block job. Both must be
present before `JobCommit` can install a schedule or the safety machine can arm.
At the abort guard, core 1 transfers the initial pair into the sole motion owner.
After the oldest block returns, the validator admits at most one replacement and
the motion service retains it as the next lookahead token. One-block jobs keep
the smaller one-token path without a fabricated second block.

## Each block has its own physical barrier

`ScheduledShiftedStepper` no longer ties block return to global output-ring
emptiness. When one logical trace completes, it records:

- the unique admitted block and exact terminal progress;
- the monotonic count of generated complete-image updates through that block;
  and
- its exact terminal device cycle, including an output-free tail or dwell.

The successor may then enter the cached executor and append images to the same
strictly increasing output timeline. The predecessor returns only when the
target-confirmed commit count reaches its recorded prefix and the terminal cycle
has independently been observed. Later-block images may remain generated,
staged, or queued. Completion barriers themselves remain FIFO, so later physical
progress cannot reorder job acknowledgement.

The final block is identified from its validated zero-based sequence, not from
the count of earlier tokens already returned. Normal driver disable is therefore
preplanned while earlier completion barriers may still exist. Job-complete still
requires both final-block acknowledgement and the separate physical commit of
that disable image.

## Continuous dense simulation

The portable motion fixture validates two chained 512-byte blocks, plans all
successor images before releasing the predecessor, and proves the first token
returns after exactly its own five-image prefix while four later motion images
remain queued. Both tokens then acknowledge in order to the same independent job
validator.

The `alumina-sim` fixture carries that trace through a four-frame externally
safe-prefilled `PcmShortDmaHorizon` and the independent bit-level PCM-short wire
observer. Every transmitted descriptor releases exactly one refill slot. Each
replacement frame crosses preview, target acceptance, serial reconstruction,
qualified latch observation, and sparse-token commit as separate facts. The
dense frame indices and boundaries remain consecutive across the logical block
boundary; the predecessor returns with successor output still queued; the final
block and normal disable complete separately; and neither DMA nor wire model
latches a fault.

The window is intentionally bounded. Until the oldest physical barrier returns,
no third block can enter. The authoritative compiler and board qualification
must therefore choose block duration and cached horizon so one successor gives
enough refill lead. If the initial pair cannot cover the required prestart
horizon, priming fails closed. This policy is software evidence, not a measured
TinyBee rate or interrupt-latency claim.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
llvm-size target/xtensa-esp32-none-elf/release/alumina-firmware \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware
git diff --check
```

The default workspace passes 234 unit tests, including 14 `alumina-job`, 26
`alumina-motion`, 11 `alumina-shift-register`, and 22 `alumina-sim` tests.
Strict all-target host Clippy, strict Clippy for both ESP targets, and both
optimized release links pass. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 860,464 | 11,976 | 250,160 | 29,964 |
| T-Deck Pro | 801,753 | 12,728 | 525,632 | 137,044 |

These are linked-capacity observations, not runtime stack watermarks or timing
evidence. No board was connected, flashed, or exercised.

## Licensing and closed source boundary

This checkpoint adds no third-party dependency. All implementation, tests, and
documentation are independently authored under `MIT OR Apache-2.0`. No GPL,
LGPL, AGPL, SSPL, copied implementation source, or incompatible asset entered
the workspace.

Sorted, deduplicated offline cargo-tree inventories for the default workspace,
all workspace features, TinyBee target, TinyBee safe HIL target, and T-Deck Pro
target contain 65, 323, 234, 234, and 241 package/license records. None has a
missing license or GPL/AGPL/LGPL/SSPL-family license. A Rust/C/header/Cargo
manifest scan of implementation and import trees is also clear. `cargo-deny` is
not installed locally; CI remains the authority for license, ban, and source
policy.
