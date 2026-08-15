# M10 browser cached-job installing-stop evidence

Date: 2026-08-15

Status: the production browser worker accepted a stop while one simulated MCU
had installed its exact future commit and the other remained prepared but had
never received a commit. It sent canonical `JobAbort` to the installed actor,
sent canonical `JobCancel` to the never-installed actor, retained the exact
`Aborted`/`Cancelled` participant facts, and terminated as global `aborted`.
After the rendering realm cleared that terminal job, it staged a distinct job
on the same authenticated sessions and unchanged simulated MCU boots. Both
actors replaced their prior terminal state, selected a new shared epoch, and
completed. This is localhost software evidence only. The connected bare MKS
TinyBee V1.0, workstation WLAN, GPIO, motors, motor power, and every physical
output or safety path remained untouched.

The implementation checkpoint is `alumina-interface` commit `ae5ed87`. No
`aluminafw` source change was required; the existing authenticated simulator at
checkpoint `fe93c24` already implemented terminal-only job replacement and
canonical cancellation. The interface resolved the actively edited sibling
CSGRS/Hyper workspace directly. No published legacy CSGRS release was used and
no sibling source was modified or pinned.

## Exact installation-stop contract

Binding a distributed future start constructs a planned commit for every
participant atomically, but it does not claim that any MCU received that
commit. The participant controller records local commit authority only when it
actually emits `JobCommit`. A browser stop during global `installing` now
partitions participants by that fact:

```text
locally bound commit + Installing/Installed/Confirmed -> JobAbort
no locally bound commit + Preparing/Ready/Faulted     -> JobCancel
```

The coordinator does not declare the bound job safe merely because a planned
commit was never sent. A no-commit participant must report `Cancelled` (or
authoritatively report no correlated actor) before global abort can terminate.
An installed participant must report `Aborted` or `Expired`. Post-guard
participants remain irrevocable and are not relabelled as stopped.

The reported local start cycle follows the same authority boundary. A planned
commit does not create a local cycle fact. The UI receives a cycle only from a
participant controller that bound the emitted commit or from an exact device
schedule report containing committed policy. Consequently, the cancelled
participant in this run retained `local_start_cycle = null` rather than the
browser's unsent prediction.

Schema V9 retains its wire shape. Its independent snapshot validator now
requires every global `aborted` participant to have a complete immutable cache
and exactly one of:

- `aborted` or `expired` with a retained local start cycle; or
- `cancelled` with no local start cycle.

A ready, running, complete, faulted, cache-incomplete, or cycle-bearing
cancelled participant cannot be accepted as that terminal.

## Distinct-job reuse after terminal clear

The two-attempt qualification exposed and closed a separate initial-status
seam. A new coordinator begins with read-only `JobStatus`; an unchanged MCU is
allowed to retain the previous job's terminal report until a new prepare
replaces it. The participant controller now treats that report as empty for the
new target only when all of these are true:

- the pending operation is the initial read-only status request;
- the controller has no prior accepted report and no locally bound commit;
- the service and real-time records correlate a prepare ID distinct from the
  requested descriptor; and
- the old state is replaceable: both actors are cancelled with no schedule, or
  its schedule is `Aborted`, `Expired`, `Complete`, or `Faulted`.

An active foreign job remains an identity error. A report using the new
prepare ID with a substituted descriptor token also remains an identity error.
The rule therefore permits terminal reuse without weakening same-job
reattachment or collision detection.

## Chromium contract and result

The browser driver mode `installing-stop` navigates to
`expect=cached-job-installing-stop` and installs two distinct strict schema-V9
requests. The first attempt requests stop only after a snapshot contains
exactly one `installed` and one `ready` participant. It requires complete cache
facts, one `aborted` participant with a local cycle, one `cancelled` participant
without a cycle, no transport failures, and terminal global `aborted`. The
harness then clears that owner, stages the second request without reconnecting
or restarting either actor, and requires ordinary all-participant completion.

Two fresh no-fault `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, simulated drifts of +37 ppm and -41
ppm, normal 1 ms processing and 2 ms response delay, and the same boot ID
`[0x31; 16]`. No request or response fault selector was active.

Attempt one, job `2047934465`, passed after 386 validated snapshots and
traversed:

```text
caching -> preparing -> ready -> installing -> aborting -> aborted
```

Its selected browser epoch was `45,512,900,002 ns`. Connection/generation one
retained exact `aborted` and local cycle `145,923,234`. Connection/generation
two retained exact `cancelled` and no local cycle. Both retained 127,264
accepted of 127,264 cache bytes. The terminal had zero consecutive failures,
no error, no failure observation, and no recovery flag.

After terminal clear, attempt two, job `2047934466`, passed on the same
generations and boots after 60 validated snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

Its new epoch was `46,811,900,002 ns`, mapping to local cycles `147,222,234`
and `147,149,467`. Both schedules were exactly `complete`, both retained
127,264 accepted cache bytes, and the terminal again had zero failures and no
error. The second attempt therefore proves that cancellation left the
never-installed actor quiescent and that initial terminal reconciliation did
not weaken active-job exclusion.

Representative commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41

node tests/browser/read-cached-job-result.mjs 9224 installing-stop
```

## Verification performed

The following passed on the final implementation checkpoint:

- `cargo test --workspace --locked --offline --quiet`: 40 application, 75
  client, 125 core, one integration, and the compile-fail documentation test;
- warnings-denied native and `wasm32-unknown-unknown` all-target Clippy with
  dependency linting excluded;
- `scripts/audit-source-policy.sh`, Node driver syntax, and inline harness
  module syntax;
- optimized Trunk release assembly plus `wasm-tools validate`, gzip, and Brotli
  integrity checks; and
- the dedicated two-attempt production Chromium run described above.

Native tests prove exact per-participant abort/cancel operation selection,
terminal local phases, and absence of a fabricated cycle. Client tests accept
distinct cancelled and aborted terminal predecessors during initial status,
reject an active predecessor, preserve the same-ID descriptor-token rejection,
and independently validate the mixed terminal snapshot.

The optimized production WASM was 6,061,923 bytes, 2,687,044 bytes under gzip,
2,125,589 bytes under Brotli, and SHA-256
`a3ce18ef903e715e94f456c67abadb0064f2fa048a8d4afc65084fa24a40219f`.

## License and moving-Hyper boundary

The implementation and qualification are repository-owned under the existing
MIT OR Apache-2.0 and MIT terms. They add no dependency. No GPL, AGPL, LGPL,
SSPL, Synthetos, SimpleFOC, or other copyleft implementation, source, or asset
was copied or introduced.

HyperCurve and every sibling Hyper repository remained user-owned and strictly
read-only. They were compiled as a moving workspace boundary; none was edited,
formatted, reset, pinned, staged, committed, or intentionally diff-inspected.
Warnings emitted by transient coherent HyperCurve snapshots remained outside
the warnings-denied Alumina lint boundary.

## Claims deliberately kept closed

This evidence closes only the exact stop boundary with one installed and one
never-installed participant, plus same-session reuse after its terminal clear.
It does not cover a faulted participant split, full control/status outage,
request reordering or duplication, browser crash or background reliability,
hardwired stop or E-stop behavior, ESP32 execution, physical Wi-Fi/AP behavior,
real SD-card durability, GPIO or bus timing, I2S/RMT/MCPWM/ADC behavior, motor
motion, multi-MCU electrical simultaneity, endstop or interlock response,
attended or cached-autonomous production policy, production credentials, or
any machine-arm/safety qualification. A Wi-Fi stop remains supplementary
control traffic and cannot replace a hardwired physical safety chain.
