# M6 circular-DMA horizon evidence

Date: 2026-08-11

Status: portable exact circular-ring ownership, deterministic host simulation,
and an unreachable compile-only TinyBee HAL binding. This is not physical
ESP32 I²S/FIFO/WS timing, a DMA interrupt qualification, safe static/stream
handoff, starvation behavior, armability, or machine evidence.

## Exact ring ownership

`PcmShortDmaHorizon<TAG, UPDATES, FRAMES>` models a circular PCM-short TX ring
without importing target or motion types. Construction requires a nonempty ring
that an external safe transaction has already filled with `FRAMES` copies of
the complete safe image. The owner records those frames as hardware-owned; it
does not claim to have observed them on a pin.

The target-facing refill transaction is deliberately two phase:

1. the target reconciles its current whole-frame released capacity;
2. the portable owner previews the one exact next dense frame;
3. the target pushes exactly those four bytes into a released descriptor; and
4. only successful target acceptance advances the sealed horizon and consumes
   one refill credit.

Repeated previews return the same frame. A different acceptance, missing
preview, descriptor-order mismatch, capacity above the ring, or availability
shrink not caused by the recorded acceptance latches a fault. Checked cumulative
release/seal counters detect consumption beyond the known hardware-owned
horizon. Sparse complete-image updates remain strictly ordered and cannot target
an already sealed frame. They become materialized only when their exact dense
frame is accepted.

DMA descriptor release is not physical commit authority. The model accepts a
separate exact, frame-aligned, monotonic latch observation. An observation beyond
the sealed dense horizon is an underrun; regression is an ordering fault. Only a
tag that is both materialized and covered by that independent observation can
leave `take_commit`. An external peripheral or safety fault invalidates every
tag and permanently closes the owner.

## Terminal disable retains hardware lead

A circular serializer refills a physical slot several frames before that slot
is transmitted. Waiting for the final motion token to commit before scheduling
normal driver disable would therefore lose the required lead. The scheduled
motion owner now exposes the earliest legal finish while the logically complete
final block and all physical output ownership remain retained. Firmware stages
that disable before allowing the final block to return.

Complete images occupy strictly increasing output-grid boundaries. If a
zero-hold disable would otherwise share the last step image's boundary, the
earliest finish advances by one exact output quantum. A unit fixture with a
falling step edge at cycle 108 proves that cycle 108 rejects as duplicate and
cycle 112 is accepted. The firmware also retries insertion when refill capacity
has not yet reached the finish boundary; reaching that boundary without having
preplanned the disable fails closed.

Multi-block continuous prefill is deliberately not claimed. The current job
handoff admits the next block only after the prior block's physical drain, so a
qualified motion board must remain non-armable until cross-block planning owns
that boundary or the accepted machine policy proves a harmless gap.

## Circular host simulation and TinyBee compile surface

The new `alumina-sim` fixture uses a 1 MHz device domain, a 250 kHz exact frame
grid, and a four-frame safe-prefilled physical ring. It plans one canonical
block plus normal disable, stages every opaque motion token into the DMA owner,
and then repeats this loop:

- remove and serially consume one physical frame;
- independently reconstruct the complete image at the following latch;
- install that latch observation and retire only its covered tokens;
- report one released descriptor, preview/push/accept its exact replacement;
  and
- return the replacement to the physical ring.

The visible final image has drivers disabled before the unique block is returned
and acknowledged. The simulation keeps descriptor release, dense acceptance,
wire reconstruction, and commit observation as four separate facts.

TinyBee's unreachable HAL module now uses 256 four-byte TX descriptors for its
256 safe-prefilled internal-SRAM frames. It compiles operations that read exact
whole-frame circular availability and push one already planned frame, with the
portable `OutputCommitToken` retained as an opaque tag. No boot, runtime, fault,
or arm path can construct this owner. Descriptor EOF still does not identify the
physical FIFO/WS phase; logic-analyzer capture and a qualified independent
observation source remain mandatory.

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
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree --locked --offline --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32-none-elf \
  --no-default-features --features board-mks-tinybee --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32s3-none-elf \
  --no-default-features --features board-t-deck-pro --locked --offline \
  --prefix none --format '{p}|{l}'
llvm-size target/xtensa-esp32-none-elf/release/alumina-firmware \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware
git diff --check
```

The focused suites pass 25 `alumina-motion`, 11
`alumina-shift-register`, and 21 `alumina-sim` tests; the default workspace
passes 232 unit tests. Strict all-target host Clippy, strict Clippy for both ESP
targets, and both optimized release links pass. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 858,724 | 11,976 | 250,160 | 32,076 |
| T-Deck Pro | 800,197 | 12,728 | 525,632 | 139,044 |

These are linked-capacity observations, not runtime stack watermarks or timing
evidence.

## Licensing and closed source boundary

This checkpoint adds no third-party dependency. Its implementation and
documentation are independently authored under `MIT OR Apache-2.0`. The pinned
`esp-hal` surface consulted for API integration is `MIT OR Apache-2.0`; other
resolved dependencies remain subject to the repository's reviewed permissive
allowlist. GPL, LGPL, AGPL, SSPL, copied implementation source, and assets under
those license families are excluded.

Sorted, deduplicated offline cargo-tree inventories for the default workspace,
all workspace features, TinyBee target, and T-Deck Pro target contain 65, 323,
234, and 241 nonempty package/license records. None has a missing license or a
GPL/AGPL/LGPL/SSPL-family license. A Rust/C/header/Cargo-manifest scan of the
implementation and import trees is also clear.

No GPL-family repository or implementation source was fetched, cloned,
vendored, or consulted for this work. Functional hardware facts came from the
Espressif documentation and the already locked local permissive HAL source
listed in `docs/SOURCES.md`. `cargo-deny` remains the CI authority for license,
ban, and source policy; if it is unavailable locally, this checkpoint makes no
local `cargo deny` claim.
