# M10 simultaneous PWM commit-barrier evidence

## Claim

Implementation commit
`4e3cc8b759a56052e1475bff9a718882039d9a30` adds a portable,
allocation-free correlation barrier for one to four physical PWM stages. One
complete vector of controller-prepared compare images is admitted for the sole
next common timer-zero boundary. Per-stage latch witnesses may arrive in any
order, but no partial `PowerStageCommit` vector becomes visible. Only the exact
complete set advances the barrier boundary and physical-period sequence.

The two-axis simulator now places this barrier between pure
`ServoFocBank::prepare` and logical `ServoFocBank::commit`. A controller bank
therefore cannot publish estimator, cascade, current-controller, compare-owner,
or sequence state merely because an earlier physical stage reported success.

This is a fixed-memory software ownership and correlation checkpoint. It is not
MCPWM register readback, a proof that two timers latch simultaneously, a
qualified peripheral owner, an electrical plant, or powered-board evidence.

## Fixed state and exact admission

`PwmCommitBankBarrier<AXES>` supports one through four axes and retains only:

- the nonzero configuration digest;
- the nonzero common PWM period;
- the sole next `DeviceCycle` boundary and `u64` physical-period sequence;
- at most one complete `[PwmCompareImage; AXES]` vector;
- at most one exact `PowerStageCommit` report per physical-axis slot; and
- the first terminal fault.

Staging is transactional. Every image must carry the barrier's configuration
digest and exact next boundary before any image is retained. `Busy` is the only
retryable rejection: it preserves the already staged complete vector. Foreign
identity, off-grid schedule, missing sequence, out-of-range axis, duplicate
report, substituted token/schedule/boundary, wrong close boundary, missing
report, arithmetic overflow, and safety invalidation all clear pending
authority and latch the first cause.

The target owner must write the exposed staged images through separately
qualified peripheral owners, service every stage's hardware status, submit a
truthful report for each physical slot, and call `finish_boundary` only after
that status pass. Completion checks every report before it returns one ordered
array. Checked arithmetic then selects the next boundary and sequence. There is
no allocation, dynamic dispatch, lock, partial completion iterator, or output
enable/write method in this type.

The barrier deliberately does not lower duty values a second time. Its inputs
are the complete exact integer images already validated and prepared by each
axis controller; at this seam it correlates their configuration identity,
schedule, opaque compare token, and observed physical boundary. A future target
owner can retrieve the exact staged vector, but this implementation cannot
claim that those values reached registers.

## Complete-bank publication order

The simulator's `ConfiguredServoFocHardwareBank<AXES>` now owns both the
logical `ServoFocBank` and one barrier initialized from that bank's digest,
common PWM period, current boundary, and next sequence. One simulated period is
ordered as follows:

1. construct every sensor/setpoint input against the unchanged live bank;
2. prepare the complete controller transition without publishing it;
3. stage every prepared compare image as one vector;
4. record each modeled physical latch witness;
5. close the sole common boundary and receive one complete ordered commit set;
6. pass only that set to `ServoFocBank::commit`.

The barrier advances when the physical acknowledgement set closes, before the
logical controller commit. That ordering reflects physical reality: output
hardware cannot be rolled back if a later software check fails. An unexpected
logical commit failure after physical completion therefore requires the
enclosing target's independently qualified safe-shutdown transaction; it must
never be described as atomic physical rollback. The current simulator's
prepared bank and acknowledgement set share the same unchanged prefix and the
normal path commits successfully.

## Canonical dual-axis and cached replay

The existing 5,072-byte canonical MKS Configuration V6 fixture, digest
`c2b780f774cccb37875bf9778819316e3d7d2e682f14aa74749b808478cd2b4b`,
continues to derive both configured FOC axes and the cached-servo schedule.

The one-period dual-axis replay closes both modeled latches at cycle 84,000.
Both logical controllers advance from cycle 80,000/sequence 0 to cycle
84,000/sequence 1, while the emptied barrier selects cycle 88,000/sequence 2.

The cached two-axis replay executes 401 current periods. Both controllers end
at sequence 401 and the empty, healthy barrier selects cycle 1,688,000 and
sequence 402. A one-cycle-late axis-1 report at 84,001 instead returns
`CommitBarrier(Observation { axis: 1 })`, clears the staged bank, leaves the
barrier at cycle 84,000, and leaves both controllers at cycle 80,000/sequence 0
without encoder-estimator or cascade advance. Separately completed safe
invalidation closes both the barrier and controller bank; subsequent execution
is rejected before preparation.

These observations are deterministic simulator witnesses. The simulator
constructs reports from supplied timer-zero observations and does not model
MCPWM registers, cross-timer skew, ADC hardware, encoder hardware, or shutdown
electronics.

## Fixed-memory evidence

Host DWARF from the warnings-clean test artifacts reports:

| Host type | Bytes | Alignment |
| --- | ---: | ---: |
| `PwmCommitBankBarrier<2>` | 432 | 8 |
| `PwmCommitBankCompletion<2>` | 64 | 8 |
| `ConfiguredServoFocHardwareBank<2>` | 4,272 | 16 |
| `ScheduledServoFocHardwareBank<2>` | 4,448 | 16 |

The four-axis regression executes explicit size assertions that
`PwmCommitBankBarrier<4>` is no larger than 1,024 bytes and
`PwmCommitBankCompletion<4>` is no larger than 256 bytes. Those conservative
portable caps are public alongside the maximum axis count. No heap footprint is
hidden behind either value.

## Reproduction

From the `alumina-firmware` repository at the implementation commit:

```sh
cargo fmt --all -- --check
cargo test --locked --offline
cargo test --locked --offline -- --list
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee-4mb \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings

cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release

llvm-size \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1
sha256sum \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1
llvm-nm -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg 'PwmCommitBankBarrier|PwmCommitBankCompletion|ConfiguredServoFocHardwareBank|ServoFocBank'

llvm-dwarfdump --name='PwmCommitBankBarrier<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='PwmCommitBankCompletion<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='ConfiguredServoFocHardwareBank<2>' \
  target/debug/deps/alumina_sim-4da3b50a8a9fe8de
llvm-dwarfdump --name='ScheduledServoFocHardwareBank<2>' \
  target/debug/deps/alumina_sim-4da3b50a8a9fe8de
```

The default-member listing contains 513 tests. Relevant totals are 74 FOC, 53
simulator, and 27 configuration tests. Formatting, diff checks, all host tests,
warnings-denied host Clippy, warnings-denied rustdoc, and warnings-denied Clippy
for all four ESP packages passed.

All four optimized release images linked with the same section totals as the
preceding canonical-configuration checkpoint:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,984 | 12,432 | 249,712 | `46313c46276fbf7935831725c8a7120ab4b7d30e6954cc2d376786a1d0447fea` |
| TinyBee V1.0, 4 MiB variant | 1,112,004 | 12,432 | 249,712 | `7aebd4f93c86df673224277290c96191ebc194116bc573dda061a7f7ecd88a15` |
| T-Deck Pro | 1,046,157 | 13,184 | 525,184 | `f2cb5da373f2481d715f0f78e0e5296e04e0ad701cd28d307cf400c4c0e52b5c` |
| MKS ESP32 FOC V1.0 | 1,046,728 | 11,184 | 250,960 | `b10c0eb191d5b313f25d9abd5f4fb14dc2c850bcd069d46cca5789e03bdc36b8` |

The hashes renew with the exact source/debug identity. The symbol scan finds no
barrier, configured simulator-bank, or `ServoFocBank` symbol in any optimized
firmware image. Unchanged section totals and absent symbols are expected because
this slice adds a portable contract and host simulator integration while all
target servo gates remain closed; they are not target execution evidence.

## License and moving-Hyper boundary

This slice changes no manifest or lockfile and imports no third-party source.
Changed code contains no GPL-family, SimpleFOC, Synthetos/g2core, FluidNC, or
Klipper implementation. It remains repository-owned `MIT OR Apache-2.0` code.

This slice did not read, edit, format, pin, reset, or build Hypercurve and did
not change `alumina-interface`. Firmware still has no Hyper/CSGRS path
dependency. Continuing sibling Hypercurve edits therefore do not alter this
exact firmware/simulator checkpoint.

## Open physical work

- Implement the sole core-1 target owner which stages the complete image vector,
  aligns both MCPWM epochs, reads each real latch status, and closes the barrier
  only after the full status pass.
- Define terminal handling if physical completion succeeds but logical commit
  rejects, including immediate invocation and verification of the qualified
  all-stage safe transaction.
- Measure cross-MCPWM timer-zero skew, compare-write and latch latency, ADC
  trigger alignment, encoder observation age, reporting latency, WCET/jitter,
  stack high-water, and safe-shutdown latency under service-core load.
- Replace synthetic stage qualification only after powered-board, load,
  watchdog, reset, brownout, and fault-injection evidence exists.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven. No motor,
driver, or process-power path was energized.
