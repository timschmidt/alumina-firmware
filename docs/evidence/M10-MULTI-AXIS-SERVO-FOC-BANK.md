# M10 multi-axis servo/FOC bank evidence

## Claim

Implementation commit
`6a46a75c5dd828446d3afbc18354e928123edf71` extends the portable complete-axis
FOC owner and permanent cached-servo lifecycle across a fixed simultaneous axis
bank without enabling a physical output.

`ServoFocBank<AXES>` accepts one through four already activated complete-axis
controllers only when they share the canonical configuration digest, exact
nested loop grid, current-period boundary, and period sequence and retain
distinct boot-local activation identities. It calculates all axis candidates,
validates the whole modeled physical-commit set against the unchanged live
prefix, and installs the complete controller array only after every axis
succeeds.

A two-axis deterministic simulator joins that transaction to the exact cached
servo stream for 401 current-loop periods. This is portable control-state and
simulation evidence. It does not establish synchronized MCPWM, ADC, encoder,
interrupt, shutdown, motor, power-stage, or machine behavior.

## Complete-bank transaction

The existing `ServoFocAxisController` already prepared encoder, cascade,
current-controller, angle, SVPWM, compare-owner, and counter state against
copies. Its public commit path has been factored through one private pure
candidate derivation: ordinary one-axis commit still installs immediately, but
the bank can derive every accepted next controller without mutating any live
axis.

The bank then applies these rules:

1. reject zero axes or more than four axes;
2. reject a faulted axis, mismatched digest/grid/boundary/sequence, missing
   active image, or duplicate activation identity;
3. prepare every current-period input before any successful axis advances;
4. retain the indexed first cause if one axis rejects during preparation;
5. derive every full next controller and validate every exact
   `PowerStageCommit` while the live controller array is unchanged;
6. reject if any derived next boundary or sequence differs; and
7. replace the complete controller array only after all candidates exist.

A commit failure on axis 1 after axis 0 has produced a valid candidate leaves
both live controllers at their prior boundary. The bank retains the indexed
axis cause and all later preparation or commit calls return `FaultLatched`.

The configuration-derived simulator adds the board/resource seam not carried
by the portable mathematical profile: configuration slots must be unique, and
test-lowered axis instances must be unique, before controller construction.
The two-axis regression replay uses instances 0 and 1 plus distinct activation
identities. It does not duplicate one motor profile under two command axes.

## Permanent cached-job replay

`ScheduledServoFocHardwareBank<2>` implements `ServoSetpointOutput<2>` around
the complete bank. It owns at most one future simultaneous cached batch. At the
exact due position boundary it injects both setpoints, runs both full current
periods, and publishes one opaque cached-stream commit only after the complete
bank transaction succeeds.

The two-block replay proves:

- independently admitted kind-3 blocks and the descriptor-bound two-axis
  admission profile agree;
- the first batch commits at the distributed start epoch;
- axes 0 and 1 use distinct positive and negative position targets while
  retaining the same command identity and exact application boundary;
- 401 current/PWM periods advance identically across both axes;
- each axis performs 21 encoder/velocity updates and three position updates;
- all three position updates are simultaneous and produce opposite-signed
  velocity targets;
- both block tokens return only at their continuation or terminal barriers;
- command identities are exactly `[1, 2, 3]` across start, continuation, and
  terminal hold; and
- explicit finish reaches `RealtimeJobState::Complete` only after both blocks
  and the terminal hold complete.

## Cross-axis failures and safe invalidation

One regression supplies an exact axis-0 timer zero and a timer zero one device
cycle late on axis 1. The result is indexed
`ServoFocAxisError::PowerStageCommit`; both axes retain period sequence 0,
boundary 80,000, encoder sequence 0, and no current-loop prefix.

A second regression omits the axis-1 encoder observation on a boundary where
both velocity loops require it. Axis 0 has already calculated a candidate but
still retains its prior live estimator and controller state when the bank
latches the axis-1 presence failure.

The cancellation regression follows the required ownership order: it primes
and starts a two-axis cached job, observes one staged future batch, models the
enclosing all-stage safe action, clears the simulated staged/commit mailbox,
invalidates the FOC bank, and only then faults cached-job ownership. No later
hardware step or cached acknowledgement is possible. The modeled safe action
is an ordering witness, not a qualified MKS shutdown implementation.

## Fixed-memory shape

The bank and simulator add no allocator and no dependency. Controllers,
prepared transitions, setpoints, commits, and updates use const-generic arrays
and inline `Option` storage. Unit tests retain explicit upper bounds for the
one-axis and two-axis actors.

Host x86-64 DWARF for the exact implementation commit reports:

| Type | Host bytes |
| --- | ---: |
| `ServoFocAxisController` | 1,840 |
| `PreparedServoFocAxisTransition` | 2,112 |
| `ServoFocBank<2>` | 3,840 |
| `PreparedServoFocBankTransition<2>` | 4,240 |
| `ConfiguredServoFocHardwareBank<2>` | 3,840 |
| `ScheduledServoFocHardwareBank<2>` | 4,016 |
| `ScheduledServoExecution<2>` | 2,968 |

These are host ABI layout observations, not ESP stack high-water or target
static placement. The physical target must place its permanent bank and
candidate regions deliberately and measure worst-case stack, WCET, interrupt
jitter, and commit-report latency before an implementation gate can open.

## Reproduction

From the `alumina-firmware` repository at the implementation commit:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo test --locked -- --list
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
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
  rg 'ServoFocBank|ScheduledServoFocHardwareBank|ConfiguredServoFocHardwareBank|ScheduledServoExecution|CachedServoSetpointRunner|ServoSetpointOutput'

llvm-dwarfdump --name='ServoFocBank<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='PreparedServoFocBankTransition<2>' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='ServoFocAxisController' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='PreparedServoFocAxisTransition' \
  target/debug/deps/alumina_foc-6db61dedae00baf9
llvm-dwarfdump --name='ConfiguredServoFocHardwareBank<2>' \
  target/debug/deps/alumina_sim-de51b6bf610ac440
llvm-dwarfdump --name='ScheduledServoFocHardwareBank<2>' \
  target/debug/deps/alumina_sim-de51b6bf610ac440
llvm-dwarfdump --name='ScheduledServoExecution<2>' \
  target/debug/deps/alumina_sim-de51b6bf610ac440
```

The default-member listing contains 507 tests. The relevant crate totals are 70
FOC and 52 simulator tests. Formatting, diff checks, warnings-denied host
Clippy, warnings-denied rustdoc, and warnings-denied Clippy for all four ESP
packages passed.

All four optimized exact-commit images linked:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,984 | 12,432 | 249,712 | `e2c5030be7f04a579c81a32a0605e3c740df6434be8b723096d568f1b91a9dd7` |
| TinyBee V1.0, 4 MiB variant | 1,112,004 | 12,432 | 249,712 | `789736ceb58195e2d8ce6b91a00f55125916e8ad21ef7ab9dc0181537080676c` |
| T-Deck Pro | 1,046,157 | 13,184 | 525,184 | `a1b9de3188746f5c5179696b01453992878205717f3d848783ae3ef4ac3066a2` |
| MKS ESP32 FOC V1.0 | 1,046,728 | 11,184 | 250,960 | `0ab19007146cac87d74c2ab424fbc28000a94abe982b71aaa71b79e5be1b196a` |

Text, data, and BSS totals are unchanged from the permanent-servo-lifecycle
checkpoint on every board. The hashes are renewed and prove the ELF files are
not byte-identical. The demangled symbol scan returns no match for the portable
bank, simulated bank, scheduled-servo actor, runner, or output trait in any
optimized image. With every target servo gate false, link-time elimination is
expected; this is closed-gate evidence, not target execution evidence.

## License and moving-Hyper boundary

The implementation adds no external package and changes no lockfile. All
changed source remains `MIT OR Apache-2.0`; no GPL source, binary, or dependency
was copied or linked.

This slice did not read, edit, format, pin, or build Hypercurve and did not
change `alumina-interface`. The firmware still has no Hyper/CSGRS dependency.
Moving sibling Hyper work therefore cannot change or invalidate this exact
firmware/simulator checkpoint.

## Open physical work

- Construct a canonical two-axis MKS configuration using two distinct physical
  stages, ADC pairs, encoder endpoints, and qualified shutdown contracts. The
  current simulation fixtures are not that physical configuration.
- Establish each axis's homing/multi-turn seed authority before activation.
- Implement the sole core-1 target owner which prepares both MCPWM images,
  synchronizes their timer-zero epochs, correlates all ADC/encoder observations,
  and reports a commit only after the whole hardware set is observed.
- Qualify the real all-stage high-impedance shutdown transaction, including the
  reset/watchdog path and bounded latency.
- Measure static placement, stack high-water, current-loop WCET/jitter, ADC and
  encoder availability, cross-MCPWM skew, and cached commit-report latency under
  Wi-Fi/web load.
- Keep `SERVO_OUTPUT_IMPLEMENTED`, `SERVO_OUTPUT_QUALIFIED`, commit-latency, and
  prime-lead gates closed until the named physical evidence passes.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven.
