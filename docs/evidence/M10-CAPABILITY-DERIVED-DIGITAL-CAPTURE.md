# Capability-derived digital capture

Date: 2026-08-15

This checkpoint replaces the simulator waveform path's accidental dependency
on graph-readable resources with a separate, bounded, image-specific digital
capture authority. It is a greenfield protocol rollover: firmware and
interface move together to capability-document V4, with no V3 decoder,
translation shim, or compatibility interface.

The reviewed implementation commits are:

- `alumina-firmware`:
  `7f34731da8a489e927362c2e2d279c39111fc8e3`
- `alumina-interface`:
  `b5b710ad7a5155d80dc3fdf296c4eea86113d109`

## Independent authority

`BoardPackage` now has three independent, fail-closed input authorities:

1. `DiagnosticOverviewDescriptor` permits only fixed, low-rate semantic
   observations;
2. `DigitalCaptureDescriptor` permits only bounded device-produced edge
   acquisition; and
3. `GraphExecutorDescriptor` permits only audited graph operations.

A typed resource in the descriptive ledger, passive overview, graph palette,
interrupt table, or GPIO namespace does not enter the capture palette. A
capture record names an exact acquisition mechanism: simulated, RMT, PCNT, DMA,
or qualified bounded software sampling. It still grants no graph operation,
pin lease, output control, pin-mode change, interrupt configuration, arm
transition, interlock reset, motion, process energy, or safety authority.

Provider validation requires:

- exact schema, support floor, known flags, and nonempty trigger set;
- fixed channel, transition, configure, record, chunk, pretrigger, duration,
  and arm-horizon bounds;
- strictly ordered, unique typed resources;
- only nonhazardous, high-impedance, real-time-owned GPIO or safety inputs;
- every resource record at or above the whole-provider support floor; and
- `ALLOW_SOFTWARE` if and only if at least one advertised source is software.

The service owner consumes the complete descriptor. Configure admission checks
the canonical byte length, channel and transition shapes, flags, trigger,
resource palette, chunk ceiling, exact clock-to-microsecond timing bounds, and
maximum possible retained record before mutating session state. A retained
record must fit the fixed arena and report the exact catalogued source for
every channel. Both live chunks and side-effect-free recovery ranges remain at
or below the configured and advertised chunk ceiling. Overwide recovery reads
are rejected and covered by a negative test.

## Capability-document V4

The exact outer wire identities are now `ALMCAP04`, `ALMCPQ04`, and
`ALMCPR04`. The graph subsection deliberately retains `ALMGRC02`; the passive
overview subsection retains `ALMDOV01`. The new `ALMDCP01` subsection has a
fixed 64-byte header followed by 12-byte resource/source/support records.

Independent capture decoding validates its own magic, schema, reserved bytes,
support relations, limits, source enum, software flag, record ordering, and
exact subsection length. Complete board decoding additionally reconciles each
record with the later descriptive resource table and its ownership, safe-state,
and hazard facts. Tests reject reserved-byte changes, absent providers carrying
facts, unknown sources, duplicate records, resource substitution, unsupported
flags/triggers, and missing descriptive resources.

The physical image identities are:

| Board image | Document bytes | SHA-256 |
| --- | ---: | --- |
| MKS TinyBee V1.x, 8 MiB primary | 3,595 | `4c7054f601d16887c2c2cc8598cc3019c624904800cecca4f982af7bb45b7c57` |
| MKS TinyBee V1.x, 4 MiB variant | 3,608 | `eb123ad7b5641e36d5fe7eb74c4ab5724d13f3dd15aa2c23b8a61160f69b9091` |
| T-Deck Pro | 2,837 | `1de707aa21a0f8427e619c6501836cb8b281ff59e7294707c24b766be4e163d5` |
| MKS ESP32 FOC V1.0 | 3,040 | `cbe9b541f90a0f9a63487f7fc43855b742bc4e7c1bc4776aca27aea3fbc60384` |

Every physical image publishes the canonical capture-absent form: zero schema,
support, flags, triggers, budgets, timing, count, and records. Nothing about the
connected TinyBee is inferred from the simulator.

The host fixture instead serves a distinct package:

| Fact | Simulator value |
| --- | --- |
| board ID | `sim-mks-tinybee-v1` |
| revision | host-only deterministic TinyBee V1.x topology model |
| document bytes | 3,655 |
| SHA-256 | `4ea9bbf0b44c8664808b4e13b20294a0006371cfe1d843478a197b37b6be6cc7` |
| resources | GPIO22, GPIO32, GPIO33, GPIO35 |
| source | simulated |
| support / schema | compiles / `ALMDIG01` |
| flags / triggers | edge timestamps / immediate only |
| channels / transitions | 4 / 64 |
| configure / record / chunk | 208 / 2,048 / 168 bytes |
| pretrigger / duration / arm horizon | 0 / 2,000,000 / 30,000,000 microseconds |

The package copies only the physical TinyBee topology tables needed for a
deterministic host model. Its board ID, revision, capability digest, capture
authority, and non-armable declaration are distinct. Constructing it opens no
device, GPIO, serial port, radio, or network interface.

Because the complete board capability digest is embedded in canonical machine
configuration, the representative dual-MKS servo configuration identity is now
`72272e44b862ed071b8c306977dd201f17accbc55d4cf1ee293f24f6cdb88255`.

## Interface admission and presentation

The owned board explorer now retains five facts without conflation: descriptive
resource data, aliases, passive observations, digital acquisition source, and
graph operations. Its summary and filters separately expose digitally
capturable and capture-closed resources. The paired offline diagnostic view
uses the simulator package because the physical TinyBee package truthfully has
no capture provider; the graph deployment target remains the physical package.

The browser worker requires an implemented `ALMDIG01` provider and derives all
of these values from the authenticated V4 document:

- selected resources and exact source kinds;
- edge-timestamp and software-source flags;
- immediate-trigger support;
- maximum channels and transitions;
- configure, retained-record, and range/chunk byte ceilings; and
- duration and arm-horizon bounds converted conservatively onto the signed
  device-cycle lattice.

No waveform selection or budget is borrowed from graph authority. The client
contract performs only transport-level shape checks; the worker performs the
image-specific checks after capability authentication. The rendering realm
then independently decodes the complete capture and checks provider schema,
record/channel/transition/timing/trigger bounds, resource membership, and exact
reported source before exposing it to the plot/UI. JSON transfer is not trusted.

The live panel reports the three catalogs separately and offers a 2 ms capture
only when the admitted capture provider can represent it. That duration is a UI
preference clipped by capability, not firmware policy. The trace cursor remains
an integer-cycle display observation and cannot become a command.

## Localhost optimized-browser evidence

The final optimized bundle was served from `127.0.0.1:8097`; the authenticated
simulator listened on `127.0.0.1:8098`. Headless Chromium 147 had background
networking, component updates, default apps, extensions, and sync disabled, and
its host resolver rejected non-loopback names. The final
`capability-digital-capture-v4-final` / `waveform-repeat` run used the production
module worker and optimized WASM artifact and reached `passed` with:

- simulator board ID `sim-mks-tinybee-v1` and the exact 3,655-byte V4 identity
  above;
- worker/capture generation 1;
- seven accepted and zero rejected authenticated clock samples;
- available service/real-time health;
- two distinct nonzero capture IDs;
- two canonical 544-byte `ALMDIG01` records;
- exact requested posttrigger durations of 2,000 then 3,000 cycles at 1 MHz;
- four channels and 16 retained transitions in each record;
- source value 1 (`Simulated`) on all four channels;
- exact final range progress 544/544 bytes; and
- zero consecutive waveform failures and no waveform error.

The browser, simulator, and static server were stopped, and the exact temporary
Chromium profile was removed after the run. This is loopback software evidence,
not radio, ESP, physical-input, timing, or operator-usability evidence.

## Verification

At the implementation commits named above:

- repository-owned Rust files passed formatting checks, and both repositories
  passed `git diff --check`;
- `cargo test --locked --offline` in `alumina-firmware` enumerated and passed 552
  tests, including 13 board, 9 capability, and 60 simulator tests;
- firmware portable warnings-denied Clippy and no-dependency warnings-denied
  Rustdoc passed;
- all four physical package capability identities reproduced their compiled
  digest and length;
- `cargo xtask check --board ...` passed for TinyBee 8 MiB, TinyBee 4 MiB,
  T-Deck Pro, and MKS ESP32 FOC V1 without flashing;
- strict ESP-target warnings-denied Clippy passed for all four image variants;
- optimized ESP release links passed for all four image variants;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface` passed 215 tests, and the separate compile-fail
  documentation boundary test passed;
- native and `wasm32-unknown-unknown` no-dependency workspace
  warnings-denied Clippy passed;
- native and WASM no-dependency warnings-denied interface Rustdoc passed;
- `scripts/audit-source-policy.sh` reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- optimized locked/offline Trunk build and `wasm-tools validate` passed;
- gzip and Brotli decompression reproduced the uncompressed WASM digest; and
- the final Chromium repeated-waveform expectation passed as described above.

Tool versions were firmware Rust 1.88.0, ESP Cargo 1.90.0-nightly, interface
Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium 147.0.7727.137, and
Node.js 22.22.2.

The final optimized firmware artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-firmware-mks-tinybee-v1` | 11,523,540 | `fc683200a2abb352d937684ab906e889b530286832b36d8bf5e37470271ac995` |
| `alumina-firmware-mks-tinybee-v1-4mb` | 11,516,716 | `4dc33cb698ec585b10e24bc9f4418c7f0cd85afa09fc25d680ca6e7f14f32cb2` |
| `alumina-firmware-t-deck-pro` | 11,129,844 | `9bdab6a37e09d4ac5c64991b5b5d9e168da7b2c3d0be65434c079b8353dce037` |
| `alumina-firmware-mks-esp32-foc-v1` | 10,799,396 | `ce56ef32f53d26150855204582aa1470b8d4385a77c7970643d79ec7bf0ccede` |

These are ELF compile/link artifacts, not flash-image byte counts, and none was
written to a device.

The final optimized browser artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 5,876,051 | `8c66d02b3d1efd0d34a7c353203a825733c44b871cc2bc753b6b86c49d9dd93e` |
| `alumina-interface_bg.wasm.gz` | 2,615,921 | `f7d6eb7522eedc581e52aeb5c9dc077cb923c2b9b1225dad5bf3bff0ddb349b5` |
| `alumina-interface_bg.wasm.br` | 2,077,856 | `a7995e07ec9ac681413928567af614720ffd1a60484d9b71ec0dbe452f697b9d` |
| `alumina-interface.js` | 91,813 | `70a3a45ee373356cc3f95eb1f73c67a997c2c5102f94e4b556e13ffd31e23d4f` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

## License and moving-Hyper boundary

All implementation is repository-owned under the repositories' existing
permissive licenses. No manifest, lockfile, registry dependency, copied vendor
implementation, asset, or GPL/AGPL/LGPL/SSPL-family source was introduced.
Configured `cargo-deny` CI remains the release authority; no local `cargo deny`
result is claimed.

Interface verification compiled coherent observed snapshots of the current
sibling CSGRS/Hyper stack, including Hypercurve, Hyperpath, Hypersolve, and
Hypergraphics. Hypercurve remained a user-owned, actively edited, read-only
dependency boundary. One initial read-only workspace-wide formatting check
crossed into the live Hypercurve workspace and printed existing formatting
suggestions; it made no edit and was not repeated. Every subsequent formatting
check named only repository-owned files. This checkpoint did not edit, reset,
pin, stage, or commit any sibling Hyper repository. Artifact hashes identify
the integration snapshot that built; they do not freeze Hypercurve.

## Claims deliberately kept closed

This checkpoint does not establish:

- physical TinyBee identity, GPIO levels, acquisition timing, interrupt load,
  queue occupancy, stack use, or core placement;
- browser-to-ESP Wi-Fi, AP association, radio/CORS behavior, or workstation
  WLAN handling;
- RMT, PCNT, DMA, software-sampling, ADC, or external-analyzer behavior on an
  ESP target;
- comparison with the available SLogic16U3, DSO, or other instruments;
- annotated-board photography, hotspot placement, connector correctness, or
  operator-usability review;
- SD, I2S/shift-register, MCPWM, encoder, current-sense, endstop, E-stop, or
  safety-chain behavior;
- motor, servo, stepper, heater, laser, plasma, relay, or any other energized
  output behavior; or
- machine arm, motion, process energy, interlock reset, production, or physical
  qualification authority.

The connected bare MKS TinyBee V1.0 was not contacted: no flash, reset, serial,
GPIO, Wi-Fi association, or other agent-initiated board operation occurred. No
motors or motor power were connected. The SLogic16U3 was not requested or used.
All target commands in this checkpoint were compile/link only.
