# M10 repeated cached-job lifecycle evidence

Date: 2026-08-15

Status: two distinct browser-authoritative cached-job attempts completed on two
long-lived simulated MCUs without rebooting either actor, reconnecting between
the attempts, or republishing content-addressed cache bytes. The first terminal
snapshot was retained and validated before the browser worker acknowledged a
local clear; the second attempt then prepared, installed, confirmed, primed,
observed, and completed at a fresh deterministic epoch. This is loopback
software/browser evidence only. The connected bare MKS TinyBee V1.0,
workstation WLAN, GPIO, motors, motor power, and all physical output/safety paths
remained untouched.

The coordinated implementation checkpoints are:

- `alumina-firmware` commit `0796303`;
- `alumina-interface` commit `b4310e7`.

## Replacement contract

The production target already requires terminal, quiescent core-0, core-1, and
schedule state before accepting a different descriptor. The standalone HTTP
simulator now mirrors the externally observable contract:

- an exact complete descriptor retry is idempotent in any state and returns the
  retained report;
- a different descriptor returns `Busy` while the current job is nonterminal;
- a different descriptor may replace a cancelled or schedule-terminal job;
- publication, stream, identity, and new-schedule validation complete before
  the retained actor changes; and
- a failed replacement leaves the previous terminal report intact.

The focused simulator regression first proves an active prepared job rejects a
different prepare ID. It completes that job, proves its exact terminal retry
does not reset state, attempts a replacement naming a missing partition and
observes `NotFound` with the old prepare ID still reported, then installs a valid
new prepare ID and observes a distinct prepared token.

Attempt identity and immutable storage identity deliberately remain separate.
A new logical execution uses a new nonzero prepare ID even when its canonical
partition and manifest content are unchanged. Rendering-realm
`clear_cached_job` removes only the bounded worker owner after evidence capture;
it is not a firmware erasure or replacement command.

## Uninterrupted two-job Chromium run

The existing optimized production worker bundle was served from
`127.0.0.1:8097`; two independent `alumina-sim-http` processes listened on
ports 8098 and 8099 with device identities `ALUM-SIM:TINYBEE` and
`ALUM-SIM:TINYBEF` and configured clock drifts of +37 ppm and -41 ppm. The
browser harness used `expect=cached-job-repeat`. Its native CAM driver created
strict schema-V6 requests for job IDs `2047934465` and `2047934466`, both bound
to the same two stable devices.

Job `2047934465` uploaded a 125,952-byte partition and 1,312-byte global
manifest to each MCU, for 127,264 accepted bytes per participant. It completed
after 432 validated snapshots with this global sequence:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

Its shared browser epoch was `43,526,700,002 ns`; the fitted device maps selected
local start cycles `93,422,208` and `93,419,669`. Both participant reports
retained complete cache and schedule state, exact start cycles, zero failures,
and simulated-latch authority.

After validating that terminal snapshot, the harness sent only the strict V6
worker `clear_cached_job` command and waited for the matching `job_removed`
event. It did not reload the page, replace the worker, reconnect either session,
or restart either simulator. Job `2047934466` then inspected the already
published objects, reused all 127,264 bytes on each participant, and traversed
the same complete phase sequence in 60 snapshots. Its new browser epoch was
`49,326,700,002 ns`, mapping to local start cycles `99,222,208` and
`99,219,669`. Worker generations remained `1` and `2`, both authentication boot
IDs remained sixteen bytes of `0x31`, and both terminal snapshots reported zero
consecutive failures and no error.

An additional ordinary `cached-job` run against the same still-live simulator
actors replaced the second terminal attempt with job `2047934465`, again reused
the complete cache, and reached the full terminal phase sequence in 60
snapshots. This regression preserved the original single-request harness and
driver behavior.

## Verification performed

The following passed on the recorded source:

- `cargo build -p alumina-sim --bin alumina-sim-http --locked --offline`;
- `cargo test -p alumina-sim --locked --offline`: 63 tests;
- `cargo clippy -p alumina-sim --all-targets --no-deps --locked --offline -- -D warnings`;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface`: 34 application, 65 client, 125 core, and one integration
  test;
- native all-target workspace Clippy with dependency linting excluded and every
  warning denied;
- syntax checks for the Node driver and extracted browser module;
- `scripts/audit-source-policy.sh`, including native/WASM GPL-family and
  missing-license rejection;
- the uninterrupted two-job Chromium run; and
- the subsequent single-job Chromium regression on the same simulator actors.

## License and moving-Hyper boundary

The changes are repository-owned under the existing MIT terms and use only the
accepted MIT/Apache-compatible dependency set. No GPL, AGPL, LGPL, SSPL,
Synthetos, SimpleFOC, or other copyleft implementation or dependency was copied
or introduced.

The interface checks compiled the actively edited local CSGRS/Hyper stack
directly. HyperCurve and every sibling Hyper repository remained user-owned and
strictly read-only: none was edited, formatted, reset, pinned, staged,
committed, or intentionally diff-inspected.

## Claims deliberately kept closed

This evidence does not establish ESP32 execution, Wi-Fi/AP behavior, SD-card
durability, GPIO or bus timing, I2S/RMT/MCPWM/ADC behavior, motor motion,
multi-MCU electrical simultaneity, endstop/E-stop/interlock response, browser
background reliability, production credentials, or any machine-arm/safety
qualification. The simulator's latch authority cannot authorize physical
output.
