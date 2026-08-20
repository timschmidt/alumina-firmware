# M9 mixed-signal control trace evidence

Date: 2026-08-20

## Scope and source

This checkpoint promotes the two interlock results from the existing exact
PID fixture into named host probes, public component outputs, replay indicators,
and visible Boolean logic-analyzer lanes. The implementation is
`alumina-interface` commit
`d1cfe8f8e19b1762d96dafb360e10fa50b30bbe7`, against preceding
`alumina-firmware` evidence commit
`8b6fb8c897ba1b51d1830a2f07d8829cd3fa8081`.

No firmware source, target image, board package, workstation network
configuration, or physical I/O changed. The bare MKS TinyBee V1.0 was not
contacted, reset, flashed, or probed. This is deterministic host
simulation/editor evidence, not a live telemetry, Realtime opcode, deployment,
safety, timing, or physical-input claim.

The interface compiled from the live path-based CSGRS/Hyper stack rather than
an obsolete released CSGRS baseline. Hypercurve emitted five ordinary
user-owned compiler warnings during qualification. No Hyper repository was
modified, formatted, staged, pinned, or treated as a reproducible release
snapshot by this checkpoint.

## Mixed-signal model and rendering

`RepresentativeControlSignal` now names the exact inclusive-range result and
the external-permit/range conjunction at their existing typed output endpoints.
The canonical probe sidecar selects those endpoints after the four existing
exact-rational controller outputs. Probe replay continues to resolve output
direction and exact `GraphTypeId` against the bound workspace; the UI neither
copies a second value schema nor treats a Boolean as a number.

Each admitted trace series has one immutable value kind. Exact rationals retain
their exact text plus a certified finite `f64` enclosure used only for display.
Booleans remain Booleans and render as exact high/low step lanes. A series that
is empty, changes kind, has an unsupported value, exceeds the retained-point
bound, or lacks a finite rational enclosure fails workspace construction.

The visible mixed-signal surface partitions probe-selected series into four
analog lanes and two labeled digital lanes. Both sections use the same exact
control-clock ticks and cursor. The analog plot alone has a lossy certified
coordinate projection; Boolean labels and cursor values never acquire a bogus
physical unit or pass through `f64`. Mixed, analog-only, and digital-only probe
selections remain renderable. The canonical Boolean traces are
`[true, true, true, false, false, false]`, so both interlock lanes visibly fall
at tick 3 while retaining their independent typed endpoints.

The reusable component now exposes six public Stream outputs and fourteen
front-panel bindings: eight exact PID/interlock parameter controls, four
exact-rational replay indicators, and two Boolean replay indicators. These
outputs remain immutable host replay observations. Invalidating any bound
workspace endpoint still detaches the component/probe presentation rather than
weakening the draft or inventing a current value.

## Canonical identities

The underlying graph, implementation registry, trace, and reference workspace
do not change. The presentation sidecar, component, and source hierarchy
advance because they bind the two additional public observations. No
compatibility shim is provided.

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| `ALGR` graph | — | `96a3348264a9b65d267b45f9a6419a44ee60473fd961abcf4436295e10b3735f` |
| `ALSI` V2 registry | — | `fc68d37f279782c5a5368bc0e44aa695a3b2babbfaf967b09cf4fc75287eae83` |
| `ALGT` trace | 8,292 | `e2f8a0f20b3e5f9fdfc12c394e1e325d7b65243efad8c9c7f558f8845c965fe3` |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| six-series `ALGP` | 348 | `5e1dcccb37920329208fd9c97bc08ea8c909064c061e6d8c95e265cdbe15c4b5` |
| `ALGC` component | 4,734 | `c309db7780ac40006a243a505d3650865b8783c30a0ac31b62aea95dbc1fce11` |
| source `ALGH` | 5,706 | `e4835614a0857cd62a4b7876cfd0e9e336751e4586638d7b8d5187fae059d8dd` |
| flattened `ALGW` | 3,755 | `e39c5396539689b8b563a7220e1180d7717e70893b71580ebbe51873fa13b68f` |

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt -p alumina-interface -p alumina-interface-core -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
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

All 261 executable workspace tests pass: 51 application/coordinator, 82
protocol-client, 127 core, and one public exact-control integration test, plus
the intentional compile-fail rustdoc test. The application suite exercises the
complete mixed view and a direct digital-only frame. Native and WASM
warnings-denied Clippy, strict Alumina rustdoc, the local-source/permissive-
license audit, optimized Trunk assembly, both WASM validators, and compressed-
artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,296,729 | `5d499cb10009332d9e75d80c5caf7a713c752c17668902629bb9db98becb99b7` |
| `alumina-interface_bg.wasm.gz` | 2,790,167 | `a38a9b8b88f9002ac4391c1689a131e49802ad7d2b352a4c8fc87a7148aad489` |
| `alumina-interface_bg.wasm.br` | 2,207,050 | `5689e23abedb366153da7d149391e670d59e35a3cbe333cc80b8e687e77707e2` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.
The optimized bundle and dedicated worker loaded from `127.0.0.1:8765` in
headless Chromium using ANGLE/SwiftShader. The final scrolled 1,440-by-1,057
capture visibly contains all four analog series, both labeled Boolean lanes,
the shared tick cursor at 3, and exact cursor values. It is 285,292 bytes with
SHA-256
`26dcc65b677da88269a40b19650dd1f8329bf2bdcde494645c2993c8db7fc832`.
The first browser attempt requested an unsupported graphics backend and
produced a blank canvas; it was excluded. The supported-backend rerun passed,
and the loopback browser and server were stopped afterward.

## Closed claims and licensing

The mixed-signal plot remains a bounded `HostExact` simulation/editor surface.
It cannot subscribe to device telemetry, arm an output, grant resource access,
install a graph, or execute either interlock primitive on firmware. Promotion
requires separately authenticated capability/configuration binding, bounded
capture transport, loss/timing evidence, fixed-memory lowering, and reviewed
Service/Realtime authority.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
