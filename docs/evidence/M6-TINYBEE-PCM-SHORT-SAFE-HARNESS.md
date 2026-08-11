# M6 TinyBee PCM-short safe capture harness evidence

Date: 2026-08-11

Status: an isolated, release-only, build-only TinyBee safe-image capture artifact
and a reproducible operator procedure are implemented. No board was connected,
flashed, energized, or captured for this checkpoint. This is not waveform,
electrical, phase, refill-margin, safe-stop, armability, or machine evidence.

## Isolated artifact

`alumina-hil-mks-tinybee-pcm-short-safe` is a separate Cargo binary behind the
explicit `hil-mks-tinybee-pcm-short-safe` feature. Production firmware emits a
targeted compile error if that HIL feature is accidentally selected for the
production binary. Conversely, ordinary TinyBee and T-Deck builds do not compile
the HIL binary.

The artifact reuses the production TinyBee singleton split but follows a smaller
authority path:

1. initialize the original ESP32 at maximum CPU clock;
2. synchronously establish the complete static `0x001249` disabled/off image;
3. retain Wi-Fi, storage, UART, second-core, and interrupt tokens without
   initializing their services;
4. start the Embassy time driver solely to record bounded local-cycle facts;
5. configure the compile-only I²S0/DMA owner with 256 safe-prefilled four-byte
   descriptors;
6. bracket the circular-start HAL call with two local cycle readings;
7. reconcile released descriptors and preview/push/accept only dense copies of
   the same safe image through `PcmShortDmaHorizon`, with no RTT logging or await
   point while the transfer is live;
8. stop after at least 50,000 accepted refills or fail closed at two seconds;
9. explicitly stop the transfer, write two further safe samples, report the
   software outcome, and park without starting any other service.

No reachable HIL control path constructs a sparse motion update or non-safe
image. A DMA/owner/model error exits the refill loop, stops the transfer,
attempts the safe rewrite, and reports failure. The local epoch logged before
start remains labeled a hypothesis; it is never converted into a physical
commit observation.

## Build and operator boundary

`cargo xtask hil list` identifies the fixture as build-only and
disconnected-load. `cargo xtask hil build mks-tinybee-pcm-short-safe` always
selects the release profile, exact original-ESP32 target, sole HIL feature, and
named HIL binary. It invokes `cargo build`, never `cargo run`, a Cargo runner, or
`espflash`, and prints the resulting ELF path.

The physical sequence is intentionally manual. [`../HIL.md`](../HIL.md) requires
meter-verified disconnection of motor and process power before flashing, records
the GPIO25/GPIO26/GPIO27 analyzer mapping, distinguishes the 10 MHz DSO from a
logic analyzer capable of resolving nominal 16 MHz BCLK, lists every startup,
steady-state, wrap, stop, and safe-image fact to decode, and defines the archived
run record. A log containing `HIL_RESULT capture complete` is only evidence that
software calls returned success; it cannot change any qualification field.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware \
  --bin alumina-hil-mks-tinybee-pcm-short-safe \
  --no-default-features --features hil-mks-tinybee-pcm-short-safe \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask hil list
cargo xtask hil build mks-tinybee-pcm-short-safe
llvm-size \
  target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32-none-elf \
  --no-default-features --features hil-mks-tinybee-pcm-short-safe \
  --locked --offline --prefix none --format '{p}|{l}'
git diff --check
```

The HIL target passes strict Clippy and optimized release linking. `llvm-size`
reports 62,140 text bytes, 3,120 data bytes, and 193,488 aggregate BSS bytes. The
linker-residual `.stack` region is 183,972 bytes; these are linked capacities,
not a measured runtime watermark. The default workspace remains at 232 passing
unit tests. Strict all-target host Clippy and strict ordinary TinyBee/T-Deck Pro
target Clippy also pass; the fixture does not alter either production image's
feature selection or armability.

## Licensing and source boundary

The harness, build routing, and documentation are independently authored under
`MIT OR Apache-2.0` and add no dependency. They reuse only repository-owned code
and the already locked `MIT OR Apache-2.0` `esp-hal` API. GPL, LGPL, AGPL, SSPL,
copied implementation code, and assets from those license families remain
excluded. No GPL-family source was fetched, cloned, vendored, or consulted for
this checkpoint.

Sorted, deduplicated offline cargo-tree inventories for the default workspace,
all workspace features, ordinary TinyBee, the TinyBee HIL feature, and T-Deck Pro
contain 65, 323, 234, 234, and 241 nonempty package/license records. None has a
missing license or a GPL/AGPL/LGPL/SSPL-family license. The implementation and
import-tree Rust/C/header/Cargo-manifest scan is also clear. `cargo-deny` is not
installed locally; CI remains the authority for its configured license, ban, and
source checks.
