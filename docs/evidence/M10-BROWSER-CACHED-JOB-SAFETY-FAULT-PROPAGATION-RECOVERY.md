# M10 browser cached-job safety-fault propagation recovery evidence

Date: 2026-08-20

Status: after ordinary authenticated status discovered one MCU's modeled local
`SafetyStop`, the remaining MCU applied the coordinator's automatic
`JobAbort` but its complete successful response was discarded. The production
browser worker retained the exact `Faulted`/`Confirmed` facts during the
ambiguous exchange, performed read-only status reconciliation before any later
mutation, recovered the peer's exact `Aborted` state, and terminated with
zero-error global `faulted`. No operator stop request was sent.

This is localhost software evidence only. The simulator models completion of a
board-safe output transaction before it latches `SafetyStop`; no physical safe
output was driven or observed. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or
safety path remained untouched.

The relevant checkpoints are `alumina-firmware` commit `addf015`, which contains
the combined one-shot simulator seams, and `alumina-interface` commit `c39d5bb`,
which binds the existing ambiguity machinery to a dedicated native invariant
and strict browser expectation. The production automatic-fault coordinator is
the preceding interface commit `fcb1b3f`.

The interface resolved the actively edited sibling CSGRS/Hyper workspace
directly. No published legacy CSGRS release was used and no sibling source was
modified or pinned.

## Exact ambiguity and reconciliation rule

Actor one uses `--safety-stop-after-operation job-confirm`. After returning its
successful confirmation response, it models a completed safe-output
transaction and latches the canonical local `SafetyStop`. Ordinary
post-confirmation `JobStatus` exposes that fault and automatically places the
global coordinator in fault cleanup.

Actor two uses `--drop-operation-response job-abort`. Its first matching
authenticated operation follows this exact order:

1. authenticate and dispatch the canonical `JobAbort` once;
2. apply the abort to the actor's schedule;
3. construct the complete successful native response;
4. consume the selector exactly once; and
5. close the HTTP exchange without writing the response.

The browser cannot infer application from the failed fetch. It abandons that
pending mutation, retains actor two's last authoritative `Confirmed` state,
and marks the participant as requiring reconciliation. The coordinator still
retains actor one's authoritative `Faulted` state and the global attempt stays
`Aborting`, not terminal.

The next operation for actor two must be `JobStatus`. Only its authenticated
report may advance actor two from retained `Confirmed` to exact `Aborted`.
Because all committed participants are then terminal under the latched fault
cleanup rule, the coordinator reports global `Faulted`; it does not replay the
ambiguous abort, mutate actor one, or misclassify the result as a successful
distributed abort.

The native two-variant helper proves the direct-response and lost-response
paths from identical confirmed/faulted starting state. In the lost path it
requires `abandon_pending`, checks retained `Faulted`/`Confirmed`, requires the
next request to be `JobStatus` for the same peer, and accepts terminal
`Faulted`/`Aborted` only after that report.

## Strict browser expectation

The `confirmed-safety-propagation-recovery` driver mode selects
`cached-job-confirmed-safety-propagation-recovery` in the production worker
harness. It sends no `stop_cached_job` command. Success requires:

- ordinary exact cache, prepare, install, and confirm progression;
- a zero-error `aborting` transition containing exactly one `faulted` and one
  `confirmed` participant;
- exactly one subsequent failure observation, still in `aborting`, still with
  exact `faulted`/`confirmed` facts, failure count one, and a `fetch failed`
  cause;
- complete immutable cache facts and nonnull local start cycles throughout;
- a later zero-error recovery observation; and
- exact terminal global `faulted` with one `faulted` and one `aborted`
  participant.

This expectation is distinct from the zero-failure automatic-propagation case
and the operator-requested native-`Conflict` case. An unexpected fault remains
fail-fast in ordinary job expectations.

## Final production-browser run

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Actor one selected the post-`JobConfirm` safety stop; actor two
selected loss of the first applied `JobAbort` response.

Job `2047934465` passed after 391 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> faulted
```

Its selected browser epoch was `43,730,200,001 ns`, mapping to local cycles
`68,357,413` and `67,176,426`. Both actors retained 127,264 accepted of
127,264 cache bytes. The exact recovery boundary was:

```text
automatic discovery:  count 0; Faulted / Confirmed
lost abort response:  count 1; Faulted / Confirmed; fetch failed
status reconciliation: count 0; Faulted / Aborted; global Faulted
```

The terminal retained no error and recorded recovery. Actor one logged exactly
one `fault injection: latched cached-job safety stop after operation 0x0509`.
Actor two logged exactly one
`fault injection: dropped applied response for operation 0x0504`.

Representative actor and driver commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --safety-stop-after-operation job-confirm

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-response job-abort

node tests/browser/read-cached-job-result.mjs 9224 \
  confirmed-safety-propagation-recovery
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but neither selector,
completed job `2047934465` after 434 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `43,731,000,001 ns`, mapping to local cycles
`75,754,129` and `74,595,936`. Both actors retained exact `Complete` state and
127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag.

Both runs used fresh Chromium profiles, localhost-only static and MCU servers,
the same finalized production artifact, and a resolver policy that rejected
non-loopback hosts. All localhost processes were stopped afterward.

## Verification performed

The following passed on the final interface qualification checkpoint:

- `cargo test --workspace --locked --offline`: 45 application, 76 client, 125
  core, integration, and compile-fail documentation coverage;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- warnings-denied interface rustdoc and the complete WASM all-target test link
  set;
- `scripts/audit-source-policy.sh`, Node driver syntax, package-scoped
  formatting, and diff checks;
- optimized locked/offline Trunk assembly plus `wasm-tools validate`, gzip,
  and Brotli integrity checks; and
- the dedicated propagation-recovery Chromium run and fresh no-fault control.

The optimized production WASM was unchanged at 6,069,041 bytes, 2,689,131
bytes under gzip, 2,127,679 bytes under Brotli, and SHA-256
`be46851c48e477509ab9f183bd43f115c4c49e9523719e77a40e52296ddba925`.

Tool versions were Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
151.0.7922.137, and Node.js 22.22.2.

## License, moving-Hyper, and physical boundary

The qualification is repository-owned under the existing MIT OR Apache-2.0
and MIT terms. It adds no dependency. No GPL, AGPL, LGPL, SSPL, Synthetos,
SimpleFOC, or other copyleft implementation, source, or asset was copied or
introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. The release was a coherent build of their live workspace state at
that build boundary; the recorded artifact hash identifies what Chromium ran.
No sibling was reset, formatted, staged, committed, pinned, or otherwise
stabilized, and later concurrent edits are intentionally allowed.

This closes one applied-response-loss case during automatic cleanup of one
modeled post-confirmation `SafetyStop`. It does not qualify initial abort
request loss, repeated or sustained loss, status loss during that cleanup,
guard crossing before reconciliation, other fault families or lifecycle
points, simultaneous local faults, arbitrary terminal mixtures, physical ESP
behavior, real SD or Wi-Fi behavior, E-stop or interlock latency, safe-output
electrical state, motion, or motor power. Those remain separate native,
simulation, and ultimately instrumented HIL work.
