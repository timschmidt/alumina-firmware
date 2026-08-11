# M6 TinyBee PCM-short serializer model evidence

Date: 2026-08-11

Status: portable complete-image frame planning and independent bit-level host
simulation. This is not an ESP32 I²S/DMA backend, observed waveform, physical
commit detector, safe handoff, armability claim, or machine qualification.

## Research and provenance boundary

The functional model uses the MKS TinyBee schematic facts already recorded in
[`M3-SAFE-BOOT-OBSERVATION.md`](M3-SAFE-BOOT-OBSERVATION.md), Espressif's public
ESP32 I²S programming guide and technical reference manual, and the locally
locked `esp-hal` 1.0.0 crate source. Espressif documents standard mode as two
slots, one BCLK per serial data bit, and PCM-short WS as a one-BCLK pulse. The
selected HAL describes `Channels::MONO` as sending the same sample in both
channels and exposes 32-bit slots plus circular DMA.

No FluidNC, g2core, Klipper, SimpleFOC, or other copyleft implementation was
opened, copied, translated, or added. The new repository code is independently
authored under `MIT OR Apache-2.0`; its only new dependency edge is from the
repository's host simulator to the repository's own shift-register crate.

## Implemented software contract

`PcmShortMonoFrame` accepts only a complete 1–32-bit image. For a most-
significant-bit-first cascade it retains the logical image word; for a least-
significant-bit-first cascade it reverses exactly the defined low-width bits.
The target contract repeats that one sample across both 32-bit slots. Thus the
last `width` bits of each 64-BCLK software frame encode every physical output,
and the following rising WS/latch boundary has an explicit one-frame pipeline
relationship to that image. The little-endian DMA byte view is explicit, but it
is not treated as proof of original-ESP32 FIFO or wire ordering.

`PcmShortFrameGrid` rejects zero rates and every fractional relationship between
the device-cycle counter and frame rate. It exposes checked frame boundaries,
the required 64-times frame-rate BCLK, and the unique transmit frame immediately
before an aligned commit. A commit at the stream epoch is rejected because no
preceding transmit frame exists. The tested `1 MHz` device counter / `250 kHz`
frame example yields four device cycles per frame and a `16 MHz` BCLK; these are
model parameters, not selected board qualification values.

`PcmShortTimeline<N>` is an allocation-free fixed-capacity sparse-update ring.
It admits only a constant width/mask/order contract, strictly increasing future
commit cycles, exact grid alignment, and updates whose transmit frame has not
already been emitted. `next_frame` makes the update present exactly one frame
before its latch and repeats the complete current image through every other
frame. Capacity, ordering, alignment, contract, state, and checked-arithmetic
failures are explicit.

The separate `alumina-sim::shift_register` observer does not trust the planned
image alone. It checks dense indices and both boundary cycles, shifts each of
the 64 serial bits through an independently structured suffix accumulator,
reconstructs logical bit order, compares the result with metadata, and changes
the visible image only at the following latch cycle. A missing dense frame at
its exact boundary, wrong sequence, timing, or contract latches a fault; the
model retains its last proven image and accepts no later frame.

## Deterministic tests

The shift-register crate has eight tests. Four new cases cover duplicated
complete-image suffixes in both bit orders, exact grid/pipeline/overflow
rejection, sparse updates becoming visible one frame later without gaps, and
capacity/order/alignment/contract rejection. The simulator has four new tests
covering bit-level latch visibility, incorrect frame alignment, exact horizon
starvation, and least-significant-first reconstruction.

The default workspace has 220 passing unit tests after this checkpoint.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree --locked --offline --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32-none-elf \
  --no-default-features --features board-mks-tinybee --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32s3-none-elf \
  --no-default-features --features board-t-deck-pro --locked --offline \
  --prefix none --format '{p}|{l}'
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
git diff --check
```

The focused `alumina-shift-register` suite passes 8 tests and `alumina-sim`
passes 19. The default workspace passes 220 tests and strict all-target host
Clippy. Strict Clippy and optimized release linking pass for both ESP targets.
`llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 865,176 | 11,976 | 250,160 | 35,428 |
| T-Deck Pro | 804,129 | 12,728 | 525,632 | 142,332 |

These are linked-capacity observations, not runtime stack watermarks or timing
measurements. The portable APIs are not referenced by the target yet, so dead
code elimination leaves no new target behavior or authority.

Sorted default, all-feature workspace, TinyBee, and T-Deck Pro cargo-tree
inventories contain 65, 323, 234, and 241 unique nonempty package/license
records. None has a missing license or a GPL/AGPL/LGPL/SSPL-family license.
An implementation-source/header/manifest scan is clear. The remaining FluidNC
references in board source are explicit hardware-evidence URLs/comments, not a
dependency or imported implementation.

## Closed physical claims and next boundary

No board was connected, flashed, or energized. In particular, this checkpoint
does not establish original-ESP32 FIFO packing, DMA memory endianness, actual
MSB-first wire order, WS polarity/phase, the HAL's documented startup-clock
workaround, BCLK rate accuracy, GPIO-matrix transition behavior, circular
descriptor refill margin, underrun output, or whether a DMA/software completion
observation proves that a physical latch edge occurred. It also does not provide
a safe way to stop the continuous peripheral and reclaim pins for the static
safe transaction.

The next software boundary is a bounded lookahead producer that separates
future logical event generation from later physical latch acknowledgement. It
must fill a safe continuous horizon, retain block ownership until all scheduled
images are physically observed, and make starvation terminal. Only then should
a compile-only TinyBee circular-DMA adapter be added. Logic-analyzer capture
with disconnected loads, including startup, steady state, forced starvation,
stop/reclaim, and saturated Wi-Fi/SD load, remains mandatory before changing
`MOTION_OUTPUT_QUALIFIED` or `armable`.
