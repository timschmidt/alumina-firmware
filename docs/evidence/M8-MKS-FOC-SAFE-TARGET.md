# M8 MKS ESP32 FOC V1.0 safe-target evidence

Date: 2026-08-11

Status: an independently authored typed board package and a compile-only,
non-armable classic-ESP32 firmware composition are implemented. The target
owns the established V1.0 motor resources and synchronously makes all six
phase-control pins no-pull inputs before core 1 can await. It provides no
PWM/ADC FOC adapter, no motion commit path, and no permission to connect or
energize a power stage.

## Hardware-source reconciliation

The official V1.0 schematic and manual, plus the official
ESP32-WROOM-32D/32U and EG2133 datasheets, were used as hardware references.
The schematic identifies an ESP32-WROOM-32D; the normal module carries 4 MiB
flash and 520 KiB on-chip SRAM. The actual module marking and flash identity
remain unobserved.

The revision-specific routes are:

| Function | Motor 0 | Motor 1 |
| --- | --- | --- |
| phase U/V/W | GPIO32 / GPIO33 / GPIO25 | GPIO26 / GPIO27 / GPIO14 |
| current A/B | GPIO39 ADC1_CH3 / GPIO36 ADC1_CH0 | GPIO35 ADC1_CH7 / GPIO34 ADC1_CH6 |
| encoder SCL/SDA | GPIO18 / GPIO19 on I2C0 | GPIO5 / GPIO23 on I2C1 |
| encoder index/auxiliary | GPIO15 | GPIO13 |

Two negative facts are equally important:

- GPIO22 and GPIO12 are marked unconnected. No independent inverter enable is
  established, so the board package contains no enable resource or alias.
- The schematic/manual establish no fitted SD card or other mutable job-cache
  medium. The board package exposes no storage resource and firmware returns a
  permanently transport-faulted `NotFitted` cache backend.

The schematic ties each single phase signal to the corresponding EG2133
active-high HIN and active-low LIN-bar inputs. The component truth table makes
the consequence explicit:

| MCU phase pin | HIN / LIN-bar | High-side / low-side |
| --- | --- | --- |
| driven low | 0 / 0 | off / on |
| driven high | 1 / 1 | on / off |
| high impedance | internally biased 0 / 1 | off / off candidate |

Thus “drive all phases low” is not a disable operation. The target instead
constructs six ESP HAL `Input` owners with `Pull::None`; the EG2133's internal
HIN pull-down and LIN-bar pull-up then select the documented both-off state.
Actual board-level impedance, transition behavior, and MOSFET gate state remain
unmeasured. GPIO2 is also not a user auxiliary route: the schematic ties it to
the USB auto-programming/strap circuit, so it is retained by the service domain.

These facts supersede the earlier enable assumption derived from example-level
material. Vendor test-code and third-party FOC implementation source were not
inspected or used for this checkpoint. The referenced PDFs were reviewed from
a temporary research directory and were not copied into the repository.

## Typed package and core ownership

`board-mks-esp32-foc-v1` describes a dual-core classic ESP32 with 4 MiB flash,
two fitted three-phase power-stage devices, two distinct timed-output engines,
ADC1 current routes, two realtime I2C sensor buses, index inputs, service Wi-Fi
and UART resources, and phase-control GPIO hazards. Its canonical capability
digest is:

```text
9acdbe01888aed63c3427aa71feeff211c45532cb501eb80bc436773891bdb99
```

The firmware split gives core 0 the Wi-Fi token, UART0, boot input, and
USB-controlled GPIO2 strap. Core 1 solely owns MCPWM0/1, ADC1, I2C0/1, the six
phase pins, four current inputs, six encoder routes, and its timer group.
`establish_safe_outputs` is synchronous: it constructs all six phase pins and
all observation routes as no-pull inputs before returning resources to the
realtime executor. Their Rust types expose no output operation; the nominal
safe reassertion retains those owners unchanged.

Every step-image, writable-horizon, stage, seal, and commit operation returns
`MotionUnsupported`. There are no configured safety inputs, no FOC power-stage
trait implementation, and no code that programs MCPWM, samples ADC, talks to an
encoder, or emits nonzero duty. The current configuration IR also requires a
qualified shutdown contract. V2 has no `FocEnable` selector and structurally
supports the MKS phase-high-impedance strategy, but deliberately rejects this
board's `Described` power stages. Physical evidence must promote the immutable
stage capability to `Qualified`; no compatibility binding is fabricated.

## Reproduced build evidence

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test -p board-mks-esp32-foc-v1 -p xtask --locked --offline
cargo clippy -p board-mks-esp32-foc-v1 -p xtask --all-targets \
  --locked --offline -- -D warnings
cargo xtask board check mks-esp32-foc-v1
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-esp32-foc-v1 --profile release
```

The focused board/xtask run has 11 passing tests: five board-package tests and
six xtask tests. The complete portable workspace has 275 passing tests. Strict
host and target Clippy pass. The release firmware links
for `xtensa-esp32-none-elf`; its ELF SHA-256 is:

```text
e9b5da06c1cd39f8b1babddc70d59683a10890cc0f0c976aa6735b703d18df11
```

The all-feature dependency inventory has 845 nonempty package/license records,
no missing license expression, and no GPL/AGPL/LGPL/SSPL-family expression.
`llvm-size` reports 824,024 bytes of text, 10,704 bytes of data, and 251,440
bytes of BSS for the linked image. These are reproducibility facts, not
flash-fit or runtime-memory qualification; the ESP image layout and
stack/high-water behavior still require target evidence.

## Safety and qualification boundary

No serial device or board was available to this environment. Nothing was
flashed, connected to motor power, or energized. The target remains
`SupportLevel::Compiles`, `armable = false`, and unavailable in the physical
hardware ledger.

The following gates remain open:

- identify the received board revision, module marking, flash, and assembly;
- review and measure reset/boot levels, phase-pin impedance, EG2133 outputs, and
  whether the both-off candidate survives reset, brownout, panic, and watchdog
  paths;
- establish local E-stop/interlock inputs and measured shutdown latency, then
  promote the exact phase-high-impedance contract and board package;
- decide and implement an explicit cache medium/board variant before cached
  autonomous or synchronized jobs are advertised;
- bind the portable exact-angle generator to qualified sensor ownership and
  physical alignment; implement MCPWM/ADC synchronization, calibrated current
  sampling, dead time, deadlines, fault handling, and a sole-owner `PowerStage`
  adapter; and
- review a disconnected, current-limited, one-motor HIL procedure before any
  nonzero phase command exists.

All new source is repository-owned and licensed `MIT OR Apache-2.0`. No vendor
source or asset was copied, no GPL-family source was used, and no GPL-family
dependency was added.
