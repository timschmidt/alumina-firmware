# M10 browser cached-job confirmed safety-fault evidence

Date: 2026-08-19

Status: the production browser worker retained one MCU's canonical local
`SafetyStop` fault after both participants had confirmed, did not issue a
second abort mutation to that terminal actor, aborted the still-confirmed peer,
and terminated with the exact global `faulted` result and exact participant
states `faulted`/`aborted`. A separate fresh-actor run completed normally with
no fault selector.

This is localhost software evidence only. The simulator models completion of a
board-safe output transaction before it latches `SafetyStop`; no physical safe
output was driven or observed. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or
safety path remained untouched.

The implementation checkpoints are `alumina-firmware` commit `addf015` and
`alumina-interface` commit `4a7a0b1`. The interface resolved the actively
edited sibling CSGRS/Hyper workspace directly. No published legacy CSGRS
release was used and no sibling source was modified or pinned.

## Exact simulator boundary

`alumina-sim-http` now accepts the one-shot selector:

```text
--safety-stop-after-operation NAME_OR_WIRE
```

For the first matching canonical control operation whose native response
indicates success, the fixture performs this exact order:

1. dispatch and apply the selected operation normally;
2. delay and write its complete successful HTTP response to the browser;
3. consume the selector exactly once;
4. model a completed simulator-only safe-output transaction; and
5. call the existing canonical schedule `fault_safety_stop` transition.

The selected operation must match exactly. An unrelated operation does not
consume the selector, a failed native response cannot trigger it, and a second
matching operation cannot re-arm it. The final qualification selects
`JobConfirm` (`0x0509`) on actor one. That actor therefore returns its valid
confirmed report before its local schedule becomes `Faulted` with
`JobScheduleFault::SafetyStop`.

Simulator unit coverage proves exact trigger matching and one-shot behavior.
The cached-job lifecycle test proves the injected report is canonical
`Faulted`/`SafetyStop`, a repeated injection is rejected, and the ordinary
unfaulted lifecycle remains usable independently.

This selector is a simulator fault-injection seam, not an assertion that an
ESP backend has performed or verified its board-specific safe-output sequence.
The production `fault_safety_stop` contract continues to require its caller to
complete that transaction first.

## Coordinator recovery rule

The browser requests stop immediately after both participants are globally
confirmed. Its first `JobAbort` reaches actor one after that actor has latched
the modeled local safety stop. The simulator returns an authenticated HTTP
success carrying a native `Conflict` response and a complete canonical job
status body.

`ParticipantScheduleMachine` validates and retains that body before surfacing
the typed device-status error. The global coordinator consequently observes
the exact local `Faulted` state while preserving its bound commit and start
cycle. During abort cleanup it now:

- keeps the global phase `Aborting` while another committed participant can
  still be stopped;
- skips a repeat abort mutation to a terminal faulted participant;
- sends the exact `JobAbort` to the remaining confirmed peer; and
- returns global `Faulted`, not `Aborted`, only after every participant is in a
  terminal absent/stopped/completed/faulted state.

The dedicated native test starts two exact participants, substitutes the first
actor's canonical `SafetyStop` report for its pending abort result, verifies
the typed `Conflict`, proves the second request targets the other actor, and
requires terminal `Faulted` with retained `Faulted`/`Aborted` participant
states and no further request.

This terminal classification preserves the fault rather than treating it as a
successful distributed abort. It does not claim physical stop simultaneity or
that an arbitrary firmware/runtime fault implies safe outputs.

## Strict browser expectation

The `confirmed-safety-fault` driver mode selects
`cached-job-confirmed-safety-fault` in the production worker harness. It first
requires the ordinary exact cache, prepare, install, and confirm progression.
It then requests stop and admits success only when all of these facts hold:

- exactly one failure observation occurs in global `aborting`;
- its error contains the native `participant returned Conflict` cause;
- its participant states are exactly one `faulted` and one `confirmed`;
- both immutable cache objects are complete and both retained local start
  cycles are non-null;
- the next exact mutation aborts the still-confirmed peer;
- the terminal global phase is `faulted` with exactly one `faulted` and one
  `aborted` participant; and
- terminal failure count is zero, terminal error is absent, and bounded
  recovery was observed.

The harness's ordinary fail-fast handling for an unexpected `faulted` snapshot
remains active for every other expectation. Only this named expectation can
admit the exact faulted terminal described above.

## Final production-browser run

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Only actor one selected the post-confirmation safety stop.

Job `2047934465` passed after 389 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> faulted
```

Its selected browser epoch was `43,731,500,001 ns`, mapping to local cycles
`76,301,063` and `76,315,387`. Both actors retained 127,264 accepted of
127,264 cache bytes. The exact failure and terminal boundary was:

```text
failure 1: Conflict; Faulted / Confirmed
terminal:  count 0;  Faulted / Aborted
```

The failure text was `job response rejected: distributed schedule rejected:
schedule rejected: participant returned Conflict`. The terminal retained no
error and recorded recovery. Actor one logged exactly one
`fault injection: latched cached-job safety stop after operation 0x0509`; actor
two logged no fault injection.

Representative actor and driver commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --safety-stop-after-operation job-confirm

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41

node tests/browser/read-cached-job-result.mjs 9224 confirmed-safety-fault
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but no fault selector,
completed job `2047934465` after 434 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `43,832,900,001 ns`, mapping to local cycles
`81,285,304` and `81,282,171`. Both actors retained exact `Complete` state and
127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag.

Both browser runs used fresh Chromium profiles, localhost-only static and MCU
servers, the same finalized production artifact, and a resolver policy that
rejected non-loopback hosts. All localhost processes were stopped afterward.

## Verification performed

The following passed on the final implementation checkpoints:

- the simulator binary's eight HTTP fault-selector tests and the full locked,
  offline `alumina-firmware` default-member test suite;
- `cargo test --workspace --locked --offline` in `alumina-interface`: 42
  application, 76 client, 125 core, integration, and compile-fail documentation
  coverage;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- warnings-denied simulator and interface rustdoc plus the complete WASM test
  link set;
- `scripts/audit-source-policy.sh`, Node driver syntax, package-scoped
  formatting, and diff checks;
- optimized locked/offline Trunk assembly plus `wasm-tools validate`, gzip,
  and Brotli integrity checks; and
- the dedicated safety-fault Chromium run and fresh no-fault control above.

The optimized production WASM was 6,068,806 bytes, 2,689,022 bytes under gzip,
2,127,449 bytes under Brotli, and SHA-256
`80033d440735677ac5811df28a75f50ef3eb80dae50703b5d0fe6e846398fee7`.

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

This closes one exact simulator/worker case: a modeled post-confirmation local
`SafetyStop` on one participant while a peer remains remotely abortable. It
does not qualify other fault families or lifecycle points, simultaneous local
faults, faults after the abort guard, loss during this recovery, arbitrary
terminal mixtures, physical ESP behavior, real SD or Wi-Fi behavior, E-stop or
interlock latency, safe-output electrical state, motion, or motor power. Those
remain separate native, simulation, and ultimately instrumented HIL work.
