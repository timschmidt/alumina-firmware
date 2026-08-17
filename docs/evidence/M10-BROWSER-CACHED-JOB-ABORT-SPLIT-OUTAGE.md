# M10 browser cached-job abort-split outage evidence

Date: 2026-08-15

Status: one simulated MCU applied the requested pre-guard `JobAbort`, while a
second simulated MCU repeatedly discarded that same mutation before
authentication or schedule application until its guard closed. Authenticated
read-only `JobStatus` remained available on both actors. The production browser
worker retained the accepted stop, observed the exact `Aborted`/`Complete`
split, and terminated under schema V9 as `split_after_stop_request` instead of
polling forever or misreporting global abort/completion. A fresh-actor ordinary
no-fault run remained `complete`. This is localhost software evidence only, not
a full network-outage or safety qualification. The connected bare MKS TinyBee
V1.0, workstation WLAN, GPIO, motors, motor power, and every physical output or
safety path remained untouched.

The implementation checkpoints are:

- `alumina-firmware` bounded pre-application operation-outage selector commit
  `a4772a8`; and
- `alumina-interface` exact split terminal, schema-V9 worker/UI contract, and
  Chromium qualification commit `d748081`.

The interface resolved the actively edited sibling CSGRS/Hyper workspace
directly. No published legacy CSGRS release was used and no sibling source was
modified or pinned.

## Exact fault topology

Both simulator actors began on fresh boots after the browser's stop command was
configured to wait for exact global `confirmed` state:

- `ALUM-SIM:TINYBEE` on port 8098, +37 ppm, had no fault selector and accepted
  its first authenticated abort before the guard;
- `ALUM-SIM:TINYBEF` on port 8099, -41 ppm, used
  `--drop-operation-request job-abort --drop-operation-request-count 10000`;
- both retained normal 1 ms processing and 2 ms response delay; and
- all status, heartbeat, health, capability, configuration, telemetry, cache,
  and schedule-observation requests remained available.

The second actor logged exactly 18 matching dropped requests. Each connection
was closed before the authenticated fixture could consume its counter, apply
the abort, run a reboot selector, or form a response. The 10,000-request bound
was not exhausted: the schedule crossed the abort guard before a nineteenth
retry could remain legal. The first actor logged no dropped abort and reported
exact `Aborted`; the second never reported `Aborted` and eventually reported
exact `Complete`.

This asymmetry is deliberate. The prior abort-guard-outage qualification proved
all-participant completion after no abort applied. This qualification proves
the more hazardous mixed case after only a strict subset of participants
stopped.

## Exact terminal contract

The distributed coordinator now derives `SplitAfterAbort` only when every
participant has reached one of these exact terminal schedule facts:

```text
stopped side:  Aborted | Expired
executed side: Complete
```

At least one participant must be in each class. A confirmed, priming, primed,
running, faulted, cancelled, empty, or merely prepared participant keeps the
coordinator nonterminal. All-aborted remains `Aborted`; all-complete after an
accepted stop remains `CompletedAfterStopRequest`.

Worker schema V9 projects the mixed terminal as
`split_after_stop_request`. Independent snapshot validation requires:

- the bound non-null browser epoch;
- complete content-addressed cache facts for every participant;
- a retained local start cycle for every participant;
- only `aborted`, `expired`, or `complete` local schedule phases; and
- at least one stopped and one completed participant.

The terminal is clearable but not stoppable. The UI renders a red warning:

```text
The stop split the job: some participants stopped while others crossed the
point of no return and completed. Treat machine state as indeterminate; Wi-Fi
is not a safety chain.
```

Native tests replay an applied abort on the first participant, ambiguous loss
on the second, second-participant execution/completion, and the mandatory
status reconciliation. They prove the coordinator reaches one terminal split,
emits no further request, and retains the exact local phases. Client tests
reject a split snapshot unless both terminal classes and every authority fact
are present.

## Chromium contract and result

The browser driver mode `abort-split-outage` navigates to
`expect=cached-job-abort-split-outage`. It installs one strict schema-V9 request
and lets the production worker cache, prepare, install, and confirm both
participants. Only after the global `confirmed` snapshot does the harness send
one strict `stop_cached_job` command.

The expectation requires:

- actor one to become and remain exactly `aborted`;
- repeated one-failure abort-fetch observations for actor two;
- zero mutation of actor two to `aborted`;
- authenticated recovery to zero failures between retries;
- a visible transition through global `irrevocable`;
- an exact terminal participant set containing one `aborted` and one
  `complete`; and
- global `split_after_stop_request`, a retained shared epoch/local cycles,
  complete cache facts, zero terminal failures, and no error.

The fault-selected Chromium 147 run passed after 434 validated snapshots and
traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> irrevocable ->
split_after_stop_request
```

Its selected browser epoch was `45,393,200,002 ns`. Actor one retained local
start cycle `114,043,126` and exact `aborted`; actor two retained cycle
`113,891,323` and exact `complete`. Both retained 127,264 accepted of 127,264
cache bytes. The browser observed repeated one-failure `fetch failed` snapshots
with the exact `aborted`/`confirmed` split, then zero-failure recovery. The
terminal had zero consecutive failures and no error. Simulator logs contained
exactly 18 dropped unapplied aborts on actor two and none on actor one.

Both actors were then replaced by fresh no-fault instances. The unchanged
ordinary `single` mode passed after 431 snapshots through
`irrevocable -> complete`, with no failure observations. Its epoch was
`45,703,300,001 ns`, mapping to local cycles `72,329,183` and `72,264,483`.
Both schedules were `complete`; neither split terminal was emitted.

Representative commands were:

```console
# actor one: ordinary
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37

# actor two: abort mutation outage only
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-request job-abort \
  --drop-operation-request-count 10000

# alumina-interface, with the production bundle and Chromium CDP served
node tests/browser/read-cached-job-result.mjs 9224 abort-split-outage
```

## Verification performed

The following passed on the implementation checkpoint:

- `cargo test --workspace --locked --offline --quiet` on the final source: 39
  application, 72 client, 125 core, one integration, and the compile-fail
  documentation test;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy on the
  final source, with dependency linting excluded;
- `scripts/audit-source-policy.sh`, Node driver syntax, and inline harness
  module syntax;
- optimized Trunk release assembly plus `wasm-tools validate`, gzip, and Brotli
  integrity checks;
- the dedicated two-participant abort-split Chromium run; and
- the fresh-actor ordinary no-fault Chromium regression.

The optimized production WASM was 6,049,322 bytes, 2,681,197 bytes under gzip,
2,122,526 bytes under Brotli, and SHA-256
`80ab484e96e9cbd24937f4f529886d4053f69173fbeb830c997609ba7dc99d88`.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.

## Claims deliberately kept closed

This evidence closes one exact mixed `Aborted`/`Complete` result after an
accepted browser stop and bounded repeated abort-mutation loss on a strict
participant subset. It does not cover full control/status outage, request
reordering or duplication, a mix involving faulted, cancelled, or
never-installed actors, browser crash or background reliability, hardwired stop
or E-stop behavior,
ESP32 execution, physical Wi-Fi/AP behavior, real SD-card durability, GPIO or
bus timing, I2S/RMT/MCPWM/ADC behavior, motor motion, multi-MCU electrical
simultaneity, endstop or interlock response, attended or cached-autonomous
production policy, production credentials, or any machine-arm/safety
qualification. A Wi-Fi stop remains supplementary control traffic and cannot
replace a hardwired physical safety chain.
