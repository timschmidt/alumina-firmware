# M10 transactional servo-bank activation evidence

## Claim

Implementation commit
`5ad072b38941d0ae038e8104d2eb3bac16f5f66b` makes initial activation of a
one-to-four-axis `ServoFocBank` one sealed complete-bank transaction. All axis
profiles, encoder seeds, boot-local activation identities, and neutral compare
images are prepared before any physical staging. No live bank value can be
returned until one configuration-bound `PwmCommitBankCompletion` proves the
exact complete initial latch set at the common first boundary and physical
period sequence zero.

The public API which joined independently activated axes has been removed.
Both initial `ServoFocBank::activate` and steady-state `ServoFocBank::commit`
now accept only the opaque completion value produced by
`PwmCommitBankBarrier`; neither accepts a caller-assembled commit array.

This is a portable type/state and deterministic-simulator checkpoint. It does
not configure or attach an MCPWM operator, read a hardware latch register,
qualify timer synchronization, or energize a power stage.

## Prepared complete-bank authority

`ServoFocBank::prepare_activation` accepts fixed arrays of profiles,
activation identities, encoder seeds, and one first `DeviceCycle`. Before it
returns `PreparedServoFocBankActivation<AXES>`, it proves:

- the width is between one and four axes;
- every boot-local activation identity is nonzero and pairwise distinct;
- every complete axis profile independently replays its configuration,
  calibration, clock, loop, encoder, current, and compare contracts;
- every encoder seed has the same configuration identity and is available and
  fresh enough at the first boundary;
- all axes share the configuration digest, exact nested servo grid, exact PWM
  period in device cycles, and first boundary; and
- every retained initial image is complete, neutral, configuration-bound, and
  scheduled at that sole first boundary.

The opaque candidate exposes only its identity, first boundary, PWM period,
individual image lookup, and a copied complete image vector. Its per-axis
controller candidates remain private and there is no output-write or enable
method.

Activation first validates every commit in the sealed completion against every
initial image before constructing any live controller. It then constructs all
controllers into a private temporary fixed array and performs the final common
bank validation before returning ownership. If a later internal activation
check rejects, earlier temporary values are dropped and no partial bank becomes
observable.

This software atomicity is not physical rollback. By the time activation is
called, the target reports that the complete hardware image set already
latched. Any subsequent logical rejection must cause the future enclosing
target owner to invoke its independently qualified all-stage safe transaction.
A neutral 50% duty image is also not, by itself, a safe electrical state; real
output attachment, dead-time, gate-driver behavior, shutdown, and startup order
remain physical qualification work.

## Sealed completion identity

`PwmCommitBankCompletion<AXES>` now retains the barrier's configuration digest
in addition to its common observed boundary, physical-period sequence, and
ordered complete commit array. Its fields remain private.

Initial bank activation requires:

- the exact prepared configuration digest;
- physical-period sequence zero;
- the exact prepared first boundary; and
- each expected image token, schedule, and observed boundary.

Steady-state bank commit requires the live bank's exact configuration digest
and next sequence before deriving any next controller. The existing per-axis
candidate-prefix and exact image checks still run. Thus a completion made by a
foreign barrier or one initialized at a substituted sequence cannot cross the
bank publication boundary even if its untyped numeric fields happen to look
similar.

## Canonical simulator activation

The two-axis simulator no longer constructs two independently active axes and
then joins them. It now:

1. lowers both selected slots into complete `ServoFocAxisProfile` values;
2. prepares one `PreparedServoFocBankActivation<2>`;
3. stages its complete neutral image vector in a barrier at cycle 80,000 and
   sequence zero;
4. records both modeled initial latch witnesses;
5. closes the complete initial barrier;
6. consumes that sealed completion to create the live bank; and
7. creates the ordinary period barrier for cycle 84,000 and sequence one.

The canonical 5,072-byte MKS Configuration V6 document retains digest
`c2b780f774cccb37875bf9778819316e3d7d2e682f14aa74749b808478cd2b4b`.
Immediately after modeled activation, both axes are at cycle 80,000/sequence
zero and the empty runtime barrier admits only cycle 84,000/sequence one. The
existing one-period and 401-period cached replays then proceed through the same
sealed completion API and retain their prior exact results.

The activation regression proves that an absent axis-1 report returns
`MissingObservation { axis: 1 }`; duplicate axis-0 status returns
`DuplicateObservation { axis: 0 }`; a substituted axis-1 token returns
`Observation { axis: 1 }`; and an axis-1 report one cycle late at 80,001 returns
`Observation { axis: 1 }`. None returns a configured hardware-bank value.
Separate portable tests reject zero or five axes, zero and duplicate activation
identities, independently valid foreign configuration profiles, a foreign
completion identity, and a completion initialized at sequence one.

The simulator creates reports from supplied observations. It is not evidence
that an ESP peripheral produced them or that physical outputs were safe.

## Fixed-memory evidence

Host DWARF reports:

| Host type | Bytes | Alignment |
| --- | ---: | ---: |
| `PreparedServoFocBankActivation<2>` | 3,792 | 16 |
| `PreparedServoFocBankActivation<4>` | 7,536 | 16 |
| `PwmCommitBankBarrier<2>` | 432 | 8 |
| `PwmCommitBankCompletion<2>` | 96 | 8 |
| `ConfiguredServoFocHardwareBank<2>` | 4,272 | 16 |
| `ScheduledServoFocHardwareBank<2>` | 4,448 | 16 |

The executed maximum-width regression requires the four-axis activation value
to remain within the public 8,448-byte bound. The four-axis barrier and
completion retain their separately executed 1,024-byte and 256-byte bounds.
All values are inline and allocation-free.

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
  rg 'PreparedServoFocBankActivation|PwmCommitBankBarrier|PwmCommitBankCompletion|ConfiguredServoFocHardwareBank|ServoFocBank'

llvm-dwarfdump --name='PreparedServoFocBankActivation<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='PreparedServoFocBankActivation<4>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='PwmCommitBankCompletion<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='PwmCommitBankBarrier<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='ConfiguredServoFocHardwareBank<2>' \
  target/debug/deps/alumina_sim-4da3b50a8a9fe8de
llvm-dwarfdump --name='ScheduledServoFocHardwareBank<2>' \
  target/debug/deps/alumina_sim-4da3b50a8a9fe8de
```

The default-member listing contains 516 tests. Relevant totals are 76 FOC, 54
simulator, and 27 configuration tests. Formatting, diff checks, all host tests,
warnings-denied host Clippy, warnings-denied rustdoc, and warnings-denied Clippy
for all four ESP packages passed.

All four optimized release images retain the preceding section totals:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,984 | 12,432 | 249,712 | `b7a63fcd4f03803a1a5903b785db066c722015e4df21bd4a434e05d5f4edf151` |
| TinyBee V1.0, 4 MiB variant | 1,112,004 | 12,432 | 249,712 | `d10811759a41d1bf70be2d03947a6dfe313e52423bd8796478a1d06fec086874` |
| T-Deck Pro | 1,046,157 | 13,184 | 525,184 | `2b8e7c597c2070c59dd8fa44e303eb09ee1652357b2d5e7d9c66eb4c9ee7901d` |
| MKS ESP32 FOC V1.0 | 1,046,728 | 11,184 | 250,960 | `e8b87ec68acf8e73f0477cc920eb878500a0c0bbf9fa0f424dcd4a85464fc9ae` |

The hashes renew with source/debug identity. The optimized symbol scan finds no
prepared bank activation, barrier, completion, configured simulator bank, or
`ServoFocBank` symbol in any firmware image. Unchanged section totals and
absent symbols are expected while every target servo gate remains closed; they
are not target execution evidence.

## License and moving-Hyper boundary

This slice changes no manifest or lockfile and imports no third-party source.
Changed code contains no GPL-family, SimpleFOC, Synthetos/g2core, FluidNC, or
Klipper implementation. It remains repository-owned `MIT OR Apache-2.0` code.

This slice did not read, edit, format, pin, reset, or build Hypercurve and did
not change `alumina-interface`. Firmware still has no Hyper/CSGRS path
dependency. Continuing sibling Hypercurve edits do not alter this exact
firmware/simulator checkpoint.

## Open physical work

- Implement the sole core-1 aggregate owner which holds the prepared bank and
  barrier, stages all neutral images while outputs remain safely disconnected,
  aligns both MCPWM epochs, and attaches outputs only under a qualified startup
  procedure.
- Invoke and verify the complete all-stage safe transaction on every staging,
  observation, barrier, activation, or later logical-publication rejection.
- Establish truthful latch/readback semantics and measure cross-MCPWM skew,
  write/latch latency, startup timing, dead-time, ADC/encoder correlation,
  WCET/jitter, stack high-water, and shutdown latency.
- Replace synthetic stage qualification only after powered-board, load,
  watchdog, reset, brownout, and fault-injection evidence exists.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven. No motor,
driver, or process-power path was energized.
