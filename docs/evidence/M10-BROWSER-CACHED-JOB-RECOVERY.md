# M10 browser cached-job ambiguity-recovery evidence

Date: 2026-08-15

Status: the production browser worker completed one two-MCU cached job while
each independent simulator discarded one successful, already-applied mutation
response. A lost `StoragePutChunk` response was reconciled by publication
inspection and a lost `JobCommit` response was reconciled by schedule status.
The same browser qualification then passed without fault injection. This is
loopback software/browser evidence only. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and every physical output/safety
path remained untouched.

The coordinated implementation checkpoints are:

- `alumina-firmware` commit `1bedfa7`;
- `alumina-interface` commit `dbdfa9b`; and
- the current sibling CSGRS/Hyper workspace resolved directly by the interface.
  No published legacy CSGRS release is used.

## Applied-response-loss contract

The standalone HTTP simulator now accepts one optional
`--drop-operation-response NAME_OR_WIRE` selector. Canonical names cover storage
inspection/begin/chunk/finalize and job prepare/status/commit/confirm/abort/
cancel; an assigned decimal or hexadecimal `u16` wire value is also accepted.

The selector is deliberately narrower than the existing request-number fault:

- it recognizes only an exact `POST /api/v1/control` canonical native request,
  including exact frame length and request direction;
- the authenticated fixture processes the request before fault selection;
- a response is discarded only when HTTP status is 200 and its canonical
  same-operation native response has response direction and `StatusCode::Ok`;
- rejection, malformed traffic, foreign paths, nonmatching operations, and
  failed native responses do not consume the one-shot fault; and
- after the first successful match, all later responses are delivered normally.

This creates transport ambiguity without claiming that a rejected mutation was
applied. Focused simulator tests cover names and assigned wire values, reject
response-direction, trailing-byte, and foreign-path inputs, and require a
successful matching native response before selecting the drop.

## Two-ambiguity Chromium run

The existing optimized interface bundle was served from `127.0.0.1:8097`.
Two independent simulator actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, ports 8098 and 8099, and clock drifts
of +37 ppm and -41 ppm. Actor one dropped its first successful
`StoragePutChunk` response; actor two dropped its first successful `JobCommit`
response. Their logs confirmed the exact selected wire operations:

```text
fault injection: dropped applied response for operation 0x0904
fault injection: dropped applied response for operation 0x0503
```

The browser driver selected `expect=cached-job-recovery`. That expectation uses
one strict schema-V6 cached-job request, retains compact failure snapshots, and
passes only after at least two independent nonzero-failure observations,
subsequent zero-failure recovery, and a fully valid terminal snapshot.

The run observed exactly two independent transient failures:

1. In global phase `caching`, the worker reported one consecutive failure and
   `fetch failed`. Both participant cache machines were still in their
   partition/inspection path. The next mutation was chosen only after read-only
   storage inspection exposed the durable prefix created before the response
   was lost.
2. In global phase `installing`, the worker again reported one consecutive
   failure and `fetch failed`; participant one was installed while participant
   two was still installing. The coordinator issued read-only `JobStatus`,
   observed the already-installed commit, and advanced without assuming that
   the lost response meant failure.

After 434 validated snapshots the worker reached the full monotone sequence:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

The shared browser epoch was `43,727,900,002 ns`, mapping to local start cycles
`76,497,108` and `76,498,616`. Both participants retained 127,264 accepted of
127,264 cache bytes, terminal schedule phase `complete`, their exact local start
cycle, worker generations 1 and 2, and boot IDs of sixteen `0x31` bytes. The
terminal snapshot reported zero consecutive failures and no error.

## No-fault regression

Both simulator actors were restarted without a fault selector. The unchanged
single-request driver mode then passed in 432 validated snapshots with no
failure observations and no recovery flag. It traversed the same complete
phase sequence at shared epoch `43,527,100,001 ns`, mapping to local start
cycles `71,159,501` and `71,163,621`.

Representative commands for the faulted case were:

```console
# from alumina-firmware
cargo build -p alumina-sim --bin alumina-sim-http --locked --offline
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --drop-operation-response storage-put-chunk
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-operation-response job-commit

# from alumina-interface, with the release bundle and Chromium CDP already served
node tests/browser/read-cached-job-result.mjs 9224 recovery
```

The ordinary regression used the same driver with mode `single` after restarting
both actors without either `--drop-operation-response` option.

## Verification performed

The following passed on the recorded implementation:

- `cargo build -p alumina-sim --bin alumina-sim-http --locked --offline`;
- `cargo test -p alumina-sim --locked --offline`: 63 library tests and three
  simulator-binary tests;
- `cargo clippy -p alumina-sim --all-targets --no-deps --locked --offline -- -D warnings`;
- syntax checks for the Node driver and extracted browser module;
- the two-ambiguity Chromium run;
- the fresh no-fault Chromium regression;
- the interface native all-target workspace tests and warnings-denied Clippy;
  and
- `scripts/audit-source-policy.sh`, including native/WASM GPL-family and
  missing-license rejection.

## License and moving-Hyper boundary

The implementation is repository-owned under the repositories' existing MIT
terms and uses only the accepted MIT/Apache-compatible dependency set. No GPL,
AGPL, LGPL, SSPL, Synthetos, SimpleFOC, or other copyleft implementation or
dependency was copied or introduced.

The interface checks compile the actively edited local CSGRS/Hyper stack
directly. HyperCurve and every sibling Hyper repository remain user-owned and
strictly read-only: none was edited, formatted, reset, pinned, staged,
committed, or intentionally diff-inspected. Validation is retried only against
a coherent workspace snapshot if concurrent Hyper editing intersects a build.

## Claims deliberately kept closed

This evidence does not establish ESP32 execution, physical Wi-Fi/AP behavior,
background-tab reliability, response reorder or duplication, sustained outage,
lost confirm/abort reconciliation, real SD-card durability, GPIO or bus timing,
I2S/RMT/MCPWM/ADC behavior, motor motion, multi-MCU electrical simultaneity,
endstop/E-stop/interlock response, attended or cached-autonomous production
policy, production credentials, or any machine-arm/safety qualification. The
simulator's latch authority cannot authorize physical output.
