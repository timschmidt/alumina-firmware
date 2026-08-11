# M8 qualified FOC shutdown-contract evidence

Date: 2026-08-11

Status: configuration V2 now represents inverter shutdown as a first-class,
axis-local hardware contract and retains the admitted resource topology on core
1. This is canonical-format, validator, and linked-image evidence. It does not
qualify the MKS ESP32 FOC V1.0 power stage, implement a target `PowerStage`, or
permit any phase pin to become an output.

## Deliberate V2 boundary

The canonical document magic is now `ALMCFG02`, its schema version is exactly
`2`, and the inter-core configuration command/report version is exactly `2`.
V1 bytes are rejected rather than translated. Binding selector 28, the old
`FocEnable` fiction, is unassigned and rejects; no alias or compatibility shim
exists.

Record kind 3 carries one FOC shutdown contract in the existing fixed 64-byte
record envelope. It binds:

- the logical FOC-axis instance and selected dedicated-enable,
  dedicated-disable, or phase-high-impedance strategy;
- the fitted power-stage device and, only for a dedicated strategy, its control
  resource and active polarity;
- a nonzero inclusive maximum transition-to-off bound in device cycles; and
- exactly `Qualified` evidence.

Reserved bytes, absent-control encoding, strategy shape, polarity, evidence,
and record order are canonical. Every FOC axis still requires U/V/W bindings
and its six existing motor/control facts. It now additionally requires exactly
one shutdown contract, and a logical instance cannot mix stepper and FOC
bindings. Core 1 retains up to four complete FOC profiles in logical-instance
order.

## Immutable qualification and ownership gates

The validator resolves the stage as a realtime-owned hazardous `Device` and
requires its immutable board-package device descriptor to be
`SupportLevel::Qualified`. A configuration record that labels itself qualified
cannot promote `Described`, `Compiles`, or `Bench` board evidence.

The fitted-device topology must contain all three bound phase resources and any
dedicated control. Phase-high-impedance shutdown additionally requires the
stage and U/V/W resources to advertise `SafeValue::HighImpedance`. A dedicated
enable must have an inactive board safe level; a dedicated disable must have an
active board safe level. Electrical polarity/input constraints are checked, and
stage/control ownership is claimed as one preflighted transaction: duplicate or
capacity failure changes none of the claim set.

Host tests promote cloned MKS descriptors to `Qualified` only inside a fixture,
prove the structurally correct phase-high-impedance contract is retained
exactly, and exercise both dedicated-control safe-level interpretations. The
real MKS package remains `Described`; it therefore returns the stable shutdown
fault family and remains non-armable.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
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

The default portable workspace has 275 passing tests; `alumina-config` has 20,
including canonical V2, removed-selector, immutable-qualification, topology,
safe-polarity, retained-profile, and transactional-claim cases. Strict host
Clippy and strict target Clippy for both ESP32 families pass. Release firmware
links for TinyBee, T-Deck Pro, and MKS ESP32 FOC V1.0. The final MKS ELF has
SHA-256:

```text
e9b5da06c1cd39f8b1babddc70d59683a10890cc0f0c976aa6735b703d18df11
```

`llvm-size` reports 824,024 bytes of text, 10,704 bytes of data, and 251,440
bytes of BSS for that linked image. The all-feature dependency inventory has
845 nonempty package/license records, no missing license expression, and no
GPL/AGPL/LGPL/SSPL-family expression.

## Physical and licensing boundary

No board was visible to this environment. Nothing was flashed, connected to
motor power, or energized. Actual reset impedance, both-off behavior, shutdown
latency, fault response, and watchdog/brownout behavior remain unmeasured. The
next physical evidence must promote a specific immutable board package; a UI
configuration cannot do so.

All implementation in this checkpoint is repository-owned and licensed
`MIT OR Apache-2.0`. No vendor motion/FOC source, Synthetos/g2 source,
SimpleFOC source, or GPL-family implementation material was used, and no
GPL-family dependency was added.
