# Capability-derived passive diagnostic overview

Date: 2026-08-15

This checkpoint replaces an accidental coupling between passive telemetry and
graph-execution authority with one explicit, bounded, image-specific diagnostic
catalog. It is a greenfield protocol rollover: firmware and interface move
together to capability-document V3, with no V2 parser, translation shim, or
compatibility interface.

The reviewed source commits are:

- `alumina-firmware`:
  `bb7c59a02a0cb1e7a1ab4511e6fa1efe3f89d3dc`
- `alumina-interface`:
  `9f7befe1fc363937ccfd7500ae35b51b7e785bf7`

## Authority boundary

`BoardPackage` now contains a `DiagnosticOverviewDescriptor` independent of
the descriptive resource ledger and `GraphExecutorDescriptor`. Its canonical
absent form has no support, schema, budget, timing, or records. A present form
binds:

- the exact emitted `ALMOVW01` schema;
- a whole-provider support floor;
- the maximum resource selection;
- fixed subscription and event storage budgets;
- nominal publication period and maximum fresh-sample age; and
- a strictly ordered palette of typed semantic observations with per-resource
  evidence.

Validation accepts only nonhazardous, high-impedance, real-time-owned GPIO or
safety-input resources for the current `StableBooleanInput` observation. A
record cannot grant graph execution, a pin lease, pin-mode change, interrupt
configuration, raw electrical acquisition, output control, arming, or safety
authority. Described-only providers and records remain metadata: the browser
and owned explorer model admit them only after both provider and record reach
compile support.

Capability-document V3 uses `ALMCAP03`, `ALMCPQ03`, and `ALMCPR03`. The
unchanged graph subsection deliberately retains its exact `ALMGRC02` V2 wire
encoding. The new `ALMDOV01` subsection has a fixed 48-byte header followed by
12-byte diagnostic-resource records. Complete decoding validates its limits,
reserved bytes, canonical ordering, support relationships, and reconciliation
against the later descriptive resource table.

No V2 decoding path remains. This is intentional: there are no deployed units,
firmware and UI are updated together, and compatibility machinery would create
an unnecessary second authority surface.

## Board declarations and identities

TinyBee 8 MiB and 4 MiB images publish the same implemented passive provider:

| Fact | Value |
| --- | --- |
| observations | `GPIO22`, `GPIO32`, `GPIO33`, `GPIO35` |
| observation kind | freshness-bounded debounced stable Boolean input |
| support | `Compiles` |
| maximum selection | 4 |
| subscription storage | 176 bytes |
| event storage | 432 bytes |
| nominal publication period | 100,000 microseconds |
| maximum fresh-sample age | 500,000 microseconds |

T-Deck Pro and MKS ESP32 FOC V1.0 publish the canonical absent provider. Their
firmware queue capacities and provider flags derive from that declaration and
therefore remain zero/unsupported. TinyBee firmware derives its queue sizes,
sample count, period assertion, and freshness duration from the same immutable
descriptor instead of duplicating literals.

The new canonical identities are:

| Board | Document bytes | SHA-256 |
| --- | ---: | --- |
| MKS TinyBee V1.x, 8 MiB primary | 3,531 | `27dcdd9ea4a1f9fcb1a4aeefb34984a4e4a0ca146c660f669bf632f98cac74af` |
| MKS TinyBee V1.x, 4 MiB variant | 3,544 | `0c1a0b1bc8a92e24ad0b7f68fa92171e1fbe724507269ac785787f16d384e13b` |
| T-Deck Pro | 2,773 | `835faa62f3d2a623a75db1a45135b267943130172de0d0e4c4308148a3331b21` |
| MKS ESP32 FOC V1.0 | 2,976 | `f7bcc15848aac2ad750dbf078bc29daac3042a60c680999cc80ee54980fd9f52` |

Because a configuration binds the complete board capability digest, the
canonical dual-MKS servo configuration identity changed to
`4a21132eca68df830c27f8a00ccafdeb26e9754aec883c1f5026f72f2a8e8535`.
No configuration bytes were made backward compatible with the prior identity.

## Interface behavior

The browser worker now chooses passive telemetry only from the authenticated
`ALMDOV01` catalog. It requires an implemented provider, exact `ALMOVW01`
schema, implemented stable-Boolean records, and encoded request/event lengths
within the advertised budgets. It rounds the advertised microsecond cadence
upward onto the authenticated device-cycle lattice. Page code cannot choose raw
pins, cadence, byte ceilings, context, or subscription identity.

The rendering realm independently revalidates transferred overview bytes and
rejects any sample outside the passive palette, even if the typed resource is
present in the descriptive ledger. Board-explorer state and UI now keep three
independent facts visible:

1. the resource is described and has an owner/safe state;
2. the exact image admits a passive semantic observation; and
3. the exact image admits a graph operation.

TinyBee happens to publish the same four selectors in the diagnostic and graph
palettes today, but neither palette is inferred from the other. Search filters
separately expose passively observable, diagnostic-closed, graph-readable, and
graph-closed resources. Raw GPIO/ADC acquisition remains closed. The existing
waveform client remains an explicit simulator-only bridge through the graph
palette until a separate bounded capture capability is designed; all physical
board images continue to report capture unsupported.

## Loopback browser evidence

The final optimized interface bundle was served from `127.0.0.1:8097`; the
authenticated opt-in simulator listened on `127.0.0.1:8098`. Chromium 147 ran
`worker-clock-harness.html?scenario=passive-overview-final&expect=telemetry`
through the production module worker and optimized WASM artifact. The harness
reached `passed` with:

- connection and worker generation 1;
- seven accepted and zero rejected authenticated clock samples;
- the exact 3,531-byte TinyBee V3 capability and digest above;
- subscription ID 1 and digest
  `2ea1cb228e285deaf1e388960df3f341cc7299ebb02461a1a89b580627c10e38`;
- event sequences 1 and 2 at device cycles `42,468,828` and `49,261,383`;
- exactly four samples, 176 subscription bytes, and 432 event bytes;
- zero dropped events, zero telemetry failures, and no telemetry error; and
- available service/real-time health under a qualified clock interval.

The browser, static server, and simulator were stopped after the run, and the
temporary browser profile was removed.

## Verification

At the source commits above:

- explicit-file Rustfmt checks and repository `git diff --check`: passed;
- `cargo test --locked --offline` in `alumina-firmware`: 548 tests passed;
- firmware portable warnings-denied Clippy and no-dependency warnings-denied
  rustdoc: passed;
- hostile capability tests reject reserved bytes, absent-with-facts,
  unsupported observations, duplicate records, missing descriptive resources,
  and described-only admission;
- `cargo xtask check --board ...` passed for TinyBee 8 MiB, TinyBee 4 MiB,
  T-Deck Pro, and MKS ESP32 FOC V1 without flashing;
- strict ESP-target firmware Clippy passed for all four image variants;
- optimized release links passed for all four image variants;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface`: 214 tests passed, with the core documentation test also
  passing separately;
- native and `wasm32-unknown-unknown` workspace warnings-denied Clippy passed
  with dependency linting excluded;
- no-dependency warnings-denied interface rustdoc passed;
- `scripts/audit-source-policy.sh` reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- optimized locked/offline Trunk build and `wasm-tools validate`: passed;
- gzip and Brotli decompression reproduced the uncompressed WASM digest; and
- the final Chromium passive-overview telemetry expectation passed.

Tool versions were Rust 1.97.0, ESP Rust 1.90.0-nightly, Trunk 0.21.14,
wasm-tools 1.235.0, Chromium 147.0.7727.137, and Node.js 22.22.2.

The optimized firmware artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-firmware-mks-tinybee-v1` | 11,498,700 | `d403b7af52181fe1f5778991d5a65eb77b8cb0ef35e6e6707f331009622f7b62` |
| `alumina-firmware-mks-tinybee-v1-4mb` | 11,491,832 | `3a8b33b5af862ef311f84cac96adf3b4b6449b62bcaa4b613ea0236dd4710ca8` |
| `alumina-firmware-t-deck-pro` | 11,084,208 | `91a1010472ce8d2dfe4575acecc206d32fe9398180f8d4aa5524b5f763212c4a` |
| `alumina-firmware-mks-esp32-foc-v1` | 10,753,300 | `c44be347fefd2ded6f773a84e851d3ff98b7f8bcf538c14b3c6b34d86e4f752b` |

These are ELF artifacts for compile/link evidence, not flash-image byte counts.

The optimized browser artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 5,847,703 | `0fb9f9cf3f39c0ee8b77264ff89cc1ae371810115655f912c30da7a8e922b152` |
| `alumina-interface_bg.wasm.gz` | 2,604,312 | `8e3745613cd377bf5f995a0066a9c51f1c374a65975f78aed729b09a62b9dda1` |
| `alumina-interface_bg.wasm.br` | 2,068,972 | `7990cd1d78622eae596abc22be373cf5cab62cc254dd04a9cb75739d0d2c58b5` |
| `alumina-interface.js` | 91,813 | `ae966aae9228a081c55d2e6570c914e09471aff0ace6505641f98205ebb60f0e` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

## License and moving-Hyper boundary

All implementation is repository-owned under the repositories' existing
permissive licenses. No manifest, lockfile, registry dependency, copied vendor
implementation, or GPL/AGPL/LGPL/SSPL-family source was introduced. Configured
`cargo-deny` CI remains the release authority; no local `cargo deny` result is
claimed.

Interface verification compiled coherent observed snapshots of the current
sibling CSGRS/Hyper stack, including Hypercurve, Hyperpath, Hypersolve, and
Hypergraphics. Hypercurve remained a user-owned, actively edited, read-only
dependency boundary. This checkpoint did not edit, format, reset, pin, stage,
commit, or inspect diffs in any sibling Hyper repository. Artifact hashes
identify the integration snapshot that built; they do not freeze Hypercurve.

## Claims deliberately kept closed

This checkpoint does not establish:

- physical TinyBee identity, GPIO levels, sample timing, interrupts, queue
  occupancy, stack use, or core placement;
- browser-to-ESP Wi-Fi, AP association, radio/CORS behavior, or workstation WLAN
  handling;
- raw GPIO, ADC, RMT, PCNT, DMA, logic-analyzer, or waveform acquisition;
- annotated-board photography, hotspot placement, or operator-usability review;
- SD, I2S/shift-register, MCPWM, encoder, current-sense, endstop, E-stop, or
  safety-chain behavior;
- motor, servo, stepper, heater, laser, plasma, relay, or other energized output
  behavior; or
- machine arm, motion, process-energy, interlock-reset, production, or physical
  qualification authority.

The connected bare MKS TinyBee V1.0 was not contacted: no flash, reset, serial,
GPIO, Wi-Fi association, or other board operation occurred. No motors or motor
power were connected. All target commands in this checkpoint were build-only.
