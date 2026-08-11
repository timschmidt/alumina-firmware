# M6 scheduled output-horizon evidence

Date: 2026-08-11

Status: portable exact motion-to-PCM ownership model and deterministic host
simulation. This is not an ESP32 I²S/DMA adapter, physical latch detector,
TinyBee timing claim, safe static/stream handoff, armability claim, or machine
qualification.

## Proven software boundary

`StepperTiming` now carries a nonzero `output_quantum_cycles`. Job epochs and
absolute segment boundaries must be exactly divisible by that quantum. Centered
step interpolation occurs in integer output quanta and is multiplied back with
checked arithmetic, so the backend never receives an edge it cannot represent.
For quantum `q`, the reported conservative interpolation error is `q` half-
device-cycle units, or `q/2` device cycles. Normal enable-hold completion rounds
up to the next exact output boundary; requested terminal commits outside the
grid reject.

`ScheduledShiftedStepper<AXES, OUTPUTS>` adds a fixed-capacity, allocation-free
future-image owner. Its state transitions are deliberately distinct:

1. the cached exact executor generates a complete future image and token;
2. the sole hardware timeline accepts that exact image in order; and
3. a target observation retires the oldest staged token at or after its
   scheduled latch, within the configured lateness bound.

A zero-capacity ring rejects construction. A full ring preserves the next
logical event and returns `HorizonFull`; committing before staging, staging a
different image, reordering tokens, observing an early latch, or exceeding the
lateness bound latches a fault. Faulting invalidates all future images and
retains the unique admitted block as unacknowledgeable work until the caller
requests the complete safe image.

Logical segment completion alone is insufficient to return a block. The ring
must be empty and the caller must have observed the exact terminal cycle. This
second condition covers a final dwell with no output transition. Normal driver
disable is then generated, staged, and physically committed through the same
token path; only its commit produces the one-shot job-complete fact.

## Independent end-to-end simulation

The new `alumina-sim` integration uses a modeled 1 MHz `DeviceCycle` domain and
a 250 kHz PCM-short frame rate. That creates an exact four-cycle output quantum
and a 16 MHz, 64-bit modeled frame. It admits one real canonical motion block,
generates five complete images at cycles 100, 112, 116, 132, and 136, schedules
each into `PcmShortTimeline` before its one-frame lead deadline, and marks it
staged only after that acceptance succeeds.

`SimPcmShortLatch` then consumes every dense frame one serial bit at a time. The
motion token is committed only when the independently reconstructed image is
visible at the matching latch. After the last image commits at 136, block
release at observed cycle 139 is still refused because the segment terminal is
140. One unchanged dense frame proves that boundary; only then does the job
actor recover and acknowledge the unique block. The exact eight-cycle enable
hold moves normal disable to cycle 144, which requires another scheduled frame,
wire reconstruction, and physical commit before job completion.

These numbers are deterministic fixture parameters. They neither select nor
qualify a TinyBee clock.

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
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
git diff --check
```

The focused `alumina-motion` suite passes 24 tests, `alumina-sim` passes 20,
and the default workspace passes 226 unit tests. Strict all-target host Clippy
and strict Clippy plus optimized release linking for both ESP targets pass. The
only dependency metadata change gives simulator tests direct access to two
repository-owned crates; there is no new third-party package. `llvm-size`
reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 865,340 | 11,976 | 250,160 | 35,420 |
| T-Deck Pro | 804,369 | 12,728 | 525,632 | 142,324 |

These are linked capacities, not runtime stack watermarks or timing evidence.

## Licensing and closed claims

All new implementation and documentation is independently authored under the
repository's `MIT OR Apache-2.0` policy. No GPL, LGPL, AGPL, SSPL, FluidNC,
g2core, Klipper, or SimpleFOC implementation source was used. Existing
dependency/license inventory and forbidden-license/source scans remain release
gates.

The sorted default, all-feature workspace, TinyBee, and T-Deck Pro dependency
inventories contain 65, 323, 234, and 241 nonempty package/license records.
None has a missing license or GPL/AGPL/LGPL/SSPL-family license, and the
implementation-source/header/manifest license-header scan is clear. The local
environment does not have `cargo-deny` installed; CI remains configured to run
its bans, licenses, and sources checks.

Firmware now instantiates `ScheduledShiftedStepper` and exposes the structural
target calls needed to query writable capacity, stage exact sparse images, seal
every changed and unchanged frame through a continuous hardware horizon, and
consume physical commit observations. A full sparse ring or a first block that
ends before the required prime boundary cannot satisfy that seal. Both selected
board implementations reject these calls. TinyBee's
`MOTION_OUTPUT_QUANTUM_CYCLES = 1` is still an inert placeholder, not a hardware
fact. A compile-only HAL module can compose original-ESP32
PCM-short TX and a safe-prefilled internal-SRAM circular buffer, but no boot,
arm, or motion path can reach it. A target adapter must still establish a common
cycle/frame epoch, prove complete dense-frame materialization through the
reported horizon, correlate a completion source to the physical WS/latch edge,
make static/stream handoff, starvation, and stop fail safe, and demonstrate the
phase on a logic analyzer with motor/process loads disconnected. Until that
evidence exists, `MOTION_OUTPUT_QUALIFIED` and board armability stay false.
