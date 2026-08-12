# M8 classic ESP32 ADC1 commissioning-owner evidence

Date: 2026-08-11

Status: a compile-only, software-started ADC1 owner exists for all four current
routes on MKS ESP32 FOC V1.0. It is unscheduled, uncalibrated, not PWM
synchronized, and cannot implement the real-time `CurrentSense` boundary. No
hardware was read, flashed, powered, or energized for this checkpoint.

## Honest HAL boundary

The implementation was written against the locally locked `esp-hal 1.0.0`
source. Its classic-ESP32 ADC API exposes one-shot conversions started by
software. It does not expose an MCPWM-triggered acquisition aperture through
this path. Alumina therefore does not infer a hardware trigger, sample-and-hold
cycle, simultaneous pair, DMA transfer, or switching-edge relationship from a
successful conversion.

The target type-state transition consumes the sole ADC1 token and these exact
schematic routes:

| Logical input | GPIO | Classic ESP32 ADC1 channel |
| --- | ---: | ---: |
| motor 0, channel 0 / phase A route | 39 | 3 |
| motor 0, channel 1 / phase B route | 36 | 0 |
| motor 1, channel 0 / phase A route | 35 | 7 |
| motor 1, channel 1 / phase B route | 34 | 6 |

Construction requires one explicit approximate HAL attenuation selection for
each route: 0 dB, 2.5 dB, 6 dB, or 11 dB. It retains those selections for later
reporting and uses the HAL-default 12-bit resolution, so the raw code lattice is
`0..=4095`. Selecting attenuation proves no voltage range, gain, offset,
linearity, noise, temperature behavior, or current accuracy. Configuration V3
does not yet carry or authorize these hardware selections, so the transition is
not connected to configuration activation.

## Diagnostic pair contract

`SequentialAdcAcquisition` is an allocation-free portable state machine. One
request names a logical axis, nonzero diagnostic correlation token, and request
cycle. While a request is active it admits exactly:

1. channel 0 conversion completion; then
2. channel 1 conversion completion.

Overlap, zero tokens, reversed channel order, code overflow, time reversal, and
completion without a request reject. Raw-range or time-order failure aborts the
partial pair. Explicit abort returns the original request and never creates a
result.

The completed `SequentialAdcPair` records raw counts and the cycles at which
software observed each conversion complete. Those cycles are deliberately
named `channel*_conversion_completed_at`; they are not renamed or promoted to
sample-and-hold times. The result contains no configuration digest, duty token,
PWM period sequence, switching edges, or calibrated current interval. There is
no conversion into `PwmAdcSampleStamp` or `CurrentSample`.

The MKS owner polls only the channel selected by the portable state machine and
maps it to the corresponding typed GPIO. `WouldBlock` preserves the pending
state. A terminal peripheral error aborts the pair. Neither the raw owner nor
the enclosing target implements `alumina_foc::CurrentSense`.

## Power-stage boundary

ADC setup changes only the four input-only ADC routes to analog mode. Both
MCPWM singleton tokens remain sealed together with their three no-pull phase
inputs. No MCPWM peripheral, operator, timer, compare register, GPIO output, or
nonzero duty is constructed. All motion operations continue to reject and the
board package remains `compiles`, non-armable, and physically unqualified.

The next separate checkpoint is exact duty-interval to integer-compare
lowering plus a closed MCPWM timer-zero ownership model. It must still leave the
phase GPIOs high impedance. Canonical attenuation/clock selection, synchronized
sampling, truthfully derived edge stamps, calibrated analog evidence, fault
shutdown, and energization remain later gates.

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
git diff --check
```

The portable workspace has 305 passing tests. `alumina-foc` has 31, including
three new sequential-acquisition state tests. Strict host Clippy,
warnings-denied focused rustdoc, strict no-std Clippy for classic ESP32 and
ESP32-S3, and strict MKS firmware Clippy pass. All three release board targets
link.

The final MKS release ELF has SHA-256:

```text
5df04c6f860ff23cb06155d3c2ceeb9a676d23cef93b3bbe9111e276b705f047
```

`xtensa-esp32-elf-size` reports 855,760 bytes of text, 10,824 bytes of data,
and 251,312 bytes of BSS. The sections are the same sizes as the preceding
configuration checkpoint because the diagnostic transition remains
unscheduled and link-time dead-code elimination removes it from the live image.
The changed hash is a reproducibility observation, not proof that the ADC path
runs or that timing is safe.

## Hardware and licensing boundary

The connected bare MKS TinyBee V1.0, which has no motors or motor supply
connected, is unrelated to this target slice and was not touched. The MKS ESP32
FOC board is not yet available. There was no ADC source, encoder, motor supply,
logic-analyzer capture, or current measurement.

An SLogic16U3 is available for later disconnected-load timing work but was not
connected. Before use, the HIL procedure must name every channel/test point,
common-ground and power condition, voltage limit, trigger, sample rate, expected
waveform, and abort condition; availability is not evidence.

The only added third-party dependency edge is a direct use of the already
locked `nb 1.1.0` package used by `esp-hal`, under `MIT OR Apache-2.0`. All new
implementation and documentation are independently authored under
`MIT OR Apache-2.0`. No SimpleFOC, Synthetos/g2, vendor example, or GPL-family
implementation source was inspected, copied, or used.

The all-feature workspace Cargo tree has 850 nonempty package/license records,
zero missing license expressions, and zero GPL/AGPL/LGPL/SSPL-family
expressions. The extra tree record is the new direct firmware edge to the
already locked `nb` package, not a newly resolved package. A Rust/C/C++/header
and Cargo-manifest scan outside documentation likewise has no GPL-family match.
`cargo-deny` remains configured in CI but is not installed locally, so no local
`cargo deny` result is claimed.
