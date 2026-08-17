# M5 / Interface I0 exact baseline evidence

Date: 2026-08-11

Status: implemented development checkpoint; not a reproducible compiler release
or an M5 exit-gate claim.

## Scope and source identity

This checkpoint replaces the legacy `alumina-interface` application with a
greenfield native/browser workspace at commit
`640e9987414eb3eb02f70a3e42ee9c13ce31e4e5` and adds the checked native
Hypermesh adapter to Hypergraphics at commit
`b7f197dfe8ddac5112a4411db33815598905f973`. It consumes the real protocol and
machine-IR crates from the sibling `alumina-firmware` tree at
`188f212d6f838cacfa103fb2f82300b4b5523ea2`.

CSGRS is the sibling working tree at
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0`. Its manifest currently reports
package version 0.23.0, but that string is not the selected published release.
The interface manifest has a direct path dependency and patches every used
CSGRS/Hyper package to its sibling checkout. The source-policy test separately
walks the WASM inverse dependency graph and rejects any package that does not
resolve from that path. There is no registry fallback.

The complete revision/status table is in the interface's
`docs/HYPER-BASELINE.md`. At this evidence capture, Hypercurve and Hyperphysics
had concurrent tracked edits and Hyperlimit had untracked fuzz/output files.
Those repositories were not modified by this checkpoint. This is therefore an
identified live development input, not an atomic or reproducible release pin.
Release qualification remains closed until a clean, coherently reviewed stack
is committed and checked out by exact revision.

## Implemented boundary

- The old endpoint/device models, serialized node graph, hand-built renderer,
  font module, and legacy mesh/sketch application were deleted; no compatibility
  route or schema was retained.
- `alumina-interface-core` is independent of windows, browsers, networking, and
  GPU contexts. It defines structurally disjoint exact design/CAM values,
  exact-rational bounded measurements, canonical firmware cycles/counts using
  the real `alumina-machine-ir` types, and finite display-only values.
- Decimal design input reaches `hyperreal::Real` without a primitive-float
  round trip. A compile-fail test proves that a projected display value cannot
  satisfy an exact millimetre input.
- The core exposes the current local Hyperpath and Hypersolve seams and builds a
  native CSGRS `TriangleMesh`. Hypergraphics validates every source triangle
  index, retains `Real` coordinates to its named render boundary, and owns the
  exact camera, grid, axes, GPU upload, and projection path.
- `alumina-interface-client` frames the real greenfield protocol and uses the
  same canonical bytes through a deterministic in-memory simulator transport.
- The release pipeline builds one native/WASM source base, explicitly invokes
  Binaryen with the current Rust bulk-memory and saturating-conversion features,
  and emits integrity-tested gzip and Brotli assets for later firmware
  embedding.
- Native and WASM dependency inventories reject GPL, LGPL, AGPL, SSPL, and
  missing license metadata. New interface code remains MIT licensed; no
  GPL-family source was imported.

## Verification record

Run from the sibling `alumina-interface` repository:

```console
cargo fmt -p alumina-interface -p alumina-interface-core \
  -p alumina-interface-client -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --offline -- -D warnings
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz
gzip -t dist/alumina-interface.js.gz
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Observed results:

- 6 exact-core tests, 2 protocol/simulator tests, and 1 compile-fail boundary
  test passed; all other package/doc test targets passed with no failures.
- Strict native and WASM Clippy checks passed.
- The source-policy gate reported only sibling CSGRS/Hyper packages and accepted
  both permissive-license inventories.
- Trunk 0.21.14 completed an offline release bundle. `wasm-tools` and all four
  compressed-stream checks passed.
- `Cargo.lock` SHA-256:
  `6b43622e69679cec160db78d52bdc71abd39748c75a7da714bf653e66b6fbd2a`.
- Final WASM: 2,473,935 bytes; Brotli: 1,057,275 bytes; gzip: 1,241,123
  bytes; WASM SHA-256:
  `52115af358164ff7bff43cebbcceca6961340903089a472855d50cb4ddd5da7f`.
- Generated JavaScript SHA-256:
  `6674a4763dda4a6440e569ca0a1f3499b85d5cf316dbe1d66a6b3898280c6b56`.

No board was connected, flashed, armed, or energized.

## Open gates

This checkpoint does not yet implement the authoritative curve-to-machine job
compiler, a browser Wi-Fi transport, capability-generated board configuration,
browser automation/visual goldens, storage/job workflows, annotated board
diagnostics, or the typed LabVIEW-style graph. It also does not qualify the
current moving Hyper sources as a release compiler. Those remain I1–I7/M5 work.
