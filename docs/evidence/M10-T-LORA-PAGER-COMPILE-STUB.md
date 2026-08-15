# Current T-LoRa Pager compile-only stub

Date: 2026-08-15

Implementation commit:
`31a66357d6d60b007ea149c215a02f81e939f28e`.

This checkpoint turns the late current T-LoRa Pager registry entry into an
exactly selectable ESP32-S3 firmware image while keeping every board-specific
operational claim closed. It is a `compiles`, `late-stub`, non-armable target,
not a peripheral-support or hardware-bring-up milestone.

## Pinned hardware and license boundary

The sole Pager hardware source was the official
[LilyGoLib hardware page at revision `38e6f8dee3ba78b340512af9a013365ef248a7d0`](https://github.com/Xinyuan-LilyGO/LilyGoLib/blob/38e6f8dee3ba78b340512af9a013365ef248a7d0/docs/hardware/lilygo-t-lora-pager.md).
That repository declares the
[MIT license](https://github.com/Xinyuan-LilyGO/LilyGoLib/blob/38e6f8dee3ba78b340512af9a013365ef248a7d0/LICENSE).
The independently authored Rust package uses only the functional hardware
description: no LilyGoLib implementation, third-party library, generated
asset, photograph, or binary was copied.

No registry dependency was added. The new package depends only on existing
workspace path crates and inherits `MIT OR Apache-2.0`. No GPL, AGPL, LGPL,
SSPL, or other copyleft implementation source was introduced. `cargo-deny` was
not installed locally, so no local deny result is claimed; the existing
`cargo deny check bans licenses sources` CI job remains the release authority.

## Typed capability

The package fixes these image facts:

| Fact | Value |
| --- | --- |
| board ID | `t-lora-pager-current` |
| chip / target | dual-core ESP32-S3 / `xtensa-esp32s3-none-elf` |
| service / realtime core | 0 / 1 |
| flash / PSRAM / internal SRAM | 16 MiB / 8 MiB / 512 KiB |
| qualification / implementation | `compiles` / `late-stub` |
| armable | false |
| capability document | 3,157-byte `ALMCAP04` V4 |
| SHA-256 | `38b450496cb2a53d188eff6f06061b68dffc6a29573a093d0e012ac1e7672d1a` |

The descriptive inventory contains the documented shared I2C and SPI routes,
external and GNSS UART routes, audio I2S route, direct GPIOs, interrupts, and
16 fitted endpoints: ES8311, XL9555, BHI260AP, TCA8418, PCF85063A, BQ27220,
DRV2605, BQ25896, ST7796U, SD, MIA-M10Q, ST25R3916, the fitted-option LoRa
module, NS4150B, AW9364, and the rotary encoder. Every endpoint is exactly
`SupportLevel::Described`.

The package does not infer whether the RF population is SX1262 or SX1280. That
is a named board-identity HIL gate. Every direct GPIO is service-owned,
nonhazardous, and high-impedance. There is no machine-output contract, safe
output image, graph-addressable physical resource, passive overview provider,
digital-capture provider, flash layout, or arm authority.

Package tests cover canonical identity reproduction, memory/core facts,
independent shared-SPI chip selects, structural validation, absence of
operational providers, and the complete no-machine-output invariant. The
canonical capability test suite also now includes this fifth physical package.

## Closed dual-core image

The `board-t-lora-pager` feature participates in the workspace registry,
exactly-one-board firmware selection, ESP32-S3 target check, short selector,
capability command, and artifact-preserving board build.

The common Embassy composition retains its fixed domains:

- core 0 receives the Wi-Fi token and runs the existing network/web/service
  tasks;
- core 1 receives only its timer and establishes an empty safe-output and
  safety-input contract before Wi-Fi starts; and
- all unimplemented HAL peripheral tokens remain inaccessible after the
  singleton split.

The fitted SD endpoint is deliberately backed by a zero-block, permanently
faulted cache transport. Firmware neither probes SD nor configures its pins:
XL9555-mediated SD power/detect, shared-SPI chip-select establishment, and
reset behavior require a later reviewed implementation and fixture evidence.
The same closed boundary applies to display, audio, keyboard, GNSS, LoRa, NFC,
sensors, power management, haptics, backlight, rotary input, overview, and
waveform capture.

## Annotated-photo gate

The package publishes an empty `visuals` catalog. No upstream image was copied
and no synthetic board image was substituted for the user's preferred
real-object view. A `visual.top-hotspots` HIL requirement blocks `bench`
promotion until a licensed photograph of the exact physical revision is
captured, digested, annotated in normalized coordinates, and reconciled with
the typed resources.

## Verification

At the implementation commit above:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- `cargo test --locked --offline` passed 558 tests, including five Pager
  package tests and its xtask selector test;
- portable all-target warnings-denied Clippy and no-dependency
  warnings-denied Rustdoc passed;
- all five physical board manifests/packages passed `cargo xtask board check`;
- `cargo xtask capabilities --board t-lora-pager --json` independently
  reproduced the 3,157-byte identity and reported
  `capability_digest_verified: true`;
- `cargo xtask check --board t-lora-pager` passed without flashing;
- strict ESP-target warnings-denied Clippy and Rustdoc passed for
  `board-t-lora-pager`; and
- `cargo xtask build --board t-lora-pager --profile release` linked and
  preserved the selected release ELF without flashing.

Tool versions were portable Rust/Cargo 1.88.0 and ESP Rust/Cargo
1.90.0-nightly.

The optimized compile/link artifact is:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-firmware-t-lora-pager-current` | 10,607,956 | `b2fd9eafa90d5d0f9ac84e13b27acf1a4d2c06bcb1b4bf6a4836783d3ada10d0` |

This is an ELF compile/link artifact, not a flash-image size or boot result.

## Hardware and moving-Hyper boundary

No board was opened, flashed, reset, enumerated, or contacted. In particular,
the connected bare MKS TinyBee, its serial endpoints and GPIOs, the SLogic16U3,
and workstation Wi-Fi remained untouched. No Pager fixture was claimed
available.

This firmware-only slice required no `alumina-interface` or CSGRS/Hyper build.
Hypercurve and every sibling Hyper/CSGRS repository remained user-owned,
actively editable, and read-only: no file, diff, status, pin, reset, format,
stage, or commit operation was performed there.

## Claims deliberately kept closed

This checkpoint does not establish Pager boot, Wi-Fi/AP behavior, web serving,
storage, peripheral communication, reset polarity, electrical safety, timing,
core isolation, PSRAM behavior, visual accuracy, RF population, radio
operation, machine I/O, HIL, or production qualification. It adds an honest
compile target and capability-shaped implementation seam for those later
milestones.
