# M10 attended-job worker-loss reattachment

Date: 2026-08-20

## Closed software boundary

A network-attended job now has an explicit replacement-owner outcome after the
browser control worker disappears. The production harness terminates the worker
only after both independently clocked actors report `Running`, sends no browser
traffic for twelve seconds, then constructs a new production worker. The actors
retain only their finite local lease authority and therefore fault locally
before the longer exact job can complete.

The replacement worker repeats public identity, authenticated session, clock,
configuration, capability, and immutable-cache reconciliation. Its distributed
coordinator completes the entire passive initial `JobStatus` round before a
first observed fault can activate peer cleanup. Two same-descriptor terminal
faults therefore remain two authenticated facts rather than one fault plus one
unobserved participant.

Worker schema V12 admits this terminal reconstruction without weakening active
state. It exposes the MCU-reported start and lease cycles, but the replacement
owner has no authority to claim the old browser-selected UI epoch, commit,
greatest authorized renewal, or renewal-round history. Those browser-only facts
are `null`/zero. An active or irrevocable snapshot with a reported lease and no
browser authority still fails validation.

## Fresh optimized Chromium run

All traffic was restricted to `127.0.0.1`. Two fresh simulator actors used
stable identities `ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, with +37 ppm and
-41 ppm modeled drift. The physical TinyBee and workstation WLAN were not
contacted.

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41
node tests/browser/read-cached-job-result.mjs 9224 worker-suspension
```

The schema-V12 run passed after 441 validated cached-job snapshots. Before
termination, the retained snapshot had the original UI epoch
`43,729,000,001 ns`, one completed renewal round, and both actors running.
After replacement it had no UI epoch, zero claimed renewal rounds, zero
transport-failure observations, and exact global `faulted` state:

| MCU | local start | lease before loss | browser authority before loss | reported after reattach | browser authority after reattach |
| --- | ---: | ---: | ---: | ---: | --- |
| `ALUM-SIM:TINYBEE` | 64,685,522 | 69,085,622 | 69,085,622 | 69,085,622 | unknown (`null`) |
| `ALUM-SIM:TINYBEF` | 64,682,557 | 69,082,657 | 69,082,657 | 69,082,657 | unknown (`null`) |

The harness independently requires every final start and reported lease to
equal the corresponding all-running pre-termination fact. Its exact phase
sequence was:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable ->
caching -> preparing -> faulted
```

The second `caching -> preparing` segment is the new worker's read-only cache
and participant reconciliation. It emits no new start and cannot renew or
replace the terminal same-attempt actors.

## Verification and license boundary

Portable coordinator coverage proves a passive initial scan observes every
same-attempt lease fault and retains no commit or authorized ceiling. Worker
contract coverage accepts the terminal unknown-authority form and rejects the
same form while active. The optimized WASM bundle and production worker are
used by the Chromium run. The locked/offline workspace completed 49 interface
tests, 79 client tests, 125 interface-core tests, and the exact-control/doc-test
set. Native and `wasm32-unknown-unknown` all-target project Clippy passed with
warnings denied; all WASM test targets linked; strict project rustdoc, the
native/WASM source-and-license audit, `wasm-tools validate`, and gzip/brotli
integrity checks passed.

No dependency or external implementation source was added. The implementation
remains MIT licensed in `alumina-interface`; this evidence and the surrounding
firmware documentation remain `MIT OR Apache-2.0`. No GPL-family code was used.
The live CSGRS/Hyper stack was compiled as a read-only path dependency.

## Deliberately open boundary

This is deterministic process-loss simulation, not proof of browser background
timer behavior, OS suspension, AP/radio failure, ESP task starvation, real SD
durability, physical local safe output, or multi-board motion. The harness
deliberately kills a worker and starts another in the same rendering realm; it
does not provide crash-durable browser job history or recover a nonterminal job.
The connected bare MKS TinyBee V1.0 was not flashed, reset, opened over serial,
contacted over Wi-Fi, or otherwise touched.
