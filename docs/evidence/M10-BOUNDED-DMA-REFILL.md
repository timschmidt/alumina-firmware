# M10 bounded DMA refill — offline evidence

Date: 2026-08-14

Status: implemented portable transaction and isolated target-HIL checkpoint.
The PCM-short stream owner now services target release reports through a
fixed-memory, fixed-budget preview/push/accept transaction. The TinyBee
safe-image HIL target uses that boundary, but production streaming remains
unreachable and non-armable. This is not a hardware run, interrupt-latency or
WCET result, waveform qualification, motion path, or armability result.

## Result and moving-source isolation

Firmware commit `df23d7fa6753afda0cae69b4e86fa203190a3b4f` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and all 35 manifest paths are beneath
the `alumina-firmware` repository. Neither `Cargo.toml` nor `Cargo.lock` changed.
Hypercurve remains an intentionally moving sibling worktree; no transient
Hypercurve state is frozen or fingerprinted here, and Alumina did not edit,
format, reset, pin, or otherwise constrain it.

## Portable bounded transaction

`PcmShortDmaStreamOwner::refill_batch_with` accepts three inputs: the target's
current whole-frame availability report, a caller-selected maximum frame count,
and a target closure that returns true only after accepting one complete exact
four-byte frame. One call proceeds in this order:

1. validate and reconcile the availability report with retained credits;
2. preview the next exact dense frame without extending the sealed horizon;
3. ask the target to accept that named frame completely;
4. accept the same frame into the portable model; and
5. report exact accepted progress, remaining credit, and sealed horizon.

The loop admits no more than `maximum_frames`. Availability validation rejects
any report beyond compile-time `FRAMES`, and the loop ends when retained credit
is exhausted, so target work is bounded by both the caller and physical ring
shape. A zero budget still reconciles availability but invokes no target push.
The transaction allocates no memory and retains no target-specific error type.

Model failures carry the rejecting phase (`Availability`, `Preview`,
`Acceptance`, or `Finalize`), exact frames already accepted in this call, and
the underlying first-cause owner error. If the target closure returns false,
the error names the exact pending frame and accepted prefix. The stream owner is
immediately invalidated because it cannot distinguish no write from a partial
or otherwise uncertain write. Further refill authority stays closed, while the
existing ordered stop, two-frame safe rewrite, and independent reclaim path
remains available without erasing `ExternalFault`.

This transaction is suitable as the fixed work unit for a future core-1
interrupt/wake actor. It does not itself provide that actor, an interrupt
source, a service deadline, physical phase observation, or underrun margin.

## TinyBee safe-HIL adoption

The isolated `alumina-hil-mks-tinybee-pcm-short-safe` image now takes one target
availability sample per outer capture turn and passes it to the portable batch.
Its maximum is the exact remaining portion of `TARGET_REFILLS`, so the batch
cannot cross 50,000 accepted frames. The former nested per-frame hand loop is
gone. Each target turn is additionally limited by the HIL artifact's fixed
256-frame ring.

The existing stable exit codes and `HIL_PCM_ATTEST_V2` schema are unchanged.
Availability, preview, target-push, and acceptance/finalization failures map to
the pre-existing fail-closed capture exits. A target-push failure now also
invalidates the portable owner before stop/rewrite recovery. The success
horizon remains exactly:

```text
model_epoch + (256 + 50,000) * 4
```

The HIL image still remains in `StartIssued` throughout live traffic, never
supplies a physical first-latch observation, emits only the complete safe image,
and ends at software-only `SafeRewriteIssued`. It does not initialize Wi-Fi,
storage, motion, process commands, or the second core. Production continues to
use the blocking static writer and rejects streaming.

## Adversarial verification

Portable regressions prove:

- three released slots plus a two-frame budget pushes exactly indices 4 and 5,
  retains one credit, and seals through cycle 124;
- a zero budget calls no target closure and preserves both credit and horizon;
- a later exact frame can consume retained credit, and the same transaction can
  materialize a staged motion image only after observed-safe authority exists;
- target rejection after one accepted frame returns that exact partial prefix
  and pending frame, latches `ExternalFault`, rejects further refills, and still
  permits ordered stop/rewrite/reclaim without clearing first cause;
- a shrinking availability report is classified at `Availability` before any
  push and retains the exact underlying ring-credit failure; and
- near `u64::MAX`, one accepted frame followed by preview arithmetic overflow
  reports `accepted_frames=1`, classifies the error at `Preview`, and preserves
  the arithmetic first cause.

The HIL target passes strict classic-ESP32 Clippy with the portable batch wired
to the HAL's complete-frame push. The optimized production ELFs contain no
`refill_batch_with` or `PcmShortDmaRefillBatch` symbol under `llvm-nm -C`; this
supports build unreachability but is not a physical or armability argument.

## Verification record

The following completed offline against firmware commit
`df23d7fa6753afda0cae69b4e86fa203190a3b4f`:

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
strings target/xtensa-esp32-none-elf/release/\
alumina-hil-mks-tinybee-pcm-short-safe | \
  rg 'HIL_PCM_ATTEST_V2|HIL_PCM_STOPPED|HIL_RESULT'
```

Observed results:

- all 441 portable default-member tests passed, including 29 xtask, 19
  shift-register, 50 motion, and 40 simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for classic ESP32 TinyBee production, ESP32-S3
  T-Deck Pro production, and the classic ESP32 TinyBee HIL image;
- all four optimized images linked successfully;
- the primary 8 MiB TinyBee production image has 1,068,100 bytes text, 12,224
  bytes data, and 249,920 bytes BSS, with SHA-256
  `53d2e0948705ff82afb2403ce9d0e36bde571ef2df34be55e69904ef1fc6ca4a`;
- the opportunistic 4 MiB TinyBee production image has 1,068,140 bytes text,
  12,224 bytes data, and 249,920 bytes BSS, with SHA-256
  `7def7eead2166ab42ebcd560a7f55bdd3f88f06239b62e3ec6858fc9fac41749`;
- the T-Deck Pro production image has 1,002,533 bytes text, 12,976 bytes data,
  and 525,392 bytes BSS, with unchanged SHA-256
  `fced1674beacddedf5ca371c223da51d8d08a91b39b003a887250ce96cf06845`;
  and
- the separate TinyBee HIL image has 69,692 bytes text, 3,120 bytes data, and
  193,488 bytes BSS, with SHA-256
  `56a356b36b466ee89be1be9420cb9a7cfcb0818e358e95dc4404a25ed3b429ce`.

All production section sizes are unchanged from the immediately preceding
software-attestation checkpoint. The T-Deck Pro ELF is byte-identical. The two
TinyBee whole-ELF digests changed because those artifacts link the modified
shift-register crate; their equal section sizes and absent batch symbols do not
justify a stronger byte-identity claim. The separate HIL text section grew by
908 bytes; its data and BSS sizes are unchanged. These are linked static
sections, not runtime stack/heap watermarks, DMA bandwidth, refill WCET, or
physical timing evidence.

No dependency, manifest, or lockfile changed, so the existing
MIT/Apache-compatible dependency inventory is unchanged. An added-Rust scan
found no GPL/AGPL/LGPL/SSPL identifier or copied FluidNC, Klipper,
Synthetos/g2core, or SimpleFOC implementation reference. The local environment
does not have `cargo-deny` installed, so no ad hoc installation was attempted;
the configured CI license policy remains required.

## Closed claims and next boundary

This checkpoint closes one portable allocation-free, ring-bounded refill work
unit; exact partial-progress/error reporting; fail-closed target uncertainty;
and adoption by the isolated TinyBee safe-image HIL composition. It does not
provide the permanent production core-1 refill actor, interrupt/wake source,
measured target deadline, qualified physical latch observation, safe
stop/reclaim evidence, or production stream authority.

The next physical boundary remains the documented disconnected SLogic16U3
capture with retained RTT, raw session, VCD, photographs, and independent
review. Offline work can proceed first on the core-1 wake/deadline actor and
simulation without touching that fixture. Until hardware evidence exists,
production adapters reject streaming and TinyBee remains non-armable for this
path.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
