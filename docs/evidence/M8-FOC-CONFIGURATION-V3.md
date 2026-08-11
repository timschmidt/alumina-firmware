# M8 canonical FOC configuration V3 evidence

Date: 2026-08-11

Status: canonical storage records and digest-bound portable FOC lowering are
implemented. This checkpoint does not initialize ADC1 or MCPWM, produce a
current sample, drive a phase input, qualify a power stage, or make any board
armable.

## Deliberate schema replacement

Machine configuration is now version 3 with magic `ALMCFG03`. Version 1 and
version 2 documents reject; there is no compatibility decoder or migration shim.
The UI and firmware are controlled together, so the simpler single authority is
preferred over preserving obsolete representations. The intercore command/report
wire remains independently versioned at 2.

The existing fixed 80-byte header and 64-byte record width remain. V3 adds five
record kinds:

- normalized FOC runtime rates, dividers, pole pairs, and unit scales;
- independently selected direct and quadrature fixed-period PI parameters;
- absolute-count rotor calibration, exact uncertainty, and rotation ULP policy;
- channel-zero/channel-one outward ADC calibration maps; and
- qualified two-shunt phase selection plus integer PWM/ADC timing and dead time.

All reserved bytes must be zero, all selectors are closed enums, and decoding
re-encodes to the unique canonical representation. Full layouts are normative in
[`CONFIGURATION.md`](../CONFIGURATION.md).

## Cross-record authority and lowering

A complete FOC axis now requires U/V/W phase bindings, exactly the two current
ADC bindings named by AB/BC/CA selection, an encoder binding, a qualified
shutdown contract, all physical scalar facts, and all seven new runtime records
(one runtime, two PI, one rotor, two current channels, one timing record).

The validator checks exact rational equality between scalar and retained facts
for pole pairs, encoder counts per turn, PWM rate, current-loop rate, and
dead-time seconds. It also requires the selected ADC resources to belong to the
same qualified stage topology as the phase resources. Controller output corners,
certified rotation precision, raw-current endpoints, reconstructed phase bounds,
integer PWM/device-cycle identity, and synchronization bounds are revalidated as
one profile.

`RealtimeConfiguration` fields are private outside `alumina-config`. Only the
independently validated service path can pair a profile with its SHA-256
identity. Lowering injects that identity into and revalidates:

- `FocParameterSnapshot`;
- `RotorCalibration` plus its `RotationPrecision`; and
- proof-wrapped `ValidatedTwoShuntCurrentCalibration`.

The lowerer also returns exact dead-time cycles. It deliberately returns no HAL
token, pin, ADC sample, duty image, or power-stage transition.

## MKS encoder capability identity

The MKS ESP32 FOC V1.0 capability package now identifies two distinct
compile-supported AS5600-compatible endpoints, each at seven-bit address `0x36`
on its independent core-1 I²C bus. This reflects the already compiled read-only
driver ownership while leaving each actual sensor, alignment, and transaction
unqualified. The canonical capability SHA-256 changed from
`9acdbe01888aed63c3427aa71feeff211c45532cb501eb80bc436773891bdb99` to
`8b14c17fc2787bce93e10610a39157e33a5a10fac477e910021f166b562532e3`.

The two power stages remain `Described`, the board remains non-armable, and an
ordinary configuration still fails its immutable qualification gate. Tests use
an explicitly synthetic package copy with only the two stage descriptors
promoted to exercise the subsequent validator; this is not persisted board
evidence.

## Verification

The focused V3 suite checks all new fixed record round trips and reserved-byte
rejection, unknown selectors, missing records, mismatched exact scalar/runtime
facts, board topology, and every possible nonempty two-chunk split of a complete
FOC document before lowering. It additionally rejects an ADC binding from the
wrong stage and a current channel whose declared sample rate cannot cover the
current loop.

Reproduced from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc \
  -p alumina-board -p alumina-config -p alumina-foc \
  -p board-mks-esp32-foc-v1 --no-deps --locked --offline
cargo +esp clippy -p alumina-config -p alumina-foc \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-config -p alumina-foc \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask capabilities --board mks-esp32-foc-v1 --json
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
git diff --check
```

The portable workspace has 302 passing tests. `alumina-config` has 22 and the
MKS board package has six. Strict host Clippy, warnings-denied focused rustdoc,
strict no-std Clippy for classic ESP32 and ESP32-S3, and strict MKS firmware
Clippy pass. All three release firmware targets link. A host-target rustdoc
invocation over the literal whole workspace is intentionally not claimed:
ESP HAL target-only packages reject a host build, so the changed portable
packages are selected explicitly above.

The final MKS release ELF has SHA-256:

```text
0d3f7eb83664e0921179b07940ebca3dd9808a3548d9318eaaaad75a1821600b
```

`xtensa-esp32-elf-size` reports 855,760 bytes of text, 10,824 bytes of data,
and 251,312 bytes of BSS. The image is larger than the preceding portable-only
checkpoint because the live configuration validator now links the certified FOC
shape checks and the expanded canonical capability document. It still contains
no ADC sampler, MCPWM operator, or energizing transition.

The all-feature workspace Cargo tree has 849 nonempty package/license records,
zero missing license expressions, and zero GPL/AGPL/LGPL/SSPL-family
expressions. The additional record is an internal workspace edge from
`alumina-config` to `alumina-foc`, not a new third-party package. A source and
manifest scan outside documentation likewise has no GPL-family match.
`cargo-deny` remains configured but is not installed locally, so no local
`cargo deny` result is claimed.

## Safety and licensing boundary

The connected MKS TinyBee V1.0 is unrelated to this portable configuration
exercise and was not touched by this checkpoint. No MKS ESP32 FOC hardware,
motor, motor supply, ADC source, encoder, or power-stage output was used.

All code is independently authored under `MIT OR Apache-2.0`. No GPL-family
source or dependency is accepted. No SimpleFOC, Synthetos/g2, vendor example, or
other copyleft implementation source was inspected or copied for this work.
