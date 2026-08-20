# M9 interlock cause trace evidence

Date: 2026-08-20

## Scope and source

This checkpoint makes the cause of a representative interlock stop visible
instead of plotting only its predicate and final conjunction. The implementation
is `alumina-interface` commits
`29bb429197782032ad4d97d8b9d813282c5cb705` and
`3fd6a31aa2d0795684569fa880f6a619f0ecdf87`, against preceding
`alumina-firmware` evidence commit
`0425dc47a53ed87b1a1ef36bbc8a94140e53d075`.

No firmware source, target image, board package, workstation network
configuration, or physical I/O changed. The bare MKS TinyBee V1.0 was not
contacted, reset, flashed, or probed. This remains deterministic host
simulation/editor evidence, not live telemetry, Realtime execution, deployment,
safety qualification, timing evidence, or a physical-input claim.

The interface compiled from the live path-based CSGRS/Hyper stack rather than
an obsolete released CSGRS baseline. Hypercurve changed throughout
qualification. Several optimized-build attempts observed transient incomplete
field/method refactors; unchanged retries later compiled successfully. No Hyper
repository was inspected for status/diff, modified, formatted, staged, pinned,
or treated as a reproducible release snapshot by this checkpoint.

## Exact causal observations

`RepresentativeControlSignal::ExternalPermit` names node 6/output 2, the
external safety permit after the existing audited 50 Hz-to-10 Hz
latest-at-or-before transition. It is appended as probe identity 7, public
component output 7, and panel item 15, preserving all six preceding public
identities. Display-only ordering places the three Boolean lanes in causal
order: external permit, measurement in range, then combined permit.

The representative source retains the external permit through control tick 3
and drops it at tick 4. Exact traces are therefore:

- external permit: `[true, true, true, true, false, false]`;
- measurement in range: `[true, true, true, false, false, false]`; and
- combined permit: `[true, true, true, false, false, false]`.

At tick 3, the UI can state exactly that the range predicate caused the stop
while the independent external permit remained true. The final safe output
remains `[5, 5, 4, 0, 0, 0]` mm; later loss of external permit cannot re-enable
it. The existing disagreeing-operand conjunction test remains independent of
this presentation fixture.

The reusable component now exposes seven public Stream outputs and fifteen
front-panel bindings: eight exact parameter controls, four exact-rational
replay indicators, and three Boolean replay indicators. Canonical probe IDs are
kept in append-only authoring order while visual logic lanes and cursor labels
use causal order. Neither ordering changes graph execution.

## Exact no-op identity repair

The first browser qualification used a profile that was restarted after the
workspace had been persisted. That reload exposed an existing defect:
`GraphProbeDocument::replace_workspace` consumed a revision even when the
incoming `ALGW` had the same canonical identity, needlessly changing the ALGP
digest. The capture was rejected as evidence.

The core replacement boundary now computes the admitted workspace identity
first. An identical identity returns success without mutating the sidecar or
consuming a revision; a changed identity still constructs and validates a
complete candidate before replacement. Core regression coverage
proves the document remains equal after an identical rebind. The browser
persistence regression additionally restores canonical ALGW bytes and proves
the ALGP encoding remains exactly equal. A subsequent fresh-profile browser
run displayed the expected canonical `c2e2e41c…` sidecar identity.

## Canonical identities

The graph, implementation registry, reference workspace, and flattened
workspace remain unchanged. Delaying only an external sample changes `ALGT`;
the additional public observation changes `ALGP`, `ALGC`, and source `ALGH`.
No compatibility shim is provided.

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| `ALGR` graph | — | `96a3348264a9b65d267b45f9a6419a44ee60473fd961abcf4436295e10b3735f` |
| `ALSI` V2 registry | — | `fc68d37f279782c5a5368bc0e44aa695a3b2babbfaf967b09cf4fc75287eae83` |
| `ALGT` trace | 8,292 | `1a1f7e0e80e24f112787bfcc9d5e04c2012a5122a9013d14c998fd6fbdc95f72` |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series `ALGP` | 391 | `c2e2e41cfd3ef8d89605d188a884263ebac57d08907cfa5f38815d63cf323d46` |
| `ALGC` component | 4,815 | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |
| source `ALGH` | 5,814 | `232603de0d4a17ff45b7bda1c363745b316405c99fbdbdc44347fa12bd00e0c8` |
| flattened `ALGW` | 3,755 | `e39c5396539689b8b563a7220e1180d7717e70893b71580ebbe51873fa13b68f` |

## Reproduced checks

Run from `alumina-interface` at the final implementation commit:

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
the intentional compile-fail rustdoc test. Native and WASM warnings-denied
Clippy, strict Alumina rustdoc, the local-source/permissive-license audit,
optimized Trunk assembly, both WASM validators, and compressed-artifact
integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,302,582 | `043dddfdfba7cb92cee1aea22c3174bda3453776573f8dd589c340916cbbb705` |
| `alumina-interface_bg.wasm.gz` | 2,792,475 | `f0b5c4e31b77da131c69d8bc47f682b8f4f056d2214f1365e31362476420445e` |
| `alumina-interface_bg.wasm.br` | 2,208,082 | `00a7622c4d740a14a4b90d7f8aa1be52264609cefd20e56e2e91b84b59d3cc40` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.
The final optimized bundle and dedicated worker loaded from
`127.0.0.1:8765` in headless Chromium using ANGLE/SwiftShader and a fresh
profile. The scrolled 1,440-by-1,057 capture visibly retains canonical ALGP
`c2e2e41c…`, seven probes, the three causal lanes, and exact tick-3 values. It
is 280,346 bytes with SHA-256
`8004672081566cff1d02f838257950b3af6650089b06f30f07e5d93950d8a47f`.
The loopback browser and server were stopped afterward.

## Closed claims and licensing

These observations remain bounded `HostExact` simulation/editor state. They do
not subscribe to a device, grant a pin or telemetry resource, arm an output,
install a graph, or execute the interlock primitives on firmware. Physical
cause attribution requires authenticated capability/configuration binding,
bounded loss-aware telemetry, device-cycle timing evidence, and separately
qualified safety semantics.

Both changes are independently authored under MIT and add no dependency. The
source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks and
permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
