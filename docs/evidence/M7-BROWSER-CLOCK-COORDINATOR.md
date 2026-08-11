# M7 browser clock and cached-start coordinator evidence

Date: 2026-08-11

Status: a worker-capable authenticated browser clock-acquisition boundary and a
headless deterministic cached-start coordinator are implemented. This is
native/WASM software evidence. It is not a live-browser, radio, SD-card,
physical-start, synchronization-tolerance, or safety qualification.

The coordinated source checkpoints are:

- `aluminafw` clock contract commit `3d7671b`;
- `alumina-interface` coordinator commit `003bafa` plus operator-fixture commit
  `4f467c4`; and
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
available for isolation from rendering stalls. The shipped application does not
yet create or supervise that worker, and no browser-throttling claim is made.

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

All 46 native tests pass: 8 application/coordinator, 18 headless client, and 20
exact compiler/core tests. Native and WASM strict Clippy, WASM checking,
warnings-denied rustdoc, current-sibling source/permissive-license audit, Trunk
release, WASM validation, and compression integrity all pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 3,984,582 | `b4a00862551ac4515685e7fbf1fc07fc362a3a5d6b125c6bde831ff9041327ab` |
| `alumina-interface_bg.wasm.br` | 1,524,339 | `36a1c4e5f09ce0f7d9101f58fa1d369a300dba6a55b1856372ff91a7eb23694f` |
| `alumina-interface_bg.wasm.gz` | 1,858,428 | `65734ce6659e127ebb7b384ded63db01b16627b94000ea539eca42d5a95cd656` |
| `alumina-interface.js` | 75,566 | `2b92d9e70c2ed9895b4cd361eb35f8a936f9a302a4c749762cd86c353f3ad28c` |
| `index.html` | 1,290 | `04926872cbdf270b953f89c920cc8c3600c4bc224282e3c578be757ddeae81cb` |
| `Cargo.lock` | - | `cd31aaadb49b3c2d537c0184fda3e2f8b24411801175ce6b85c8a3b48f3eb250` |

From `aluminafw`:

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

The UI still needs worker creation/supervision, live multi-device session
ownership, clock-delay history, explicit attended-policy controls, and live
participant/safety-chain state. Live-browser tests must cover background
throttling, AP loss, delay spikes, reboot, corrupt/full storage, partial
readiness, and ambiguous responses. Firmware telemetry still needs qualified
observed-edge capture. Two simulated and then two physical boards must run
harmless cached GPIO traces under nominal and saturated Wi-Fi before any motion
or process-energy claim.

Both first board packages remain non-armable. No board was connected, flashed,
or energized for this checkpoint.

New code is independently authored under `MIT OR Apache-2.0`. The audited native
and WASM inventories have no missing or GPL/AGPL/LGPL/SSPL-family licenses. No
GPL-family implementation source, Synthetos implementation source, or SimpleFOC
implementation source was introduced or consulted.
