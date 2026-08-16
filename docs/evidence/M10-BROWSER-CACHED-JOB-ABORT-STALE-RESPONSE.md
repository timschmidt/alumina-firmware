# M10 browser cached-job abort/stale-response evidence

Date: 2026-08-15

Status: the production browser worker rejected a previously valid signed
`JobConfirm` HTTP response when each simulated MCU returned it in place of a
later successful schedule-operation response. The first MCU had already
applied `JobAbort`; the second had already applied the read-only `JobStatus`
used by the resulting all-participant reconciliation sweep. In both cases the
old authentication counter failed the browser session's exact pending-counter
check before response proof or native-frame acceptance. The worker spent the
ambiguous request counter, reopened boot-correlated session authority, required
a new complete status sweep, and reached the exact all-participant `aborted`
terminal. A separate fresh-actor run completed normally with no fault
selector.

This is localhost software evidence only. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or
safety path remained untouched.

The implementation checkpoints are `aluminafw` commit `15ef223` and
`alumina-interface` commit `467aaeb`. The interface resolved the actively
edited sibling CSGRS/Hyper workspace directly. No published legacy CSGRS
release was used and no sibling source was modified or pinned.

## Exact stale-response contract

`alumina-sim-http` now accepts the one-shot selector:

```text
--replay-response-after-operation NAME_OR_WIRE
```

For the first matching canonical control operation whose native response
indicates success, the fixture performs this exact sequence:

1. dispatch the trigger request normally;
2. write its complete successful HTTP response to the client;
3. retain a byte-identical copy of its status, reason, ordered headers, signed
   authentication counter and proof, media type, and native body;
4. ignore unrelated and non-schedule operations without consuming the fault;
5. dispatch the next canonical job-schedule request normally and require its
   native response to indicate success; and
6. return the retained old response in place of that later response.

The later operation is therefore applied before its response is substituted.
The retained response is consumed exactly once and cannot be re-armed during
the process lifetime. The eligible later operation class is `JobPrepare`,
`JobStatus`, `JobCommit`, `JobConfirm`, `JobAbort`, or `JobCancel`. Existing
request-drop, response-drop, and bounded schedule-outage selectors retain
precedence and their own semantics.

Simulator unit coverage proves mismatched triggers do not arm the fault,
non-schedule traffic does not consume it, the complete response is cloned
exactly, the first later schedule operation consumes it, and neither another
schedule operation nor another matching trigger can replay it again.

## Why this is not a general packet-reordering claim

The production cached-job worker owns one global job fetch at a time, and each
`AuthenticatedHttpSession` owns at most one pending request. Browser fetch and
TCP response demultiplexing therefore do not ordinarily allow two live
schedule responses from one session to arrive against the wrong pending
request. Claiming arbitrary response reordering at that boundary would test an
unreachable concurrency model.

This qualification instead exercises the strongest adjacent bounded fault:
an endpoint or intermediary substitutes a previously valid, previously
accepted, origin-bound signed response for the response to a later request.
It demonstrates strict stale-counter rejection and ambiguous-operation
reconciliation. It does not demonstrate concurrent in-flight response
reordering, arbitrary packet reassembly, proxy behavior, cross-session
response injection, or radio-layer ordering.

## Browser rejection and recovery rule

`AuthenticatedHttpSession::accept_response` parses the response authentication
metadata and compares its counter with the exact pending request before proof
verification or native decoding. A mismatch returns
`HttpSessionError::CounterMismatch` and leaves the pending request intact;
unauthenticated or stale input cannot consume it. The asynchronous fetch owner
then explicitly abandons that pending request, so its counter remains spent,
and a unit regression proves the next request advances rather than reuses it.

For cached-job coordination, that session error makes the HTTP session
reopenable against the same observed boot and marks the schedule result as
ambiguous. The job owner preserves only its last authenticated native facts and
requires a clean `JobStatus` round across every participant before another
schedule mutation. A failure within that round requires another complete
round. No stale response can fabricate `Confirmed`, `Aborted`, `Complete`, a
local start cycle, or cache progress.

The final two-actor run exercises both sides of this rule:

- actor one retains `JobConfirm` response operation `0x0509`, applies
  `JobAbort` operation `0x0504`, and substitutes the retained response;
- the resulting reconciliation obtains actor one's exact `Aborted` status;
- actor two retains its own `JobConfirm` response, applies reconciliation
  `JobStatus` operation `0x0508`, and substitutes the retained response; and
- a subsequent clean all-participant sweep recovers `Aborted`/`Confirmed`,
  after which the coordinator applies actor two's exact abort.

Thus even ambiguity in the read-only reconciliation sweep cannot partially
clear the all-participant gate.

## Chromium contract and result

The browser driver mode `abort-stale-response` navigates to
`expect=cached-job-abort-stale-response`. It requests stop only after both
actors are globally confirmed and requires:

- the ordinary exact cache, prepare, install, and confirmation progression;
- exactly two one-failure `aborting` observations;
- locally aborted participant counts `0 -> 1` across those observations;
- every failure cause to contain both `HTTP response counter` and
  `does not match pending counter`;
- complete immutable cache facts and retained local start cycles throughout;
  and
- terminal all-participant `aborted`, zero consecutive failures, and no error.

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Both selected a stale replay armed after `job-confirm`.

Job `2047934465` passed after 393 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> aborted
```

Its selected browser epoch was `45,554,500,001 ns`, mapping to local cycles
`108,672,059` and `108,662,753`. Both actors retained exact `Aborted` state and
127,264 accepted of 127,264 cache bytes. The retained boundary was:

```text
failure 1: counter mismatch; Confirmed / Confirmed
failure 2: counter mismatch; Aborted / Confirmed
terminal:  count 0;          Aborted / Aborted
```

The first response carried stale counter `1786850669514450157` against pending
counter `1786850669514450160`. The second carried stale counter
`1786850669516954516` against pending counter `1786850669516954521`. Recovery
was recorded and the terminal retained no error. Each actor logged exactly one
successful-response retention and one substitution; actor one substituted for
`JobAbort`, while actor two substituted for `JobStatus`.

Representative actor commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --replay-response-after-operation job-confirm

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --replay-response-after-operation job-confirm

node tests/browser/read-cached-job-result.mjs 9224 abort-stale-response
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but no fault selector,
completed job `2047934465` after 433 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `45,203,400,002 ns`, mapping to local cycles
`69,581,658` and `69,601,448`. Both actors retained exact `Complete` state and
127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag.

## Verification performed

The following passed on the final implementation checkpoints:

- the simulator binary's seven HTTP fault-selector tests and the full locked,
  offline `aluminafw` default-member test suite;
- `cargo test --workspace --locked --offline --quiet` in
  `alumina-interface`: 41 application, 76 client, 125 core, integration, and
  compile-fail documentation coverage;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- warnings-denied simulator and interface rustdoc plus the complete WASM test
  link set;
- `scripts/audit-source-policy.sh`, Node driver syntax, inline harness module
  syntax, package-scoped formatting, selector search, and diff checks;
- optimized Trunk release assembly plus `wasm-tools validate`, gzip, and Brotli
  integrity checks; and
- the dedicated stale-response Chromium run and fresh no-fault control above
  against that exact final artifact from a fresh Chromium profile.

The optimized production WASM was 6,064,865 bytes, 2,688,155 bytes under gzip,
2,126,529 bytes under Brotli, and SHA-256
`34a18faee1eaf9166b66979e167433782da92150c2d4c352362e9279620ad70f`.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.
The final production artifact and browser runs used a freshly compiled
coherent snapshot.

## Claims deliberately kept closed

This evidence closes only a one-shot substitution of a previously successful,
validly signed schedule response for the successful response to one later
applied schedule operation on each live simulator actor. It proves exact
pending-counter rejection, spent-counter advancement, session reopening,
truthful last-fact retention, and full all-participant reconciliation through
both an ambiguous mutation and an ambiguous status read.

It does not cover arbitrary or concurrent packet/response reordering,
cross-session injection, repeated or unsigned substitution, total endpoint or
authentication/bootstrap loss, indefinite schedule outage, browser crash or
background reliability, process or MCU crash, hardwired stop or E-stop
behavior, ESP32 execution, physical Wi-Fi/AP behavior, real SD-card durability,
GPIO or bus timing, I2S/RMT/MCPWM/ADC behavior, motor motion, multi-MCU
electrical simultaneity, endstop or interlock response, attended or
cached-autonomous production policy, production credentials, or any
machine-arm/safety qualification. A Wi-Fi stop remains supplementary control
traffic and cannot replace a hardwired physical safety chain.
