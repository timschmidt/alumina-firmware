# M10 browser cached-job abort/status-outage evidence

Date: 2026-08-15

Status: the production browser worker retained exact two-MCU schedule facts
while both simulated MCUs discarded a bounded run of every canonical job
schedule request. Each actor armed only after its successful `JobConfirm`
response was written, then discarded 24 schedule requests before
authentication or application. Unrelated clock, health, configuration,
capability, and telemetry traffic remained available. The browser treated
every dropped request as ambiguous, required a clean all-participant status
sweep before any later schedule mutation, crossed the abort guard without
inventing an abort, and terminated as `completed_after_stop_request` only after
both locally clocked schedules reported exact completion. A separate
fresh-actor run completed normally with no fault selector.

This is localhost software evidence only. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or safety
path remained untouched.

The implementation checkpoints are `alumina-firmware` commit `ca3d4d3` and
`alumina-interface` commit `b082438`. The interface resolved the actively edited
sibling CSGRS/Hyper workspace directly. No published legacy CSGRS release was
used and no sibling source was modified or pinned.

## Exact bounded-outage contract

`alumina-sim-http` now accepts:

```text
--drop-schedule-after-operation NAME_OR_WIRE
--drop-schedule-after-operation-count N
```

The count is nonzero, defaults to one when the selector is present, and is
invalid without the selector. A matching operation arms the outage only when
its canonical native response indicates success and that HTTP response has
been written. It therefore cannot suppress or ambiguously classify the trigger
operation itself.

Once armed, the actor discards the next `N` requests whose decoded canonical
operation is one of:

```text
JobPrepare JobStatus JobCommit JobConfirm JobAbort JobCancel
```

The discard occurs before authentication and native dispatch, closes the HTTP
exchange without a response, and logs the exact drop index, bound, and wire
operation. Non-schedule operations neither consume the bound nor lose their
responses. After exactly `N` schedule drops, ordinary handling resumes. The
selector is one-shot for the process lifetime.

The standalone simulator now also advances its locally owned cached schedule
from the device clock on every received HTTP request before fault selection.
Schedule priming, start, and completion therefore do not depend on successful
`JobStatus` polling. Clock and diagnostic traffic can drive simulated wall-time
observation during a complete schedule-request outage, matching the firmware
contract that an installed cached actor is locally clocked rather than
browser-stepped.

This models a bounded outage of the complete job-schedule request class while
the MCU and unrelated HTTP services remain alive. It does not model an
indefinite schedule outage, loss of the authentication/bootstrap route, loss of
all endpoint traffic, AP failure, process crash, packet reordering, or physical
radio behavior.

## All-participant reconciliation rule

Per-participant ambiguity handling alone is insufficient after a long outage:
one actor may advance autonomously while another participant's last retained
fact is stale. A later mutation must not be authorized from that mixture.

`LiveCachedJob` now marks every abandoned coordinator schedule exchange as
requiring an all-participant reconciliation sweep. It first drains any already
constructed status queue, then constructs a new `JobStatus` round for every
participant before returning to ordinary coordinator operations. A failure
inside that round marks another complete sweep as required. Consequently only
a wholly successful sweep can clear the gate; no single successful peer read
can authorize a later mutation while another peer remains unknown.

The coordinator continues to preserve exact last-authoritative facts. An
ambiguous abort or status does not fabricate `Aborted`, `Complete`, a local
cycle, or progress. Native regression coverage repeatedly abandons abort and
status requests, verifies the retained `Confirmed` facts, then admits only
explicit remote `Complete` reports.

Two exploratory, non-qualifying runs exposed these seams before the final
contract was frozen. The first showed that reconciling only the failed peer
could leave another participant stale enough to permit a late mutation. The
second showed that simulator progress was accidentally coupled to job polling.
Those runs are not evidence; they led respectively to the complete sweep gate
and independent local schedule advancement used by the passing run below.

## Chromium contract and result

The browser driver mode `abort-status-outage` navigates to
`expect=cached-job-abort-status-outage`. It requests stop only after both actors
are globally confirmed and requires:

- exact cache, prepare, install, and confirmation progression;
- exactly 48 failed schedule exchanges, matching 24 drops per actor;
- failures 1 through 47 in `aborting`, with consecutive counts `1..47` and
  both last-authoritative participant states still `Confirmed`;
- the final dropped peer status in `irrevocable`, with a fresh exact
  `Complete` fact for one actor and retained `Confirmed` for the other;
- complete immutable cache facts and nonzero local start cycles throughout;
- a recorded `irrevocable` transition; and
- terminal `completed_after_stop_request`, both schedules exactly `Complete`,
  zero consecutive failures, and no error.

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Both selected a 24-request outage armed after `job-confirm`.

Job `2047934465` passed after 438 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> irrevocable ->
completed_after_stop_request
```

Its selected browser epoch was `45,288,300,001 ns`, mapping to local cycles
`125,687,306` and `125,572,665`. Both actors retained exact `Complete` state
and 127,264 accepted of 127,264 cache bytes. The retained outage boundary was:

```text
failure 1:  Aborting, count 1;  Confirmed / Confirmed
failure 47: Aborting, count 47; Confirmed / Confirmed
failure 48: Irrevocable, count 1; Complete / Confirmed
terminal:   count 0;             Complete / Complete
```

All 48 failure causes contained browser `fetch failed`. Recovery was recorded
and the terminal retained no error. Actor one logged one discarded `JobAbort`
wire operation `0x0504` followed by 23 discarded `JobStatus` operations
`0x0508`. Actor two logged 24 discarded `JobStatus` operations. Each actor
logged exactly one 24-request arm after successful `JobConfirm` wire operation
`0x0509` and exactly 24 indexed drops.

Representative actor commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --drop-schedule-after-operation job-confirm \
  --drop-schedule-after-operation-count 24

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-schedule-after-operation job-confirm \
  --drop-schedule-after-operation-count 24

node tests/browser/read-cached-job-result.mjs 9224 abort-status-outage
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but no fault selector,
completed job `2047934465` after 434 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `45,003,400,002 ns`, mapping to local cycles
`138,147,888` and `138,109,058`. Both actors retained exact `Complete` state
and 127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag.

## Verification performed

The following passed on the final implementation checkpoints:

- the simulator binary's six HTTP fault-selector tests and the full locked,
  offline `alumina-firmware` default-member test suite;
- `cargo test --workspace --locked --offline --quiet` in
  `alumina-interface`: 41 application, 75 client, 125 core, integration, and
  compile-fail documentation coverage;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- warnings-denied simulator and interface rustdoc plus the complete WASM test
  link set;
- `scripts/audit-source-policy.sh`, Node driver syntax, inline harness module
  syntax, package-scoped formatting, stale-selector search, and diff checks;
- optimized Trunk release assembly plus `wasm-tools validate`, gzip, and Brotli
  integrity checks; and
- the dedicated bounded schedule-outage Chromium run and fresh no-fault control
  above against that exact final artifact.

The optimized production WASM was 6,063,431 bytes, 2,687,625 bytes under gzip,
2,127,014 bytes under Brotli, and SHA-256
`c24197b2a60de554b2d2fee44573b12e8475a2b3bed8102c53ecf54e2e33e612`.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.
The final production artifact and browser run used a freshly compiled coherent
snapshot.

## Claims deliberately kept closed

This evidence closes only a bounded, post-confirmation outage of all canonical
job-schedule request operations on both live simulator actors while unrelated
HTTP diagnostics remain available. It proves exact fact retention, autonomous
simulated schedule progress, repeated ambiguity handling, a clean
all-participant reconciliation gate, and truthful completion after a missed
Wi-Fi stop request.

It does not cover an indefinite schedule outage, total endpoint loss,
authentication/bootstrap loss, AP or browser failure, browser suspension,
background reliability, packet reordering, arbitrary duplication, process or
MCU crash, hardwired stop or E-stop behavior, ESP32 execution, physical Wi-Fi/AP
behavior, real SD-card durability, GPIO or bus timing, I2S/RMT/MCPWM/ADC
behavior, motor motion, multi-MCU electrical simultaneity, endstop or interlock
response, attended or cached-autonomous production policy, production
credentials, or any machine-arm/safety qualification. A Wi-Fi stop remains
supplementary control traffic and cannot replace a hardwired physical safety
chain.
