# M8 portable complete servo/FOC axis — offline evidence

Date: 2026-08-14

Status: firmware commit
`983ecd7bf793edee88e629f040977a30ad66c575` composes the validated
Configuration V6 encoder, cascaded-servo, synchronized-current, rotor-angle,
dq-current, interval-SVPWM, and integer-compare contracts behind one portable
transactional owner. This closes a software composition seam. It is not a
target task, hardware-register acknowledgement, shutdown invocation, WCET,
motor, or energization claim.

## Source and dependency boundary

This checkpoint changes only `aluminafw`. Firmware remains independent of
CSGRS and the Hyper stack: `cargo metadata --no-deps` reports 35 workspace
packages and every manifest is below the `aluminafw` root. No manifest or lock
file changed.

Hypercurve is an intentionally moving, user-owned worktree. It was not read,
edited, formatted, reset, pinned, copied, or built for this checkpoint. No
interface artifact was rebuilt. Future browser work that needs Hypercurve must
use an explicit isolated source snapshot rather than making a live sibling
worktree an implicit reproducibility input.

## One validated owner

`LoweredFocAxisConfiguration::servo_foc_axis_profile` first replays the entire
private lowered V6 bundle and then returns one `ServoFocAxisProfile`. Its
validation joins all of these immutable facts:

- one nonzero complete configuration digest across controller, rotor, current,
  compare, cascade, and encoder profiles;
- the exact PWM/current rate and integer current-period device-cycle lattice;
- PWM/ADC and encoder device clocks plus encoder cadence equal to the velocity
  grid period;
- rotor/encoder count modulus, reference, direction, and count uncertainty;
- configured pole pairs, maximum current, admitted velocity, observation
  latency, sample-age policy, and all existing component precision gates.

The configuration cannot create boot-local facts. Activation separately
requires a nonzero activation identity, explicit multi-turn encoder seed, and
first timer-zero boundary. `prepare_activation` validates them and exposes a
complete neutral `PwmCompareImage`. `activate` creates a live controller only
after a `PowerStageCommit` names that exact token, schedule, and observed
boundary.

Each current/PWM-period input then names the current actor boundary, setpoint
and encoder-observation presence required by the nested loop grid, calibrated
raw current pair plus its active-image acquisition witness, and a raw rotor
count. `prepare` performs this fixed composition against copied candidate
state:

1. correlate the active compare token, period sequence, and period start;
2. estimate multi-turn position/velocity on exactly the due encoder ticks;
3. service contiguous position, velocity, and current cascade boundaries;
4. reconstruct the synchronized two-shunt current and certified rotor angle;
5. Park-transform the interval current, update the anti-windup dq PI, and
   inverse-Park the resulting voltage interval;
6. run interval SVPWM and lower the whole result onto the configured integer
   compare lattice; and
7. stage that complete future image in a copied latch owner.

No live estimator, cascade, PI, image, token, command ID, period, or boundary
changes during calculation. The opaque `PreparedServoFocAxisTransition` binds
the result to its activation and exact source-state prefix. `commit` installs
all candidate state together only after an exact future timer-zero
acknowledgement. Wrong or stale prefixes, substituted observations, late or
foreign commits, arithmetic failures, and nested-controller rejections retain
the prior logical state and latch the first cause.

Successful preparation may be repeated before hardware staging and produces an
identical candidate. Once one candidate commits, another candidate from that
consumed prefix rejects terminally. Prepared activation and transition values
are opaque and non-`Clone`; the target is still obligated to permit at most one
physical image in flight.

## Deterministic and adversarial replay

Nine focused `alumina-foc` actor tests cover:

- every profile identity, clock, geometry, rate, and seed-schedule seam;
- neutral-image visibility before activation and wrong initial acknowledgement;
- exact nested position/velocity/current cadence with no pre-commit advance;
- late inner-current failure after candidate estimator/cascade calculations;
- wrong physical commit with no accepted candidate state;
- activation substitution, stale-prefix replay, and deterministic repeated
  preparation;
- exact encoder-presence and active-current-witness requirements; and
- fixed host-size ceilings.

The configuration-derived `ConfiguredServoFocHardwareLoop` creates the same
profile through V6 lowering, models the neutral activation acknowledgement,
derives each current stamp from the exact active integer compare image, and
models the next timer-zero commit. Two independently constructed owners replay
401 periods byte-for-byte. Assertions retain exactly 401 current updates and
commits, 21 velocity/encoder updates, three position updates, monotonically
bound commands/images, a non-neutral output, and identical terminal state.

Adversarial simulation injects a one-cycle-late commit and a missing required
encoder observation. Both reject without estimator or cascade advance, retain
the exact first cause, and reject subsequent use as fault-latched.

On the host test ABI, debug information reports these concrete allocation-free
value sizes:

| Value | Observed bytes | Enforced ceiling |
| --- | ---: | ---: |
| `ServoFocAxisController` | 1,840 | 2,048 |
| `PreparedServoFocAxisActivation` | 1,872 | 2,048 |
| `PreparedServoFocAxisTransition` | 2,112 | 2,304 |
| `ServoFocAxisPreparedUpdate` | 992 | 1,024 |

These are host value-layout checks, not target stack high-water measurements or
proof that copying the candidate fits a real current-loop deadline.

## Reproduced verification

The following completed against implementation commit `983ecd7`:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo test --locked -- --list
cargo clippy --all-targets --locked -- -D warnings
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
llvm-nm -C <all-four-images> | \
  rg 'ServoFocAxisController|ServoFocAxisProfile|ConfiguredServoFocHardwareLoop|servo_foc_axis_profile'
```

Observed results:

- all 488 portable default-member tests passed, including 68 `alumina-foc`, 25
  `alumina-config`, and 48 `alumina-sim` tests;
- formatting, diff, warnings-denied all-target Clippy, and warnings-denied
  rustdoc passed;
- strict ESP target Clippy passed for primary 8 MiB TinyBee, opportunistic
  4 MiB TinyBee, T-Deck Pro, and MKS ESP32 FOC V1.0;
- all four optimized board-qualified images linked; and
- the production symbol query returned no match for the new actor, profile,
  lowering helper, or host simulator. This confirms the documented fact that
  no target task selects the actor; it is not an execution or safety claim.

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,092,960 | 12,304 | 249,840 | `7d1938d02eee7c0e6820c6a58fb82856397300851fea6441100f6dd6d2d6d7c0` |
| TinyBee V1.0, 4 MiB variant | 1,092,980 | 12,304 | 249,840 | `d2b75bd6c69bcc0387f72e14dd0be498025865bd8ec061fd385825cfff6329d7` |
| T-Deck Pro | 1,027,869 | 13,056 | 525,312 | `671201702e10971580490cbd3ac90aff92cdfc5b1c1a4f1f7d65f796634189ae` |
| MKS ESP32 FOC V1.0 | 1,028,156 | 11,056 | 251,088 | `275aad5036b90f86dd0183f37358cf5f74be00e6a049329ba605daab482e4c7d` |

All section-size triples equal the preceding Configuration V6 checkpoint. The
whole debug-bearing ELF hashes changed after the workspace source changed;
equal linked text/data/BSS totals do not establish byte or behavior identity.
These are static linked-image measurements, not runtime memory, stack, latency,
or WCET evidence.

## Licensing and hardware boundary

All changed source remains `MIT OR Apache-2.0`. No dependency, manifest, or
lock file changed. The locked all-feature Cargo tree produced 876 nonempty
package/license records, with zero missing expressions and zero
GPL/AGPL/LGPL/SSPL-family expressions. A scan of every changed Rust source file
found no GPL-family identifier and no FluidNC, Klipper, Synthetos/g2core, or
SimpleFOC implementation reference. The actor is independently authored from
the repository's existing exact numeric and control contracts.

The actor's `PowerStageCommit` is supplied evidence binding a token and exact
boundary; it is not a register-image readback. A future sole-owner target must
stage the complete image, source truthful current/rotor/encoder timing facts,
detect missed deadlines, invoke the qualified shutdown transaction on any
rejection, and measure WCET before this path can become reachable. The current
MKS ESP32 FOC V1.0 package remains `Described`, non-armable, and has no compare
write path. The first actor also supports only one current update per PWM period
and zero voltage feed-forward.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, or
driven. No USB/serial transaction was attempted, no SLogic16U3 lead was used,
and no workstation Wi-Fi association changed. No motor, motor supply, or MKS
ESP32 FOC board participated.

The next safe offline boundary is a canonical cached servo-command stream and
portable core-1 queue owner, followed by a bounded electrical-angle prediction
contract suitable for a slower physical encoder than the current/PWM loop.
Neither should activate a target power stage before the separately measured
shutdown, ADC/PWM timing, sensor, and WCET qualifications exist.
