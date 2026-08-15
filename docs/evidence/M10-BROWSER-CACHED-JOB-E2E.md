# M10 exact browser cached-job end-to-end evidence

Date: 2026-08-15

Status: the optimized production browser worker compiled the representative
Hyper-backed CAM fixture, reconciled immutable cache objects through authenticated
native HTTP on two independent simulator processes, prepared both participant
actors, translated one shared browser epoch into two local MCU cycles, installed
and confirmed both schedules, crossed both abort guards, observed simulated
start latches, and reached exact completion. This is loopback software/browser
evidence only. The connected bare MKS TinyBee V1.0, workstation WLAN, motors,
motor power, GPIO, and every physical safety/output path remained untouched.

## Authority and artifact boundary

The run used worker schema V6 and two stable simulator identities:

- `ALUM-SIM:TINYBEE`, worker connection/generation `1/1`;
- `ALUM-SIM:TINYBEF`, worker connection/generation `2/2`.

Both exposed the same immutable simulated TinyBee capability digest
`4ea9bbf0b44c8664808b4e13b20294a0006371cfe1d843478a197b37b6be6cc7`
and the same active, job-authorized configuration digest
`e3e2c3324b3cae636ac30c9d93087bca5411ac7675622179ca1af256d31f7fbe`.
The active canonical configuration was 2,064 bytes with 31 records, seven
bindings, two stepper axes, a required safety binding, and zero FOC axes.

The native `alumina-cam-fixture` entry point used the same
`MachineCamWorkspace` as the visible UI. It emitted one strict
`WorkerCachedJobRequest`; it did not perform network access or invent a second
job schema. Each participant package contained:

- one 125,952-byte canonical `ALMBLK03` machine partition;
- the identical 1,312-byte canonical `ALMJMF02` global manifest; and
- 127,264 total published bytes after independent partition and manifest
  transactions.

The worker independently decoded every descriptor, upload plan, manifest,
participant identity, capability digest, and configuration digest before I/O.
The simulator independently decoded each real block and replayed
`ExecutionBlock::validate_motion` before accepting preparation.

## Observed browser lifecycle

`tests/browser/read-cached-job-result.mjs` generated the request natively,
loaded `tests/browser/worker-clock-harness.html?expect=cached-job` in headless
Chromium, and injected only the strict V6 command. The harness required both
sessions to be clock-qualified, capability-complete, configuration-active, and
identity-matched before staging.

The clean run emitted 432 validated job snapshots and reached this monotone
global sequence:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The shared worker epoch was `43,527,500,001 ns`. Its independently fitted clock
models selected local starts:

- connection 1: cycle `64,009,108`;
- connection 2: cycle `64,022,695`.

Both terminal participant snapshots reported cache artifact/phase `complete`,
127,264 accepted of 127,264 bytes, schedule phase `complete`, and a retained
local start cycle. The global terminal snapshot reported simulation-only mode,
zero consecutive failures, and no error.

## Liveness and evidence regressions closed

The browser run exposed and fixed four integration defects that narrower unit
tests did not cover:

1. simulator fixture generations now follow the worker's globally monotone
   generation allocation instead of assuming generation one for every device;
2. cached-job operations receive first choice on each worker tick, preventing
   coincident heartbeat/health/telemetry work from starving cache progress;
3. a valid start request is retained inside the worker until every bound session
   is idle and freshly qualified, removing a UI retry race without selecting the
   future epoch early; and
4. authenticated schedule evidence accepts a reachable `Primed -> Complete`
   observation when polling legitimately misses the intermediate `Running`
   report, while observation erasure and true regression remain rejected.

The last case has a focused native regression test. Terminal worker JSON is also
revalidated: a complete snapshot with incomplete cache bytes, a non-complete
participant schedule, or no retained local start cycle is rejected.

## Verification performed

The following passed during this checkpoint:

- `cargo test -p alumina-sim --locked --offline`: 63 tests;
- `cargo clippy -p alumina-sim --all-targets --no-deps --locked --offline -- -D warnings`;
- host-workspace and TinyBee firmware-target `cargo tree` license inventories:
  no GPL-family or missing-license entries;
- `cargo test -p alumina-interface-client --locked --offline`: 65 tests;
- `cargo test --workspace --all-targets --locked --offline`: 224 unit tests
  plus one integration test;
- native all-target and WASM-target workspace Clippy with dependency linting
  excluded and every warning denied;
- native `alumina-cam-fixture` check against a coherent current Hyper snapshot;
- `cargo check -p alumina-interface --target wasm32-unknown-unknown --locked --offline`;
- optimized `env NO_COLOR=false trunk build --release --offline`;
- `wasm-tools validate` on the optimized 6,039,234-byte module, `gzip -t`
  on every precompressed Gzip asset, and `brotli -t` on the 2,119,834-byte
  Brotli module;
- `scripts/audit-source-policy.sh`, including native/WASM GPL-family and
  missing-license rejection plus sibling checkout resolution;
- the two-simulator cached-job Chromium run above; and
- a separate ordinary Chromium qualification run requiring active
  configuration, exact capability, runtime health, and clock qualification.

The exact optimized module used by the final browser replay had SHA-256
`89b3b8c3237007de789d66774e83fa226491cc6f641ff92ca3e8e1f9b8718c1d`.
Its Gzip and Brotli representations had SHA-256
`ef2852395cd19077978174ed136c34bb43ec5f70991771233a8199ffe3277067`
and `939777e6dba59a6e5c81f9cf9a16cf8e8874924977f1ea64deeaa711c67ed4cc`,
respectively.

The standalone simulator accepts `--device-id 32_HEX_DIGITS`, allowing multiple
loopback MCUs to expose distinct stable identities. Its authenticated HTTP
fixture now serves the canonical active configuration plus storage, prepare,
status, commit, confirm, abort, and cancel operations through the production
native protocol.

## License and moving-Hyper boundary

All new implementation is repository-owned under the repositories' existing
MIT terms and uses the existing MIT/Apache-compatible dependency set. No copied
Synthetos, SimpleFOC, GPL, AGPL, LGPL, SSPL, or other copyleft implementation or
dependency was introduced.

The interface compiled the actively edited sibling CSGRS/Hyper stack directly.
HyperCurve remained user-owned and read-only. Some attempted checks intersected
transient mid-edit compile states; work continued elsewhere and validation was
retried only on coherent snapshots. No Hyper repository was edited, formatted,
reset, pinned, staged, committed, or intentionally diff-inspected.

## Claims deliberately kept closed

This evidence does not establish physical ESP32 execution, Wi-Fi/AP behavior,
SD-card durability, GPIO timing, I2S/RMT/MCPWM/ADC behavior, motor motion,
multi-MCU electrical simultaneity, endstop/E-stop/interlock response, browser
background reliability, production credentials, or any machine-arm/safety
qualification. The simulated latch is explicitly reported as simulated and
cannot authorize physical output.
