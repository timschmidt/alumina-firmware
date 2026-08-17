# M10 browser cached-job abort-duplicate evidence

Date: 2026-08-15

Status: the production browser worker recovered when each simulated MCU applied
one authenticated `JobAbort`, then received a byte-identical replay carrying
the already-consumed authentication counter and proof. Each MCU rejected its
replay with HTTP 401 before a second native dispatch. The browser treated that
non-success response as ambiguous mutation delivery, opened a fresh
authenticated session after spending the failed counter, reconciled through
read-only `JobStatus`, and reached the exact
all-participant `aborted` terminal without blindly repeating an already-applied
mutation. A separate fresh-actor run completed normally with no fault selector.
This is localhost software evidence only. The connected bare
MKS TinyBee V1.0, workstation WLAN, GPIO, motors, motor power, and every
physical output or safety path remained untouched.

The implementation checkpoints are `alumina-firmware` commit `7aa15df` and
`alumina-interface` commit `6be1bd2`. The interface resolved the actively
edited sibling CSGRS/Hyper workspace directly. No published legacy CSGRS
release was used and no sibling source was modified or pinned.

## Exact duplicated-request contract

`alumina-sim-http` now accepts the one-shot selector:

```text
--duplicate-operation-request NAME_OR_WIRE
```

For the first matching authenticated control request whose native response
indicates success, the fixture performs this exact sequence:

1. dispatch the original request normally and retain its successful response;
2. dispatch the same request bytes again, including the same origin-bound HMAC
   counter, proof, media type, operation, and body;
3. require the replay-window check to return HTTP 401 before native operation
   dispatch; and
4. return that later 401 response to the browser instead of the withheld first
   success.

Any non-401 replay result is a fixture error. A successful run logs exactly one
`duplicated applied request ... replay rejected` record for the selected
operation. The selector is exact and one-shot: another operation, another wire
value, or a later matching request is unaffected.

This models a bounded post-application duplicate at the authenticated device
boundary. It proves that firmware authentication suppresses the second native
mutation and that the browser reconciles the uncertain first application. It
does not model arbitrary proxy behavior, reordered traffic, concurrent
duplicates, a duplicated unauthenticated request, or a full control/status
outage.

## Browser recovery rule

An HTTP error response is not an authenticated native success envelope. The
WASM fetch boundary now classifies any status other than 200 before inspecting
success media or authentication headers. This preserves the meaningful
`device returned HTTP status 401` cause even when the replay rejection uses a
plain-text body.

The cached-job controller does not retransmit the ambiguous `JobAbort` from the
same pending request. It abandons that pending exchange and permanently spends
its HMAC counter. Because an HTTP status failure cannot carry a trusted native
response, the job owner also invalidates that HTTP session; the ordinary device
owner opens a replacement session against the same MCU boot before the next job
operation. It then issues read-only `JobStatus`. Only the returned exact
schedule state determines the next transition. In this qualification the first
status round found one participant already `Aborted` and the other still
`Confirmed`; the next participant was then handled independently. This is the
same status-before-next-mutation invariant used for lost successful responses,
but the injected failure is an actual authenticated replay rejection rather
than a discarded response.

## Chromium contract and result

The browser driver mode `abort-duplicate` navigates to
`expect=cached-job-abort-duplicate`. It requests stop only after both actors are
globally confirmed and requires:

- the ordinary exact caching, prepare, install, and confirmation path;
- exactly two one-failure `aborting` observations with locally aborted counts
  `0 -> 1`;
- every retained failure cause to contain `HTTP status 401`;
- complete immutable cache facts throughout reconciliation; and
- an all-participant `aborted` terminal with zero failure and no error.

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Both selected `--duplicate-operation-request job-abort`.

Job `2047934465` passed after 391 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> aborted
```

Its selected browser epoch was `45,850,900,002 ns`, mapping to local cycles
`148,249,511` and `148,199,565`. Both actors retained exact `Aborted` state and
127,264 accepted of 127,264 cache bytes. The two retained failure observations
were:

```text
failure 1: HTTP status 401; participant states Confirmed / Confirmed
failure 2: HTTP status 401; participant states Aborted / Confirmed
terminal:  failure count 0; participant states Aborted / Aborted
```

The browser reported recovery, the terminal retained no error, and each actor
logged exactly one rejected replay of `JobAbort` wire operation `0x0504`.

Representative actor commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --duplicate-operation-request job-abort

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --duplicate-operation-request job-abort

node tests/browser/read-cached-job-result.mjs 9224 abort-duplicate
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but no fault selector,
completed job `2047934465` after 431 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `45,497,600,001 ns`, mapping to local cycles
`156,476,385` and `156,563,632`. Both actors retained exact `Complete` state and
127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag. This control guards against a fixture
or client change that merely converts all stop-capable jobs into failure or
abort.

## Verification performed

The following passed on the final implementation checkpoints:

- the simulator binary's five HTTP fault-selector tests and the full locked,
  offline `alumina-firmware` default-member test suite;
- `cargo test --workspace --locked --offline --quiet` in
  `alumina-interface`: 40 application, 75 client, 125 core, one integration,
  and the compile-fail documentation test;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- `scripts/audit-source-policy.sh`, Node driver syntax, inline harness module
  syntax, formatting, and diff checks;
- optimized Trunk release assembly plus `wasm-tools validate`, gzip, and Brotli
  integrity checks; and
- the dedicated duplicate/replay Chromium run and fresh no-fault control above.

The optimized production WASM was 6,061,999 bytes, 2,687,752 bytes under gzip,
2,126,089 bytes under Brotli, and SHA-256
`dc7dadc8007e20e1456b068eb29ead838fae7e7a194b2e3fe015e8c99eac5ee3`.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.
Transient coherent Hyper snapshots were retried only when necessary.

## Claims deliberately kept closed

This evidence closes only a one-shot, byte-identical replay of an already
authenticated and applied `JobAbort` on each participant, the replay-window
rejection before second native dispatch, browser HTTP-error classification, and
status-based recovery to exact abort. It does not cover general request or
response duplication, reordering, simultaneous duplicates, full
control/status outage, browser crash or background reliability, hardwired stop
or E-stop behavior, ESP32 execution, physical Wi-Fi/AP behavior, real SD-card
durability, GPIO or bus timing, I2S/RMT/MCPWM/ADC behavior, motor motion,
multi-MCU electrical simultaneity, endstop or interlock response, attended or
cached-autonomous production policy, production credentials, or any
machine-arm/safety qualification. A Wi-Fi stop remains supplementary control
traffic and cannot replace a hardwired physical safety chain.
