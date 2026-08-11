# M7 authenticated browser/HTTP simulation evidence

Date: 2026-08-11

Status: the production browser control worker has exchanged authenticated
heartbeat traffic with a deterministic host MCU over real browser `fetch`, CORS,
and localhost TCP. Nominal traffic, a lost response, a finite outage, reboot,
bounded delay, and excessive delay were exercised. This is software/network
integration evidence only: it is not ESP radio, background-tab, physical-edge,
motion, process-energy, or safety qualification.

The coordinated source checkpoints are:

- `aluminafw` simulator commit `ba888db`;
- `alumina-interface` worker/harness commit `0e0a53e`; and
- the current sibling CSGRS/Hyper workspace resolved by the interface lockfile
  and source-policy audit. No published legacy CSGRS release is used.

## Production boundaries under test

`alumina-sim-http` is a localhost-only `std` adapter around portable production
contracts. Its fixture uses `alumina-net` route classification, exact CORS
admission, HMAC-SHA256 V2 request authorization, boot-nonce replay protection,
and signed response proofs. It decodes requests through `alumina-service`, emits
the canonical native frame and `alumina-clock` heartbeat response, and samples
a configurable affine device counter on receive and transmit. Reboot replaces
both `BootNonce` and `BootId` and clears replay state.

The socket boundary accepts one bounded connection-close HTTP request at a time.
Request lines, header lines, aggregate headers, header count, content length,
and authenticated body size are finite. Fault controls can add processing or
response delay, drop one selected control response, drop an initial finite run,
or reboot immediately before a selected control request. This adapter does not
model the ESP WiFi/HTTP stack and is never part of a target image.

The browser page creates the shipped `dist/alumina-worker.js`, which starts the
same optimized WASM module as the application. The worker opens authentication
discovery, owns the secret and session, creates signed native heartbeat
requests, validates signed responses, and updates its exact boot-scoped causal
clock model. The harness observes only versioned redacted worker events. It
passes a qualification case after four accepted observations and a usable exact
interval. Its separate conservative-rejection mode passes only after at least
three rejected observations, zero accepted observations, and no estimate.

## Browser results

Chromium 147.0.7727.137 and Node.js 22.22.2 ran the release bundle and harness.
The sampling policy admitted at most a 1,000,000,000 ns round trip, at most
2,000,000 device processing cycles, 250 ppm drift, a 1,000,000-cycle estimate
uncertainty, and four samples. The simulator declared a 1 MHz clock with 37 ppm
drift and normally added 1 ms of device processing plus 2 ms response delay.

| Scenario | Observed result |
| --- | --- |
| Nominal | Qualified after probes 1–4; four accepted, zero rejected; approximately 15–16 ms causal spans. |
| Drop control response 2 | Qualified from probes 1, 3, 4, and 5. The ambiguous request spent probe/counter identity 2 and was not replayed. |
| Drop initial four control responses | Recovered and qualified from probes 5–8; four accepted, zero rejected; final uncertainty 8,348 cycles. |
| Reboot before control request 3 | Reopened discovery, rejected stale boot authentication, reset retained model/history, and qualified from four fresh probes under boot ID `52` repeated 16 times; final uncertainty 7,171 cycles. |
| Add 300 ms response delay | Qualified from four observations with 611.3–612.3 ms causal spans and 306,446-cycle final uncertainty. |
| Add 1,200 ms response delay | Conservatively refused qualification: zero accepted, three `RoundTrip` rejections, no boot identity, no history, and no estimate. |

The reboot run found a worker recovery defect before the passing result. An
unsigned `401 text/plain` response was rejected at media validation before the
HTTP-status branch, so the stale session remained installed. The worker now
reopens authentication discovery for unsupported response media as well as
status, missing-proof, session, and boot-change failures. A new boot still
cannot enter the old model: the first authenticated new-boot response reports
`BootChanged`, clears the model/history, and forces another discovery before
fresh samples are admitted.

## Reproduction

Build the current interface release from `alumina-interface`:

```console
env -u NO_COLOR trunk build --release --locked --offline
python3 -m http.server 8097 --bind 127.0.0.1
```

In another terminal, open the harness and CDP endpoint:

```console
chromium-browser --headless=new --disable-gpu --no-sandbox \
  --remote-debugging-port=9224 \
  --user-data-dir=/tmp/alumina-clock-browser-profile-20260811 \
  http://127.0.0.1:8097/tests/browser/worker-clock-harness.html
```

Run one simulator case at a time from `aluminafw`, restarting it between cases:

```console
cargo run --locked --offline -p alumina-sim --bin alumina-sim-http
cargo run --locked --offline -p alumina-sim --bin alumina-sim-http -- \
  --drop-control-request 2
cargo run --locked --offline -p alumina-sim --bin alumina-sim-http -- \
  --drop-initial-control-requests 4
cargo run --locked --offline -p alumina-sim --bin alumina-sim-http -- \
  --reboot-control-request 3
cargo run --locked --offline -p alumina-sim --bin alumina-sim-http -- \
  --response-delay-ms 300
cargo run --locked --offline -p alumina-sim --bin alumina-sim-http -- \
  --response-delay-ms 1200
```

For the corresponding case, run from `alumina-interface`:

```console
node tests/browser/read-cdp-result.mjs 9224 nominal qualified
node tests/browser/read-cdp-result.mjs 9224 drop-response qualified
node tests/browser/read-cdp-result.mjs 9224 finite-outage qualified
node tests/browser/read-cdp-result.mjs 9224 reboot qualified
node tests/browser/read-cdp-result.mjs 9224 bounded-delay qualified
node tests/browser/read-cdp-result.mjs 9224 excessive-delay conservative-rejection
```

The scenario string forces a fresh navigation. The Node harness polls the DOM
through Chrome DevTools Protocol and exits nonzero unless the page reaches its
declared expectation.

## Reproduced checks

From `aluminafw`:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
git diff --check
```

All 245 portable tests pass, including 25 `alumina-sim` tests and three pure
authenticated HTTP fixture tests. Warnings-denied portable Clippy passes.

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
exact compiler/core tests. Native and WASM warnings-denied Clippy, WASM checking,
warnings-denied rustdoc, Trunk release, WASM validation, and compressed-artifact
integrity pass. The strengthened source audit pins every resolved Alumina crate
and driver plus CSGRS/Hyper to current sibling checkout paths and rejects missing
or GPL/AGPL/LGPL/SSPL-family license metadata.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,247,771 | `2b435949cb334064fbd38eaef836b8a560ea1a272d0b80f279decb740f85a916` |
| `alumina-interface_bg.wasm.br` | 1,610,326 | `bf612aa6de1db44de7cf39a4515d7e8641b8dfc2ff7bef158c70d7d102687846` |
| `alumina-interface_bg.wasm.gz` | 1,974,437 | `65a05b2c3286d04d2ad6fd47668b263b490070569c4afa86db9a8f3edb0c0cba` |
| `alumina-interface.js` | 89,162 | `2b8fac549b7cfd4cdc3d9c46d676f60e621365ceba4de7c1a1975d34871a92b6` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |
| `index.html` | 1,295 | `76d9936c60127de02c623dbc6dc5f0fa6a1498c581a41d0922a6e677812f175f` |
| `Cargo.lock` | — | `d30a66cdb03fddda03f41001f719b9475124d4f15dc6de8461d8ffae3041aff3` |

## Remaining qualification boundary

Background-worker suspension/throttling, abrupt listener/AP removal and later
reacquisition, asymmetric/reordered traffic, concurrent session-counter
selection, host wall-clock rollback, corrupt/full storage, partial multi-MCU
readiness, and ambiguous schedule mutations still need browser scenarios.
Production identity/capability discovery, cache upload, and schedule ownership
are not yet connected to the live worker lifecycle.

Firmware telemetry still needs qualified observed-edge capture and replay. Two
simulated and then two physical boards must run harmless cached GPIO traces
under nominal and saturated Wi-Fi before any synchronization-tolerance claim.
Both first board packages remain non-armable; no board was connected, flashed,
or energized for this checkpoint.

New code is independently authored under `MIT OR Apache-2.0`. No GPL-family
implementation source, Synthetos implementation source, or SimpleFOC
implementation source was introduced or consulted.
