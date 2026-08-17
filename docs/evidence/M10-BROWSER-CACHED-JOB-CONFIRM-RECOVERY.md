# M10 browser cached-job confirmation-recovery evidence

Date: 2026-08-15

Status: two independent simulated MCUs each applied one `JobConfirm` request and
discarded its successful response. The production browser worker exposed both
transport ambiguities, reconciled each participant through authenticated
read-only schedule status before the abort guard, and completed the synchronized
job. A fresh-actor no-fault regression also passed. This is loopback control-
traffic evidence only. The connected bare MKS TinyBee V1.0, workstation WLAN,
GPIO, motors, motor power, and every physical output/safety path remained
untouched.

The coordinated implementation checkpoints are:

- `alumina-firmware` simulator commit `1bedfa7`; and
- `alumina-interface` qualification commit `f98012d`.

The interface resolved the current sibling CSGRS/Hyper workspace directly; it
did not use a published legacy CSGRS release.

## Exact qualification contract

Both simulator actors selected `--drop-operation-response job-confirm`. The
selector operates only after the authenticated fixture has applied a canonical
request and produced a successful same-operation native response. Each actor
logged the assigned confirmation operation exactly once:

```text
fault injection: dropped applied response for operation 0x0509
```

The browser driver mode `confirm-recovery` navigates to
`expect=cached-job-confirm-recovery` and installs one strict schema-V6 request.
The expectation cannot pass from unrelated retry noise. In addition to the
ordinary terminal invariants, it requires:

- exactly two retained failure observations;
- both in global phase `confirming`, with one consecutive `fetch failed` error;
- two cache-complete participants in only `installed` or `confirmed` schedule
  state;
- zero locally confirmed participants at the first lost response and exactly
  one at the second; and
- subsequent zero-failure recovery before terminal completion.

This progression demonstrates that neither successful packet loss was treated
as confirmation proof. After each ambiguous mutation, the coordinator first
used `JobStatus` to bind the already-applied remote state, then proceeded to the
next participant.

## Chromium results

Two independent `alumina-sim-http` processes listened on ports 8098 and 8099
with stable device identities `ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF` and
clock drifts of +37 ppm and -41 ppm. The optimized application/worker bundle
and browser harness were served on `127.0.0.1:8097`.

The dedicated confirmation-recovery run passed after 432 validated snapshots
and traversed the full sequence:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

Its two exact failure observations were:

1. both participants locally `installed` after participant one's successful
   confirmation response was lost; and
2. participant one locally `confirmed` and participant two locally `installed`
   after participant two's successful confirmation response was lost.

The shared browser epoch was `43,528,200,001 ns`, mapping to local start cycles
`75,687,009` and `75,677,076`. Both terminal participants retained 127,264
accepted of 127,264 cache bytes, schedule phase `complete`, their exact local
start cycle, worker generations 1 and 2, and boot IDs of sixteen `0x31` bytes.
The global terminal snapshot reported zero consecutive failures and no error.

Both actors were then restarted without fault selectors. The unchanged ordinary
`single` mode passed in 432 snapshots with no failure observations and no
recovery flag. Its shared epoch was `43,527,800,001 ns`, mapping to local cycles
`68,849,586` and `68,840,991`.

Representative faulted commands were:

```console
# two alumina-firmware terminals
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --drop-operation-response job-confirm
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-response job-confirm

# alumina-interface, with the bundle and Chromium CDP already served
node tests/browser/read-cached-job-result.mjs 9224 confirm-recovery
```

## Same-attempt reattachment boundary found

A deliberately separate follow-up replaced the browser worker while retaining
the two already-terminal simulator actors and submitted the same prepare ID and
descriptor again. Firmware/simulator exact retry correctly returned the retained
terminal report. The fresh browser coordinator had no locally bound commit and
rejected that report as `device reported an unbound commit`; the bounded attempt
was stopped rather than allowed to retry indefinitely.

Consequently this checkpoint proves in-owner confirmation ambiguity recovery,
not fresh-owner discovery or reattachment to an already-terminal same attempt.
The existing consecutive-job workflow uses a distinct prepare ID after an
acknowledged local clear and is unaffected. Durable same-attempt browser
reattachment, including bounded terminal treatment instead of an unbounded
retry loop, remains explicit follow-on work.

That boundary was subsequently closed by schedule/status schema replacement and
replacement-worker qualification at `alumina-firmware` commit `a02a877` and
`alumina-interface` commit `c7fda5b`. See
[`M10-BROWSER-CACHED-JOB-REATTACHMENT.md`](M10-BROWSER-CACHED-JOB-REATTACHMENT.md).
Applied pre-confirm abort-response loss was subsequently qualified separately
at `alumina-interface` commit `205e059`; see
[`M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md`](M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md).

## Verification performed

The following passed on the recorded implementation:

- Node syntax validation for the driver;
- module syntax validation for the extracted browser harness;
- `git diff --check`;
- the dedicated two-participant confirmation-recovery Chromium run; and
- the fresh-actor ordinary no-fault Chromium regression.

The simulator selector itself was already covered by 63 library tests, three
focused binary tests, and warnings-denied package Clippy at commit `1bedfa7`.
The immediately preceding checkpoint also passed all 225 interface native tests,
warnings-denied workspace Clippy, and `scripts/audit-source-policy.sh` against a
coherent current Hyper snapshot.

## License and moving-Hyper boundary

The new qualification code is repository-owned under the existing MIT terms
and uses only the accepted MIT/Apache-compatible dependency set. No GPL, AGPL,
LGPL, SSPL, Synthetos, SimpleFOC, or other copyleft implementation or dependency
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only: none was edited, formatted, reset, pinned, staged, committed, or
intentionally diff-inspected.

## Claims deliberately kept closed

This confirmation-loss run does not itself establish abort-response-loss
recovery, fresh-owner same-attempt reattachment, response reorder or
duplication, sustained outage, background-tab reliability, ESP32 execution,
physical Wi-Fi/AP behavior, real
SD-card durability, GPIO or bus timing, I2S/RMT/MCPWM/ADC behavior, motor motion,
multi-MCU electrical simultaneity, endstop/E-stop/interlock response, attended
or cached-autonomous production policy, production credentials, or any
machine-arm/safety qualification. The simulator's latch authority cannot
authorize physical output. Fresh-owner terminal reattachment is established
only by the later evidence linked above, and applied abort-response loss is
established only by its separate later checkpoint.
