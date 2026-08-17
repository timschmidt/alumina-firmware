# M10 scheduled direct motion-to-PCM ownership — offline evidence

Date: 2026-08-14

Status: implemented portable checkpoint. Direct Q31.32 finite-difference
records now feed an allocation-free future complete-image owner, explicit
same-cycle composition boundary, independent cached-block commit barriers,
terminal pulse-tail drain, dense PCM-short horizon, and bit-level physical-latch
simulation. This is not an ESP peripheral adapter, target WCET result, Wi-Fi
load result, physical timing measurement, motor test, or armability claim.

## Result and source isolation

Firmware commit `5a6b45e6234a520a9bf8b6afa8f0818d1fa5bfe2` implements and documents this
boundary. It changes only repository-owned Rust and Markdown and adds no
dependency or compatibility path. Alumina Interface remains unchanged at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This slice deliberately has no Hyper/CSGRS input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, all with manifests below the
`alumina-firmware` repository, and every declared dependency path is repository-local.
Consequently continued Hypercurve editing cannot change these firmware build
results. During this work Hypercurve remained at HEAD
`3ef8689ff2c33ad9fd0c9eb7fbdf9fa015fc395c`, while its independent tracked
edits grew from `src/bezier_offset.rs` to include `src/policy.rs` and then
`src/bezier_region.rs`; the final binary-diff SHA-256 observed was
`2fa569afeb5e38dffad2539236bad67e644667c913f2c093ea0e8ca605e64b55`.
Alumina did not read that moving source as a build input, edit it, format it,
reset it, pin it, or claim a test run against it.

## Exact scheduled-image contract

`ScheduledShiftedFiniteDifferenceStepper<AXES, OUTPUTS>` composes the existing
cached direct executor and complete shift-image mapper without allocation. It
keeps logical generation, hardware-timeline acceptance, and physical latch
observation as three separate monotonic transitions.

Dense recurrence updates remain exact `DeviceCycle` deadlines, including
updates that do not cross an integer step. Only a logical direction, enable,
rise, or fall change consumes a sparse image slot. When multiple logical polls
occur at the same cycle, the owner retains one token and replaces its complete
future image with the cumulative image. It never asks a hardware timeline to
accept two different images at one latch.

The newest same-cycle image is an explicitly unsealed ring tail. It cannot be
returned by `next_unstaged_output` until the executor proves there are no more
logical changes at that cycle. A cached-block completion intentionally leaves
the cycle open. The caller then makes exactly one choice:

1. admit the contiguous successor before planning again, allowing its boundary
   direction/enable event to compose at the same latch; or
2. plan again without a successor, irrevocably close continuation, seal the
   boundary, and drain every executor-owned pulse fall.

A successor offered after terminal-tail planning is rejected unchanged. Thus
the physical image cannot depend on whether service code happened to stage a
prefix before deciding how the stream continues.

At each block horizon the owner records `committed_updates + queued_outputs`.
That fixed prefix and the exact numerical terminal cycle form the block's
physical-return barrier. A fall after the horizon belongs to the executor and
does not become ambiguously owned by the old or new block. The earlier token may
return independently once its prefix is observed even while later-block or
tail images remain queued. Final disable remains illegal until the owner tail
is complete and the exact enable-hold deadline is met.

The fixed ring stops before polling a new recurrence when no output slot is
available, preserving the next deadline. A staging mismatch, commit before
stage, wrong/reordered token, early latch, excessive commit lateness, image
mapping error, or arithmetic failure latches the entire window. No retained
block becomes acknowledgeable until the caller requests the complete safe
image.

## Independent PCM/wire replay

The new `alumina-sim::shift_register` integration admits two real finite-
difference `ALMBLK02` blocks through `RealtimeJob`. Its 1 MHz device counter and
250 kHz PCM-short frame rate define an exact four-cycle lattice. The first
block starts at cycle 116 and completes at 128 with a pulse still high. The
successor retains that pulse, emits its fall at 132, completes at 144 with its
own pulse high, and leaves the final executor-owned fall at 148. Exact
enable-hold moves terminal disable to 156.

The only sparse complete-image cycles are therefore:

```text
116, 128, 132, 144, 148, 156
```

Recurrence-only updates allocate no image. `PcmShortDmaHorizon` expands this
plan into a continuous safe-prefilled circular frame stream. An independent
observer consumes all 64 modeled serial bits per frame and reconstructs the
visible 24-bit cascade at the following latch. Only the matching reconstructed
latch retires a motion token. The first block returns at 128, the second at
144, the cross-block/final falls remain visible at 132/148, and job completion
appears only after the disable image is physically observed at 156.

Focused production regressions additionally prove:

- a record-ending rise and next-record direction/enable boundary compose into
  one token at one cycle;
- an open boundary image cannot be staged prematurely;
- two cached blocks retain independent physical prefixes across a pulse fall;
- choosing terminal-tail planning rejects a later successor without live
  mutation;
- zero ring capacity and a full ring fail closed without consuming the next
  recurrence;
- final disable rejects before tail closure; and
- mutated staging plus wrong physical tokens latch faults and preserve an
  unacknowledgeable block until the safe image is requested.

## Verification record

The following completed offline against the committed firmware source:

```sh
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
```

Observed results:

- all 423 portable default-member tests passed, including 45 motion and 40
  simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for classic ESP32 TinyBee and ESP32-S3 T-Deck
  Pro;
- both optimized target images linked successfully;
- the TinyBee image has 1,053,124 bytes text, 12,224 bytes data, and 249,920
  bytes BSS, with SHA-256
  `1f4ad27dd8a83dc71fbf37c04ff82fc6e83d71251e6da0c1e0360e917b872d74`;
  and
- the T-Deck Pro image has 989,073 bytes text, 12,976 bytes data, and 525,392
  bytes BSS, with SHA-256
  `62a46f417476c7eba4c066e2492ea5d1b924f81c174cd3fa772ca2a1281a5cce`.

The sizes are linked static sections, not runtime stack/heap watermarks, DMA
bandwidth measurements, or timing evidence. The direct scheduled type is not
yet selected by the target motion actor, so the target builds prove portable
compatibility only.

No manifest or lockfile changed. The default dependency-license inventory
contains only repository `MIT OR Apache-2.0` packages and compatible MIT,
Apache-2.0, BSD-3-Clause, or Unlicense/MIT dependencies. Source scans found no
GPL/AGPL/LGPL/SSPL identifier or copied FluidNC, Klipper, Synthetos/g2core, or
SimpleFOC implementation reference in the changed Rust. `cargo-deny` is not
installed locally, so no local `cargo deny` result is claimed; the configured
CI policy remains required.

## Closed claims and next boundary

This checkpoint closes portable direct recurrence-to-complete-image ownership
and independent PCM/wire simulation. It does not connect that owner to the
TinyBee PCM-short peripheral, select direct execution in the permanent target
motion actor, establish static-safe-to-stream handoff, prove descriptor lead or
physical latch phase, measure deadline/WCET margin under Wi-Fi load, or grant
arm authority. T-Deck Pro still has no motion backend. Both boards remain
non-armable.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. Physical work remains deferred.
