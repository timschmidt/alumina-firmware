# M6 target exact-motion commit evidence

Date: 2026-08-11

Status: the portable cached step executor is now composed into the core-1
firmware lifecycle behind an exact generated-versus-physically-committed output
boundary. Qualified interlocks, cached work, configuration identity, and the
distributed local epoch jointly gate arm/start. This is compile and portable-
behavior evidence, not I2S/DMA timing, physical pulse, armability, or machine
qualification.

## Implemented boundary

`ShiftedCachedStepper` owns at most one pending complete shifted image. Every
new image receives a nonzero boot-local token and retains its exact scheduled
`DeviceCycle`. The target owner must apply that image once and return a post-
write upper-bound cycle with the same token. A wrong token, an observation
before the scheduled cycle, or a commit beyond the configured lateness bound
latches the coordinator without clearing the pending transaction. No later edge
can be generated and no cached-block token can return to the job actor until
the prior physical commit succeeds.

Normal terminal driver disable uses the same two-phase transaction. It cannot
be generated until the exact maximum enable-hold deadline derived from the last
falling edge and active configuration. An asynchronous fault follows the
opposite safety ordering: the selected board first applies its complete safe
physical transaction, then the motion owner invalidates pending commit tokens
and drops the now-unacknowledgeable block before the job/schedule and safety
actors latch their terminal state. Output-write, mapping, deadline, token, and
state errors all converge on that local fault path.

Core 1 constructs this owner only from its independently validated active
`RealtimeConfiguration`. The first step backend requires exactly the selected
board's dense executor width and rejects mixed stepper/FOC profiles. A confirmed
distributed schedule starts the owner at the exact local epoch and the exact
absolute machine-lattice position from `JobDescriptor` V2. Ownership crosses
back to `RealtimeJob` only after all events in the block are physically
acknowledged; the next cached block can then enter at the same logical boundary.
After the last block, schedule completion and the safety `Finish` transition
wait for the physically acknowledged terminal disable.

The real-time task now wakes at the minimum of its nominal 1 ms management
cadence, the next schedule transition, and the next exact motion edge. A
motion-only wake services the urgent mailbox first but does not invent extra
safety samples, management deadline samples, command drains, or periodic
telemetry. Equal-cycle output and block handoffs are bounded. A future deadline
returned at or before the current observation is treated as an execution fault.

The canonical prepare descriptor changes incompatibly and intentionally from
248-byte `ALMJOBD1` to 312-byte `ALMJOBD2`. It adds eight signed `i64` absolute
starting-position slots; every unused slot above `axis_count` must be zero. The
descriptor hash now covers bytes `0..280`, and the fixed intercore job command
grows from 272 to 336 bytes. No compatibility decoder or shim is retained. The
default cross-core payload storage is 13,120 bytes and its stack-plus-boundary
budget is 45,888 bytes, still below the reviewed 64 KiB internal-memory limit.

TinyBee exposes its exact 24-bit shifted-image contract to this composition and
can write a complete image through the existing blocking static bootstrap
transport. That writer has no bounded motion-commit latency claim and is
explicitly marked unqualified; `MOTION_OUTPUT_QUALIFIED` is false and the board
package remains non-armable. T-Deck Pro reports no machine-output contract and
rejects every motion image. Thus this target wiring creates no reachable motion
authority on either current package. A hardware-timed I2S/DMA serializer and
logic-analyzer qualification remain required.

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

The default workspace has 211 passing unit tests. The focused motion suite has
19 tests, including block-release-after-commit, wrong-token latching, early and
late physical-commit rejection, and exact terminal enable-hold behavior. Strict
host Clippy, both complete ESP firmware Clippy targets, and both optimized links
pass. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 865,420 | 11,976 | 250,160 | 35,428 |
| T-Deck Pro | 804,497 | 12,728 | 525,632 | 142,332 |

These are linked-capacity observations, not runtime stack watermarks or timing
measurements. The increased text includes the target executor, exact image
mapping, commit validation, and failure coordination. BSS aggregate is unchanged;
the linker-computed stack reserve remains positive on both targets.

Sorted default, all-feature workspace, TinyBee, and T-Deck Pro cargo-tree
inventories contain 64, 323, 234, and 241 nonempty package/license records.
None has a missing license or a GPL/AGPL/LGPL/SSPL-family license. The dependency
expressions are permissive MIT/Apache-compatible choices, including BSD,
ISC/0BSD, Zlib, Unicode-3.0, BSL-1.0, and Unlicense alternatives. An
implementation-source/header/manifest scan is also clear. New code is
repository-owned `MIT OR Apache-2.0`; no GPL-family source or behavioral
implementation was imported. `cargo-deny` remains configured in CI but is not
installed locally, so this checkpoint does not claim a local `cargo deny` run.

## Closed physical claims

Neither available board was connected, flashed, or energized. No TinyBee word
order, latch phase, edge time, update rate, deadline bound, pulse width,
direction setup/hold, safe image, input response, or service-load isolation was
measured. The static bootstrap writer is unsuitable as a qualified realtime
serializer and its zero-lateness setting would fail any physical call; the
unqualified arm gate makes that call unreachable. Constrained hold/resume,
underrun deceleration, execution telemetry, observed-start capture, lease
renewal, and cached-autonomous execution remain open. Both current board
packages remain non-armable.
