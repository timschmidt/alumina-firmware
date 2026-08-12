# M8 configured FOC hardware-loop simulation evidence

Date: 2026-08-11

## Scope

This checkpoint joins canonical Configuration V4, synchronized current
sampling, exact electrical angle, dq current control, interval SVPWM, and exact
integer compare ownership in one deterministic host transition. It does not
join a target HAL peripheral, physical ADC, inverter, motor, or powered bench
fixture.

The sole public constructor accepts the private `RealtimeConfiguration`
container and a FOC slot. `LoweredFocAxisConfiguration::validate` independently
replays every digest, channel, calibration, clock, timer, pulse, dead-time, and
precision invariant before simulation starts. Tests can reach a private lowered
constructor to prove that a copied and altered bundle is rejected; target or
application callers cannot use that path.

## One exact virtual period

`ConfiguredFocHardwareLoop` is allocation-free and owns one controller plus one
complete-image latch. Its first boundary receives a canonical neutral
three-phase image. Each subsequent step requires exactly one command ID and the
exact active-period start cycle, then performs this fixed transition:

1. replay the active compare image against its digest-bound contract;
2. map every center-aligned compare edge from counter ticks to device cycles by
   exact integer division, rejecting a fractional mapping;
3. select the configured nominal acquisition point and deterministic virtual
   channel/conversion offsets, then validate the complete nearest-edge timing
   witness against jitter, aperture, skew, conversion, and switching guards;
4. turn the two raw integer ADC codes into calibrated interval currents and
   reconstruct the unmeasured phase;
5. turn the raw rotor count into a certified electrical rotation, Clarke/Park
   the current, and execute the anti-windup dq PI controller;
6. inverse-Park the voltage interval, run interval SVPWM, and lower all three
   duties onto the exact timer lattice under the configured error policy; and
7. stage the entire image and accept it only at the sole next timer-zero.

The returned sample retains the input, active image, current/rotor evidence,
measured dq interval and midpoint, controller update, modulation result, and
committed image. Controller state and sequence counters advance only after the
complete next image commits. A command, raw sample, timing witness, clock grid,
precision budget, timer-zero, or arithmetic rejection terminally faults the
virtual owner.

The first model intentionally requires `current_loop_hz == pwm_hz`. It chooses
a deterministic point inside each configured timing allowance; it does not
claim those latencies describe the ESP32. Different rates, injected aperture
and latency cases, interrupt/WCET modeling, and a configuration-derived
electrical plant remain later work.

## Exactness cases

The primary fixture replays eight periods twice from the same input and requires
identical complete samples. Its interval-valued angle/control path produces a
largest observed full interval-to-lattice distance of 1,165,467 Q2.30 ULPs. A
named 1,200,000-ULP policy accepts the replay, while the otherwise identical
1,000,000-ULP policy rejects the second transition with `Quantization` and
latches closed. The test records the distinction instead of silently narrowing
an interval or substituting a half-tick estimate.

Additional cases reject an altered timer peak during lowered-bundle replay, an
unsupported divided current-loop rate, an off-grid timer-zero, an
out-of-calibration raw ADC code, attempted reuse after a fault, and a timer
lattice whose switching edge maps to a fractional device cycle.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc -p alumina-sim -p alumina-config \
  --no-deps --locked --offline
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --features board-mks-tinybee --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --features board-mks-tinybee-4mb --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --features board-mks-esp32-foc-v1 --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --features board-t-deck-pro --target xtensa-esp32s3-none-elf \
  --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

The default portable workspace has 320 passing tests. Strict full host Clippy,
warnings-denied focused rustdoc, strict target Clippy for both TinyBee flash
profiles, T-Deck Pro, and MKS ESP32 FOC V1.0 pass. All four release targets
link.

Board-qualified release ELF observations are:

| Target | SHA-256 | text | data | BSS |
| --- | --- | ---: | ---: | ---: |
| MKS ESP32 FOC V1.0 | `45a17f740f0c8fe5a3da0ba8dc49d593b4aff4067399e896c8c4914e9eb7983d` | 860,152 | 10,792 | 251,344 |
| MKS TinyBee V1.0, 8 MiB primary | `5fb4ec1c7fab3d48856b3895b67e7690088a5e8485b7897230bff1c014b0f414` | 917,400 | 12,040 | 250,096 |
| MKS TinyBee V1.0, 4 MiB variant | `624cf0d8ea8ddf64b83fad0ff261c4b99d3f7a5096f022a281dbd5b6481463c5` | 917,420 | 12,040 | 250,096 |
| T-Deck Pro | `95ab611da6f56b54c29352632cb90ead6262b8c633b4ad421bad15193dd80313` | 858,825 | 12,800 | 525,568 |

The replay validator adds 1,924–1,936 bytes of target text relative to the V4
checkpoint and no data or BSS. These hashes and linked section sizes are
reproducibility observations, not physical timing, flash-fit, ADC, PWM, or
energization evidence.

## Hardware and licensing boundary

No target code invokes this simulator. It writes no MCPWM comparison, reads no
ADC, implements no target `PowerStage`, and provides no energization route. The
connected bare MKS TinyBee V1.0 was not read, reset, flashed, or otherwise
touched; no motor or motor power was connected. No MKS ESP32 FOC board was
available, and the SLogic16U3 remained disconnected because this checkpoint
makes no physical waveform claim.

All new code is independently authored under `MIT OR Apache-2.0`. It adds no
external dependency and uses only the current workspace stack. No GPL-family
source, dependency, or asset was introduced or consulted. The locked
all-feature Cargo tree still contains 851 nonempty package/license records,
zero missing license expressions, and zero GPL/AGPL/LGPL/SSPL-family
expressions. A Rust/C/C++/header/Cargo-manifest scan outside documentation has
no GPL-family match.
