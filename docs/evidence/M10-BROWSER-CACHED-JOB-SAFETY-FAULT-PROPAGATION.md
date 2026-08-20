# M10 browser cached-job safety-fault propagation evidence

Date: 2026-08-20

Status: the production browser worker discovered one MCU's canonical local
`SafetyStop` during its ordinary post-confirmation status round, automatically
entered peer cleanup without an operator stop request, aborted the remaining
confirmed participant, and terminated with exact global `faulted` and exact
participant states `faulted`/`aborted`. A separate fresh-actor run completed
normally with no fault selector.

This is localhost software evidence only. The simulator models completion of a
board-safe output transaction before it latches `SafetyStop`; no physical safe
output was driven or observed. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or
safety path remained untouched.

The relevant implementation checkpoints are `alumina-firmware` commit
`addf015` for the one-shot simulator injection seam and `alumina-interface`
commit `fcb1b3f` for automatic coordinator propagation. The interface resolved
the actively edited sibling CSGRS/Hyper workspace directly. No published
legacy CSGRS release was used and no sibling source was modified or pinned.

## Automatic coordinator rule

Every accepted participant response first passes through the existing strict
participant state machine. The coordinator then inspects the retained
authoritative participant phases even when that state machine returns a typed
non-OK device result. Observing any exact `Faulted` phase permanently latches
fault cleanup for that distributed attempt:

- before any control commit is bound, cleanup becomes cancellation and every
  prepared actor must become absent, cancelled, or faulted;
- once a target epoch and commits are bound, cleanup becomes abort and only
  participants with remaining abort authority receive `JobAbort`;
- a participant already faulted is never treated as a successful stop and is
  not sent a repeat terminal mutation;
- global state remains nonterminal while another participant is still
  abortable or must be status-reconciled; and
- after exact cleanup, the global result is `Faulted`, never `Aborted` or
  `Cancelled`, preserving the initiating fault as the attempt outcome.

The ordinary worker status queue remains active after the first report. In the
qualified post-confirmation case it therefore records the exact intermediate
state `Faulted`/`Confirmed`, sends one abort only to the still-confirmed peer,
and admits the final `Faulted`/`Aborted` set.

Two native coordinator tests cover both sides of the commit boundary. One
observes a fault after both actors confirm and proves automatic peer abort with
no call to the operator-stop entry point. The other observes a precommit fault,
requires cancellation operations for both prepared actors, and proves the
terminal global result remains faulted. Existing explicit-stop, loss,
duplicate, stale-response, split, and normal-completion tests remain green.

## Strict browser expectation

The `confirmed-safety-propagation` driver mode selects
`cached-job-confirmed-safety-propagation` in the production worker harness. It
sends no `stop_cached_job` command. Success requires all of these facts:

- ordinary exact cache, prepare, install, and confirm progression completes;
- no cached-job transport or protocol failure is observed and no recovery flag
  is set;
- an intermediate zero-error global `aborting` snapshot contains exactly one
  `faulted` and one `confirmed` participant;
- both immutable cache objects are complete and both retained local start
  cycles are non-null in that snapshot;
- the coordinator aborts the remaining confirmed participant; and
- terminal global state is zero-error `faulted` with exactly one `faulted` and
  one `aborted` participant.

The harness still fails fast on an unexpected fault for every other ordinary
expectation. This named case does not weaken normal success criteria and is
separate from `confirmed-safety-fault`, which deliberately exercises an
operator stop and the resulting native `Conflict` recovery path.

## Final production-browser run

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Only actor one selected the one-shot post-`JobConfirm` safety
stop.

Job `2047934465` passed after 389 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> faulted
```

Its selected browser epoch was `43,732,400,002 ns`, mapping to local cycles
`81,755,075` and `80,574,113`. Both actors retained 127,264 accepted of
127,264 cache bytes. There were zero failure observations, no terminal error,
and no recovery flag. The exact automatic boundary was:

```text
intermediate: Faulted / Confirmed
terminal:     Faulted / Aborted
```

Actor one logged exactly one
`fault injection: latched cached-job safety stop after operation 0x0509`;
actor two logged no fault injection. Representative actor and driver commands
were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --safety-stop-after-operation job-confirm

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41

node tests/browser/read-cached-job-result.mjs 9224 \
  confirmed-safety-propagation
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but no fault selector,
completed job `2047934465` after 434 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `43,732,900,002 ns`, mapping to local cycles
`69,565,124` and `68,385,292`. Both actors retained exact `Complete` state and
127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag.

Both browser runs used fresh Chromium profiles, localhost-only static and MCU
servers, the same finalized production artifact, and a resolver policy that
rejected non-loopback hosts. All localhost processes were stopped afterward.

## Verification performed

The following passed on the final interface implementation checkpoint:

- `cargo test --workspace --locked --offline`: 44 application, 76 client, 125
  core, integration, and compile-fail documentation coverage;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- warnings-denied interface rustdoc and the complete WASM all-target test link
  set;
- `scripts/audit-source-policy.sh`, Node driver syntax, package-scoped
  formatting, and diff checks;
- optimized locked/offline Trunk assembly plus `wasm-tools validate`, gzip,
  and Brotli integrity checks; and
- the dedicated automatic-propagation Chromium run and fresh no-fault control
  above.

The optimized production WASM was 6,069,041 bytes, 2,689,131 bytes under gzip,
2,127,679 bytes under Brotli, and SHA-256
`be46851c48e477509ab9f183bd43f115c4c49e9523719e77a40e52296ddba925`.

Tool versions were Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
151.0.7922.137, and Node.js 22.22.2.

## License, moving-Hyper, and physical boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. The release was a coherent build of their live workspace state at
that build boundary; the recorded artifact hash identifies what Chromium ran.
No sibling was reset, formatted, staged, committed, pinned, or otherwise
stabilized, and later concurrent edits are intentionally allowed.

This closes one automatic simulator/worker case: an authenticated ordinary
status report exposes a modeled post-confirmation local `SafetyStop` while its
peer remains remotely abortable. It does not qualify other fault families or
lifecycle points, simultaneous local faults, faults after the abort guard,
loss during automatic cleanup, arbitrary terminal mixtures, physical ESP
behavior, real SD or Wi-Fi behavior, E-stop or interlock latency, safe-output
electrical state, motion, or motor power. Those remain separate native,
simulation, and ultimately instrumented HIL work.
