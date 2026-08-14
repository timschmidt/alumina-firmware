# M10 refill wake/deadline supervisor — offline evidence

Date: 2026-08-14

Status: implemented portable supervisor and independent simulation checkpoint.
The PCM-short stream owner now has an allocation-free, exact-cycle supervisor
around its bounded refill transaction. No production or HIL target selects the
supervisor. This is not a target interrupt, WCET, DMA-prefetch, waveform,
physical-latch, motion-output, or armability result.

## Result and moving-source isolation

Firmware commit `6289870da9039881af7f836c2680654c2f41d6ff` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and every manifest path is beneath the
`aluminafw` repository. Neither a manifest nor `Cargo.lock` changed. Hypercurve
is an intentionally moving sibling worktree; this work did not read a transient
Hypercurve result into firmware evidence or edit, format, reset, pin, or
otherwise constrain that worktree.

## Fixed exact policy

`PcmShortDmaRefillPolicy` binds three immutable values to one actor:

- a nonzero maximum number of complete frames serviced per turn;
- a nonzero qualified maximum device-cycle duration for one complete target
  push; and
- a nonzero minimum completion lead before the modeled frame start.

The exact begin reserve is

```text
begin_reserve = maximum_push_cycles + minimum_complete_lead_cycles
```

with checked integer arithmetic. `PcmShortDmaRefillActor<FRAMES>` additionally
rejects a per-turn frame budget larger than `FRAMES`, a push duration larger
than one exact frame period, and a begin reserve larger than the initial ring
lead. The actor is bound to one `PcmShortFrameGrid`; an owner carrying any other
grid faults closed before target activity.

For each exact planned frame, the actor derives without rounding:

```text
latest_complete = frame.starts_at - minimum_complete_lead_cycles
latest_begin    = frame.starts_at - begin_reserve
```

The target closure receives the complete four-byte frame and both absolute
deadlines. It returns a before/after device-cycle bracket and an explicit
complete-acceptance result. The actor rejects, before model acceptance:

- a wake already past `latest_begin`;
- a target begin before the wake or prior target return;
- a reversed target interval;
- incomplete or uncertain four-byte acceptance;
- a begin after `latest_begin`;
- a target interval longer than the fixed push budget; or
- a return after `latest_complete`.

The existing bounded preview/push/accept transaction remains the only path that
extends the portable sealed horizon. Target or model failure retains the exact
accepted prefix and phase. The actor invalidates the stream owner, retains its
own first cause, and leaves the established ordered stop, safe rewrite, and
independent reclaim path available without clearing the owner first cause.

## Wake and fallback contract

A successful turn reports the exact target availability, accepted frames,
retained credit, sealed horizon, and last target return. If credit remains, or
the turn finishes exactly at the next latest-begin boundary, the result is
`ReserviceNow`; the caller may not await. With zero retained credit before that
boundary, the result is `TargetReleaseOrFallback` with an absolute device cycle.
An earlier release interrupt may service the actor. Reaching the fallback with
no released slot faults as `NoReleasedSlotAtFallback`, and finishing after the
next window faults as `NextWindowElapsed`.

Wake observations and target returns are monotonic. The actor also retains
cumulative attempted turns and complete accepted frames with checked counters.
After its first fault, later service attempts return `FaultLatched` and cannot
restore stream authority.

This contract deliberately does not invent a target timing value. A future
adapter must identify and qualify the descriptor-release interrupt, exact cycle
counter, complete-frame write operation, worst-case service behavior under
load, and completion lead including DMA descriptor/FIFO prefetch. The portable
actor cannot establish those facts, and neither TinyBee production nor the
safe-image HIL fixture supplies them in this checkpoint.

## Adversarial and independent simulation

Nine new shift-register regressions cover exact policy rejection and ring
bounds; two-frame turn limits with retained-credit immediate reservice; the
zero-credit interrupt/fallback result; missing release at fallback; preexisting
owner-fault retention; model-availability first-cause retention; grid and wake
substitution; a partial target rejection followed by successful ordered
stop/rewrite/reclaim; late wake rejection before target activity; and every
target-window failure class.

Two simulator regressions exercise a separately modeled bit-level wire:

- a four-frame ring wakes one cycle after each physical latch, pushes one frame
  in two cycles with one cycle of required completion lead, runs 12 supervised
  turns continuously, and commits a staged image at exact cycle 150 without an
  actor, owner, or wire fault; and
- a wake at cycle 138 misses the exact cycle-137 latest begin for the frame
  starting at 140. The actor faults before invoking the target. The independent
  wire then drains the remaining initial ring and reports starvation at latch
  cycle 150, demonstrating that the software fault preceded the physical model
  failure rather than hiding it.

These synthetic timing values test the contract; they are not TinyBee timing
measurements or target qualifications.

## Verification record

The following completed offline against the contents committed as
`6289870da9039881af7f836c2680654c2f41d6ff`:

```sh
cargo fmt --all -- --check
cargo test --locked --offline
cargo test --locked --offline -- --list
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware \
  --bin alumina-hil-mks-tinybee-pcm-short-safe \
  --no-default-features --features hil-mks-tinybee-pcm-short-safe \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings

cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask hil build mks-tinybee-pcm-short-safe

llvm-size \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe
sha256sum \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe
llvm-nm -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro | \
  rg 'PcmShortDmaRefillActor|PcmShortDmaRefillPolicy|PcmShortDmaRefillPushRequest|PcmShortDmaRefillNextWake|refill_batch_with'
strings target/xtensa-esp32-none-elf/release/\
alumina-hil-mks-tinybee-pcm-short-safe | \
  rg 'HIL_PCM_ATTEST_V2|HIL_PCM_STOPPED|HIL_RESULT'
```

Observed results:

- all 452 portable default-member tests passed, including 29 xtask, 28
  shift-register, 50 motion, and 42 simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for classic ESP32 TinyBee production, ESP32-S3
  T-Deck Pro production, and the classic ESP32 TinyBee HIL image;
- all four optimized images linked successfully;
- the primary 8 MiB TinyBee production image has 1,068,100 bytes text, 12,224
  bytes data, and 249,920 bytes BSS, with SHA-256
  `51ef4564b1494525542a941b5f3b197aed9ef0f94ba16e572f7c9f3a1e641884`;
- the opportunistic 4 MiB TinyBee production image has 1,068,140 bytes text,
  12,224 bytes data, and 249,920 bytes BSS, with SHA-256
  `007f2801f4bf8a56e6ab1ba6c284bcbf871d69a3061bf5077f22c72cb11cf919`;
- the T-Deck Pro production image has 1,002,533 bytes text, 12,976 bytes data,
  and 525,392 bytes BSS, with unchanged SHA-256
  `fced1674beacddedf5ca371c223da51d8d08a91b39b003a887250ce96cf06845`;
  and
- the separate TinyBee HIL image has 69,692 bytes text, 3,120 bytes data, and
  193,488 bytes BSS, with SHA-256
  `0a9367c8d95d4448622105f00b18829a44c2ffe32a11c38f5d2330a7f5de8413`.

All four section-size triples are unchanged from the bounded-refill checkpoint.
The T-Deck Pro ELF is byte-identical. The two TinyBee production ELFs and the
separate HIL ELF have changed whole-file digests while linking the modified
shift-register crate; equal linked sections do not justify a stronger behavior
or byte-identity claim. These static section measurements are not runtime
stack/heap watermarks, DMA bandwidth, service WCET, or physical timing evidence.

The production-symbol query returned no match for the supervisor, its policy,
request/wake types, or the portable batch method. This supports optimized build
unreachability, not physical safety or armability. The isolated HIL ELF still
contains the stable `HIL_PCM_ATTEST_V2`, `HIL_PCM_STOPPED`, and success/failure
`HIL_RESULT` records; it still uses the earlier untimed bounded batch, not this
supervisor.

No dependency, manifest, or lockfile changed, so the existing
MIT/Apache-compatible dependency inventory is unchanged. An added-Rust scan
found no GPL/AGPL/LGPL/SSPL identifier or copied FluidNC, Klipper,
Synthetos/g2core, or SimpleFOC implementation reference. The local environment
does not have `cargo-deny` installed, so no ad hoc installation was attempted;
the configured CI license policy remains required.

## Closed claims and next boundary

This checkpoint closes the portable allocation-free scheduling policy,
deadline calculation, target-call observation contract, exact fallback result,
first-cause retention, and timely-versus-delayed independent simulation. It
does not provide a target interrupt source, permanent production core-1 actor,
qualified cycle/WCET/prefetch values, physical latch observation, safe
stop/reclaim evidence, motion stream authority, or an armable board path.

The next physical boundary remains the documented disconnected SLogic16U3
capture. Offline development may continue on target-independent observation,
configuration, simulation, and protocol surfaces while the board and network
constraints remain unchanged. Hypercurve may continue moving independently;
firmware work must retain the same no-path-dependency isolation, while future
browser/WASM CAM work should integrate the current sibling Hyper stack only in
an explicitly isolated interface checkpoint.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
