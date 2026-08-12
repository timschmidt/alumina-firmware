# M8 FOC hardware configuration V4 evidence

Date: 2026-08-11

## Scope

This checkpoint closes the configuration gap between the portable synchronized
current/PWM contracts and the still-disconnected MKS ESP32 FOC V1.0 peripheral
owners. It changes the greenfield machine-configuration authority from version
3 to version 4 with magic `ALMCFG04`. Versions 1–3 are rejected; there is no
compatibility decoder or inferred target default.

No output is enabled by this work. The compiled MKS package continues to mark
both power stages `Described`, so its mandatory qualified shutdown record
rejects before stored target selection can become reachable.

## Canonical hardware records

Every FOC axis now requires three additional fixed 64-byte records:

- one qualified ADC-frontend record for each selected current channel, retaining
  the exact classic-ESP32 attenuation setting (`0`, `2.5`, `6`, or `11` dB);
- one qualified PWM-hardware record retaining the exact peripheral source
  clock, post-divider counter clock, center-aligned timer peak, minimum active
  and inactive ticks, maximum complete Q2.30 compare error, and both raw
  zero-based HAL prescalers.

The source clock must exactly equal the counter clock multiplied by both
prescaler divisors. Independently, the counter clock must equal PWM frequency
times twice the timer peak, while the device-cycle frequency must equal PWM
frequency times the PWM period. Unknown settings, nonzero reserved bytes,
unqualified frontend/timer evidence, an empty compare domain, missing records,
either inconsistent clock representation, and a minimum pulse shorter than the
dead time after exact cross-clock multiplication reject canonically.

The attenuation setting deliberately makes no voltage-range claim. The
separately measured outward code-to-current interval remains the sole current
authority, including half-count and additive uncertainty.

## One digest-bound lowering

Full-stream SHA-256 validation retains both frontend records and the hardware
timer record beside the controller, rotor, current maps, and synchronization
envelope. `lower_foc_axis` now constructs and revalidates all of these under the
same nonzero configuration digest:

- `FocParameterSnapshot`;
- `RotorCalibration` plus its certified rotation policy;
- proof-wrapped `ValidatedTwoShuntCurrentCalibration`;
- `PwmCompareContract` with exact timer lattice and precision policy.

The lowered result retains both attenuation selections, source clock, raw
prescalers, dead time, and the portable compare contract. Tests cover every V4
record's unique fixed-width image, unknown attenuation, reserved-byte
corruption, unqualified frontend evidence, invalid pulse domain, missing
frontend state, an incompatible timer peak, insufficient dead-time coverage,
exact clock/digest replay, and the existing every-byte-split stream validation.

## Closed MKS target selection

`ClosedMcpwmConfiguration` is no longer freely constructible outside the MKS
target module. `StoredFocAxisHardwareSelection::from_configuration` accepts only
the private validated `RealtimeConfiguration` container and then requires:

- the configuration capability digest to equal the compiled MKS package;
- one exact fitted power-stage identity;
- the corresponding ordered U/V/W MCPWM resources;
- the corresponding ordered ADC1 routes;
- an AB two-shunt pair;
- both current maps to use the fixed 12-bit maximum code `4095`.

Only then can it carry the stored attenuation and timer facts toward the
unqualified ADC commissioning owner and stopped MCPWM owner. The live HAL clock
check additionally reconstructs the stored source clock from the observed
post-peripheral-divider clock before checking the timer divider and PWM rate.
The closed owner still attaches no operator or phase pin, has no compare-write
method, and implements no `PowerStage`.

The current real package cannot supply such a `RealtimeConfiguration`: the
earlier immutable `SupportLevel::Described` stage gate rejects first. Synthetic
qualified-package tests exercise portable lowering only and have a different
capability digest, so they cannot masquerade as the compiled target package.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc -p alumina-config --no-deps \
  --locked --offline
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

The portable workspace has 316 passing tests. Strict host Clippy,
warnings-denied focused rustdoc, strict target Clippy for both TinyBee flash
profiles, T-Deck Pro, and MKS ESP32 FOC V1.0 pass. All four release targets link.

Board-qualified release ELF observations are:

| Target | SHA-256 | text | data | BSS |
| --- | --- | ---: | ---: | ---: |
| MKS ESP32 FOC V1.0 | `87f2d544c18a2af3fe4548861c14efd7f5abd6cc328fdc3185417fd9908dbb70` | 858,228 | 10,792 | 251,344 |
| MKS TinyBee V1.0, 8 MiB primary | `7f76961ca1ca49793d670cdad6ed5c00e4e17189d4492062a3ce7e8284e53fc0` | 915,464 | 12,040 | 250,096 |
| MKS TinyBee V1.0, 4 MiB variant | `c3ce65608811a99d64bb8744dab43f6b2824ead08333a320feea270d47bc66c5` | 915,484 | 12,040 | 250,096 |
| T-Deck Pro | `806c55bfac0b4d9e774eb717ea0d7a32c08c0541fc0309b4d3d56fabeb29f8d3` | 856,897 | 12,800 | 525,568 |

These hashes and section sizes are reproducibility observations, not physical
timing, ADC, PWM, or energization evidence.

## Hardware and licensing boundary

The connected bare MKS TinyBee V1.0 was not read, reset, flashed, or otherwise
touched. No motor or motor power was connected. No MKS ESP32 FOC board was
available. The SLogic16U3 remained disconnected because this checkpoint makes
no waveform claim.

All code is independently authored under `MIT OR Apache-2.0` and adds no
dependency. No GPL-family source, dependency, or asset was introduced or
consulted. The locked all-feature Cargo tree contains 851 nonempty
package/license records, zero missing license expressions, and zero
GPL/AGPL/LGPL/SSPL-family expressions. A Rust/C/C++/header/Cargo-manifest scan
outside documentation has no GPL-family match.
