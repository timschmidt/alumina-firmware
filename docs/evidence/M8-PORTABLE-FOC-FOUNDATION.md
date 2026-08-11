# M8 portable exact FOC foundation evidence

Date: 2026-08-11

Status: allocation-free exact-lattice FOC mathematics, controller contracts,
scheduled command validation, and a deterministic functional dq plant are
implemented. This is portable software evidence. It is not a physical motor
model, MKS ESP32 FOC V1.0 board implementation, power-stage qualification,
PWM/ADC timing measurement, or permission to energize an inverter.

## Exact numerical boundary

The new `alumina-foc` crate is `no_std` and uses no floating-point operation or
allocation. `Q30` is the exact rational `bits / 2^30`, with a deliberate
`[-2, 2)` domain. Ordinary controller products select one deterministic value by
rounding to nearest with ties to even. They never saturate implicitly: an
unrepresentable result is an error. `Q30Interval` instead rounds every endpoint
outward and is used where the browser's exact value, a trigonometric reduction,
or measured uncertainty may straddle adjacent motor-control lattice points.

The portable layer now provides:

- exact/outward add, subtract, negate, multiply, divide, square, and rational
  reduction, including negative denominators and the asymmetric signed endpoint;
- adjacent Q2.30 enclosures for `2/3`, `1/sqrt(3)`, and `sqrt(3)/2`;
- three-phase, alpha/beta, and dq domains with Clarke, inverse Clarke, Park, and
  inverse Park transforms;
- a `Rotation` whose sine/cosine intervals are accepted only with an explicit
  squared-unit-norm certificate;
- min/max common-mode space-vector modulation that refuses overmodulation rather
  than clipping it, including exact 0% and 100% duty endpoints without an
  unrepresentable intermediate `2`;
- independent direct/quadrature PI loops with conditional-integration
  anti-windup and complete retained state in every update;
- widened exact current-error evaluation, so the legitimate normalized
  endpoint `+1 - (-1) = +2` survives until the declared output clamp even though
  it is not itself a Q2.30 point;
- immutable, digest-bound timing/controller/limit snapshots and scheduled
  dq-current commands with exact circular current and voltage limits; snapshot
  admission also proves the entire rectangular d/q PI output domain fits inside
  the circular voltage limit; and
- allocation-free `RotorSensor`, `CurrentSense`, and sole-owner `PowerStage`
  boundaries. Their target implementations remain absent and therefore cannot
  drive hardware.

Clarke evaluation uses widened integer linear forms. This avoids rejecting a
representable result merely because `a - (b + c)/2` or `b - c` is temporarily
outside Q2.30. Tests cover balanced vectors whose old narrow intermediates would
have been `2.25` and `3` while their final alpha/beta results remain representable.

The browser/WASM machine configuration remains authoritative for physical
units. It must reduce exact current, voltage, sensor, timing, and uncertainty
facts to an interval containing one or more Q2.30 points and bind the chosen
snapshot digest. This crate neither invents missing hardware parameters nor
silently converts a physical floating-point value.

## Deterministic functional simulation

`alumina-sim::foc` adds a dimensionless first-order dq plant:

```text
current[n + 1] = (1 - response) current[n] + response voltage[n]
```

Every term is an exact selected Q2.30 lattice point. The response must be in
`(0, 1]`; plant state and drive must remain in the exact unit circle. A trace
retains every measured pre-state, complete two-axis PI result, applied voltage,
and post-state. Two independently constructed 800-update runs are byte-for-byte
equal, the direct axis remains exactly zero, and the quadrature axis converges to
within `1/10000` of its target. Separate fixtures expose saturation and prove an
invalid response or out-of-circle drive cannot change plant state.

This recurrence is intentionally not an R/L/back-EMF/mechanical motor model. It
tests state and numerical behavior independently of unmeasured motor, inverter,
sensor, load, or sampling facts. A later identified plant and HIL trace must not
reuse this result as physical evidence.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo +esp clippy -p alumina-foc --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-foc --target xtensa-esp32s3-none-elf \
  --locked --offline -- -D warnings
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

The default workspace has 265 passing unit tests. The focused FOC crate has 11
tests, including an independently checked integer-ratio grid, signed outward
division, integer certificates for transform constants, ties-to-even cases, wide
transform intermediates, transform and modulation vectors, PI anti-windup,
widened endpoint errors, complete snapshot rejection, and scheduled-command
identity/range checks. `alumina-sim` has 28
tests, including three new FOC functional-simulation tests. Strict host Clippy
and strict no-std Clippy for classic ESP32 and ESP32-S3 pass.

The all-feature cargo-tree inventory contains 837 nonempty package/license
records, no missing license, and no GPL/AGPL/LGPL/SSPL-family dependency
expression. `cargo-deny` is configured in the repository and CI but is not
installed locally, so this checkpoint does not claim a local `cargo deny`
result. The sole GPL-family text outside documentation is the pre-existing
`THIRD_PARTY.toml` behavioral-reference ledger entry for Synthetos/g2; it has
`destination = "none"` and records that no implementation source may be copied.
No GPL-family code or dependency was added or used.

## Clean-room and qualification boundary

The implementation was derived from the repository's published mathematical
requirements and independently authored integer fixtures. No Synthetos/g2,
SimpleFOC, FluidNC, or MKS example implementation source was copied or used as
implementation material. All new code is repository-owned and licensed
`MIT OR Apache-2.0`.

Portable exact angle reduction, sine/cosine production, and rotor calibration
now have a separate later checkpoint. Still open for M8 are physical electrical
alignment, voltage and estimated/DC-current modes, velocity/position cascades,
canonical machine-configuration calibration records, MCPWM/ADC synchronization,
real power-stage/current/sensor adapters, FOC safety faults, deadline/WCET
evidence, physical shutdown measurement, and interface-side exact parameter
reduction. The MKS board remains unavailable and non-armable. See the
[exact-angle evidence](M8-EXACT-ELECTRICAL-ANGLE.md).
