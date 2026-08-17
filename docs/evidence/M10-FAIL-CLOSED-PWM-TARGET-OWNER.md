# M10 fail-closed aggregate PWM target-owner evidence

## Claim

Implementation commit
`4c757efc9966c8a6b77792c505a5108719a84637` adds an allocation-free
`PwmCommitBankTargetOwner<Hardware, AXES>` which uniquely owns an aggregate PWM
backend and the existing simultaneous commit barrier. The owner is now the
portable seam between a complete one-to-four-axis compare-image vector,
hardware-owned latch reports, transactional logical FOC-bank publication, and
an independently qualified all-stage safe action.

Every terminal stage-validation, hardware-stage, hardware-observation,
report-correlation, boundary-close, overflow, or logical-publication failure
automatically attempts the complete backend safe transaction. Successful
safety closes the owner in `SafeFault`. Failed safety closes it in
`UnsafeFault`, returns the safe error separately, and permits only explicit
safe retry. A busy complete-vector stage is the sole retryable error; an absent
latch report is an ordinary poll wait rather than an error.

This is a portable type/state and deterministic-simulator checkpoint. No ESP
target implements the backend trait, the MKS servo/FOC gates remain closed,
and this evidence does not claim truthful MCPWM latch status, cross-peripheral
synchronization, safe physical outputs, or motor operation.

## Aggregate hardware contract

`PwmCommitBankHardware<AXES>` has exactly three mutable operations:

- `stage_images` receives the complete fixed array and must place it into
  inactive/shadow hardware state as one aggregate operation or return an
  error;
- `take_latch` returns at most one truthful physical `PowerStageCommit` for a
  selected axis, with `None` meaning that status is not yet available; and
- `force_safe` performs the independently qualified complete all-stage safe
  action without relying on later logical publication.

The hardware field is private. The owner exposes only immutable backend access
for bounded diagnostics; it has no mutable backend extractor. Construction
first validates the delegated barrier and returns the never-staged hardware
value on rejection.

The target owner has four explicit states:

| State | Meaning | Allowed progress |
| --- | --- | --- |
| `Ready` | No pending vector | Stage the sole next exact vector, or force safe |
| `AwaitingLatches` | Complete vector staged | Poll reports, close/publish, retry busy stage, or force safe |
| `SafeFault` | First cause retained and safe action succeeded | Read diagnostics; repeated `force_safe` is idempotent |
| `UnsafeFault` | First cause retained and safe action failed | Read diagnostics or retry `force_safe` only |

Polling before staging is a terminal `Sequence` rejection and does not touch
the backend before forcing safe. Polling an out-of-range axis, receiving a
duplicate, late, substituted, foreign-token, or wrong-schedule report, or
closing a missing/wrong boundary clears all pending authority and invokes
safety. A hardware-stage error is treated as physically uncertain even if the
backend intended an all-or-none write.

`finish_and_publish` is the sole release of the opaque
`PwmCommitBankCompletion`. Its closure must be all-or-none and non-panicking.
If logical publication rejects after physical completion, the owner does not
claim rollback: it retains `Publication` as first cause and immediately forces
future outputs safe. The barrier's next sequence may already describe the
completed physical boundary, but the terminal owner can never publish or stage
again.

## Executed fault coverage

The scripted portable backend proves:

- an invalid complete-vector schedule forces safety before any hardware stage;
- polling before staging forces safety without a hardware observation call;
- busy staging preserves the original vector and performs no safe action;
- hardware-stage failure plus safe failure enters `UnsafeFault`, and a later
  explicit safe retry reaches `SafeFault`;
- a backend observation error retains its exact axis operation and forces
  safety;
- late, substituted, and duplicate reports force the complete safe path;
- an unsolicited axis forces safety;
- a missing report at boundary close forces safety;
- a wrong boundary prevents the publication closure from running and forces
  safety;
- a publication rejection invokes safety after the full physical completion;
  and
- an explicit external stop invokes safety once and is idempotent thereafter.

The existing barrier tests continue to cover zero/five-axis rejection,
configuration substitution, cycle and sequence overflow, and the complete
one-to-four-axis fixed-capacity bound. The owner delegates those structural
limits rather than duplicating them.

## Canonical dual-axis simulator integration

`ConfiguredServoFocHardwareBank<2>` now owns one deterministic aggregate
backend through the target-owner type instead of exposing a raw barrier. The
same owner performs initial and steady-state publication:

1. the complete prepared neutral vector is staged for cycle 80,000 and physical
   sequence zero;
2. both modeled initial latch reports are polled;
3. the sealed completion transactionally activates the entire live FOC bank;
4. the retained owner returns to `Ready` for cycle 84,000/sequence one;
5. the next complete period publishes both controllers and advances the owner
   to cycle 88,000/sequence two; and
6. the canonical cached two-axis run reaches next boundary 1,688,000 and next
   physical sequence 402 after 401 controller periods, with zero modeled safe
   transactions.

The modeled backend receives supplied timer-zero values; it does not invent a
peripheral readback claim. A one-cycle-late axis-1 report at cycle 84,001 now
leaves both controllers at cycle 80,000/sequence zero, records
`Barrier(Observation { axis: 1 })`, executes exactly one modeled complete safe
transaction, and invalidates the controller bank only after that success.

An absent required encoder observation fails during pure bank preparation,
before images are staged. The controller retains the exact axis-1
`EncoderPresence` first cause while the aggregate owner records external safety
and executes safety exactly once. Explicit scheduled-job invalidation likewise
clears the two-axis setpoint mailbox before job fault, leaves the owner in
`SafeFault`, and makes every later step return the closed-owner fault.

## Fixed-memory evidence

Host DWARF for the executed two-axis types reports:

| Host type | Bytes | Alignment |
| --- | ---: | ---: |
| `PwmCommitBankBarrier<2>` | 432 | 8 |
| `PwmCommitBankCompletion<2>` | 96 | 8 |
| `ModeledPwmCommitHardware<2>` | 328 | 8 |
| `PwmCommitBankTargetOwner<ModeledPwmCommitHardware<2>, 2>` | 792 | 8 |
| `ConfiguredServoFocHardwareBank<2>` | 4,640 | 16 |
| `ScheduledServoFocHardwareBank<2>` | 4,816 | 16 |

The owner stores the backend, barrier, state, and first-cause tag inline. It
contains no allocator, trait object, queue, or unbounded collection. The
configured and scheduled simulator banks grew by 368 bytes from the preceding
checkpoint because the modeled backend and owner state now accompany the
barrier.

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
! llvm-nm -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg 'PwmCommitBankTargetOwner|PwmCommitBankHardware|PwmCommitBankBarrier|PwmCommitBankCompletion|ConfiguredServoFocHardwareBank|ServoFocBank'
```

The default-member listing contains 523 tests. Relevant totals are 83 FOC, 54
simulator, and 27 configuration tests. Formatting, all host tests,
warnings-denied host Clippy, warnings-denied rustdoc, and warnings-denied Clippy
for all four ESP configurations passed.

All four optimized images retain the preceding exact section totals:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,984 | 12,432 | 249,712 | `a8eabcc3a2e1ea8fd6a15a30a548e81844dc58fc51411fc54e64fc9b83e7aa26` |
| TinyBee V1.0, 4 MiB variant | 1,112,004 | 12,432 | 249,712 | `dae38ec8bf15a7fee415c0997821a8eea29adb177c9e2c6d35b5b70d6422b04b` |
| T-Deck Pro | 1,046,157 | 13,184 | 525,184 | `71dfbd121a116d2a33373c7a2a4f647182c66a1d630747aeab4c1bb78e15b783` |
| MKS ESP32 FOC V1.0 | 1,046,728 | 11,184 | 250,960 | `90cfe5a925408dadff358ddc0aa150dcd8779b7de75ec24fd5f0155c1af6b71e` |

The hashes renew with source/debug identity. The optimized symbol scan finds no
target owner, backend trait, barrier, completion, configured simulator bank, or
`ServoFocBank` symbol in any image. Unchanged sections and absent symbols prove
that this portable code did not silently open a target path; they are not
physical execution evidence.

## License and moving-Hyper boundary

This slice changes no manifest or lockfile and imports no third-party source.
An all-features Cargo-tree inventory contains 879 nonempty license records,
zero missing expressions, and zero GPL/AGPL/LGPL/SSPL-family entries. Local
`cargo-deny` is not installed, so CI remains the authority for its bans,
licenses, and sources policy.

All new implementation and documentation is independently authored under the
repository's `MIT OR Apache-2.0` terms. No SimpleFOC, Synthetos/g2core,
FluidNC, Klipper, or other GPL-family implementation source or asset was copied
or used.

This slice did not read, edit, format, pin, reset, or build Hypercurve and did
not change `alumina-interface`. Firmware still has no Hyper/CSGRS path
dependency. Continuing sibling Hypercurve edits therefore cannot perturb this
exact portable firmware/simulator checkpoint.

## Open physical work

- Implement the core-1 MKS aggregate backend without exposing an output-enable
  path before independent safe-state qualification.
- Establish truthful MCPWM stage/latch/readback semantics and qualify aligned
  timer epochs across both MKS power stages.
- Integrate the complete shutdown topology so hardware stage, observation,
  publication, watchdog, reset, and brownout faults all reach a measured safe
  state within a named bound.
- Measure stage/write/latch latency, cross-MCPWM skew, ADC/encoder correlation,
  dead-time, WCET/jitter, stack high-water, and shutdown latency before opening
  any servo or FOC arm gate.
- Repeat fault-injection and powered-load qualification on a physically
  reviewed MKS ESP32 FOC V1.0 board after the hardware is available.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven. No motor,
driver, analyzer, or process-power path was energized.
