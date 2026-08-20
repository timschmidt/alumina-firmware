# M10 browser cached-job safety-fault propagation request-recovery evidence

Date: 2026-08-20

Status: after ordinary authenticated status discovered one MCU's modeled local
`SafetyStop`, the remaining MCU discarded the coordinator's first automatic
`JobAbort` request before authentication or application. The production
browser worker retained exact `Faulted`/`Confirmed` facts, performed read-only
status reconciliation, proved the peer was still `Confirmed`, retried the exact
abort, and terminated with zero-error global `faulted` and exact
`faulted`/`aborted` participants. No operator stop request was sent.

This is localhost software evidence only. The simulator models completion of a
board-safe output transaction before it latches `SafetyStop`; no physical safe
output was driven or observed. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output or
safety path remained untouched.

The relevant checkpoints are `alumina-firmware` commit `addf015`, which contains
the combined one-shot simulator seams, and `alumina-interface` commit `e7f99f1`,
which extends the automatic-fault delivery matrix and adds the strict browser
expectation. The production automatic-fault coordinator remains interface
commit `fcb1b3f`.

The interface resolved the actively edited sibling CSGRS/Hyper workspace
directly. No published legacy CSGRS release was used and no sibling source was
modified or pinned.

## Exact non-application and retry rule

Actor one uses `--safety-stop-after-operation job-confirm`. After returning its
successful confirmation response, it models a completed safe-output
transaction and latches canonical local `SafetyStop`. Ordinary
post-confirmation `JobStatus` exposes that fault and automatically places the
global coordinator in fault cleanup.

Actor two uses `--drop-operation-request job-abort`. Its first matching HTTP
request follows this exact order:

1. parse enough canonical framing to identify `JobAbort`;
2. consume the selector exactly once;
3. close the HTTP exchange before authentication and native dispatch; and
4. leave actor two's schedule unchanged at `Confirmed`.

The browser treats the failed fetch as ambiguous because transport alone
cannot prove non-application. It abandons the pending mutation, retains actor
two's last authoritative `Confirmed` state, and marks that participant as
requiring reconciliation. Actor one's authoritative `Faulted` state remains
latched and global state stays `Aborting`.

The next operation for actor two must be `JobStatus`. Its authenticated report
confirms unchanged `Confirmed`; only then may the participant state machine
re-emit `JobAbort`. The native delivery-matrix test requires that retry to be
exactly equal to the lost device-targeted request. After the retry applies and
reports `Aborted`, the coordinator terminates as global `Faulted`. It does not
mutate actor one, invent application of the lost request, or classify cleanup
as a successful distributed abort.

The same native fixture also retains the direct-response and applied-response-
loss variants, proving the request-loss branch is selected by where delivery
stopped rather than by weakening the common terminal invariant.

## Strict browser expectation

The `confirmed-safety-propagation-request-recovery` driver mode selects
`cached-job-confirmed-safety-propagation-request-recovery` in the production
worker harness. It sends no `stop_cached_job` command. Success requires:

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

The browser-level facts deliberately share the ambiguity envelope with applied
response loss. The simulator's unapplied-drop log and native unchanged-status/
exact-retry assertions establish which side of that envelope this run took.
An unexpected fault remains fail-fast in ordinary job expectations.

## Final production-browser run

Two fresh `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. Actor one selected the post-`JobConfirm` safety stop; actor two
selected loss of the first `JobAbort` request before application.

Job `2047934465` passed after 392 validated snapshots and traversed:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> aborting -> faulted
```

Its selected browser epoch was `43,731,900,002 ns`, mapping to local cycles
`82,293,368` and `81,107,236`. Both actors retained 127,264 accepted of
127,264 cache bytes. The exact observable boundary was:

```text
automatic discovery: count 0; Faulted / Confirmed
lost abort request:  count 1; Faulted / Confirmed; fetch failed
retry completion:    count 0; Faulted / Aborted; global Faulted
```

The terminal retained no error and recorded recovery. Actor one logged exactly
one `fault injection: latched cached-job safety stop after operation 0x0509`.
Actor two logged exactly one
`fault injection: dropped unapplied request for operation 0x0504`.

Representative actor and driver commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --safety-stop-after-operation job-confirm

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-request job-abort

node tests/browser/read-cached-job-result.mjs 9224 \
  confirmed-safety-propagation-request-recovery
```

## Fresh no-fault control

Fresh actors with the same identities and drifts, but neither selector,
completed job `2047934465` after 434 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The selected epoch was `43,731,600,002 ns`, mapping to local cycles
`79,296,952` and `78,115,894`. Both actors retained exact `Complete` state and
127,264 accepted of 127,264 cache bytes. There were no failure observations,
no terminal error, and no recovery flag.

Both runs used fresh Chromium profiles, localhost-only static and MCU servers,
the same finalized production artifact, and a resolver policy that rejected
non-loopback hosts. All localhost processes were stopped afterward.

## Verification performed

The following passed on the final interface qualification checkpoint:

- `cargo test --workspace --locked --offline`: 46 application, 76 client, 125
  core, integration, and compile-fail documentation coverage;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- warnings-denied interface rustdoc and the complete WASM all-target test link
  set;
- `scripts/audit-source-policy.sh`, Node driver syntax, package-scoped
  formatting, and diff checks;
- optimized locked/offline Trunk assembly plus `wasm-tools validate`, gzip,
  and Brotli integrity checks; and
- the dedicated request-recovery Chromium run and fresh no-fault control.

The optimized production WASM was 6,069,233 bytes, 2,689,065 bytes under gzip,
2,127,474 bytes under Brotli, and SHA-256
`a679be527fff8d845f2112fdd86a64ae61cabd83c682ed6dad54aaccf9409bcb`.

The artifact changed from the preceding qualification because the user-owned
live Hyper workspace advanced between coherent builds; no Hyper repository was
stabilized or altered by Alumina work. The hash identifies the exact boundary
executed by both Chromium runs.

Tool versions were Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
151.0.7922.137, and Node.js 22.22.2.

## License, moving-Hyper, and physical boundary

The qualification is repository-owned under the existing MIT OR Apache-2.0
and MIT terms. It adds no dependency. No GPL, AGPL, LGPL, SSPL, Synthetos,
SimpleFOC, or other copyleft implementation, source, or asset was copied or
introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. The release was a coherent build of their live workspace state at
that build boundary. No sibling was reset, formatted, staged, committed,
pinned, or otherwise stabilized, and later concurrent edits are intentionally
allowed.

This closes one initial pre-application abort-request-loss case during
automatic cleanup of one modeled post-confirmation `SafetyStop`. It does not
qualify repeated or sustained request loss, status loss during cleanup, guard
crossing before retry, other fault families or lifecycle points, simultaneous
local faults, arbitrary terminal mixtures, physical ESP behavior, real SD or
Wi-Fi behavior, E-stop or interlock latency, safe-output electrical state,
motion, or motor power. Those remain separate native, simulation, and
ultimately instrumented HIL work.
