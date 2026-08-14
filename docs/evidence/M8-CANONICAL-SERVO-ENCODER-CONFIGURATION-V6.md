# M8 canonical servo/encoder Configuration V6 — offline evidence

Date: 2026-08-14

Status: canonical Configuration V6 stores, independently validates, and
digest-binds the portable cascaded-servo and absolute-encoder profiles for each
FOC axis. This closes a software configuration/lowering seam. It is not an
encoder transport, homing/turn-seed, closed-loop target task, WCET, motor,
energization, or physical qualification claim.

## Result and source boundaries

Firmware commit `05405dbed6f69a4c86a31aaeb68aa66839af4ca1`
implements the V6 format and lowering. Alumina Interface commit
`13c58b6af7acb735b83b0e1bd970a8759fcf08e8` updates the controlled browser
schema surfaces and deliberately adds no compatibility path.

The firmware workspace remains isolated from CSGRS and the Hyper stack.
`cargo metadata --no-deps` reports 35 packages, with every manifest beneath
`aluminafw`; neither firmware manifest nor `Cargo.lock` changed in this
checkpoint. The browser continues to use the current local sibling Hyper stack
as its source authority.

Hypercurve was and remains an intentionally moving, user-owned worktree. The
interface release artifact recorded below was built while Hypercurve was at
commit `97b3d9237f7009bc9a1c5a98c6dfc560276e70da` with a stable pre/post diff
SHA-256 of
`a2c60afeb594cc22d67dc3d10bcc6d00a869f6f1fd17da75f96be8dc3b2fdf4e`.
After that artifact was produced, the live diff advanced to
`c8780231f5ec7b1ba0d3dc7891c58474d0c292127cc518f73bd74eb826203689`.
No Hypercurve file was edited, formatted, reset, pinned, copied into firmware,
or rebuilt merely to chase the later state. The artifact hash therefore names
an observed moving-source snapshot; the interface commit alone is not falsely
presented as a complete source release for that binary.

## One greenfield V6 wire authority

The exact document magic is now `ALMCFG06`, schema version `6`. The 80-byte
header and 64-byte fixed records retain strict canonical ordering, reserved-zero
checks, bounded counts, and whole-document SHA-256 identity. V1 through V5 are
rejected; there is no decoder fallback, version negotiation, adapter, or shim.

Three records are mandatory for every admitted FOC axis:

- kind 11 stores the complete outer position gain, velocity PI, normalized
  velocity/current limits, direct-current target, Q31.32 following-error limit,
  and sample-age limit;
- kind 12 stores signed Q31.32 position at the raw reference, reduced exact
  position-bits-per-turn and count-rate ratios, and measured-or-qualified
  evidence; and
- kind 13 stores the exact device clock, sample cadence, observation latency,
  separate wrap-trackable/admitted velocities, required estimator widening,
  position/velocity interval-width policies, and qualified evidence.

Every byte is encoded and decoded explicitly. Selectors, reserved bytes,
non-reduced or zero scales, inadmissible Q2.30 limits, weak evidence, duplicate
records, missing records, and out-of-range instances reject before an
executable profile can exist.

V6 also makes the axis gearing, travel, calibration, position bounds, velocity,
acceleration, jerk, and following-error facts mandatory for FOC, alongside the
existing electrical, count, PWM, and current-sense facts. The allocation-free
core-1 profile retains four complete FOC axes. Its verified host layout is
4,256 bytes under the explicit 4,608-byte compile-time ceiling.

## Exact machine-to-controller relationships

Nominal reduced machine scalars select the runtime lattices through checked
`u128` cross-products; no floating-point conversion or approximate equality is
used. V6 requires:

```text
position_bits_per_turn
    = 2^32 * travel_metres_per_output_turn
      / (motor_turns_per_output_turn * calibration_scale)

counts_per_second_at_velocity_one
    = velocity_limit_metres_per_second
      * encoder_counts_per_turn
      * motor_turns_per_output_turn
      * calibration_scale
      / travel_metres_per_output_turn
```

The stored following-error bits must equal the exact floor of the conservative
lower physical following-error endpoint multiplied by `2^32`. The maximum
position-observation width must fit inside that limit.

The stored estimator widening must be at least the outward Q2.30 ceiling of:

```text
acceleration_limit_upper * sample_period_cycles
------------------------------------------------
2 * device_cycle_hz * velocity_limit_lower
```

This is only the minimum bounded-acceleration secant-to-newest-sample term.
Qualification must additionally include timestamp, aperture, sensor,
transport, or model errors that are not represented by raw-count uncertainty.
Firmware does not infer those physical facts.

The adversarial uncertainty fixture uses velocity `1/10 +/- 1/100`,
acceleration `1 +/- 1/10`, and following error `1/1000 +/- 1/10000`. Exact
outward arithmetic admits estimator widening `6,561,756` Q2.30 bits and rejects
`6,561,755`; the exact conservative following limit is `3,865,470` Q31.32
bits.

## Digest-bound lowering and replay

Both cores validate the same canonical byte stream. Only the private
`RealtimeConfiguration` resulting from complete stream/hash validation can
lower a FOC slot. Lowering injects the complete configuration digest into the
existing inner-current, rotor, calibrated-current, and PWM objects and now also
constructs:

- the exact integer `ServoLoopGrid`;
- a `ServoCascadeConfig`; and
- a `ServoEncoderProfile`.

Lowering requires matching logical instances, encoder and PWM/ADC device
clocks, an encoder sample period equal to the derived velocity-loop period,
encoder admitted velocity equal to the cascade velocity limit, and a cascade
sample-age limit at least as large as observation latency and no larger than one
velocity period. The existing portable cascade and encoder validators then
replay current-circle, cadence, wrap-ambiguity, precision, and range policy.

`LoweredFocAxisConfiguration` retains both the raw V6 records and the derived
objects. Its public validation path rederives all three objects under the full
digest and rejects substituted hardware, servo grid/cascade, encoder profile,
or any incoherent raw-record/derived-object pairing. Like the pre-existing
lowered hardware bundle, this is a validation boundary for trusted firmware
composition, not an unforgeable Rust capability against arbitrary code already
linked into the image.

The configuration regressions cover fixed-width round trips, every new record's
canonical restrictions, old-V5 rejection, absent records, mismatched clocks,
cadence, admitted speed, and sample age, incorrect mechanical scales,
noncanonical following limits, overwide position observations, the exact
estimator-widening boundary, and forged lowered bundles.

## Firmware verification record

The following completed against the contents of firmware commit
`05405dbed6f69a4c86a31aaeb68aa66839af4ca1`:

```sh
cargo fmt --all -- --check
cargo test -p alumina-config --locked
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
```

Observed results:

- all 25 `alumina-config` tests and all 477 portable workspace tests passed;
- formatting, diff, warnings-denied all-target Clippy, and warnings-denied
  rustdoc passed;
- strict ESP target Clippy passed for primary 8 MiB TinyBee, opportunistic
  4 MiB TinyBee, T-Deck Pro, and MKS ESP32 FOC V1.0;
- all four optimized board-qualified images linked; and
- the primary TinyBee image contains the production
  `FocServoParameters::validate_shape`,
  `FocEncoderPolicyParameters::validate_shape`, and
  `lower_servo_profiles` symbols, while host fixtures
  `simulate_servo_encoder` and `mks_foc_records` are absent. This supports
  optimized reachability of the validator/lowering, not target-loop execution.

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,092,960 | 12,304 | 249,840 | `b2b700078eca11d09a01e3ed8c1456c1c761d2f68733680ab724e615d011baa3` |
| TinyBee V1.0, 4 MiB variant | 1,092,980 | 12,304 | 249,840 | `c27512cd86529a73d0b06640a4394ed8c8001ee13a96a63a309e8b7cd6762a14` |
| T-Deck Pro | 1,027,869 | 13,056 | 525,312 | `ca501c16d1a0ac930e1d24a0fafbf3c288c1cc6c37333a4fe2dcfcf7063d17dc` |
| MKS ESP32 FOC V1.0 | 1,028,156 | 11,056 | 251,088 | `5b6c21a281d3795a43d4f156266469311272ca0ffb6de66f90e0fa2fb6f5c95b` |

Section totals are linked-image measurements, not runtime stack/heap
watermarks, interrupt latency, WCET, sample cadence, or physical safety
evidence.

## Interface transition and moving-Hyper verification

The interface update is intentionally narrow: it imports the current firmware
schema directly, displays and accepts only `ALMCFG06`, and adds a regression
that `ALMCFG05` cannot compile. It does not add a FOC authoring screen, a legacy
document transformer, or a second machine schema.

The following interface-owned gates passed while leaving sibling Hypercurve
untouched:

```sh
cargo test --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo check --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
cargo fmt --package alumina-interface --package alumina-interface-client \
  --package alumina-interface-core -- --check
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline \
  --dist /tmp/alumina-interface-v6-dist
wasm-tools validate /tmp/alumina-interface-v6-dist/alumina-interface_bg.wasm
gzip -t /tmp/alumina-interface-v6-dist/index.html.gz \
  /tmp/alumina-interface-v6-dist/alumina-interface.js.gz \
  /tmp/alumina-interface-v6-dist/alumina-interface_bg.wasm.gz
brotli -t /tmp/alumina-interface-v6-dist/alumina-interface_bg.wasm.br
git diff --check
```

All 30 application, 37 client, 121 exact-core, and one integration tests passed
(189 total), plus the compile-fail value-boundary doctest. Strict native and
WASM Clippy, WASM checking/linking, rustdoc, package-scoped format, local-sibling
source/permissive-license audit, WASM validation, and compressed-artifact
integrity passed. Workspace-wide formatting was deliberately not run because
it would have formatted concurrent user edits in Hypercurve.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `f2920e9ee33d1173970a3ec8f9ec2b4e565c24c1474d0d6e9e2d381a91fff995` |
| `alumina-interface.js` | 91,813 | `d1079fc52dc8a755a5f91993fdfe3dd8361d6051056fb6afd2b8e99edef80ddc` |
| `alumina-interface_bg.wasm` | 5,588,760 | `4280a6695a282d968ad9d59dd3dff4b74e06ce1a03a1ee04725cf3569001e62e` |
| WASM gzip | 2,504,564 | `1d5a48f1367840f2fd8902fcf78a38a91003800b2d3cdd6a0db753e2e947ea6b` |
| WASM Brotli | 1,998,890 | `4b3231ec1e2a758564da4ca10eaa8ee2477ea8530a2146ac4295798c89233cfc` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

The fixed release directory was served on loopback only at `127.0.0.1:8099` and
opened in headless Chromium. HTML, JavaScript, WASM, worker, and favicon requests
returned HTTP 200. The 439,511-byte screenshot has SHA-256
`ba28a5262d1d8024c2d8c4b263c1911f0103badf813ef5823d2c15ba90cf67d4`
and visibly shows `ALMCFG06 → exact path → cached IR`, the 2,064-byte/31-record
canonical TinyBee configuration, exact plot, and exact schedule. This was a
render check of an offline non-armable fixture, not live firmware traffic.

## License and clean-room record

All changed source remains `MIT OR Apache-2.0`. This checkpoint adds no
dependency, manifest, lockfile, or third-party implementation. The changed
source scan found no GPL/AGPL/LGPL/SSPL identifier and no copied FluidNC,
Klipper, Synthetos/g2core, or SimpleFOC implementation reference. The interface
source-policy audit accepted current local Alumina/CSGRS/Hyper sources and the
permissive dependency inventory. No GPL code was introduced.

## Closed claims and next boundary

This checkpoint closes one canonical greenfield schema, exact mechanical-scale
cross-checks, conservative uncertainty-to-controller bounds, mandatory
servo/encoder completeness, full-digest lowering, raw/derived substitution
consistency checks, interface schema rollover, and portable/multi-target
compile evidence.

It does not supply the boot-local multi-turn seed; qualify an AS5600 sensor,
sample aperture, timestamp, latency, speed, or error bound; schedule an encoder
transport; compose encoder, cascade, current loop, and PWM into a core-1 actor;
measure WCET; promote the described MKS stage; or create an energizing path.
The bare connected TinyBee was not contacted, reset, flashed, or driven. No USB,
serial, SLogic, WLAN, NetworkManager, TinyBee AP, motor, driver-power, or process
power action occurred; only localhost browser traffic was used.
