# M10 browser cached-job abort-request recovery evidence

Date: 2026-08-15

Status: two independent simulated MCUs each discarded the first canonical
`JobAbort` request before authentication or schedule application. Starting from
globally confirmed future start authority, the production browser worker made
the ambiguity visible, performed authenticated read-only `JobStatus`, observed
that the selected participant remained exactly `Confirmed`, retried the abort,
and completed the same sequence before advancing to the next participant. The
run ended with both actors exactly `Aborted`. A fresh-actor ordinary no-fault
completion regression also passed. This is localhost software evidence only.
The connected bare MKS TinyBee V1.0, workstation WLAN, GPIO, motors, motor
power, and every physical output or safety path remained untouched.

The implementation checkpoints are:

- `alumina-firmware` pre-application operation selector commit `bf34d4f`; and
- `alumina-interface` native and Chromium qualification commit `0675bee`.

The interface resolved the actively edited sibling CSGRS/Hyper workspace
directly. No published legacy CSGRS release was used and no sibling source was
modified or pinned.

## Exact fault contract

`alumina-sim-http --drop-operation-request NAME_OR_WIRE` is independent of the
existing applied-response selector. It decodes only enough of a complete,
canonical native request frame to identify its assigned operation. On the first
exact match it closes the HTTP connection before the request reaches the
authenticated fixture. Consequently the fixture does not consume the
authentication counter, mutate schedule state, run a reboot selector, or
produce a response. The one-shot state is then spent, so an exact retry reaches
normal authentication and application. Unrelated operations do not spend it.

Both actors selected `--drop-operation-request job-abort` and each logged
exactly once:

```text
fault injection: dropped unapplied request for operation 0x0504
```

This selector is deterministic test instrumentation. The production request
was authenticated when retried normally; the pre-application selector itself
does not claim to authenticate the raw frame it deliberately discards.

## State-machine proof

The focused participant test begins from an exact `Confirmed` report and then
replays this sequence:

```text
JobAbort (transport lost without application)
  -> second mutation rejected as ReconciliationRequired
  -> JobStatus reports Confirmed
  -> reconciliation clears
  -> JobAbort retry applies
  -> successful JobAbort response reports Aborted
```

The distributed coordinator test repeats that sequence for every participant.
It records lost requests, unchanged-state reconciliations, and applied retries
separately. Before moving to the next device, the current device must have one
lost abort, one authenticated status that still projects `Confirmed`, and one
successful abort retry. It terminates only when all three counts equal the
participant count and global phase is `Aborted`.

## Chromium contract and result

The browser driver mode `confirmed-abort-request-recovery` navigates to
`expect=cached-job-confirmed-abort-request-recovery`. It installs one strict
schema-V7 request and lets the ordinary production worker cache, prepare,
install, and confirm both participants. Only after the global `confirmed`
snapshot does the harness send one strict `stop_cached_job` command.

The expectation requires:

- exactly two retained one-failure observations in global phase `aborting`;
- a `fetch failed` transport error for each selected actor;
- two cache-complete participants whose schedule states are only `confirmed`
  or `aborted`;
- locally aborted participant counts exactly `0 -> 1` across those failures;
- later zero-failure recovery; and
- a terminal global `aborted` snapshot with both participants locally
  `aborted`, complete cache facts, retained local start cycles, and no error.

Two fresh `alumina-sim-http` actors listened on ports 8098 and 8099 with stable
identities `ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37
ppm and -41 ppm, worker generations 1 and 2, and boot IDs of sixteen `0x31`
bytes. The production application/worker bundle and harness were served on
`127.0.0.1:8097` to a fresh headless Chromium profile.

The fault-selected run passed after 393 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> aborted
```

Its selected browser epoch was `63,253,300,001 ns`, mapping to retained local
start cycles `128,497,258` and `122,677,712`. The first failure retained both
local views as `confirmed`. After unchanged-state status reconciliation and the
first successful retry, the second failure retained participant one as
`aborted` and participant two as `confirmed`. The second status/retry sequence
produced the exact all-aborted terminal. Both participants retained 127,264
accepted of 127,264 cache bytes, zero terminal failures, and no error.

Both actors were then replaced with fresh no-fault instances. The unchanged
ordinary `single` mode passed after 432 snapshots, traversed the full lifecycle
through `irrevocable -> complete`, and retained no failure observations. Its
epoch was `64,995,600,002 ns`, mapping to local cycles `112,615,587` and
`106,819,653`.

Representative commands were:

```console
# two alumina-firmware terminals
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 \
  --drop-operation-request job-abort
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-request job-abort

# alumina-interface, with the production bundle and Chromium CDP served
node tests/browser/read-cached-job-result.mjs 9224 \
  confirmed-abort-request-recovery
```

## Verification performed

The following passed on the recorded source:

- `cargo test --locked --offline` and warnings-denied all-target Clippy over
  `alumina-firmware` portable default members;
- the focused simulator selector tests and package-scoped formatting;
- `cargo test --workspace --locked --offline` in `alumina-interface`: 37
  application, 70 client, 125 core, and one integration test, plus the
  compile-fail documentation test;
- warnings-denied all-target workspace Clippy with dependency linting excluded;
- `scripts/audit-source-policy.sh`, Node driver syntax, and
  `wasm-tools validate` on the unchanged optimized production bundle;
- the dedicated two-participant abort-request-loss Chromium run; and
- the fresh-actor ordinary no-fault Chromium regression.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.

## Claims deliberately kept closed

This evidence closes one-shot initial `JobAbort` request non-delivery from
globally confirmed state while still before the abort guard. It does not cover
repeated loss, sustained outage through the guard, a request arriving at or
after the point of no return, reorder or duplication, browser crash/background
reliability, hardwired stop or E-stop behavior, ESP32 execution, physical
Wi-Fi/AP behavior, real SD-card durability, GPIO or bus timing, I2S/RMT/MCPWM/
ADC behavior, motor motion, multi-MCU electrical simultaneity, endstop or
interlock response, attended or cached-autonomous production policy,
production credentials, or any machine-arm/safety qualification. Wi-Fi stop
remains supplementary control traffic and cannot replace a physical safety
chain.
