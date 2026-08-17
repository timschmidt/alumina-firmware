# M9 graph palette and exact-parameter evidence

Date: 2026-08-12

## Scope and source

This checkpoint extends the canonical in-memory graph workspace with bounded
node lifecycle and exact scalar parameter editing. The implementation is
`alumina-interface` commit
`0b3375c6c11c1abae680864e59edb27e5d733536`, against preceding `alumina-firmware`
evidence commit `0e17f5311108c3e8fa27c5581f23c7b7c473c39b`. It changes no
firmware source, target image, board configuration, workstation network
configuration, or physical I/O.

The interface continued to use the current sibling workspace rather than the
obsolete released CSGRS baseline. Relevant committed identities at the final
source checkpoint were CSGRS `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`,
Hypergraphics `31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve advanced concurrently
to commit `250a4a7086672bae028c76dea3399a8949bcb35d` and retained changing tracked
edits in `src/bezier_offset.rs` and `src/bezier_region.rs`. The checks and
artifact hashes below record successful workspace consumption, not a claim
that the later dirty Hypercurve tree is a reproducible release pin.

## Canonical node lifecycle

`ALGW` V1 bytes do not change format. The workspace adds three transactional
operations around the same embedded canonical `ALGR` authority:

- create a complete `GraphNodePrototype` at an admitted integer canvas
  coordinate, assigning the next monotonic node ID inside the workspace;
- delete one node, its placement, and every incident wire atomically without
  rewinding either node or wire identity cursor; and
- replace one exact parameter while preserving its node-local ID, stable name,
  and registered root type.

Each operation constructs and validates the complete graph/workspace candidate
before replacement. Exhausted IDs, invalid coordinates, unknown node or
parameter IDs, graph limits, malformed prototypes, parameter type drift, and
revision overflow leave the prior workspace unchanged. Creation and deletion
advance graph and workspace revisions. Deleted IDs are never recycled, even
when deletion restores the same visible set of nodes or wires.

`GraphNodePrototype` deliberately remains structural. It cannot grant semantic
or implementation admission. The UI constructs its palette separately from
the fixed simulation registry and reruns audited analysis on every candidate.

## Audited palette and exact parameters

The native/WASM workspace exposes all 11 fixed HostExact node kinds in the
representative registry. Each palette entry obtains kind/version, inputs,
outputs, and parameter contracts from its audited node schema. Exact initial
parameter values and execution domain come from the lowest-ID reviewed fixture
instance of that kind. A schema without its fixed implementation or reviewed
default prevents palette construction; the UI never invents a port, parameter,
resource, or device placement.

New unconnected nodes are retained as explicit semantic drafts. Missing
required inputs appear as audited blockers instead of receiving implicit
values. Instantaneous cycles still fail layout before live-draft replacement.
An empty draft remains renderable and can accept the next monotonic palette
node.

The first parameter editor accepts bounded Boolean, exact-rational,
measurement-interval (`lower..upper`), canonical signed/unsigned lattice-count,
and text literals. Current representative control parameters are all exact
rationals. Hyperreal parses their bounded source text directly; the registered
graph schema validates magnitude, order, and literal shape; canonical `ALGR`
encoding retains the normalized exact result. No `f32` or `f64` enters this
path. Composite, resource-handle, and job-handle literals remain visibly
read-only.

Any node, wire, or parameter edit changes the embedded graph digest and hides
the old `ALGT` reference trace. Placement-only edits still preserve it. Reset
reconstructs the reviewed graph, placement, and trace binding and clears all
ephemeral editor text.

## Reproduced checks

Run from `alumina-interface` during the implementation checkpoint:

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

All 113 executable tests pass: 13 application/coordinator, 32 client, 67 core,
and one exact-control integration test, plus the intentional compile-fail
rustdoc test. New coverage exercises monotonic create/delete, atomic incident
wire removal, exact replay after lifecycle and parameter edits, parameter type
preservation, rejected-edit immutability, all 11 palette entries, exact
Hyperreal input, invalid rational rejection, empty-draft rendering/repopulation,
trace detachment, and a complete headless egui frame. Native and WASM
warnings-denied Clippy, strict rustdoc, local-source/permissive-license audit,
optimized Trunk build, both WASM validators, and compressed-artifact integrity
pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,648,930 | `de07e401f944e7b2f34cfd65941c6dc5f35317236dc3e82798b3920ae95f75c4` |
| `alumina-interface_bg.wasm.gz` | 2,129,461 | `e6db2e616ef16d7bee8d7c316e80798545606e6d4f928f7f724e27134e58bcdc` |
| `alumina-interface_bg.wasm.br` | 1,720,915 | `b3ce7a324d8e7f801b680728f0fcd383ea3763b0028138da7433b082c875fc31` |

The 96,806-byte `Cargo.lock` has SHA-256
`5790fedc0da96498fad696e5627e6f1601ceb04dead8c5adb67d1efb3773cfc1`.
Its only dependency change is making the already present local Hyperreal crate
a direct `alumina-interface` dependency for exact UI parsing; no package was
added to the resolved inventory.

The optimized bundle was served only on `127.0.0.1:8765` and rendered with
headless Chromium software WebGL at 1,440 by 1,100 pixels. The final
209,549-byte screenshot has SHA-256
`5b3e38188d0e68430401272d6a4a5c24102c8442308bacd606600a7423a2ed79`.
Visual inspection confirms the 11-kind palette, canonical workspace status,
complete graph, reference plot, and explicit no-deployment/output warning. The
loopback server was stopped after capture.

## Closed claims, licensing, and next gate

This is still one in-memory HostExact fixture editor. It has no browser/file
persistence, undo/redo, node label/domain editing, composite or
identity-bearing parameter surface, selection sets, grouping/comments,
component/subgraph document, front-panel binding, general capability-derived
palette, or collaborative diff. It grants no Service/Realtime implementation,
firmware opcode, resource, deployment, safety, motor-control, or physical-output
authority.

The bare MKS TinyBee V1.0 remained on its existing disconnected-load HIL image;
it was not reset, flashed, or contacted. No motor, motor driver, or process
power was connected. No workstation Wi-Fi setting changed. Live AP/HTTP load
and SLogic16U3 capture remain postponed until Wi-Fi can be dedicated to the
Alumina AP without interrupting the Internet development session.

The implementation is independently authored under MIT. The only dependency
edge added points at the existing local Apache-2.0 Hyperreal workspace crate.
The source-policy audit accepted the local CSGRS/Hyper/Alumina stacks and
existing permissive native/WASM inventory. No GPL-family source, library, tool
output, or asset was copied, linked, or vendored.

The next offline editor slice is bounded undo/redo plus canonical browser/file
persistence. Component/front-panel documents, broader capability-derived
palettes, and bounded live probes remain separate work. Physical Wi-Fi and
input timing remain retained-capture gates.
