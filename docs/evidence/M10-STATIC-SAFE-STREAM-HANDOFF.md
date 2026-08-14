# M10 static-safe stream handoff — offline evidence

Date: 2026-08-14

Status: implemented portable ownership and isolated target-HIL checkpoint. A
fixed-memory owner now distinguishes static safe establishment, complete DMA
prefill, target start, independently observed physical stream phase, stop,
software safe rewrite, and independently observed safe reclaim. The TinyBee
safe-image harness uses the software-only subset, but no production target can
select this transport. This is not TinyBee waveform, deadline, WCET, Wi-Fi-load,
motor, machine-accuracy, or armability evidence.

## Result and moving-source isolation

Firmware commit `c722f11a5abcd2a9b4c48dfe1c4d5e3e6e573fe6` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and all 35 manifest paths are beneath
the `aluminafw` repository. Neither `Cargo.toml` nor `Cargo.lock` changed.
Hypercurve remains an intentionally moving sibling worktree; no transient
Hypercurve state is frozen or fingerprinted here, and Alumina did not edit,
format, reset, pin, or otherwise constrain it.

## Fixed-memory ownership lifecycle

`PcmShortDmaStreamOwner<TAG, UPDATES, FRAMES>` encloses the earlier exact dense
`PcmShortDmaHorizon` and retains one first-cause fault. Construction proves only
that a separately written complete image has the PCM-short contract. Ring
preparation then requires all of these facts before ownership can advance:

- the prefill is the byte-identical complete safe image;
- its reported frame count equals the compile-time physical ring shape;
- static establishment, prefill completion, and grid epoch are monotonically
  ordered; and
- the dense horizon can represent the complete prefetched ring.

The target start operation is represented by a closed device-cycle interval.
Its hypothesized grid epoch must fall inside that interval. Because the record
method runs after the target call, any bad state, prior fault, malformed time
window, or phase mismatch moves the owner to `StartUncertain`: it cannot claim
the peripheral stayed stopped and continues to require an explicit stop.

A successful start return advances only to `StartIssued`. In that state the
owner may refill released slots, but sparse staging remains closed, so every
possible generated frame retains the safe image. Motion authority appears only
when an independent observation reports that same complete image at exact grid
boundary one. The owner then advances to `StreamObserved`; later physical
observations and tag commits remain distinct from descriptor availability.

Stop immediately invalidates the dense horizon, pending frames, and all target
tags, including when an earlier failure is retained. A successful two-sample
software rewrite advances only to `SafeRewriteIssued`. `PeripheralSafe`
requires a separate complete-safe-image observation no earlier than the rewrite
call's return. A physical safe observation may establish safe reclaim after an
uncertain stop/rewrite, but it never erases the original fault or restores
motion authority.

Host regressions cover the complete successful lifecycle and adversarial safe
image, ring-count, epoch, first-latch, premature-motion, failed-start, and
observation-order cases. They also prove that recovery remains available after
a fault while first cause survives it.

## Independent bit-level replay

The existing circular-DMA/stepper integration now enters through the lifecycle
owner. Four exact safe frames populate the modeled physical ring. The
independent wire observer reconstructs all 64 serialized bits of frame zero and
reports the complete safe image at cycle 100. Only that observation changes the
owner from `StartIssued` to `StreamObserved` and permits the real scheduled
stepper outputs to be staged.

The slot released by that first latch is then filled with transmit index four,
whose image commits at cycle 116. Subsequent descriptor credits, dense frame
acceptance, bit-level latches, target tokens, final disable, and block release
continue through the pre-existing exact ownership path. Thus neither a HAL
return nor a descriptor completion substitutes for the physical-latch gate in
the simulation.

## TinyBee safe-capture adoption

The disconnected-load-only
`alumina-hil-mks-tinybee-pcm-short-safe` image now creates the same portable
owner after its complete static GPIO transaction, exact safe ring prefill, and
synchronous HAL start call. The model epoch is the device-cycle reading taken
immediately before that call and must fall between its before/after readings.

The live loop never supplies a physical latch observation. It therefore stays
in `StartIssued`, where it can issue only safe refills; the target-facing staging
method would reject a motion token. After the circular guard stops, the harness
brackets the blocking two-sample rewrite and records it. A successful software
run must report:

```text
owner_state=SafeRewriteIssued owner_fault=None safe_reclaimed=false
```

The false reclaim flag is intentional. Software cannot manufacture the
post-stop analyzer observation. Start, stop, rewrite, owner state, first fault,
and reclaim status are logged only after I2S activity ends. The existing marker
and VCD contract remains the authority for a future disconnected physical run.

Production firmware still establishes its static writer and rejects every
stream operation. `MOTION_OUTPUT_QUALIFIED` remains false for TinyBee and
T-Deck Pro, and the HIL binary is a separate feature-gated composition root.

## Verification record

The following completed offline against firmware commit
`c722f11a5abcd2a9b4c48dfe1c4d5e3e6e573fe6`:

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
```

Observed results:

- all 432 portable default-member tests passed, including 15 shift-register,
  50 motion, and 40 simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for classic ESP32 TinyBee production, ESP32-S3
  T-Deck Pro production, and the classic ESP32 TinyBee HIL image;
- all four optimized images linked successfully;
- the primary 8 MiB TinyBee production image has 1,068,100 bytes text, 12,224
  bytes data, and 249,920 bytes BSS, with SHA-256
  `955fe2d9527a46d7944ec945f10f21114703488a60b0fdbde4a2869808e2762a`;
- the opportunistic 4 MiB TinyBee production image has 1,068,140 bytes text,
  12,224 bytes data, and 249,920 bytes BSS, with SHA-256
  `baaad8370f8de82d2676a5625f19585a42bae32fb3afa524fb068659acac17c2`;
- the T-Deck Pro production image has 1,002,533 bytes text, 12,976 bytes data,
  and 525,392 bytes BSS, with SHA-256
  `fced1674beacddedf5ca371c223da51d8d08a91b39b003a887250ce96cf06845`;
  and
- the separate TinyBee HIL image has 68,772 bytes text, 3,120 bytes data, and
  193,488 bytes BSS, with SHA-256
  `e13a7e839a5f77f69a45f0ee593f18fd80c310ed65b001921e670358467c1f14`.

The 8 MiB TinyBee and T-Deck Pro linked production section sizes are unchanged
from the preceding checkpoint; the new owner is not reachable from either
production composition. The reported sizes are static sections, not runtime
stack/heap watermarks, DMA bandwidth, WCET, or physical timing evidence.

No dependency changed, so the existing MIT/Apache-compatible license inventory
is unchanged. A scan of added Rust lines found no GPL/AGPL/LGPL/SSPL identifier
or copied FluidNC, Klipper, Synthetos/g2core, or SimpleFOC implementation
reference. The configured CI license policy remains required.

## Closed claims and next boundary

This checkpoint closes the portable static-safe/prefill/start/observe/stream/
stop/rewrite/reclaim state machine, observed-safe gating in bit-level replay,
and compile-only adoption by the TinyBee safe-image HIL target. It does not
provide a production DMA refill actor, target physical-latch observation
source, measured ring phase, qualified stop/reclaim, deadline margin under
service load, or arm authority.

The next target boundary is a disconnected analyzer capture that qualifies—or
rejects—the hypothesized TinyBee PCM-short framing and post-stop safe image.
Until that evidence exists, production keeps the static writer, both target
adapters reject streaming, and both boards remain non-armable for this path.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
