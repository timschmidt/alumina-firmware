# M10 browser cached-job terminal-reattachment evidence

Date: 2026-08-15

Status: a fresh production browser worker reconstructed the same strict cached
job against two unchanged, already-complete simulated MCUs and terminated as
`retained_complete`. It preserved each authenticated local start cycle, carried
no replacement browser start epoch, reported no failure, and issued no new
prepare/install/confirm/start authority. This is loopback software/browser
evidence only. The connected bare MKS TinyBee V1.0, workstation WLAN, GPIO,
motors, motor power, and every physical output/safety path remained untouched.

The coordinated implementation checkpoints are:

- `alumina-firmware` protocol commit `a02a877`; and
- `alumina-interface` implementation/qualification commit `c7fda5b`.

The interface resolved the actively edited sibling CSGRS/Hyper workspace
directly. It did not use a published legacy CSGRS release and did not pin or
modify any sibling source.

## Exact retained identity contract

The old terminal schedule report discarded the boot-bound prepared token after
commit. An authenticated fresh owner could therefore see a valid complete
report but could not prove that it belonged to its exact compiled descriptor
without possessing the original browser-side commit.

This checkpoint replaces that wire contract without a compatibility decoder:

- schedule report V4 is a fixed 128-byte `ALMJSCH4` value;
- combined job status V3 is a fixed 368-byte `ALMJST03` value;
- bytes `96..128` of every schedule report retain the exact
  boot-and-descriptor-derived token before and after commit;
- a participant controller rejects any status whose retained token differs from
  its locally reconstructed descriptor token;
- before any new `JobPrepare`, the coordinator completes one read-only
  `JobStatus` round across all participants;
- only an all-participant exact `Complete` set becomes terminal
  `retained_complete` for a fresh owner; and
- mixed complete/empty state becomes `Faulted` before a schedule mutation can
  be emitted.

The retained token is identity evidence only. It does not reconstruct the old
browser commit, authorize confirmation, select a new epoch, or grant another
start. A schema-V7 `retained_complete` worker snapshot therefore requires
`target_ui_ns = null` while retaining each authenticated local start cycle.
Ordinary in-owner `complete` snapshots continue to require their non-null
selected UI epoch.

## Chromium qualification

The optimized application/worker bundle was served on `127.0.0.1:8097`. Two
independent `alumina-sim-http` processes listened on ports 8098 and 8099 with
device identities `ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, default +37 ppm
simulated drift, stable worker generations 1 and 2, and boot IDs of sixteen
`0x31` bytes. No operation-loss selector was active.

The ordinary `single` driver first completed job `2047934465` after 432
validated snapshots. Each participant retained 127,264 accepted of 127,264
cache bytes. The selected browser epoch was `52,068,200,002 ns`, mapping to
local start cycles `165,504,989` and `165,424,194`. Its global sequence was:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

Without restarting either simulator, the driver navigated to a new harness and
installed the identical strict compiled request into a replacement worker. The
dedicated `reattach` expectation passed after seven snapshots with:

```text
caching -> preparing -> retained_complete
```

The terminal snapshot retained the same local cycles `165,504,989` and
`165,424,194`, the same complete cache byte counts, complete schedule state,
worker generations, device identities, and boot IDs. It reported
`target_ui_ns = null`, zero consecutive failures, no error, no failure
observations, and no recovery flag. It never passed through `ready`, selected a
new epoch, or sent a worker start command.

The native coordinator regression independently records every scheduling
request during fresh-owner discovery. It accepts only one empty-body
`JobStatus` per participant, proves no schedule mutation follows exact terminal
reattachment, and separately proves a complete/empty split faults with no
mutation. This request-level assertion is the authority for the no-new-start
claim; the phase sequence alone is only browser-visible corroboration.

Representative commands were:

```console
# alumina-firmware terminals
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546

# same Chromium/CDP instance and unchanged simulator actors
node tests/browser/read-cached-job-result.mjs 9224 single
node tests/browser/read-cached-job-result.mjs 9224 reattach
```

## Verification performed

The following passed on the recorded implementation:

- `cargo test -p alumina-job --locked --offline`: 20 tests;
- `cargo clippy -p alumina-job --all-targets --locked --offline -- -D warnings`;
- `cargo test --workspace --locked --offline` in `alumina-interface`: 35
  application, 67 client, 125 core, and one integration test, plus the
  compile-fail documentation test;
- warnings-denied all-target interface workspace Clippy against one coherent
  current Hyper snapshot;
- package-scoped formatting and `git diff --check`;
- `scripts/audit-source-policy.sh`;
- `env -u NO_COLOR trunk build --release --locked --offline`;
- `wasm-tools validate dist/alumina-interface_bg.wasm` on the optimized
  6,042,337-byte artifact;
- Node driver syntax validation; and
- the ordinary-completion then replacement-worker Chromium sequence above.

HyperCurve continued to change concurrently after the coherent native/release
checks. The later fixture invocation observed only a new sibling dead-code
warning and still completed; no Hyper repository was edited, formatted, reset,
pinned, staged, committed, or intentionally diff-inspected by this work.

## License boundary

The changes are repository-owned under the existing MIT terms and use only the
accepted MIT/Apache-compatible dependency set. No GPL, AGPL, LGPL, SSPL,
Synthetos, SimpleFOC, or other copyleft implementation or dependency was copied
or introduced.

## Claims deliberately kept closed

This evidence does not establish mid-flight browser crash recovery, durable UI
job persistence, reattachment to prepared/installed/confirmed/running or mixed
nonterminal states, MCU reboot recovery, abort-response-loss recovery, response
reorder or duplication, sustained outage, background-tab reliability, ESP32
execution, physical Wi-Fi/AP behavior, real SD-card durability, GPIO or bus
timing, I2S/RMT/MCPWM/ADC behavior, motor motion, multi-MCU electrical
simultaneity, endstop/E-stop/interlock response, attended or cached-autonomous
production policy, production credentials, or any machine-arm/safety
qualification. The simulator's latch authority cannot authorize physical
output.

Applied pre-confirm abort-response loss was subsequently qualified in
[`M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md`](M10-BROWSER-CACHED-JOB-ABORT-RECOVERY.md);
the remaining broader abort boundaries listed above remain closed.
