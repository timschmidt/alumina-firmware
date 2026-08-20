# M10 cached-autonomous job admission and browser execution

Date: 2026-08-20

## Closed software boundary

The canonical `cached_autonomous` manifest policy now travels from the
authoritative browser/WASM CAM compiler through current schema-V11 worker state, exact
multi-MCU commits, the HTTP simulator, and both target firmware cores.

Admission is fail-closed at each independent authority boundary:

- CAM rejects cached-autonomous compilation unless the exact `ALMCFG06`
  identity contains `ConfigurationFlags::CACHED_AUTONOMOUS`;
- the browser worker decodes the canonical manifest and requires the same bit
  in every bound session's exact active configuration summary before cache I/O;
- core 0 requires the bit in its durably authorized configuration identity;
- core 1 independently derives `JobScheduleAdmission::autonomous_allowed` from
  its validated and authorized `RealtimeConfiguration`; and
- the simulator derives its admission fact from the same canonical
  representative configuration instead of a fixture-wide `true` constant.

`lease_expiry_cycle` remains finite and mandatory. Under cached-autonomous
policy it is the local maximum execution/energization horizon, not a claim that
the browser remains reachable. The later attended-only renewal protocol does
not alter this policy; see
[`M10-ATTENDED-LEASE-RENEWAL.md`](M10-ATTENDED-LEASE-RENEWAL.md).

## Browser and operator boundary

The live job panel defaults to `network_attended` and permits an explicit
pre-staging selection of `cached_autonomous`. The exact manifest is recompiled
when that selection changes. Replacement job snapshots now carry the decoded
network policy, and the rendering realm validates current schema V11 before display.
The panel explicitly states that Wi-Fi stop is not a safety chain.

The simulator fixture CLI and browser driver gained an explicit
`--network-policy cached-autonomous` / `autonomous` path. The ordinary control
continues to compile `network_attended`; the harness requires the expected
policy in every terminal snapshot rather than accepting either.

## Fresh two-MCU Chromium qualification

Two fresh localhost-only `alumina-sim-http` actors used stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, drifts +37 ppm and -41 ppm, the same
boot ID `[0x31; 16]`, and exact active configuration flags `0x00000003`.
Representative commands were:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41

node tests/browser/read-cached-job-result.mjs 9224 autonomous
```

Job `2047934465` published 127,264 bytes to each actor and passed after 434
validated replacement snapshots:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

Every snapshot retained `network_policy = cached_autonomous`. The selected
browser epoch was `43,732,400,002 ns`, mapping to local cycles `79,558,509` and
`79,557,537`. Both participants ended `Complete`, with zero failure
observations, no terminal error, and no recovery flag.

A second run used fresh actor processes, a fresh Chromium profile, and the
ordinary driver mode. It passed after 434 snapshots with
`network_policy = network_attended`, epoch `43,730,900,001 ns`, local cycles
`69,104,882` and `69,115,577`, and the same zero-failure terminal facts. This
control distinguishes the two canonical policies while preserving the prior
attended lifecycle.

## Bounded post-confirmation schedule-route outage

A third fresh-profile run armed each simulator to discard the next 24
canonical schedule requests after its successful `JobConfirm` response:

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37 \
  --drop-schedule-after-operation job-confirm \
  --drop-schedule-after-operation-count 24

target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41 \
  --drop-schedule-after-operation job-confirm \
  --drop-schedule-after-operation-count 24

node tests/browser/read-cached-job-result.mjs 9224 autonomous-outage
```

The production worker retained the exact confirmed cached job through all 48
failed authenticated schedule exchanges, with consecutive-failure evidence
from 1 through 48. Both actors' independent logs confirmed exactly 24 requests
were discarded before authentication or application. The worker recovered,
observed the first participant complete while the second remained confirmed,
crossed the exact global `irrevocable` state, and terminated `complete` after
436 validated snapshots. Both 127,264-byte publications remained complete and
the terminal failure count returned to zero. The shared epoch was
`43,732,800,002 ns`, mapping to local cycles `90,269,583` and `90,253,853`.

This is evidence for a bounded outage of the authenticated schedule-operation
route while unrelated loopback traffic and both locally clocked simulator
actors remained live. It is not evidence for browser-process loss, a total
endpoint outage, physical AP/radio loss, or autonomous electrical output.

All static, simulator, and browser traffic was loopback-only. Resolver policy
rejected non-loopback hosts, and all processes were stopped afterward.

## Verification

The implementation passed:

- 63 `alumina-sim` library tests and eight simulator HTTP-binary tests,
  including explicit cached-autonomous denial when configuration authority is
  removed;
- 76 `alumina-interface-client` tests and 47 application tests, including CAM
  policy-bit rejection and policy-distinct live manifest handoff;
- warnings-denied `wasm32-unknown-unknown` all-target Clippy;
- warnings-denied TinyBee V1 8 MB and 4 MB, T-Deck Pro, MKS ESP32 FOC V1,
  and current T-LoRa Pager stub target Clippy under the ESP toolchain; and
- optimized locked/offline Trunk assembly, `wasm-tools validate`, and gzip and
  Brotli integrity checks.

The qualified optimized WASM is 6,073,744 bytes, 2,690,004 bytes under gzip,
2,128,281 bytes under Brotli, and SHA-256
`9d1559d52b12d6acf1b0eaa270db04681fb07ed132c304eb6aadec2fb2081858`.

## License and moving-Hyper boundary

This work is repository-owned under the existing MIT OR Apache-2.0 and MIT
terms and adds no dependency. No GPL, AGPL, LGPL, SSPL, Synthetos, SimpleFOC,
or other copyleft implementation, source, or asset was copied or introduced.

HyperCurve and all sibling Hyper repositories remained user-owned and strictly
read-only. The browser artifact is one coherent build of their live workspace
state; no sibling was reset, formatted, staged, committed, pinned, or otherwise
stabilized.

## Deliberately open physical boundary

This closes exact software policy selection and admission, simulated
completion, and recovery from the bounded schedule-route outage above. It does
not qualify browser disappearance or AP loss over a real radio, an indefinite
or total endpoint outage, physical SD persistence,
power loss, ESP reset, safe-output electrical state,
E-stop/endstop/interlock latency, output timing, multi-board simultaneity,
motors, motor power, or autonomous motion. TinyBee and T-Deck Pro remain
non-armable or output-unqualified and therefore close physical `JobPrepare`
before this policy path. The connected bare TinyBee V1.0 was not flashed,
reset, opened over serial, contacted over Wi-Fi, or otherwise touched during
this qualification.
