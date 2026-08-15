# M10 browser cached-job abort-response recovery evidence

Date: 2026-08-15

Status: two independent simulated MCUs each applied one pre-guard `JobAbort`
request and discarded its successful response in two separate runs: first from
globally installed state and then after both actors had already granted future
start authority and reported `Confirmed`. The production browser worker exposed
both transport ambiguities, reconciled each participant through authenticated
read-only schedule status before mutating the next participant, and reached an
exact all-participant `aborted` terminal snapshot in both cases. A fresh-actor
ordinary no-fault completion regression also passed. This is loopback control-
traffic evidence only. The connected bare MKS TinyBee V1.0, workstation WLAN,
GPIO, motors, motor power, and every physical output/safety path remained
untouched.

The implementation checkpoints are:

- `aluminafw` operation-specific simulator selector commit `1bedfa7`; and
- `alumina-interface` installed-state qualification commit `205e059`; and
- `alumina-interface` confirmed-state qualification commit `b670a0e`.

The current run used the exact schedule/status schemas from `aluminafw` commit
`a02a877`. The interface resolved the actively edited sibling CSGRS/Hyper
workspace directly, without a published legacy CSGRS release or any sibling
source pin/modification.

## Exact qualification contract

Both simulator actors selected `--drop-operation-response job-abort`. The
selector acts only after the authenticated fixture has applied a canonical
request and produced a successful same-operation native response. Each actor
logged exactly one discarded operation:

```text
fault injection: dropped applied response for operation 0x0504
```

The browser driver mode `abort-recovery` navigates to
`expect=cached-job-abort-recovery` and installs one strict schema-V7 request.
The ordinary worker first caches and prepares both participants, selects one
future UI epoch, and installs both commits. Only after a global `installed`
snapshot does the harness send the strict `stop_cached_job` command. No
participant has received `JobConfirm` at that point.

The expectation cannot pass from unrelated retry noise. It requires:

- exactly two retained failure observations;
- both in global phase `aborting`, with one consecutive `fetch failed` error;
- two cache-complete participants in only `installed` or `aborted` schedule
  state;
- zero locally aborted participants at the first lost response and exactly one
  at the second;
- subsequent zero-failure recovery; and
- terminal global `aborted` with both participants locally `aborted`, complete
  cache facts, and retained local start cycles.

The client regression proves that abandoning a mutating abort request makes a
second mutation illegal until `JobStatus` authenticates the already-applied
state. The two-participant coordinator regression records the exact sequence
and asserts `lost_aborts == reconciled_aborts` before every next `JobAbort`.
Thus participant two cannot be mutated while participant one's outcome remains
ambiguous.

The distinct `confirmed-abort-recovery` mode uses the same strict invariants but
waits for global `confirmed` before requesting stop. Its two failure snapshots
may contain only `confirmed` or `aborted` schedule state and must retain locally
aborted counts `0 -> 1`. A focused client test independently proves an ambiguous
abort response cannot erase or re-grant the previously confirmed start
authority: only authenticated status may advance that participant to
`Aborted`.

## Chromium results

Two independent `alumina-sim-http` processes listened on ports 8098 and 8099
with stable identities `ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated
drifts of +37 ppm and -41 ppm, worker generations 1 and 2, and boot IDs of
sixteen `0x31` bytes. The optimized production application/worker bundle and
browser harness were served on `127.0.0.1:8097`.

The dedicated abort-recovery run passed after 389 validated snapshots and
traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
aborting -> aborted
```

Its selected browser epoch was `48,739,800,001 ns`, mapping to retained local
start cycles `102,654,525` and `102,579,983`. The first loss observation kept
both local views `installed`; status reconciliation then advanced participant
one to `aborted`. The second loss observation retained participant one as
`aborted` and participant two as `installed`; its status reconciliation made
the global terminal exact. Both terminal participants retained 127,264 accepted
of 127,264 cache bytes, schedule phase `aborted`, their local start cycles, zero
consecutive failures, and no error. The harness recorded exactly two failure
observations and a later recovery.

Both simulator actors were then restarted without fault selectors. The
unchanged ordinary `single` mode completed the full start lifecycle in 419
snapshots with zero failure observations. Its shared epoch was
`62,952,000,001 ns`, mapping to local cycles `98,339,464` and `98,288,738`.

The separate confirmed-state run then used fresh fault-selected actors. Both
participants reached `confirmed` before the stop command. It passed after 391
snapshots with this sequence:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> aborted
```

Its selected epoch was `61,066,300,002 ns`, mapping to local cycles
`123,253,557` and `123,219,326`. The first failure retained both participants
locally `confirmed`; the second retained participant one `aborted` and
participant two `confirmed`. Final status reported both `aborted`, zero failure,
and no error. Each simulator again logged exactly one dropped applied response
for operation `0x0504`.

Representative commands were:

```console
# two aluminafw terminals
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 \
  --drop-operation-response job-abort
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-response job-abort

# alumina-interface, with the optimized bundle and Chromium CDP already served
node tests/browser/read-cached-job-result.mjs 9224 abort-recovery
node tests/browser/read-cached-job-result.mjs 9224 confirmed-abort-recovery
```

## Verification performed

The following passed on the recorded source:

- `cargo test --workspace --locked --offline` in `alumina-interface`: 36
  application, 69 client, 125 core, and one integration test, plus the
  compile-fail documentation test;
- warnings-denied all-target workspace Clippy with dependency linting excluded;
- package-scoped formatting and `git diff --check`;
- `scripts/audit-source-policy.sh`;
- Node driver syntax validation;
- `wasm-tools validate` on the unchanged optimized 6,042,337-byte production
  bundle from the immediately preceding schema-V7 checkpoint;
- the dedicated two-participant abort-response-loss Chromium run; and
- the fresh-actor ordinary no-fault Chromium regression.

## License and moving-Hyper boundary

The new qualification code is repository-owned under the existing MIT terms
and uses only the accepted MIT/Apache-compatible dependency set. No GPL, AGPL,
LGPL, SSPL, Synthetos, SimpleFOC, or other copyleft implementation or dependency
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.

## Claims deliberately kept closed

This evidence covers successful-response loss only after each abort was applied
before the guard, from installed and confirmed state. It does not establish
initial `JobAbort` request non-delivery, sustained outage through the guard, any
action at or after the point of no return, hardwired stop/E-stop behavior,
browser crash or
background reliability, response reorder/duplication, ESP32 execution,
physical Wi-Fi/AP behavior, real SD-card durability, GPIO or bus timing,
I2S/RMT/MCPWM/ADC behavior, motor motion, multi-MCU electrical simultaneity,
endstop/interlock response, attended or cached-autonomous production policy,
production credentials, or any machine-arm/safety qualification. A Wi-Fi abort
is supplementary control traffic and cannot become a physical safety-chain
claim.
