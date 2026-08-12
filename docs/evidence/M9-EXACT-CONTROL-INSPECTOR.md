# M9 exact control-inspector evidence

Date: 2026-08-12

## Scope and source

This checkpoint makes the existing representative `HostExact` control graph
visible in the real native/browser application without duplicating its
authority in UI code. The implementation is `alumina-interface` commit
`bec1f96a42be82fe9a1fa3df4e2f61188d124440`, against the preceding
`aluminafw` evidence commit
`6924a0db2bc26d33ed2040933387a34211bf869c`. It changes no firmware source,
target image, board configuration, workstation network configuration, or
physical I/O.

The interface continued to build from the current sibling workspace stack,
not from the obsolete released CSGRS baseline. Relevant committed source
identities were CSGRS `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`,
Hypergraphics `31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve was the development
snapshot at `eff9acf01533dac64ed31f3d9777f557f77840de` with tracked-diff
SHA-256
`128a2bf6d2b322fbc3950d44635477cd14b0439d44b29ef0365eaeb6298640af`.
That dirty dependency identity is recorded rather than represented as a clean
release pin.

## One exact fixture authority

The 19-node, 22-wire PID/interlock construction is now a public fallible core
fixture. Native UI, WASM UI, unit tests, and the independent integration replay
all call the same constructor. Production construction propagates exact
rational, schema, graph-document, audited-registry, simulation, and canonical
trace errors; only tests unwrap the static fixture.

The fixture continues to expose nine fixed `HostExact` behaviors: external
Stream source, audited latest-at-or-before transition, Stream sink, exact add,
subtract, dimensionless scale, inclusive clamp, explicit read-before-write unit
delay, and fail-safe exact permit gate. No UI value, coordinate, label, or
selection can become controller or deployment authority.

Human-facing node labels were added to the canonical document, including ASCII
`50 Hz to 10 Hz` labels supported by the default browser font. Because labels
are intentionally part of the saved graph, that presentationally useful edit
changes the graph and trace identities. It does not change the complete `ALSI`
V2 semantic/implementation registry identity and receives no compatibility
shim.

| Canonical object | Identity |
| --- | --- |
| graph | `fb173fb30bc5e04269caea439dea8fa455050142fac3a4afc78f5fd16e7ac59a` |
| `ALSI` V2 registry | `6bb6f814941b632ac5c9858fbbfe599fe8febb3a04b4dcc7bf4fbc8ac2f61537` |
| 7,836-byte `ALGT` trace | `4d9b63633be3afc658cac8d6475d6ede602568ab084de005ac5dd2dfcb7542a3` |

The retained exact traces remain:

- integral prior state: `[0, 3, 5, 6, 6, 6]` mm;
- inclusive-clamped controller: `[5, 5, 4, 2, 3, 3]` mm; and
- permit-gated output: `[5, 5, 4, 0, 0, 0]` mm.

Caller input reversal still produces an identical simulation, and independent
`ALGT` replay regenerates the same complete canonical trace.

## Bounded native/WASM inspector

The application opens a read-only control-graph workspace by default while
retaining the existing Hypergraphics exact-geometry workspace as a selectable
view. Inspector admission is explicitly capped at 256 nodes, 1,024 wires, and
4,096 samples per selected trace series.

Layout derives deterministic semantic ranks from the audited current-tick
dependency declarations. Declared next-state captures do not participate in
the acyclic ranking and are routed in visible feedback lanes. Nodes expose
kind/version, execution domain, typed input/output ports, exact parameters, and
explicit state clock/type/storage facts. Boolean Streams, exact Streams, and
stateful nodes have separate visible styling. The canvas is independently
scrollable and node selection never mutates the graph.

The bounded plot displays error, integral prior state, clamped controller, and
permit-gated output. Exact `hyperreal::Rational` values remain attached to each
point and are printed at the selected tick. Plot coordinates are produced only
through named finite `to_f64_enclosure` projections and cannot flow back into
the graph, trace, compiler, or firmware protocol.

A headless egui test exercises a complete frame without a window. The optimized
WASM distribution was also served only on `127.0.0.1:8765` and loaded by
headless Chromium at 1,440 by 1,000 pixels. The resulting 153,264-byte transient
screenshot had SHA-256
`48a43a4b7e574ebfa5292ca055c440aa11c91ca421074c57151b43555e2ff115`.
Visual inspection confirmed the expected graph, feedback/state styling, exact
trace legend and cursor values, selectable geometry/control views, and no
missing label glyph. The live-MCU panel remained unconfigured; this was a
loopback browser-render check, not a device or radio test.

## Reproduced checks

Run from `alumina-interface` at the implementation commit and recorded sibling
snapshot:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 104 executable tests pass: 10 application/coordinator, 32 client, 61 core,
and one shared-fixture integration test, plus the intentional compile-fail
rustdoc test. Native and WASM warnings-denied Clippy, strict rustdoc,
local-source/permissive-license audit, optimized Trunk build, both WASM
validators, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,576,469 | `882aa7b49896036a6bec4325fc0eb7394ba10d267d96620bd1744e6f0c605132` |
| `alumina-interface_bg.wasm.gz` | 2,099,947 | `f5d1506e6b43b89e28e534184cd992a2232a7fb0808cd4346e028d3bf2882333` |
| `alumina-interface_bg.wasm.br` | 1,701,900 | `abc6836ba3f68acb0265e7936eea3413a5bfa06fdfaecbc416a55aefe967e05d` |

The 96,792-byte `Cargo.lock` has SHA-256
`789484967e2659c753722fab8ab5c21b6f2765195d95b17e7c7fa1056846989b`.

## Closed claims, licensing, and next gate

This is a bounded inspector, not an arbitrary graph editor or front panel. It
does not lower these control behaviors to Service/Realtime opcodes, deploy the
fixture, claim a peripheral, arm firmware, command an output, prove WCET, or
qualify safety. Editable placement/wiring, component/front-panel documents,
general probes and plots, live telemetry, board-photo overlays, and physical
capture remain open.

The bare MKS TinyBee V1.0 remained on its existing disconnected-load HIL image;
it was not reset, flashed, or contacted. No motor, motor driver, or process
power was connected. No workstation Wi-Fi setting changed. Live AP/HTTP load
and SLogic16U3 capture remain postponed until Wi-Fi can be dedicated to the
Alumina AP without interrupting the Internet development session.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted only the local CSGRS/Hyper/Alumina stacks and
the existing permissive native/WASM inventory. No GPL-family source, library,
tool output, or asset was copied, linked, or vendored.

The next offline graph/UI work is editable canonical graph placement and wiring,
then component/front-panel state and broader bounded probes. Firmware lowering
of selected control primitives remains a separately reviewed Service/Realtime
slice. Physical Wi-Fi and input timing remain separate retained-capture gates.
