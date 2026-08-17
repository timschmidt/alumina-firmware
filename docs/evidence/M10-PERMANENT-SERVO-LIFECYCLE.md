# M10 permanent scheduled-servo lifecycle evidence

## Claim

Implementation commit
`001a041e39aa1c2f9bbdba33d1ed90050b94c2a5` joins machine-IR V3 servo
partitions to the permanent dual-core cached-job lifecycle without granting a
physical output capability.

Core 0 and core 1 independently derive kind-3 admission from the canonical
active configuration. Core 1 then owns a fixed-memory scheduled-servo actor
through distributed prime, start, exact setpoint commit, block return, normal
finish, cancellation, and safety fault. A deterministic one-axis simulator
implements the same setpoint-output boundary over the complete
encoder/cascade/current/angle/SVPWM/compare-image owner.

This is software structure and simulation evidence. It does not qualify an
ESP PWM, ADC, encoder, interrupt schedule, shutdown transaction, motor, power
stage, or machine.

## Independently derived admission

`ServiceConfigurationValidation` now finishes with the complete compact
`RealtimeConfiguration` instead of discarding its executable profile. Its
working stream validator and completed profile occupy mutually exclusive
variants of one inline enum. A regression compares that layout against the old
split allocation and proves the working and completed values are not stored
side by side.

The permanent core-0 configuration coordinator does not retain another
4,344-byte full realtime profile. At durable activation it derives and keeps
only `CachedServoConfiguration<JOB_AXES>`: the per-axis configuration digest,
position cadence, conservative Q31.32 increment bound, and Q2.30 velocity and
q-current authorities needed for service-side cache admission. Core 1 retains
its own full `RealtimeConfiguration`, as before, because it owns physical
configuration and complete FOC-axis lowering.

For a servo job:

1. core 0 binds its compact independently derived facts to the descriptor and
   opens `ServicePrefetch::open_servo` with the resulting typed limits;
2. core 1 independently lowers its full active configuration, reconstructs the
   same limits, and calls `RealtimeJob::prepare_servo`;
3. descriptor kind, configuration digest, axis count, dense cadence, update
   bound, maximum block/segment ticks, and maximum position delta must agree;
4. the generic job path still rejects kind `3` when no typed profile is
   supplied.

The profile transfer is by independently derived value, not by trusting a
core-0 certificate and not by widening the generic motion authority.

## Scheduled core-1 ownership

`ScheduledServoExecution` adds a fixed-memory lifecycle around
`CachedServoSetpointRunner`:

- the first one or two unique admitted blocks transfer at the abort-guard
  prime transition;
- the exact first simultaneous setpoint batch is staged before local start;
- exactly one opaque batch may be staged in the sole `ServoSetpointOutput`;
- an acknowledgement cannot be consumed before its claimed applied cycle or
  after the board-bounded reporting deadline;
- reporting lateness must be strictly shorter than the configured
  position-loop period, preserving time to stage the successor;
- token, deadline, configuration, cadence, recurrence, block-chain, and
  execution-family substitution fail closed;
- a block returns only after its continuation setpoint or terminal hold was
  successfully committed;
- normal completion requires every block acknowledgement plus the separate
  terminal at-rest hold and explicit finish handshake; and
- cancellation/fault invalidates staged recurrence ownership only after the
  enclosing owner has made physical outputs safe.

The firmware `MotionService` is now a kind-bound selector. Kinds `1` and `2`
continue through the existing complete shifted-image owner. Kind `3` selects
only `ScheduledServoExecution`; there is no fallback or cross-family shim.
Distributed schedule start observation, cache refill, block acknowledgement,
normal finish, urgent stop, and safety-fault paths share the permanent core-1
lifecycle.

## Complete-axis simulation join

`ScheduledServoFocHardwareLoop` implements `ServoSetpointOutput<1>` over
`ConfiguredServoFocHardwareLoop`. It retains one future batch, injects it only
at its exact position-loop cycle, and publishes a cached-stream commit only
after the complete candidate FOC transition and next PWM compare image commit
succeed. A failed controller or compare transition leaves the cached batch
unacknowledged.

The production regression replays two independently admitted servo blocks for
401 current-loop periods. It observes:

- three contiguous scheduled command identities;
- three position-loop updates and 21 velocity/encoder updates;
- the shared block boundary only after command 2 commits;
- the final block only after command 3 installs the terminal hold;
- exact `RealtimeJobState::Complete` after both unique tokens return; and
- complete-axis late-commit and missing-encoder failures without partial
  controller advance.

Additional motion regressions cover prime/start order, nine-batch two-axis
recurrence replay, both block barriers, terminal finish, token substitution,
early acknowledgement observation, late commit reporting, and the ordered
safe-invalidate then job-cancel path.

## Target gate

All selected board modules declare separate servo gates and timing facts:

- `SERVO_OUTPUT_IMPLEMENTED = false`;
- `SERVO_OUTPUT_QUALIFIED = false`;
- `SERVO_MAXIMUM_COMMIT_OBSERVATION_LATENESS_CYCLES = 0`; and
- `SERVO_MINIMUM_PRIME_LEAD_CYCLES = u64::MAX`.

Their current `ServoSetpointOutput` implementation returns an error
transactionally for both stage and commit observation. Both job actors reject
kind `3` before cache ownership while implementation or qualification is
false, and `MotionService::ready_to_arm` repeats the same family-specific gate.
The impossible prime lead is an additional schedule barrier, not the primary
capability gate.

A demangled symbol scan of the four optimized images found only core-0
`ServicePrefetch::open_servo` async drop glue on media-equipped builds. It found
no `ScheduledServoExecution`, `CachedServoSetpointRunner`, or
`ServoSetpointOutput` symbol. This is consistent with link-time elimination of
the unreachable physical actor while every board gate is false; it is not
evidence of target execution.

## Static-memory correction

Before sealing the commit, an exploratory TinyBee release link with a second
full core-0 `RealtimeConfiguration` failed at the reserved DRAM boundary:

```text
stack.x:11 cannot move location counter backwards (from 3ffe0814 to 3ffe0000)
```

That design exceeded the boundary by 2,068 bytes. The final implementation did
not reduce the reserved stack. It instead:

- stores the working validator or completed profile in one mutually exclusive
  inline region; and
- retains only the compact servo-admission profile on core 0 after durable
  activation.

All four exact-commit release images then linked. Relative to
`M10-EXACT-CACHED-SERVO-STREAM`, BSS is 32 bytes smaller on every target.

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
```

The default-member listing contains 502 tests. The relevant crate totals are
26 configuration, 20 cached-job, 56 motion, 68 FOC, and 49 simulator tests.
Formatting, diff checks, warnings-denied host Clippy, warnings-denied rustdoc,
and warnings-denied Clippy for all four ESP packages passed.

All four optimized board-qualified images linked from the exact implementation
commit:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,984 | 12,432 | 249,712 | `d9d40a7357b6657c37b266ed2703e38d08683372aabbf5cfd98ab48c0b2cace6` |
| TinyBee V1.0, 4 MiB variant | 1,112,004 | 12,432 | 249,712 | `e7b795e074658c75c94f70b6b078f4bd559171ebea9b8e325ea3971ad7621323` |
| T-Deck Pro | 1,046,157 | 13,184 | 525,184 | `c8f8af5ad22ec5c52cfabc212b2662975b10b0e54e3905d5c1c8bb880d847e41` |
| MKS ESP32 FOC V1.0 | 1,046,728 | 11,184 | 250,960 | `84145f63d78c473843db477be5a1727d95e6c1187d91e4b9703b3e00337e7730` |

The corresponding text deltas from the preceding checkpoint are +7,656,
+7,656, +7,044, and +8,644 bytes. Data increases by 32 bytes and BSS decreases
by 32 bytes on each image.

## License and source boundary

The implementation adds no external package. Firmware now names the existing
workspace `alumina-machine-ir` dependency directly instead of relying on a
transitive edge. All changed Alumina crates remain `MIT OR Apache-2.0`; no GPL
source, binary, or dependency was copied or linked.

This slice did not read, edit, format, pin, or build Hypercurve, and it did not
change `alumina-interface`. The firmware has no Hyper/CSGRS dependency. Moving
sibling Hyper work therefore cannot alter this evidence.

## Open physical work

- Replace the unavailable target mailbox with a sole interrupt-domain owner of
  all configured axes.
- Establish homing/multi-turn seed authority before activation.
- Attach qualified synchronized ADC acquisition, encoder observation, MCPWM
  compare staging/readback, and the board-specific shutdown transaction.
- Measure current-loop and setpoint-loop WCET/jitter, commit-report latency,
  prime lead, observation aperture, and shutdown latency under Wi-Fi/web/SD
  load.
- Add multi-axis complete-owner simulation and MKS ESP32 FOC bench evidence.
- Keep all implementation and qualification gates false until those results
  meet named conservative profiles.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven.
