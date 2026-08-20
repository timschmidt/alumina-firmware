# M10 rolling network-attended job leases

Date: 2026-08-20

## Closed software boundary

Network-attended jobs now use a finite, renewable local execution lease instead
of granting enough authority for the complete cached job at commit time. The
immutable `ALMJCOM2` commit remains unchanged. `JobLeaseRenew` (`0x050a`) carries
one canonical 96-byte `ALMJREN1` request bound to its prepare ID, boot ID,
commit ID, full commit digest, and one absolute device-cycle expiry.

The portable schedule, simulator, firmware service owner, and independent
core-1 owner admit only a strictly later expiry while the schedule is
`Confirmed`, `Priming`, `Primed`, or `Running` and the current lease is still
live. Identical request bytes are idempotent. Each target core independently
requires current safety/deadline health, caps a renewal at 15 seconds beyond
local admission time, and caps total authority at one hour after scheduled
start. Cached-autonomous policy rejects renewal and keeps the commit lease
exactly immutable.

`ALMJSCH5` reports the current lease. Core 0 accepts no value beyond the
greatest expiry it explicitly enqueued, and the browser accepts no value beyond
the greatest expiry it explicitly requested. Thus delayed telemetry can trail
authority, but neither MCU nor browser may invent a later horizon.

## Browser coordination

Schema V11 adds the current reported lease, greatest browser-authorized lease,
and completed renewal-round count to every cached-job snapshot. The production
worker uses fresh boot-scoped clock bounds and the following representative
policy:

- initial attended lease: three seconds after scheduled start;
- rolling target: nine seconds after current browser time;
- renewal threshold: four seconds remaining; and
- exact-retry guard: one conservative second remaining.

One renewal round is constructed transactionally for the complete sorted MCU
set. If any request becomes ambiguous, every still-unissued peer request is
abandoned and no later mutation is permitted until a clean all-participant
`JobStatus` sweep completes. An applied-but-unobserved absolute expiry is
retried exactly while the guard proves that retry can still arrive in time.
Once the conservative device-cycle estimate reaches the reported lease, the
worker sends no late renewal and polls for the local `LeaseExpired` fault.

The standalone simulator now converts the canonical manifest duration into
device cycles with checked `u128` ceiling arithmetic and remains `Running`
until that exact deadline. The representative manifest is
`9,639,280 / 1,000,000` seconds, so the three-second initial lease cannot hide a
missing renewal behind immediate simulated completion.

## Fresh two-MCU Chromium qualification

All runs used two fresh localhost-only simulator actors with stable identities
`ALUM-SIM:TINYBEE` and `ALUM-SIM:TINYBEF`, drifts +37 ppm and -41 ppm, and a
fresh cached job `2047934465`. Each published 127,264 bytes. The physical board
and workstation WLAN were not contacted.

### Nominal attended execution

```console
target/debug/alumina-sim-http --bind 127.0.0.1:8098 \
  --device-id 414c554d2d53494d3a54494e59424545 --drift-ppm 37
target/debug/alumina-sim-http --bind 127.0.0.1:8099 \
  --device-id 414c554d2d53494d3a54494e59424546 --drift-ppm -41
node tests/browser/read-cached-job-result.mjs 9224 single
```

The run passed after 531 validated replacement snapshots and three complete
renewal rounds:

```text
caching -> preparing -> ready -> installing -> installed ->
confirming -> confirmed -> irrevocable -> complete
```

Both terminal reported leases exactly equaled browser authority:
`88,977,066` and `88,967,199` cycles. There were zero failure observations.

### Applied-response loss

Both actors selected `--drop-operation-response job-lease-renew`. Each applied
its first renewal but discarded the successful HTTP response. The production
worker exposed exactly two one-failure `confirmed` observations, reconciled the
applied values by status, completed four renewal rounds, and passed after 531
snapshots. Terminal reported/authorized leases were exactly `90,439,969` and
`90,438,513` cycles.

### Pre-application request loss

Both actors instead selected `--drop-operation-request job-lease-renew`. Each
first request was discarded before authentication or schedule mutation. Status
retained the prior lease, the worker retried exact authority without partial
multi-MCU mutation, and the run passed after 531 snapshots, two one-failure
`confirmed` observations, and five renewal rounds. Terminal
reported/authorized leases were exactly `90,349,076` and `90,323,752` cycles.

### Sustained loss through expiry

Both actors selected the same request-loss operation with count `10000`. The
worker performed 26 renewal attempts and 26 separately recovered one-failure
observations while status remained available. Both schedules entered
`Running`, then faulted locally at their unchanged initial leases before the
manifest duration ended. The exact terminal facts after 464 snapshots were:

| MCU | reported expiry | browser-authorized but unapplied expiry | terminal |
| --- | ---: | ---: | --- |
| `ALUM-SIM:TINYBEE` | 79,358,005 | 80,758,005 | `Faulted/LeaseExpired` |
| `ALUM-SIM:TINYBEF` | 79,383,466 | 80,783,466 | `Faulted/LeaseExpired` |

The browser retained the distinction between requested authority and MCU
authority; it did not rewrite either reported lease to the larger unapplied
value and did not accept normal completion.

### Cached-autonomous control

A fresh `cached_autonomous` run passed the same exact-duration simulator after
531 snapshots with zero renewal rounds and zero failures. Each terminal lease
remained exactly equal to its original commit (`101,148,015` and `101,146,845`
cycles). This distinguishes autonomous finite-horizon admission from attended
rolling authority.

## Verification and license boundary

The implementation is covered by portable schedule, protocol, cross-core,
simulator, client, distributed-coordinator, live-job, worker-contract, and
browser fault-injection tests. TinyBee V1 8 MiB warnings-denied ESP Clippy and
the optimized locked/offline WASM build are part of the release check for this
slice.

This repository-owned work remains MIT OR Apache-2.0/MIT compatible and adds no
dependency. No GPL-family, Synthetos, SimpleFOC, or other external motion source
was copied. HyperCurve and all sibling Hyper repositories remained read-only;
the interface compiled against their live workspace state.

## Deliberately open physical boundary

This is localhost software evidence. It does not qualify browser suspension,
real AP/radio loss, task starvation under physical target load, SD power loss,
electrical safe output, E-stop/endstop/interlock timing, motion output, motors,
motor power, or multi-board physical synchronization. TinyBee and T-Deck Pro
remain non-armable/output-unqualified. The connected bare MKS TinyBee V1.0 was
not flashed, reset, opened over serial, contacted over Wi-Fi, or otherwise
touched.
