# M8 exact electrical-angle and rotor-observation evidence

Date: 2026-08-11

Status: portable binary-turn angle reduction, certified fixed-point
sine/cosine generation, exact count-to-electrical-phase calibration, and
digest-bound rotor observations are implemented in `alumina-foc`. No target
sensor is read, no physical alignment is claimed, no phase pin becomes an
output, and no inverter can be energized by this checkpoint.

## Exact binary-turn boundary

`ElectricalPhase` assigns the complete wrapping `u32` range to exactly one
electrical turn. Zero, quarter, half, and three-quarter turns are therefore
exact lattice points; pole-pair multiplication, quadrant selection, and modular
wrap require no radians or floating point. A phase estimate retains a nominal
point plus a symmetric circular error in the same lattice. Estimates wider than
one thirty-second of a turn reject as unsuitable for this representation; a
machine policy normally requires a much smaller bound.

The generator reduces each phase to its nearest exact quadrant, leaving a
signed angle in `[-pi/4, pi/4]`. It then:

- encloses `pi/4` in two adjacent Q2.30 points;
- evaluates sine through degree 11 and cosine through degree 12 using
  outward-rounded reciprocal-factorial intervals;
- adds the signed alternating-series remainder, proven smaller than one Q2.30
  ULP over the reduced interval;
- restores quadrant signs/swaps exactly, with cardinal rotations remaining
  exact; and
- validates both component-width and squared-unit-norm ULP budgets supplied by
  `RotationPrecision` on every generated result.

The test certificate does not trust a host math library for the constants. It
derives `pi/4 = 4 atan(1/5) - atan(1/239)` with a 96-bit directed-integer
alternating-series enclosure and proves the embedded adjacent endpoints contain
it. Reciprocal factorials and `1/sqrt(2)` are independently checked with integer
inequalities. A 4,096-phase deterministic grid exercises every quadrant under a
128-component-ULP/256-norm-ULP test policy; the runtime checks remain
authoritative for phases between those samples.

Observation error is expanded with the global sine/cosine Lipschitz bound in
radians and clipped only to the mathematical `[-1, 1]` range. The widened
components must still produce a unit-norm interval containing exact one and
meet the caller's policy; otherwise generation fails closed.

## Count reduction and calibration

`RotorCalibration` binds a nonzero configuration digest, absolute counts per
mechanical turn, reference count, electrical phase at that reference, direction,
pole pairs, exact rational count-domain error, and an independent binary-phase
alignment error. The count error is required to be reduced and includes sensor
quantization. Its exact projection is:

```text
ceil(count_error * pole_pairs * 2^32 / counts_per_mechanical_turn)
```

The nominal count-derived phase uses nearest rounding with ties to even. Any
nonintegral reduction contributes one additional retained phase ULP; it is never
discarded. Wrap and direction are modular, and the complete sensor, alignment,
and reduction error must remain inside the admitted circular arc.

The resulting `RotorSample` carries configuration digest, pole pairs,
boot-local observation cycle, nominal phase, maximum phase error, and certified
rotation. `validate_for` checks the parameter snapshot, digest and pole-pair
identity, then regenerates the canonical rotation from the retained phase/error
pair. A plausible but substituted rotation is rejected.

Fixtures cover a 4,096-count, seven-pole-pair sensor with exact half-count
uncertainty and separate alignment error; non-power-of-two modulo-three
rounding in both directions; an independent numerator/denominator grid; exact
cardinals, quarter-turn symmetry, `1/sqrt(2)` at 45 degrees, and count-derived
30-degree `1/2` and `sqrt(3)/2` components.

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
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
git diff --check
```

The default portable workspace has 284 passing tests. `alumina-foc` has 20,
including nine exact-angle/calibration tests added after its portable foundation.
Strict host Clippy and strict no-std Clippy for classic ESP32 and ESP32-S3 pass.
All three current firmware targets link in release mode.

The MKS ELF remains byte-identical to the prior closed-gate build, with SHA-256:

```text
e9b5da06c1cd39f8b1babddc70d59683a10890cc0f0c976aa6735b703d18df11
```

This is expected: no target adapter references the portable angle generator, so
link-time elimination adds no executable MKS sensor or power-stage path.
`llvm-size` remains 824,024 bytes of text, 10,704 bytes of data, and 251,440
bytes of BSS. The dependency graph is unchanged at 845 nonempty
package/license records, with no missing or GPL/AGPL/LGPL/SSPL-family license.

## Physical and clean-room boundary

No board was visible, flashed, connected to motor power, or energized. AS5600
transport, sensor latency/repeatability, actual pole count and direction,
electrical-zero alignment, phase order, calibration stability, loop WCET, and
all target timing remain unmeasured and unqualified. The MKS package stays
non-armable and its stages stay `Described`.

The implementation uses standard published series and independently authored
integer certificates. It is repository-owned under `MIT OR Apache-2.0`; no
SimpleFOC, Synthetos/g2, vendor example, or GPL-family implementation source was
used, and no dependency was added.
