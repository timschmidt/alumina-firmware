# M8 exact current-sampling and PWM/ADC synchronization evidence

Date: 2026-08-11

Status: portable raw-ADC calibration, bounded two-shunt reconstruction, and
replayable PWM/ADC timing contracts are implemented in `alumina-foc`. No target
ADC or MCPWM peripheral is initialized, no hardware sample or switching edge is
claimed, and the MKS ESP32 FOC V1.0 package remains non-armable.

## Exact ADC boundary

`CurrentChannelCalibration` admits only an explicitly configured inclusive
interior code window. Both ADC rail codes are excluded, the selected-zero code
must be strictly inside the window, and polarity is explicit. The browser will
derive the normalized-current-per-count endpoints outward from exact shunt,
amplifier, attenuation, reference, and machine facts. Firmware retains those
endpoints as a Q2.30 interval and computes

```text
(raw_code - selected_zero_code) * gain_interval +/- additive_error
```

with an exact integer count multiplier and widened intermediate arithmetic.
There is no floating-point conversion and no hidden rescaling or nearest-point
collapse. `maximum_additive_error` represents uncertainty not already included
in the gain interval, including selected-zero rounding, drift, ADC INL/noise,
and analog error. Validation requires it to be at least half of the upper gain,
rounded outward, so a quantized ADC code can never be treated as an exact analog
current.

Both raw-code endpoints are evaluated during channel validation. They must fit
the normalized `[-1, 1]` domain and an explicit interval-width policy. Runtime
codes outside the admitted interior window reject. A two-shunt snapshot then
checks all four endpoint pairs, which covers the complete affine input box, and
requires every measured and reconstructed phase to fit its configured current
and width limits.

## Synchronized two-shunt witness

`PwmAdcSynchronization` binds a nonzero configuration digest to the boot-local
device-cycle frequency and an integer PWM period. It declares the nominal
acquisition offset and maximum trigger jitter, acquisition span, interchannel
skew, conversion latency, and edge-free guard. Static validation proves that the
worst admitted acquisition and conversion complete inside one PWM period and
that the cycle frequency is exactly divisible by that period.

Every `PwmAdcSampleStamp` retains:

- the same configuration digest and a nonzero committed-duty token;
- PWM period sequence and exact period-start cycle;
- acquisition-start and both channel sample-and-hold cycles;
- conversion-complete cycle; and
- the nearest switching-edge cycles bracketing the acquisition aperture.

The validator checks timestamp order, PWM-period containment, trigger jitter,
aperture duration, channel skew, conversion latency, and both switching guards.
This makes the evidence replayable, but it deliberately does not claim more
than the data can prove. One isolated stamp cannot establish continuity of the
period sequence, correspondence between the token and a physical compare
image, or truthfulness of the reported nearest edges. The future sole-owner
backend must derive those fields from its exact integer MCPWM image, enforce
stream continuity, and qualify the relationship with logic-analyzer/DSO HIL.

The two measured phase intervals are widened by a declared maximum normalized
current slew per device cycle multiplied by the maximum interchannel skew. The
third phase is reconstructed as the negative sum of the measured pair. Because
ordinary intervals intentionally discard correlation, the resulting
independent-box zero-sequence residual is retained explicitly and supplied to
the Clarke transform instead of being asserted away.

Only `ValidatedTwoShuntCurrentCalibration` can produce a canonical
`CurrentSample`; this moves complete immutable snapshot and endpoint validation
out of the fixed-rate path. The sample retains the original raw counts and full
timing stamp. Before controller use, `validate_for` binds the digest, exact
device-cycle/PWM-rate product, and phase-current limit to the FOC parameter
snapshot, then replays calibration and synchronization and requires byte-for-
byte value equality. A substituted phase interval, residual, raw count, stamp,
or foreign snapshot rejects.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc -p alumina-foc --no-deps \
  --locked --offline
cargo +esp clippy -p alumina-foc --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-foc --target xtensa-esp32s3-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

The default portable workspace has 299 passing tests. `alumina-foc` has 28,
including the eight new current-calibration, synchronization, reconstruction,
and replay tests. Strict host Clippy, warnings-denied FOC rustdoc, and strict
no-std Clippy for classic ESP32 and ESP32-S3 pass. The sealed MKS firmware
adapter passes strict classic-ESP32 Clippy, and all three release firmware
targets link.

The final MKS ELF has SHA-256:

```text
f5a3c2cbe2e9028e0c7d75c84ea2c6d4cc5d4d39e3e8661c55775dabc08e40c9
```

`xtensa-esp32-elf-size` reports 823,308 bytes of text, 10,704 bytes of data,
and 251,440 bytes of BSS. It is byte-identical to the preceding
AS5600/closed-ownership image because the target does not reference the new
portable current implementation. These are reproducibility observations, not
flash-fit, stack-watermark, latency, or timing qualification.

The full-workspace all-feature cargo-tree output has 848 nonempty
package/license records, no missing license, and no
GPL/AGPL/LGPL/SSPL-family expression. The dependency graph is unchanged. A
Rust/C/C++/header/Cargo-manifest scan outside documentation has no GPL-family
match. `cargo-deny` remains configured in CI but is not installed locally, so
no local `cargo deny` result is claimed.

## Open target and physical boundary

The contract does not choose classic-ESP32 ADC attenuation, characterize its
reference/linearity/noise, select a hardware trigger mechanism, allocate DMA,
construct MCPWM compare registers, or define the interval-to-controller nominal
point policy. Actual shunt/amplifier gain and polarity, offset and temperature
drift, channel ordering, acquisition aperture, edge position, PWM dead time,
ripple, aliasing, conversion latency, ISR WCET/jitter, and shutdown latency all
remain unmeasured and unqualified.

No board was visible, flashed, connected to motor power, or energized. The MKS
target still seals raw ADC1 and MCPWM ownership behind types with no sampler,
power-stage trait, token extractor, or output transition. Firmware motion still
rejects, and configuration still rejects its merely `Described` shutdown
stage.

This checkpoint adds no dependency. All implementation and documentation are
repository-owned under `MIT OR Apache-2.0`; no SimpleFOC, Synthetos/g2, vendor
example, or GPL-family implementation source was inspected, copied, or used.
