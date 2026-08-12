# M8 exact MCPWM compare and closed-owner evidence

Date: 2026-08-11

Status: exact portable center-aligned compare lowering and timer-zero sequencing
are implemented. A compile-only MKS transition configures, stops, and resets
the internal MCPWM timers while every phase pin remains a no-pull input. No
operator or pin is attached, no compare register is written, no waveform is
emitted, and no target implements `PowerStage`.

## Exact clock contract

`PwmCompareContract` binds one nonzero canonical configuration digest to:

- the boot-local device-cycle frequency;
- one complete PWM period in device cycles;
- the post-prescaler up/down-counter clock;
- the integer timer peak/period register;
- a minimum active and inactive half-period pulse; and
- a maximum whole-Q2.30-ULP interval-to-compare error.

Validation requires both integer identities:

```text
device_cycle_hz = pwm_hz * pwm_period_device_cycles
counter_clock_hz = pwm_hz * 2 * timer_peak_ticks
```

It separately binds the digest, PWM rate, device-cycle rate, and period to the
validated FOC parameter and PWM/ADC synchronization snapshots. The counter and
device clocks need not be equal. This checkpoint never rounds an edge from one
clock domain into the other.

## Interval-to-integer lowering

Each requested phase duty is already a conservative closed Q2.30 interval. The
lowerer rejects any endpoint outside inclusive `[0, 1]`, selects the interval
midpoint on the integer `0..=timer_peak` lattice, and rounds an exact halfway
case to the even comparison. The selected output is admitted only when both
the compare and `timer_peak - compare` meet the configured minimum pulse.

For selected compare `c`, peak `p`, and each Q2.30 endpoint numerator `d`, the
implementation evaluates the widened exact integer distance

```text
abs(c * 2^30 - d * p) / p
```

and rounds that error outward to a whole Q2.30 ULP. The maximum over both
endpoints includes the original interval width as well as hardware
quantization. It must fit the configured precision policy. The exact rational
`c / p` is also converted back to its outward Q2.30 enclosure; no float or
implicit saturation occurs.

The modeled action is center-aligned active-high around timer zero, matching the
future `esp-hal` `UP_DOWN_ACTIVE_HIGH` action. Its two edge locations within a
complete `2 * p` counter-tick period are retained exactly as:

```text
first_edge = c
second_edge = 2 * p - c
```

These are counter ticks, not yet `DeviceCycle` timestamps and not physical edge
observations.

## Complete-image and timer-zero owner

`PwmCompareImage` retains the digest, nonzero command token, requested device
cycle, original three duty intervals, all three comparisons, represented duty
enclosures, maximum errors, and both edge positions. Validation recreates the
entire image and requires value equality.

`PwmCompareLatchOwner` is a non-cloneable, allocation-free portable state
machine. It accepts at most one complete future image, verifies it against the
same parameter/current snapshots, requires its schedule to lie on the exact
period grid, and observes each timer-zero boundary exactly once. A successful
latch reports a monotonic `u64` period sequence and the complete image. It may
retain a prior active image across an empty period; the future current-loop
policy must separately decide whether every period requires a new command.

A forged image, foreign configuration, late/off-grid schedule, missing,
duplicated, or reordered boundary, or cycle/sequence overflow clears the
pending image and latches the owner faulted. A full pending slot is the sole
retryable error and does not replace the earlier image.

This state machine is not evidence that software wrote three registers before
a hardware update deadline or that silicon latched them atomically. Those are
obligations of the future target adapter, execution-budget proof, and HIL.

## Closed MKS HAL ownership

The MKS target was compiled against the locally locked `esp-hal 1.0.0` MCPWM
API. `ClosedMcpwmConfiguration` pairs the portable contract with explicit
peripheral and timer prescalers. Before either singleton is consumed, both
contracts are validated and both live HAL clock trees must reproduce the exact
post-prescaler counter and PWM rates.

The current HAL exposes timer configuration through `Timer::start`, not a
configure-while-stopped call. The transition therefore creates each `McPwm`,
starts timer 0 with `PwmWorkingMode::UpDown`, immediately stops it, and resets
the stopped counter to zero. During the entire transition:

- the three phase pins remain their existing no-pull `Input` owners;
- no operator is connected to a timer or GPIO;
- no `PwmPin`, action, update method, or compare value is constructed;
- timer 1 and timer 2 remain unused; and
- the controller remains private with no extractor or mutation method.

The result can report its retained contract/prescalers, but cannot stage the
portable image into hardware and cannot implement `PowerStage`. The transition
is unscheduled, not reachable from Configuration V3, and absent from the live
firmware path.

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

The portable workspace has 314 passing tests. `alumina-foc` has 40, including
nine compare/latch tests. They cover exact half duty and edge positions,
ties-to-even, range/pulse/precision rejection, clock and digest identity, image
replay, exact latch sequences, retryable capacity, terminal schedule/boundary
faults, overflow, and an independent integer error grid. Strict host Clippy,
warnings-denied focused rustdoc, strict no-std Clippy for classic ESP32 and
ESP32-S3, and strict MKS firmware Clippy pass. All three release board targets
link.

The final MKS release ELF has SHA-256:

```text
f686ddb589fb95b5e690cd3cf85ce9f0659a249657e89934d6cc0aaef3ad7e79
```

`xtensa-esp32-elf-size` reports 855,760 bytes of text, 10,824 bytes of data,
and 251,312 bytes of BSS. The section sizes are unchanged from the preceding
ADC1 checkpoint because the closed MCPWM transition is absent from the live
firmware path and is removed by link-time dead-code elimination. The changed
hash is a reproducibility observation, not evidence that a timer or phase
output ran.

## Hardware and licensing boundary

Neither the connected bare MKS TinyBee V1.0 nor any MKS ESP32 FOC hardware was
touched. No motor or motor supply was connected, no phase pin changed mode, and
no waveform was captured. The available SLogic16U3 remains disconnected; its
eventual procedure must specify channel/test-point mapping, ground and power
conditions, voltage limits, sample rate, trigger, expected edges, and abort
conditions before use.

This checkpoint adds no dependency. All implementation and documentation are
independently authored under `MIT OR Apache-2.0`. No SimpleFOC, Synthetos/g2,
vendor example, or GPL-family implementation source was inspected, copied, or
used. The all-feature workspace Cargo tree contains 850 nonempty
package/license records, zero missing license expressions, and zero
GPL/AGPL/LGPL/SSPL-family expressions. A Rust/C/C++/header/Cargo-manifest scan
outside documentation likewise has no GPL-family match. `cargo-deny` remains
configured in CI but is not installed locally, so no local `cargo deny` result
is claimed.
