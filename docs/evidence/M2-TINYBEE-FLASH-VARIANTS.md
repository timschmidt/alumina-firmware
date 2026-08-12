# M2 TinyBee flash-variant evidence

Date: 2026-08-11

Status: 8 MiB is the primary compile-time TinyBee package and 4 MiB is a
separate opportunistic compile-time variant. Both currently link and fit an
explicit image of their declared capacity. Neither result changes the board's
`compiles`, non-armable qualification.

## Decision and evidence boundary

The connected bare PCB is operator-identified as `MKS TinyBee v1.0` and a
read-only `espflash board-info` observation reported 8 MB. The operator expects
most shipped TinyBees to use that capacity and directed Alumina to make it the
primary target while retaining 4 MiB when feasible. The primary choice uses
that direction plus the observed fixture. This checkpoint does not claim an
independent population survey or identify the fixture's module marking.

The earlier safe-smoke evidence contained an erroneous sentence saying the
then-current board package declared 4 MB. Inspection of the package at its
recorded `e91886a` commit shows that it declared 8 MiB. That evidence record now
contains an explicit correction rather than carrying the contradiction forward.

## Exact package identities

| Selector | Canonical board ID | Cargo feature | Flash bytes | Capability digest | Fixture |
| --- | --- | --- | ---: | --- | --- |
| `mks-tinybee` / `mks-tinybee-8mb` | `mks-tinybee-v1` | `board-mks-tinybee` | 8,388,608 | `c4e2345314b33276263aba379a02249ec81a1ad6da00c108c00cc9837c31da0c` | connected V1.0 PCB reports 8 MB |
| `mks-tinybee-4mb` | `mks-tinybee-v1-4mb` | `board-mks-tinybee-4mb` | 4,194,304 | `179360f544c215fcc792734482d5d920a11e262326c553093e53fa55addf9ee6` | unavailable |

The canonical documents share the established PCB routing, buses, ownership,
safe-image, and HIL requirements. Their IDs, revision descriptions, memory
capacities, and SHA-256 capability identities differ. `xtask` validates each
metadata file against the corresponding Rust package. The firmware build script
accepts exactly one board feature and embeds that exact ID; the selected TinyBee
hardware module exports only its matching package. There is no flash-capacity
probe, runtime fallback, aliasing of capability identities, or compatibility
shim.

Every successful `xtask build` copies Cargo's conventional ELF to a
board-qualified filename. This prevents a later build from leaving only an
ambiguously named artifact:

```text
target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1
target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb
```

## Current linked and flash-image fit

The release ELFs have these identities:

| Package | ELF SHA-256 | text | data | BSS |
| --- | --- | ---: | ---: | ---: |
| 8 MiB primary | `0a9887909648117eb5ba6c80f1377145c0bc3797bdce695d8dc807f551ca2fe8` | 913,144 | 12,072 | 250,064 |
| 4 MiB variant | `167a9e7b04ab9e72fced5e4bbe7883da56a8c956558fa390b7421f50c8967492` | 913,164 | 12,072 | 250,064 |

`espflash 4.3.0 save-image` was run with explicit `--flash-size`, `--merge`,
`--skip-padding`, and `--skip-update-check` into a temporary directory. It
reported:

```text
8 MiB: App/part. size 925,328 / 8,323,072 bytes (11.12%)
4 MiB: App/part. size 925,344 / 4,128,768 bytes (22.41%)
```

The unpadded merged binaries were respectively 990,864 and 990,880 bytes. This
proves that the present firmware can be encoded for each declared capacity. It
does not establish a final Alumina partition table, dual update slots, embedded
web/WASM asset budget, rollback space, crash-log allocation, or future fit. The
4 MiB build remains opportunistic: later required content must either fit its
separately reviewed budget or the variant must stay at a lower support level.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee
cargo xtask board check mks-tinybee-4mb
cargo xtask capabilities --board mks-tinybee --json
cargo xtask capabilities --board mks-tinybee-4mb --json
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee-4mb \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

Both board packages and capability identities validate, both strict target
Clippy checks pass, and both release targets link. The full portable workspace
has 316 passing tests. T-Deck Pro and MKS ESP32 FOC V1.0 also link after the
feature-selection change, and `xtask` preserves a board-qualified ELF for every
successful build.

## Hardware and licensing boundary

The connected 8 MiB TinyBee was not read, flashed, reset, or otherwise touched
for this checkpoint. No motor or motor power was connected. No 4 MiB fixture is
available, so that variant has compile/encoding evidence only. The SLogic16U3
remains disconnected; flash capacity work requires no waveform capture.

All changes are independently authored under `MIT OR Apache-2.0` and add no
external dependency package. The board crate gains a dev-only edge to the
already locked, repository-owned `alumina-capability` crate so both compiled
digests are independently recomputed in tests. No GPL-family source, dependency,
or asset was introduced or consulted. The all-feature Cargo tree contains 851
nonempty package/license records, zero missing license expressions, and zero
GPL/AGPL/LGPL/SSPL-family expressions. A Rust/C/C++/header/Cargo-manifest scan
outside documentation likewise has no GPL-family match. `cargo-deny` remains
configured in CI but is not installed locally, so no local `cargo deny` result
is claimed.
