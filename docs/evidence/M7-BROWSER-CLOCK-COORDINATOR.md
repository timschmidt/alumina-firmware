# M7 browser clock and cached-start coordinator evidence

Date: 2026-08-11

Status: a supervised live-browser worker/session boundary, authenticated clock
acquisition, and a deterministic cached-start coordinator are implemented. Its
original checkpoint covered worker module smoke and native/WASM clock evidence;
the later authenticated browser/HTTP fixture is recorded separately in
[`M7-BROWSER-AUTH-HTTP-SIM.md`](M7-BROWSER-AUTH-HTTP-SIM.md). Neither checkpoint
is radio, SD-card, physical-start, synchronization-tolerance, or safety
qualification.

The coordinated source checkpoints are:

- `alumina-firmware` clock contract commit `3d7671b`;
- `alumina-interface` coordinator commit `003bafa`, operator-fixture commit
  `4f467c4`, and live-worker commit `84b3b97`; and
- the current sibling CSGRS/Hyper workspace selected by the interface lockfile
  and source-policy audit. No published legacy CSGRS release is substituted.

## Implemented boundary

`alumina-interface-client` now owns a boot-scoped `DeviceClockModel` around the
portable `alumina-clock` causal interval estimator. Probe identities are spent
before I/O, response echoes must identify the exact probe and conservative send
timestamp, authenticated failures cannot update the model, and a changed boot
cannot be folded into retained observations. The firmware clock crate exposes a
freshness- and health-checked current-cycle interval as well as the existing
future-start interval; neither is represented by a floating-point affine fit.

The only floating boundary is the browser's `DOMHighResTimeStamp`. A caller must
declare a nonzero maximum timer error. Conversion to integer nanoseconds widens
outward for both the JavaScript number and that declared error, rejects values
beyond JavaScript's exact-integer range, uses the lower endpoint before request
construction, and uses the upper endpoint after the complete signed response is
read and authenticated. Local request, dispatch, buffering, and authentication
work can therefore enlarge uncertainty but cannot silently narrow the causal
interval.

The WASM adapter supports both `Window` and `WorkerGlobalScope` fetch,
authentication discovery, cache upload, and heartbeat acquisition. The worker
variant keeps timer acquisition and network I/O in one monotonic realm and is
isolated from rendering stalls. The shipped application now creates that worker
through an explicit synchronous WASM entry, supervises its exact versioned
messages, and renders its readiness and redacted device snapshots. No
browser-throttling claim is made.

Each worker-owned device session contains the HMAC secret, canonical origin,
boot-nonce HTTP/native session, exact clock model, explicit sampling policy,
bounded retry cadence, and at most 64 accepted causal records. UI commands are
bounded before I/O. Atomic replacement/disconnect generations prevent a late
asynchronous result from restoring an erased session. Rust-owned credential
buffers are overwritten on drop, and no worker event schema contains a secret;
browser-managed structured-clone/string copies remain outside that erasure
claim.

Browser-created authenticated sessions no longer restart replay counters at one.
They use an exact epoch-prefixed integer seed covered by the HMAC. This handles
ordinary reload/reconnect but is not yet a proof against host wall-clock rollback
or simultaneous-session collision; browser/network qualification must resolve
that policy before production admission.

After exact cache delivery, `ParticipantCacheReady` binds the device, local
partition publication, and identical global-manifest publication. The global
coordinator atomically aligns those proofs with the strictly sorted canonical
participant packages, boot IDs, preparation IDs, and device clock models before
emitting work. It then:

1. prepares every exact descriptor and requires the boot-derived prepared token;
2. chooses one UI epoch and maps it independently into bounded device cycles;
3. binds global/participant/local digests, clock probe, uncertainty, timing
   margins, unique commit ID, and a finite lease that ceiling-covers the exact
   rational manifest duration;
4. installs every commit before allowing the first confirmation;
5. classifies `ConfirmationOpen`, `AbortOnly`, and `PointOfNoReturn` using the
   latest possible current cycle, with equality closing the corresponding
   firmware deadline;
6. opens the confirmation phase only while all clock models remain fresh,
   healthy, boot-matched, and within policy; and
7. supports concurrent read-only status rounds for priming, start, completion,
   or fault reconciliation.

Every ambiguous prepare/install/confirm/abort/cancel mutation is followed by
`JobStatus`; it is never blindly replayed under an assumption that a lost
response means failure. A preparation that never reached firmware reconciles to
empty and may be prepared again. Precommit cleanup uses canonical `JobCancel`
until both service and real-time actors report cancellation. Postcommit abort
uses the exact commit reference and stops being represented as safe once any
reported participant crosses the abort guard.

This coordinator intentionally does not turn Wi-Fi into an E-stop or atomic
commit protocol. A stale/unavailable clock yields no successful safe-window
proof, and a reported post-guard participant makes the global state irrevocable.
Physical safety-chain requirements remain separate.

The application now renders a deterministic two-participant diagnostic fixture
through these same production state machines. It performs both ordered cache
deliveries, accepts three asymmetric affine-clock observations per simulated
MCU, reconciles one deliberately lost install response, records global lifecycle
phases, advances the portable prime/start/complete schedule, and shows ppm,
accepted/rejected samples, uncertainty, local start cycle, simulated edge time,
edge spread, and shared-epoch error. The panel is explicitly marked simulation
only. It is a repeatable operator-view integration check, not a live device
panel or physical edge observation.

The live right-hand panel now accepts a labeled device origin and passphrase,
supports immediate probe/disconnect, and displays boot, lifecycle,
accepted/rejected observations, conservative cycle intervals, causal spans,
device work, queue state, deadline misses, flags, and bounded history. It is
explicitly diagnostic-only and exposes no arm, motion, energy, or safety-reset
control.

A release bundle was served from localhost and opened in headless Chromium with
software WebGL. The document fetched the generated JavaScript, WASM, and module
worker, and the rendered panel reached `Control worker: ready
(http://127.0.0.1:8097)` without an application JavaScript/WASM error. This
checks worker creation, explicit WASM entry, ready-event decoding, supervision,
and rendering only; it did not contact simulated firmware or a physical MCU.

## Reproduced checks

From `alumina-interface`:

```console
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo check --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 51 native tests pass: 8 application/coordinator, 23 headless client, and 20
exact compiler/core tests. Native and WASM strict Clippy, WASM checking,
warnings-denied rustdoc, current-sibling source/permissive-license audit, Trunk
release, WASM validation, and compression integrity all pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,277,111 | `eea64c1267439685ca5bb6cd7cfe875c8cfc8825669b36b6d5ecccb8f432e1bf` |
| `alumina-interface_bg.wasm.br` | 1,618,026 | `6f5677790df319640beeaf1343d249f0528aceb228118991f50c4fd813126457` |
| `alumina-interface_bg.wasm.gz` | 1,984,435 | `758b56a49edc4b7a37ed826d7c6ea0f1abe443e115b3721727cf89ef9d97574e` |
| `alumina-interface.js` | 89,162 | `b339312fdd0741ab76f7d6d02bed9239923df487bd9c9be071683153391912e8` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |
| `index.html` | 1,295 | `285e1728baa5b59b2d5b12b6ae42e5d89a6e7223238f50ecedcfa226de2f3d68` |
| `Cargo.lock` | - | `30d1bc8c99384ec1b54e073b96587b932fcc842143fe12c844bb3dd041b50363` |

From `alumina-firmware`:

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
llvm-size target/xtensa-esp32-none-elf/release/alumina-firmware \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware
git diff --check
```

All 242 portable tests, strict portable Clippy, both strict ESP target Clippy
gates, and both optimized links pass.

| Board image | text | data | bss aggregate | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| MKS TinyBee V1.x | 875,380 | 11,992 | 250,144 | `10bb97a350fedab95c1761775d5e7384423fe304eb6020417607d7370c2a051d` |
| T-Deck Pro | 815,969 | 12,752 | 525,616 | `4caad824327d602e279150da02ca03182d67b72dd44387d32c89e3b7c2bf14ee` |

These are linked-capacity observations, not stack watermarks or timing results.

## Remaining qualification boundary

Authenticated simulated HTTP now covers nominal traffic, response loss, a
finite outage, reboot, bounded delay, and excessive-delay rejection. The worker
still needs live device identity/capability binding, cache/schedule ownership,
explicit attended-policy controls, and live participant/safety-chain state.
Browser tests must still cover background throttling, abrupt AP removal and
reacquisition, asymmetric/reordered traffic, corrupt/full storage, partial
readiness, ambiguous schedule responses, wall-clock rollback, and concurrent
session-counter selection. Firmware telemetry still needs qualified
observed-edge capture. Two simulated and then two physical boards must run
harmless cached GPIO traces under nominal and saturated Wi-Fi before any motion
or process-energy claim.

Both first board packages remain non-armable. No board was connected, flashed,
or energized for this checkpoint.

New code is independently authored under `MIT OR Apache-2.0`. The audited native
and WASM inventories have no missing or GPL/AGPL/LGPL/SSPL-family licenses. No
GPL-family implementation source, Synthetos implementation source, or SimpleFOC
implementation source was introduced or consulted.
