# M10 browser cached-job abort-guard outage evidence

Date: 2026-08-15

Status: two independent simulated MCUs repeatedly discarded canonical
`JobAbort` requests before authentication or schedule application while
authenticated read-only `JobStatus` remained available. The production browser
worker accepted one stop request after both participants had granted future
start authority, retained that intent through repeated mutation failures,
observed the abort guard close, and ended with an explicit
`completed_after_stop_request` terminal after both cached schedules completed.
No abort reached either fixture. A fresh-actor ordinary no-fault completion
regression remained `complete`, proving that the missed-stop terminal is not an
alias for every completion. This is localhost software evidence only. It is not
a full network-outage qualification. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or safety
path remained untouched.

The implementation checkpoints are:

- `alumina-firmware` bounded pre-application operation-outage selector commit
  `a4772a8`; and
- `alumina-interface` worker schema-V8 missed-stop terminal and qualification
  commit `60377a3`.

The interface resolved the actively edited sibling CSGRS/Hyper workspace
directly. No published legacy CSGRS release was used and no sibling source was
modified or pinned.

## Exact fault contract

`alumina-sim-http --drop-operation-request NAME_OR_WIRE` still defaults to one
matching dropped request. `--drop-operation-request-count N` changes that bound
to a required nonzero count and is rejected unless the operation selector is
also present. Only an exact, canonical operation match spends the count.

Each selected request is closed before it reaches the authenticated fixture.
The fixture therefore does not consume an authentication counter, apply the
schedule mutation, run a reboot selector, or produce a response. Unrelated
requests, including `JobStatus`, do not spend the count. This selector is
deterministic test instrumentation; it deliberately provides mutation-only
loss rather than a general link failure.

Both actors selected:

```text
--drop-operation-request job-abort --drop-operation-request-count 10000
```

The first actor logged and discarded 18 matching requests and the second actor
logged and discarded one. Neither reached its configured bound, because the
guard closed before another remote abort was legal. No actor ever reported or
applied `Aborted`.

## State-machine and UI contract

Worker schema V8 distinguishes three exact successful-execution projections:

```text
ordinary owned completion             -> complete
fresh-owner exact terminal discovery  -> retained_complete
completion after an accepted stop     -> completed_after_stop_request
```

The worker records stop intent only after `begin_abort()` accepts it for a
bound future job. While the guard remains open, every lost abort request forces
read-only status reconciliation before another mutation. If status continues
to report `Confirmed`, retry remains legal. Once status reports `Primed` or a
later irrevocable state, the coordinator cannot revoke the already granted
local authority. It continues observing the exact job instead of fabricating an
abort, and projects `completed_after_stop_request` only after every participant
has exact complete cache and schedule facts, retained local start cycles, and
the bound UI epoch.

The terminal is retained even after transport failures recover to zero. The UI
renders it as a terminal warning rather than an ordinary success or a still
stoppable job:

```text
The job completed after a stop request crossed the abort point of no return.
Treat the stop as missed; Wi-Fi is not a safety chain.
```

Native projection tests keep ordinary completion, retained completion, and
missed-stop completion distinct. Snapshot validation rejects the new terminal
without the exact bound epoch and all-participant complete facts.

## Chromium contract and result

The browser driver mode `abort-guard-outage` navigates to
`expect=cached-job-abort-guard-outage`. It installs one strict schema-V8 request
and lets the production worker cache, prepare, install, and confirm both
participants. Only after the global `confirmed` snapshot does the harness send
one strict `stop_cached_job` command.

The expectation requires:

- repeated retained one-failure observations caused by failed abort fetches;
- complete cache facts and only confirmed-or-later schedule facts;
- no participant ever becoming `aborted`;
- an observed transition through global `irrevocable`;
- later zero-failure transport recovery; and
- an exact `completed_after_stop_request` terminal with both local schedules
  `complete`, retained local start cycles, and no error.

Two fresh `alumina-sim-http` actors listened on ports 8098 and 8099 with stable
identities `ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37
ppm and -41 ppm, normal 1 ms processing and 2 ms response delay, and the
10,000-request selector above. The production application/worker bundle and
harness were served on `127.0.0.1:8097` to a fresh headless Chromium 147
profile.

The fault-selected run passed after 432 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> irrevocable ->
completed_after_stop_request
```

Its selected browser epoch was `51,406,300,002 ns`, mapping to retained local
start cycles `96,298,842` and `90,876,643`. Both participants retained 127,264
accepted of 127,264 cache bytes and exact local `complete` schedule phases. The
browser retained repeated one-failure `fetch failed` observations while the
first participant remained selected for abort reconciliation; its later
failure evidence included the split `primed`/`confirmed` view at the guard.
The terminal retained zero consecutive failures and no error without erasing
the missed-stop phase. Simulator logs contained exactly 18 and one dropped
unapplied `JobAbort` requests, respectively.

Both actors were then replaced with fresh no-fault instances. The unchanged
ordinary `single` mode passed after 412 snapshots, traversed the full lifecycle
through `irrevocable -> complete`, and retained no failure observations. Its
epoch was `51,295,900,002 ns`, mapping to local cycles `88,614,029` and
`82,923,391`. It did not produce the missed-stop terminal.

Representative commands were:

```console
# each alumina-firmware actor, with its own bind/device/drift arguments
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 \
  --drop-operation-request job-abort \
  --drop-operation-request-count 10000

# alumina-interface, with the production bundle and Chromium CDP served
node tests/browser/read-cached-job-result.mjs 9224 abort-guard-outage
```

## Verification performed

The following passed on the recorded source:

- `cargo test --locked --offline` and warnings-denied all-target Clippy over
  `alumina-firmware` portable default members;
- `cargo test --workspace --locked --offline --quiet` in `alumina-interface`:
  38 application, 71 client, 125 core, and one integration test, plus the
  compile-fail documentation test;
- warnings-denied native and `wasm32-unknown-unknown` Clippy with dependency
  linting excluded;
- `scripts/audit-source-policy.sh`, Node driver syntax, and inline harness
  module syntax;
- optimized Trunk release assembly plus `wasm-tools validate`, gzip, and Brotli
  compression checks;
- the dedicated two-participant abort-mutation-outage Chromium run; and
- the fresh-actor ordinary no-fault Chromium regression.

The optimized production WASM was 6,043,330 bytes, 2,120,996 bytes under
Brotli, and SHA-256
`a45af95a448ea8fcb58ebe27e3c1e2aae41a8274d98bd3f7c32cbb2f24001c0c`.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.

## Claims deliberately kept closed

This evidence closes bounded repeated loss of the abort mutation from globally
confirmed state through the point of no return, with status reads still
available. It proves an explicit terminal report when a requested Wi-Fi stop
misses that point. It does not cover full control/status outage, request
reordering or duplication, mixed partial-abort/partial-complete outcomes,
browser crash or background reliability, hardwired stop or E-stop behavior,
ESP32 execution, physical Wi-Fi/AP behavior, real SD-card durability, GPIO or
bus timing, I2S/RMT/MCPWM/ADC behavior, motor motion, multi-MCU electrical
simultaneity, endstop or interlock response, attended or cached-autonomous
production policy, production credentials, or any machine-arm/safety
qualification. A Wi-Fi stop remains supplementary control traffic and cannot
replace a hardwired physical safety chain.
