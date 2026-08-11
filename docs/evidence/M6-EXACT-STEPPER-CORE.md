# M6 exact stepper core evidence

Date: 2026-08-10

Status: the portable allocation-free integer executor, independently derived
configuration profile, complete shifted-image mapper, canonical motion report,
and cached-block simulator integration are implemented. This is software
evidence, not an armability, I²S DMA, physical pulse, motion, safety-chain, or
machine qualification claim.

## Implemented boundary

The new `alumina-motion` crate consumes only independently validated machine-IR
segments and an exact profile retained from the hashed machine configuration.
It owns no timer, peripheral, output, or allocation. Before installing a segment
it checks:

- contiguous stream ticks, zero flags, epoch and signed-position overflow;
- configured maximum rising-edge frequency and pulse high/low duration;
- pulse containment within the segment horizon;
- direction setup/hold and driver enable setup/hold across segments; and
- cumulative step and segment counters.

For `n` steps in duration `d`, rising edge `k` is scheduled at the nearest
integer to `(2k + 1)d/(2n)`. Tests exhaust a bounded range of durations, counts,
and indices and prove the scaled rounding error is at most one half device tick.
Every completed segment also proves exact signed step totals and final lattice
position. A poll later than the configured bound faults without returning the
late edge or leaving a future deadline. The caller can then request one immediate
step-low/driver-disable logical transaction; normal completion separately
enforces configured driver hold time.

Configuration V1 now distinguishes `AxisEnable` from `AxisDisable`. That matters
for the TinyBee because the StepStick control outputs are active-high disables.
Step bindings carry high/low times and maximum frequency; direction and control
bindings carry setup/hold times. Both independent configuration validators derive
the same dense executable profile under the document digest; core 0 discards its
copy after semantic validation, while the core-1 configuration actor alone
retains candidate/active profiles. The executor rejects missing, sparse, mixed
stepper/FOC, zero-timing, or wrong-width profiles.

The `ShiftImageMapper` translates logical events into a complete configured
serializer image. It validates full bit coverage, engine/width, unique routes,
polarity, and a safe image in which every step is inactive and driver logically
disabled. Each transaction preserves unrelated bits and is applied only after
all mask and current-level invariants pass. The TinyBee fixture uses disable,
step, and direction bits `0/1/2`, `3/4/5`, and `6/7/8` for XYZ while preserving
E0/E1 and auxiliary outputs; its described safe image remains `0x001249`.

`RealtimeMotionReport` is a canonical fixed 128-byte snapshot with exact epoch,
stream tick, next deadline, positions, masks, completed segments, maximum
lateness, and 64-bit miss count. Unused axis fields and reserved bytes must be
zero, and decoding requires a byte-identical re-encoding.

Finally, the existing provisioned-cache simulator now passes both independently
validated cached work blocks through this executor. Its logical trace ends at
position `[6, -3, 0]` with rising-edge totals `[6, 3, 0]`, agreeing with the
machine-stream validator rather than acknowledging unexecuted blocks.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-motion \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-motion \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
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
git diff --check
```

The complete default workspace has 193 passing unit tests. Focused suites have
14 configuration, 11 motion, and 14 simulator tests. Host, both ESP firmware,
and both ESP motion-crate strict Clippy gates pass, and both optimized images
link. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 832,128 | 11,976 | 250,160 | 42,508 |
| T-Deck Pro | 782,329 | 12,728 | 525,632 | 149,084 |

These are linked capacity observations, not runtime stack watermarks or timing
measurements. The size gate caught an earlier design that copied executable
routing inside every configuration identity and consumed 12 KiB of each linker
stack reserve. The final design keeps identities compact, derives the profile on
both validators, and retains only candidate/active profiles in the core-1 actor.
That deliberate transactional state accounts for 3 KiB relative to the prior
checkpoint and is not duplicated through core-0 storage/status identities.

Sorted default, all-feature workspace, TinyBee, and T-Deck Pro cargo-tree
package/license inventories contain 63, 322, 230, and 237 nonempty records,
respectively. None has a missing license or a GPL/AGPL/LGPL/SSPL-family license;
the unique expressions are MIT/Apache-compatible permissive choices including
BSD, ISC/0BSD, Zlib, Unicode-3.0, BSL-1.0, and Unlicense alternatives. An
implementation source/header/manifest scan likewise has no GPL-family match.
`cargo-deny` remains configured in the repository and CI but is not installed
locally, so this checkpoint does not claim a local `cargo deny` result.

## License and claim boundary

New code is repository-owned `MIT OR Apache-2.0`. MIT, Apache-2.0, BSD, ISC, and
similarly permissive dependencies are acceptable after review. GPL, LGPL, AGPL,
SSPL, and other incompatible/copyleft implementation code, dependencies, assets,
or source checkouts are excluded. This work used the repository's independent
clean-room requirements and published electrical facts; it did not inspect or
copy Synthetos/g2, SimpleFOC, FluidNC, or other GPL-family implementation source.

No board was connected, flashed, or energized. The TinyBee serializer word/WS
phase, DMA buffering, edge timing, safe image, limit/E-stop response, service-load
isolation, and complete output trace still require logic-analyzer HIL. Both first
boards remain non-armable; this checkpoint creates no path that intentionally
drives physical motion.
