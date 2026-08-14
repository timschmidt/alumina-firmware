# M10 PCM software attestation — offline evidence

Date: 2026-08-14

Status: implemented capture-evidence checkpoint. The disconnected-load-only
TinyBee PCM-short image now emits one stable numeric post-stop attestation, and
run-record schema V2 hashes and parses the retained RTT log before correlating
it with the independent analyzer marker and VCD report. This is not a hardware
run, waveform result, timing qualification, motion path, or armability result.

## Result and moving-source isolation

Firmware commit `2ae16d407d0eaa4f264ed36851db9da445844070` implements and
documents this boundary. Alumina Interface is unchanged and clean at
`330e3ef40426a07962c8b768bbf5ad1911eb27cd`.

This checkpoint has no Hyper/CSGRS build input. `cargo metadata --no-deps`
reports 35 firmware workspace packages, and all 35 manifest paths are beneath
the `aluminafw` repository. Neither `Cargo.toml` nor `Cargo.lock` changed.
Hypercurve remains an intentionally moving sibling worktree; no transient
Hypercurve state is frozen or fingerprinted here, and Alumina did not edit,
format, reset, pin, or otherwise constrain it.

## Firmware-owned attestation

Human `defmt` rendering is no longer a machine-evidence format. Only after the
circular transfer has stopped, two safe rewrite samples have crossed the HAL,
and the analyzer marker report has completed, the HIL image emits this ordered
numeric suffix:

```text
HIL_PCM_ATTEST_V2 model_epoch=… start_before=… start_after=… frames=… rate_hz=… exit=… accepted_refills=… sealed_horizon=… stop_before=… stop_after=… stop_ok=… rewrite_before=… rewrite_after=… rewrite_ok=… owner_state=… owner_fault=… safe_reclaimed=… marker_code=…
```

Every field is canonical unsigned decimal. Boolean facts are zero or one.
Exit codes 1–7 are explicit, and owner-state codes 1–10 are a separately
matched contract rather than Rust enum discriminants. The success values are
exit 1 and owner state 8 (`SafeRewriteIssued`). The existing human-readable
`HIL_PCM_STOPPED` line remains diagnostic only.

The refill loop now stops inside its final descriptor batch, so a complete run
accepts exactly 50,000 frames rather than overshooting by a target-dependent
number of released slots. With the 256-frame prefill, 250 kHz frame rate, and
four device cycles per frame, the software horizon must be exactly:

```text
model_epoch + (256 + 50,000) * 4
```

No physical observation method is reachable in the HIL image. Therefore even a
successful run must attest `owner_fault=0`, `owner_state=8`, and
`safe_reclaimed=0`. The analyzer may later prove a post-stop safe latch, but it
cannot rewrite that software fact.

## Bounded independent parser

`tinybee_pcm_log` locates zero or one `HIL_PCM_ATTEST_V2` suffix behind arbitrary
monitor prefixes. It rejects duplicate records, reordered, missing, or extra
fields, noncanonical decimal spelling, invalid Boolean values, and out-of-range
exit, state, or marker codes. It does not select a convenient line from an
ambiguous log.

Run-record schema V2 adds a repository-owned RTT-log path and SHA-256. The
validator hashes the asset, limits it to 1 MiB, requires UTF-8, and parses the
numeric record independently of debug text. Whenever an attestation exists, it
requires:

- `model_epoch == start_before` and monotonic start, stop, and rewrite windows;
- the artifact's exact 256-frame and 250 kHz identity;
- a marker equal to `exit | stop-failure-bit | rewrite-failure-bit`;
- equality between that marker and the VCD decoder's pulse result; and
- `safe_reclaimed=0` for this software-only composition.

A pass additionally requires exact exit/refill/horizon facts, successful stop
and rewrite, owner state 8, and no owner fault. A failed start cannot reach the
common post-stop report, so `fail` and `inconclusive` records may retain a
digest-bound RTT log with no attestation; a `pass` may not.

The template now names the RTT asset explicitly. The complete record still
binds the ELF, raw Sigrok capture, VCD, replayed analysis report, review notes,
actual-fixture photograph, and distinct annotated derivative. No compatibility
reader for schema V1 was added because there are no deployed capture records.

## Adversarial verification

Parser and record tests cover:

- one valid report behind a monitor prefix and an explicit absent-report shape;
- duplicates, field reordering, noncanonical leading zeroes, invalid Booleans,
  state overflow, and extra fields;
- a valid schema-V2 capture bundle and digest-bound RTT tamper rejection;
- forged sealed horizons, substituted owner state, false software reclaim, and
  disagreement with the decoded hardware marker;
- a missing pass attestation while preserving valid non-pass start-failure
  evidence; and
- the existing disconnected-load, channel-map, capture-duration, safe-image,
  frame-shape, timing, report-replay, and asset-path gates.

The optimized HIL ELF also contains the complete canonical attestation format
string, verified independently with `strings`; this is build reachability, not
proof that a device emitted it.

## Verification record

The following completed offline against firmware commit
`2ae16d407d0eaa4f264ed36851db9da445844070`:

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

- all 437 portable default-member tests passed, including 29 xtask, 15
  shift-register, 50 motion, and 40 simulator tests;
- formatting, warnings-denied all-target Clippy, warnings-denied rustdoc, and
  diff checks passed;
- strict target Clippy passed for classic ESP32 TinyBee production, ESP32-S3
  T-Deck Pro production, and the classic ESP32 TinyBee HIL image;
- all four optimized images linked successfully;
- the primary 8 MiB TinyBee production image remains 1,068,100 bytes text,
  12,224 bytes data, and 249,920 bytes BSS, with unchanged SHA-256
  `955fe2d9527a46d7944ec945f10f21114703488a60b0fdbde4a2869808e2762a`;
- the opportunistic 4 MiB TinyBee production image remains 1,068,140 bytes
  text, 12,224 bytes data, and 249,920 bytes BSS, with unchanged SHA-256
  `baaad8370f8de82d2676a5625f19585a42bae32fb3afa524fb068659acac17c2`;
- the T-Deck Pro production image remains 1,002,533 bytes text, 12,976 bytes
  data, and 525,392 bytes BSS, with unchanged SHA-256
  `fced1674beacddedf5ca371c223da51d8d08a91b39b003a887250ce96cf06845`;
  and
- the separate TinyBee HIL image has 68,784 bytes text, 3,120 bytes data, and
  193,488 bytes BSS, with SHA-256
  `a5e0c8bf28f3453645c944ddd5df2a49c31da1757082671f891ed3babe29c17e`.

The HIL text section grew by 12 bytes relative to the immediately preceding
static/stream checkpoint. Production artifacts are byte-identical because the
attestation source is a separate required-feature binary. The reported sizes
are linked static sections, not runtime stack/heap watermarks, RTT delivery,
DMA bandwidth, WCET, or physical timing evidence.

No dependency changed, so the existing MIT/Apache-compatible license inventory
is unchanged. A scan of added Rust found no GPL/AGPL/LGPL/SSPL identifier or
copied FluidNC, Klipper, Synthetos/g2core, or SimpleFOC implementation
reference. The configured CI license policy remains required.

## Closed claims and next boundary

This checkpoint closes the machine-readable software side of the TinyBee
PCM-short run record and its exact correlation contract with analyzer output.
It does not establish physical frame phase, safe latch visibility, stop/reclaim,
electrical integrity, refill deadlines, or production motion streaming.

The next physical boundary remains the documented disconnected SLogic16U3
capture with retained RTT, raw session, VCD, photos, and independent review.
Until that evidence exists, the HIL result cannot promote the production board,
the production adapter rejects streaming, and TinyBee remains non-armable for
this path.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, driven,
or used by these checks. No WLAN association changed, no USB/serial transaction
occurred, and no analyzer, GPIO, motor, motor-power, or process-power action was
taken. The SLogic16U3 was not used. Physical work remains deferred.
