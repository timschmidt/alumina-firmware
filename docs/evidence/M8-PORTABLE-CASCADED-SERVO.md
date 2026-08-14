# M8 portable cascaded servo — offline evidence

Date: 2026-08-14

Status: implemented portable exact-lattice position/velocity cascade and
independent deterministic mechanical simulation checkpoint. No production
target selects this controller. This is not encoder-estimator, current-loop,
PWM/ADC, WCET, motor, energization, or physical safety evidence.

## Result and moving-source isolation

Firmware commit `90cc483245aaf9256fab7dc3bf52c19bfe0b5ff6` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and every manifest path is beneath the
`aluminafw` repository. No manifest, lockfile, or dependency policy changed.
Hypercurve is an intentionally moving sibling worktree; this work did not edit,
format, reset, pin, or otherwise constrain it, and no transient Hypercurve result
was admitted into firmware evidence.

## Exact mechanical and timing domains

`ServoPosition` is a signed Q31.32 lattice point whose physical axis unit is
deliberately left to a future complete machine configuration. Ordered closed
`ServoPositionInterval` observations retain exact endpoints, an exact `u64`
width over the entire signed-64-bit domain, and a deterministic midpoint toward
the lower endpoint. Position differences widen to signed `i128`; velocity
differences widen to signed `i64`, so valid endpoint subtraction cannot wrap at
the storage type's extremes.

`ServoLoopGrid` derives current-, velocity-, and position-loop boundaries from
one epoch, a nonzero device-cycle frequency, and the existing immutable
`FocTimingProfile`. It rejects a fractional current-loop period and checked
multiplication overflow. Velocity boundaries are exact integer multiples of the
current period; position boundaries are exact integer multiples of the velocity
period. Before-epoch and off-grid service are rejected rather than rounded.

## Immutable profile and input contracts

`ServoCascadeConfig` is tied to the complete `FocParameterSnapshot` digest and
loop timing. Validation requires a nonnegative position gain, positive bounded
normalized velocity and current limits, a nonzero following-error limit, a
sample-age bound no larger than one velocity period, a valid velocity PI
controller, and a fixed d-current plus either q-controller endpoint inside the
configured current circle.

Scheduled `ServoSetpoint` values carry a contiguous nonzero command identity,
their exact position-loop boundary, the complete configuration digest, a Q31.32
position, and bounded velocity/q-current feed-forward values.
`ServoKinematicSample` values carry a contiguous nonzero sequence, distinct
physical-sample and availability cycles, the same configuration digest, and
conservative position and velocity intervals. Samples must represent strictly
increasing physical instants, become available no earlier than their represented
instant and no later than service, remain within the fixed age bound, and stay
inside the configured velocity range.

## Allocation-free transactional cascade

`CascadedServoController` is fixed-memory and `Copy`; it services every
contiguous current-loop boundary. A setpoint is required, and only accepted, on
a position boundary. A sample is required, and only accepted, on a velocity
boundary. The first boundary is the grid epoch, so both inputs establish the
initial retained state before an update can be reported.

At a position boundary the controller forms the full conservative position
error interval, rejects its worst-case absolute endpoint above the following
limit, and maps it through the proportional gain with outward floor/ceiling
rounding. It selects the midpoint path with ties-to-even rounding, adds exact
velocity feed-forward, and clamps both the selected point and enclosure to the
configured velocity range.

At a velocity boundary it forms the widened target-minus-observation interval,
uses the deterministic midpoint observation in the existing widened-error
anti-windup PI controller, adds the retained q-current feed-forward, and emits a
complete dq-current target with the validated fixed d component. The target is
held between velocity boundaries.

Each service attempt runs against a value copy. Success commits all identities,
counters, PI state, and held targets atomically. Failure leaves the prior
accepted state unchanged, latches the exact first cause, and makes every later
attempt return `FaultLatched`. Rejections name grid or tick substitution,
presence shape, configuration/cycle/identity mismatch, observation ordering and
age, range or following-error violation, arithmetic, counter overflow, and
incomplete internal state.

## Adversarial tests and independent simulation

Nine new `alumina-foc` regressions cover:

- the complete extreme Q31.32 interval width and midpoint;
- exact nested periods, due domains, and before/off-grid rejection;
- snapshot, timing, age, and current-circle profile gates;
- update-versus-hold behavior on every current tick;
- outward position uncertainty and widened velocity error;
- transactional first-cause behavior for missing input;
- stale, overspeed, and following-error rejection before control state changes;
- skipped current ticks and noncontiguous command identity; and
- rejection of a repeated physical sample instant without partial state change.

`FirstOrderServoPlant` is a deliberately dimensionless fixture. It applies the
exact convex recurrence

```text
velocity' = (1 - response) * velocity + response * q_current
```

and integrates the result into the Q31.32 position lattice with checked fixed
point arithmetic. `simulate_cascaded_servo` independently constructs inputs
only on their due domains, services every current tick, and records every held
current target and plant transition. Two simulator regressions run the same
12,000-current-tick scenario twice, require byte-for-byte equal traces, observe
exactly 1,200 velocity updates and 240 position updates, and verify convergence
within fixed position and velocity lattice thresholds. The plant assumes ideal
q-current availability; it is not an electrical or motor model.

## Verification record

The following completed offline against the contents committed as
`90cc483245aaf9256fab7dc3bf52c19bfe0b5ff6`:

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
  rg 'CascadedServoController|ServoLoopGrid|ServoCascadeConfig|ServoKinematicSample|ServoSetpoint|position_velocity_target'
```

Observed results:

- all 463 portable default-member tests passed, including 49 FOC and 44
  simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for primary 8 MiB TinyBee, opportunistic 4 MiB
  TinyBee, T-Deck Pro, and MKS ESP32 FOC V1.0 production configurations;
- all four optimized production images linked successfully;
- the primary 8 MiB TinyBee image has 1,068,100 bytes text, 12,224 bytes data,
  and 249,920 bytes BSS, with SHA-256
  `118ad889bbc140c7cebab881ad7903de3da5fdb1abac44de8eafb760f4d9142a`;
- the opportunistic 4 MiB TinyBee image has 1,068,140 bytes text, 12,224 bytes
  data, and 249,920 bytes BSS, with SHA-256
  `9a0fd4c9731cb9d611c5bc45ba7f2a3f8a1ae317a7b6d023d3f7b80bfee03a03`;
- the T-Deck Pro image has 1,002,533 bytes text, 12,976 bytes data, and 525,392
  bytes BSS, with SHA-256
  `c9e5b47416c35d4be4234be6c2d7a792b3b85922bb4900c6de81cdf90799849d`;
  and
- the MKS ESP32 FOC V1.0 image has 1,001,076 bytes text, 10,976 bytes data, and
  251,168 bytes BSS, with SHA-256
  `3bf52dda2e589c2b085a01fd9a71b59c131576c3f44e063381a694f3e53a472f`.

The TinyBee and T-Deck Pro section-size triples are unchanged from the refill
supervisor checkpoint. Their whole-file hashes changed while linking the
modified FOC workspace crate; unchanged linked section totals do not establish
byte identity or behavior identity. The MKS FOC image is recorded here without
claiming comparison to an immediately measured prior baseline. These static
measurements are not runtime stack/heap watermarks or control-loop WCET.

The production-symbol query returned no match for the new controller, grid,
profile, sample/setpoint types, or position-to-velocity helper. This supports
optimized build unreachability, not physical safety or armability.

No dependency, manifest, lockfile, or deny policy changed, so the existing
MIT/Apache-compatible inventory is unchanged. A changed-Rust scan, including an
explicit scan of the new servo module, found no GPL/AGPL/LGPL/SSPL identifier or
copied FluidNC, Klipper, Synthetos/g2core, or SimpleFOC implementation reference.
The local environment does not have `cargo-deny` installed, so no ad hoc
installation was attempted; configured CI license enforcement remains required.

## Closed claims and next boundary

This checkpoint closes the portable numeric domains, exact nested schedule,
immutable profile/input validation, allocation-free transactional cascade,
first-cause retention, and deterministic ideal-current replay. It does not give
the Q31.32 lattice physical units; store or lower a servo profile; unwrap an
encoder; estimate a bounded velocity; define cached servo commands; compose the
inner current loop; install a core-1 actor; select MCPWM/ADC hardware; qualify
WCET; or establish an energizing path.

The next safe offline boundary is an exact encoder unwrapping and bounded
velocity-observation contract, kept portable and independent of Hypercurve.
Later configuration and browser/WASM integration must use an explicit isolated
checkpoint against the then-current sibling Hyper stack rather than treating a
moving Hypercurve worktree as reproducible input.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
