# M10 TinyBee real-time input telemetry bridge evidence

Date: 2026-08-15

Status: the TinyBee 8 MiB primary and 4 MiB variant now compile a bounded,
passive resource-overview provider over the existing core-1-owned configured
safety-input bank. Core 1 publishes exact semantic input evidence through a
nonblocking fixed inter-core frame; core 0 independently validates it and emits
the existing authenticated `ALMOVW01`/`ALMTEV01` event. This is host evidence
and linked-target evidence only. The connected bare MKS TinyBee V1.0 was not
contacted, so no physical input value, cadence, Wi-Fi behavior, or safety result
is claimed.

The coordinated source checkpoints are:

- `aluminafw` `4ae9e09a11b1a655be6e84875bb477c4a35e5c84`;
- `alumina-interface` `2880fd761b9b270de83fb1ab0e76244ed997f758`.

The interface checkpoint changes only its lockfile to record
`alumina-diagnostics`' new local `alumina-safety` edge. No compatibility layer,
legacy schema, or UI source change is required because `ALMRTI01` never leaves
the firmware's core boundary.

## Exact core-1 document

`ALMRTI01` V1 is allocation-free and has a 48-byte header followed by zero to
32 fixed 16-byte input records:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | magic `ALMRTI01` |
| 8 | 2 | version 1 |
| 10 | 2 | reserved zero |
| 12 | 4 | exact complete byte length |
| 16 | 1 | input/record count |
| 17 | 1 | watchdog-deadline-present flag |
| 18 | 2 | reserved zero |
| 20 | 4 each | known, active, required, and stale masks |
| 36 | 4 | safety-input transition generation |
| 40 | 8 | exact next watchdog deadline, or canonical zero |

Each record contains the four-byte typed `ResourceId`, a one-byte
sample-time-present flag, three reserved zero bytes, and the exact latest
physical acquisition `DeviceCycle` or canonical zero. The sample time includes
pre-debounce acquisitions; it does not claim that their electrical level was
accepted. Records remain in the configuration-stable monitor slot order and
must name unique resources.

The encoder and independent decoder reject:

- invalid `SafetyInputStatus` masks, count, unconfigured state, or deadline;
- any length other than `48 + 16 * count`, or more than 32 records;
- unknown flags, nonzero reserved fields, malformed resources, or duplicates;
- a known slot without sample-time evidence; and
- a nonstale slot without sample-time evidence.

TinyBee retains at most four records, making the complete document 112 bytes
inside the existing 128-byte telemetry payload. A compile-time equality binds
the overview sample capacity to the physical monitor capacity. Core 1 attempts
one initial unconfigured document and thereafter the current document at the
nominal 100 ms management divider. `try_publish_telemetry` never waits; a full
32-entry lossy queue drops that attempt and advances no input-document sequence.

This publication is distinct from the canonical safety snapshot. It cannot
modify the monitor, acknowledge a fault, drive an output, arm a machine, or
change the real-time schedule. On a fault, the existing safety path still owns
the board-safe transaction and watchdog reaction; the passive diagnostic view
can only become stale or unavailable.

## Core-0 admission and authenticated projection

`RealtimeInputObserver<4>` admits one latest document only after checking the
outer inter-core frame and inner canonical record together. It requires:

- a nonzero serial frame sequence newer by an unambiguous half-range;
- production no later than observation and no more than 500 ms old;
- every physical sample cycle no later than frame production;
- unchanged record count, required mask, and slot/resource mapping while the
  configuration digest is unchanged;
- nonregressing production and per-slot sample cycles; and
- a serially newer monitor generation whenever known/active semantics change.

Malformed, stale, substituted, or nonmonotonic input evidence invalidates only
this observer. The passive `Health` dispatcher distinguishes `ALMRTI01` by its
magic from the existing stack-watermark payload; a family-specific payload
rejection does not invalidate the other family's valid evidence and does not
enter the outer authority-bearing frame failure path. An invalid outer frame
conservatively invalidates both passive observations.

When an authenticated telemetry subscription is due, core 0 looks up each
strictly ordered requested resource in the accepted configuration-stable
mapping. A known semantic state becomes an exact Boolean with `Measured`
provenance and the `DEBOUNCED` quality flag. Fresh state is `Valid`; a known
stale state retains its exact last sample cycle and is marked `Stale`. Unknown,
expired, differently configured, or unmapped resources become the one canonical
`Unavailable` value. Absence never means clear. Core 0 then emits the same
complete-context 320-byte `ALMOVW01` overview and 432-byte `ALMTEV01` envelope
already independently decoded by the browser.

The service's provider borrow now also checks the subscriber's exact minimum
period before assembling an overview. This prevents the 10 ms service loop from
repeatedly creating a record that the existing event owner would reject as
premature. Context rebind still atomically discards the complete subscription
and retained event on boot/configuration change.

## Fixed board policies

| Board composition | Request | Event | Samples | Overview provider | Capture provider |
| --- | ---: | ---: | ---: | --- | --- |
| TinyBee V1.0, 8 MiB | 176 | 432 | 4 | configured semantic inputs | no |
| TinyBee V1.0, 4 MiB | 176 | 432 | 4 | configured semantic inputs | no |
| T-Deck Pro | 0 | 0 | 0 | no | no |
| MKS ESP32 FOC V1.0 | 0 | 0 | 0 | no | no |

The TinyBee request is exactly a 160-byte header plus four selectors; its event
is exactly a 112-byte envelope plus the four-sample overview. Both waveform
budgets remain zero. Provider-false boards constant-gate snapshot production
and projection rather than reserving dormant state.

## Reproducible verification

Run from `aluminafw` at the implementation commit:

```sh
cargo fmt --all -- --check
cargo test --locked --offline
cargo test --locked --offline -- --list
cargo clippy --locked --offline --all-targets --no-deps -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --locked --offline --no-deps
git diff --check

cargo xtask board check mks-tinybee-v1
cargo xtask board check mks-tinybee-v1-4mb
cargo xtask board check t-deck-pro
cargo xtask board check mks-esp32-foc-v1
cargo xtask board check t-lora-pager-current

cargo xtask check --board mks-tinybee
cargo xtask check --board mks-tinybee-4mb
cargo xtask check --board t-deck-pro
cargo xtask check --board mks-esp32-foc-v1

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee-4mb \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings

cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
```

All commands passed. The default-member listing is exactly 546 tests. The new
coverage includes canonical input-document round trip and hostile wire cases,
exact last-sample exposure, mapping/generation/time substitution rejection,
fresh/stale/unavailable overview quality, minimum-period gating, and one full
observer-to-service-to-authenticated-event path. `alumina-sim` remains 58 tests.

At the coordinated interface checkpoint:

- `cargo test --workspace --all-targets --locked --offline`: 214 tests passed,
  including 59 typed client tests;
- native all-target warnings-denied Clippy passed for interface-owned targets;
- `wasm32-unknown-unknown` workspace check and warnings-denied Clippy passed;
  and
- `scripts/audit-source-policy.sh` reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`.

The unchanged external telemetry schema, production client, worker schema v5,
and capability-bound input renderer compiled against the new local dependency
edge. No new browser artifact or physical-browser result is claimed by this
firmware checkpoint.

## Exact linked images

The optimized images built from the implementation commit are:

| Board image | ELF bytes | text | data | aggregate BSS | SHA-256 |
| --- | ---: | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 11,476,020 | 1,157,688 | 12,568 | 249,576 | `f181072cfca3194e09697d5a1144075c8d230039b1c94df9cb555e2ad0ac27c9` |
| TinyBee V1.0, 4 MiB variant | 11,469,204 | 1,157,716 | 12,568 | 249,576 | `96f66fe0312603c8ee52d6cf1b358fb2a65cf580e9c2acd3f1f93235a2f090d9` |
| T-Deck Pro | 11,062,584 | 1,064,441 | 13,184 | 525,184 | `234e7a420b025c4031bdea85d951d8cde3eef44b9500fd2d353e82c892fe8328` |
| MKS ESP32 FOC V1.0 | 10,731,616 | 1,069,312 | 11,184 | 250,960 | `2b1d93cf079a5b44e80bd5e754a0cfc8e642777c50ff111eebe61cc77ce6a821` |

All four are compile/link artifacts; none was flashed.

## TinyBee static cost

A clean 4 MiB build of the immediately preceding authenticated-telemetry
checkpoint `c4b5b4b75eb095d995f8d4689cf0ff4cb28699a4` provides the exact
like-for-like baseline:

| Fact | Baseline | Bridge | Delta |
| --- | ---: | ---: | ---: |
| text | 1,130,972 | 1,157,716 | +26,744 |
| data | 12,432 | 12,568 | +136 |
| aggregate BSS | 249,712 | 249,576 | -136 |
| live `.bss` | 171,540 | 172,340 | +800 |
| linker residual `.stack` | 12,636 | 11,700 | -936 |
| `DIAGNOSTIC_SERVICE` | 320 | 928 | +608 |
| service task pool | 46,016 | 46,208 | +192 |
| real-time task pool | 43,200 | 43,200 | 0 |
| HTTP task pool | 25,992 | 25,992 | 0 |
| inter-core boundary | 13,312 | 13,312 | 0 |

The 800-byte live `.bss` increase is exactly the 608-byte diagnostic owner plus
192-byte service-future growth. The real-time task pool and fixed inter-core
boundary do not grow. Aggregate `data + BSS` remains 262,144 bytes: the 136-byte
data increase and 936-byte linker-residual stack decrease offset the live BSS
growth. The negative aggregate-BSS delta is linker accounting, not available
runtime RAM and not permission to shrink a stack. No loaded watermark or WCET
claim follows from this table.

## License and moving-Hyper boundary

The implementation is repository-owned under `MIT OR Apache-2.0`. The only new
manifest edges are between local Alumina crates; no registry package, copied
vendor source, asset, GPL/AGPL/LGPL/SSPL-family implementation, or compatibility
shim was added. The interface lockfile records the same local dependency edge.
The native/WASM source-policy inventory found neither a forbidden family nor
missing license metadata; configured `cargo-deny` CI remains the release
authority, and no local `cargo deny` result is claimed.

The interface compatibility checks compiled one coherent observed snapshot of
the current sibling CSGRS/Hyper stack. Hypercurve emitted three ordinary
dependency warnings during that snapshot. It remained a user-owned, actively
edited, read-only dependency: this work did not edit, format, reset, pin, stage,
commit, or inspect its diffs. The artifact hashes above identify firmware bytes
only and make no claim to freeze a Hyper revision.

## Claims deliberately kept closed

This checkpoint does not establish:

- any physical GPIO level, polarity, pull behavior, debounce interval, sample
  cadence, queue loss, interrupt latency, or core-placement measurement;
- raw electrical-level telemetry, unconfigured GPIO reads, analog acquisition,
  RMT/PCNT/DMA capture, or a physical `ALMDIG01` waveform provider;
- browser-to-ESP Wi-Fi/AP transport, authenticated radio performance,
  WebSocket delivery, or background-tab behavior;
- comparison with the available SLogic16U3, DSO, multimeter, or webcam;
- a licensed annotated TinyBee photograph or hotspot reconciliation;
- loaded stack headroom, allocator margin, WCET, or safe flash/OTA budget;
- a T-Deck Pro, MKS ESP32 FOC, or T-LoRa Pager physical overview provider; or
- motor, I2S/shift-register, MCPWM, ADC, SD, endstop, E-stop, interlock, arm,
  reset, motion, process-energy, or safety authority.

The connected MKS TinyBee V1.0 remained bare and untouched: no motors or motor
power were connected, and no serial session, reset, flash, GPIO operation,
Wi-Fi association, or workstation WLAN change occurred.
