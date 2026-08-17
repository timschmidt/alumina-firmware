# M8 portable encoder estimator — offline evidence

Date: 2026-08-14

Status: implemented portable exact absolute-count unwrapping and conservative
servo-observation checkpoint. No configuration record or production target
selects the estimator. This is not homing, timestamp, sensor, speed-bound,
target-scheduling, WCET, motor, energization, or physical safety evidence.

## Result and moving-source isolation

Firmware commit `a5fb45c7d2060fbd3352ecf1a3ab81adb80fb4a5` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and every manifest path is beneath the
`alumina-firmware` repository. No manifest, lockfile, configuration wire version, or
dependency policy changed. Hypercurve is an intentionally moving sibling
worktree; this work did not edit, format, reset, pin, or otherwise constrain it,
and no transient Hypercurve result was admitted into firmware evidence.

## Explicit profile and multi-turn seed

`ServoEncoderScale` carries two positive reduced rationals:

- Q31.32 position-lattice bits per positive mechanical turn; and
- raw counts per second represented by normalized velocity `+1`.

`ServoEncoderProfile` binds those scales to the complete configuration digest,
raw-count modulus/reference/direction, a reduced rational symmetric count-error
bound, an exact Q31.32 position reference, the device-cycle frequency and exact
sample period, maximum availability latency, separate trackable and admitted
normalized speed limits, a required additive velocity-estimation error, and
explicit position/velocity ULP-width policies.

The additive velocity-estimation error is not optional or inferred from two raw
counts. A later complete machine configuration must make it cover the
interval-average-to-newest-sample difference under its acceleration bound plus
any timestamp, aperture, or model error not already included in count
uncertainty. Profile validation requires it to be positive and no larger than
the admitted speed.

An absolute sensor cannot reveal its multi-turn branch. Construction therefore
requires `ServoEncoderSeed`, whose signed directed turn index is explicit
boot-local authority. The seed maps to a conservative position interval but
emits no velocity and no `ServoKinematicSample`; a second physical observation
is required. This keeps homing or retained-position policy outside the estimator
instead of inventing it from one modulo count.

## Unique modular unwrapping proof

For one exact sample period, the profile derives the largest integer difference
between adjacent observed nominal counts as

```text
B = ceil(
      maximum_trackable_velocity
      * counts_per_second_at_velocity_one
      * sample_period_cycles / device_cycle_hz
      + 2 * maximum_count_error
    )
```

using checked `u128` rational arithmetic. Validation requires

```text
2 * B < counts_per_mechanical_turn
```

so two candidates separated by one modulus cannot both lie in `[-B, B]`.
Each new raw count is reduced by the configured reference and direction. The
estimator considers the positive modular difference and the corresponding
negative difference, accepts exactly one inside the proven window, and rejects
none or both. It never applies a nearest-wrap heuristic or resolves a half-turn
tie silently.

The physical speed limit underlying this uniqueness proof remains a supplied
qualification fact. Motion faster than the stated trackable bound can alias and
cannot be detected from two modulo samples alone; this checkpoint makes that
assumption visible but does not prove it for hardware.

## Outward position and velocity observations

For directed nominal multi-turn count `c`, count uncertainty `e`, count modulus
`M`, and the rational position-per-turn scale, the estimator encloses

```text
position_at_reference + (c +/- e) * position_bits_per_turn / M
```

with exact signed `i128` intermediates and outward floor/ceiling conversion into
`ServoPositionInterval`. It rejects Q31.32 range overflow and any interval wider
than the configured ULP policy.

For adjacent nominal counts separated by `delta`, it first encloses the secant
velocity as

```text
(delta +/- 2e) * device_cycle_hz
    / (sample_period_cycles * counts_per_second_at_velocity_one)
```

with outward Q2.30 rounding. It then widens both endpoints by the required
symmetric velocity-estimation error so the result can conservatively represent
newest-sample velocity rather than silently equating it with the interval
average. The final interval must fit both the admitted absolute speed and the
configured ULP-width policy before it becomes `ServoKinematicSample`.

Observations carry exact represented and available device cycles. They must
share the immutable digest, stay inside the modulus, have availability no
earlier than the represented instant, meet the fixed latency bound, and arrive
on the exact retained sample cadence. The estimator generates contiguous sample
identities from one.

`ServoEncoderEstimator` is fixed-memory and `Copy`. Each observation runs
against a value copy; success atomically commits count, time, position, and
sequence state. Failure leaves the last accepted prefix unchanged, latches the
exact first cause, and makes later attempts return `FaultLatched`. Rejections
distinguish profile, identity, raw range, observation window/latency, cadence,
wrap candidate, count/position/cycle/sequence overflow, velocity range, and
position/velocity precision.

## Adversarial tests and independent replay

Ten new `alumina-foc` regressions cover:

- zero/non-reduced rational scales, exact wrap-bound derivation, and ambiguous
  profile rejection;
- explicit seed position with velocity withheld;
- positive and negative motion across the modulo boundary;
- both configured raw-count directions;
- exact outward count-error and estimator-error velocity widening;
- independently cross-multiplied non-power-of-two rational bounds at negative
  multi-turn position;
- missing wrap-candidate rejection with transactional first-cause retention;
- digest, raw range, observation order, latency, and exact-cadence rejection;
- position/velocity width and admitted-speed gates before state advancement;
  and
- count, position, cycle, and sequence overflow behavior.

The host simulator independently inverts the configured reference and count
direction to generate raw modulo values from known directed multi-turn truth.
For each of increasing and decreasing sensor directions, it applies 700 `+7`
count samples followed by 900 `-7` count samples, repeatedly crossing the wrap,
and runs the complete 1,600-sample replay twice. Both traces are identical; all
3,200 estimates recover their corresponding known multi-turn truth, each
sequence is exact, and each direction finishes at count 2,690 from the seeded
count 4,090. A separate `+11` sample is outside the derived `B = 10` window and
is rejected visibly. This replay checks count recovery and determinism; it is
not a physical acceleration, timestamp, or sensor-noise model.

## Verification record

The following completed offline against the contents committed as
`a5fb45c7d2060fbd3352ecf1a3ab81adb80fb4a5`:

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
  rg 'ServoEncoderEstimator|ServoEncoderProfile|ServoEncoderObservation|ServoEncoderEstimate|select_delta|position_interval|velocity_interval|simulate_servo_encoder'
```

Observed results:

- all 475 portable default-member tests passed, including 59 FOC and 46
  simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for primary 8 MiB TinyBee, opportunistic 4 MiB
  TinyBee, T-Deck Pro, and MKS ESP32 FOC V1.0 production configurations;
- all four optimized production images linked successfully;
- the primary 8 MiB TinyBee image has 1,068,100 bytes text, 12,224 bytes data,
  and 249,920 bytes BSS, with SHA-256
  `7002a8ab3cbaa49a4d2ae6e4d703292b4c5dd424639122f57c75a1321d460caa`;
- the opportunistic 4 MiB TinyBee image has 1,068,140 bytes text, 12,224 bytes
  data, and 249,920 bytes BSS, with SHA-256
  `bbf760dc2530730c0da4922c7ecbed4207d58c11d3ebd67ff9eca4b1fee1f39f`;
- the T-Deck Pro image has 1,002,533 bytes text, 12,976 bytes data, and 525,392
  bytes BSS, with SHA-256
  `ede9e5862082f1d4c6cafbdefbd7473b22638aec8b1db3b990d2edafa2cd7a54`;
  and
- the MKS ESP32 FOC V1.0 image has 1,001,076 bytes text, 10,976 bytes data, and
  251,168 bytes BSS, with SHA-256
  `59f34fa6ec321de4cf4ef2110af319974fbb1de2cb535f483ed992a46a153862`.

All four section-size triples are unchanged from the portable cascaded-servo
checkpoint. All four whole-file hashes changed while linking the modified FOC
workspace crate; unchanged linked section totals and absent symbols do not
establish byte or behavior identity. These static measurements are not runtime
stack/heap watermarks, estimator WCET, bus latency, or sample timing evidence.

The production-symbol query returned no match for the estimator, profile,
observation/result types, exact mapping helpers, or host replay. This supports
optimized build unreachability, not physical safety or armability.

No dependency, manifest, lockfile, configuration version, or deny policy
changed, so the existing MIT/Apache-compatible inventory is unchanged. A
changed-Rust scan, including the complete new estimator module, found no
GPL/AGPL/LGPL/SSPL identifier or copied FluidNC, Klipper, Synthetos/g2core, or
SimpleFOC implementation reference. The estimator is independently authored
from elementary modular/rational arithmetic and the repository's existing
public type contracts. The local environment does not have `cargo-deny`
installed, so no ad hoc installation was attempted; configured CI license
enforcement remains required.

## Closed claims and next boundary

This checkpoint closes the portable reduced scales, explicit multi-turn seed,
unique modular-candidate proof, exact-cadence observation validation, outward
position/secant/endpoint-velocity enclosures, precision and speed gates,
transactional first-cause state, and deterministic known-truth replay.

It does not store or lower the profile in Configuration V5; establish homing or
retained-turn authority; tie mechanical and electrical calibration records;
bind the estimator period/phase to `ServoLoopGrid`; produce truthful AS5600
sample/aperture/availability stamps; qualify count error, acceleration,
trackable speed, or estimator error; schedule either encoder bus; handle target
dropout/recovery; compose the cascaded/current loops; install a core-1 actor;
measure WCET; or establish any energizing path. Those facts must remain explicit
in the next configuration and target checkpoints.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
