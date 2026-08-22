# M9 selected-library connector authoring evidence

Date: 2026-08-22

Authoritative interface source:
`4f05a167553697ffc98896de25aade6d30e57a21`
(`feat: edit selected library connectors`)

This checkpoint extends stable public-connector authoring from only the
complete-session control `ALGC` to whichever exact `ALGC` dependency is chosen
in the `ALGH` library selector. Accepted edits recursively rebuild affected
ancestors and root placeholders while preserving the complete-session control
authority unless that control component is itself the explicit edit target.
It is host-side authoring/compiler behavior only and grants no firmware,
network, storage, GPIO, motion, arming, start, timing, or safety authority.

## Complete recursive replacement report

`GraphHierarchyDocument::replace_component_with_report` wraps the existing
transactional stable-ID replacement and returns a canonical
`GraphHierarchyReplacementReport`. The report retains:

- the exact requested old and new component digests;
- every directly or recursively rebuilt old-to-new dependency identity,
  sorted by old digest;
- exact lookup and resolution of any prior dependency identity; and
- an empty remap list for an exact no-op.

The hierarchy is still reconstructed and completely validated before commit.
The report is therefore an effect of a successful candidate, not permission to
skip hierarchy validation. Core regressions prove a direct replacement, an
exact no-op, and a leaf edit whose changed connector pane rebuilds its wrapper;
the latter report names both the leaf and wrapper remaps.

## Selected dependency transaction

The visible connector editor now follows the adjacent `ALGH` dependency
selector. It labels the selected name and digest and distinguishes the
`control authority` from another `library dependency`. Its existing bounded
input/output add, metadata-update, and reference-safe removal controls operate
on that selected document with stable IDs and validated internal endpoints.

One accepted library edit performs the full transaction:

1. clone and mutate the selected dependency document;
2. encode its proposed exact `ALGC` identity;
3. replace it through the complete recursive hierarchy report;
4. remap the logical UI selection through every rebuilt identity;
5. refresh root placeholders, `ALGH`, flattening, and total `ALGM` provenance;
6. rerun component-library and ordinary graph semantics;
7. construct one complete canonical `ALGS`; and
8. record the exact prior session before history/persistence commit.

An ordinary library edit retains the selected control `ALGC`, its embedded
control `ALGW`, the bound `ALGP`, and the cached-job workspace exactly. If an
edited descendant would recursively rebuild the `ALGC` that supplies the
complete session's control workspace, the candidate is rejected atomically.
That case remains closed until a coordinated control-workspace replacement
transaction exists; it cannot silently change authority through a library
edit.

The application lifecycle regression selects the root-reachable wrapper,
updates stable output `#1`, exercises an exact no-op, removes output `#7`, and
adds fresh monotonic output `#8`. It proves recursive digest/root refresh,
unchanged control/probe/cache authority, logical selection retention, unified
Undo/Redo, and exact persisted-session restoration. A separate adversarial
regression makes the wrapper the control authority, attempts to edit its child,
and proves byte-exact session/component/history retention on rejection.

## Native and WASM qualification

`cargo test --workspace --offline` passed:

- 89 application/coordinator tests;
- 82 client tests;
- 181 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 354 unit/integration/compile-fail checks. The following stricter checks
also passed against the live, read-only workspace Hyper, Hypercurve,
Hypergraphics, and CSGRS paths:

```text
cargo check -p alumina-interface --all-targets --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Gzip and Brotli decompression reproduced the optimized WASM SHA-256 exactly.
`Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `2e3f1133188931ba352c72e80a26d044a89ad4dc5d4fceab1182ce5337866ddd` |
| `alumina-interface.js` | 91,816 | `18f47151dded0d4359e3976f2521191dbe82218cd45d2fd1133fc6a93aba726b` |
| `alumina-interface_bg.wasm` | 6,733,658 | `6510faeadbc14072d84f86e2e47fa695f8eab32665022da0bd81f48ecf57eef1` |
| WASM gzip | 2,964,565 | `558236c6c86c58f65ba4cad34b1d0450cf1c85372bd00ba3afbedc606dd2cce4` |
| WASM Brotli | 2,329,947 | `37494d5c76a2df9f1b2c9f410557d020182b53217427145ef4ab50af1ee39b73` |

The only dependency diagnostic was the already-known live Hypercurve
`dead_code` warning; no sibling repository was modified.

## Optimized browser selected-library lifecycle

Headless Chromium loaded only the final optimized bundle over loopback HTTP.
The proof cleared origin-local persistence, opened the visible Control graph,
used the visible library selector to choose
`control.reference_pid_wrapper`, and used the visible connector editor to
rename stable output `#1` to `wrapper_error_exact` at internal endpoint
`#1.1`. No application test API proposed or committed the edit.

| State | ALGS bytes / SHA-256 | Control ALGC SHA-256 | ALGH bytes / SHA-256 | Root ALGW bytes / SHA-256 | Wrapper bytes / SHA-256 | ALGM SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| initial | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` | 7,124 / `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | 889 / `3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6` | 1,222 / `941a11bb7a0a34f1ff64f510d474d7df9a3e6bfa5a0323090ffa6715a2b396a9` | `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| updated | 14,798 / `fa299d7e23afe2127213fd742eafb7e2ed2618bf82346f972c2398fd4527e6e6` | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` | 7,152 / `c5d8d914295e49e70cb8772e848a93bcfa4e108057ed421285dbf0892ec2a8bd` | 903 / `64af6cd68023b9cffd6f5f100c3aefe52d1c066a9a62a68b2ba5fd26d16c0241` | 1,236 / `2fef24f3de611b738819881cdaf5bdcc0e675744865fcd64a9c6ba632ce391de` | `22cabb738fd558438095e7eed499d9fc382b726c67029ba4d5459fe7efca0ed8` |

The wrapper's embedded 886-byte `ALGW` remained exact at SHA-256
`edc6393f928b20ebadb4aa39dc182096b45ca00ab955c1891bf9b31a214ce04c`.
Root placeholder text changed with the stable public name, while root node and
wire identities, placement, and allocation cursors remained exact. Every
non-library authority also remained byte-identical:

| Role | Bytes | SHA-256 |
| --- | ---: | --- |
| control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |

Repeating the same metadata proposal was an exact no-op. Visible Undo and Redo
reproduced the initial and updated bytes, and a fresh reload restored the exact
updated session. The retained parsed proof document is 19,693 bytes with
SHA-256
`e3045f32729d5e63f99cc0db39b46cddef101da0a47cecccd2dbd895f0898fe5`.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| library selector | 328,065 | `441b993f846972f695b0ffa33ecc614a188b2f612cb11aa013540301efc3cd93` |
| open library-selector popup | 332,512 | `2d01a62c588921c4f27db1f401df8e8a51d4fae583a301e02be166a43b4072a7` |
| selected wrapper connector editor | 314,774 | `5ef4db2d0f011b171910d0584616f1893fac0f7b8933694c192ffc4d194cc089` |
| updated wrapper output | 321,726 | `7bbe4d0aaa4d16be2bc3a9fb7d8c92c348f0370e8d0e97b7d0ae6ed3d91dab78` |
| exact no-op | 319,087 | `10f623c9a8da4b6fe7956b9868cd101dfa43f7faf35f0038df2b53ca8d4e3931` |
| exact reload | 234,497 | `09ed000be46a727409a3cc34aefec2c69a5ab17927fcc2c7006c4289a12d2751` |

The screenshots and parsed proof remain transient `/tmp` evidence and are not
repository inputs. The disposable Chromium profile was removed after replay;
Chromium and the loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or device-network path was used. Workstation Wi-Fi and NetworkManager were
not changed. Browser qualification used loopback HTTP only.

Hypercurve remained a moving read-only dependency; this work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The implementation is independently authored under MIT. The
source-policy audit accepted the existing permissive native/WASM inventories
and found no GPL-family source, dependency, compatibility layer, or copied
external planner/control implementation.

Selected-dependency public connector editing, complete recursive identity
reports, logical selection retention, control-authority isolation, complete-
session history, and origin-local persistence are now closed. General
component-library creation, nested component-definition canvases, coordinated
descendant/control-authority replacement, runtime input injection, connector
protocol lowering, collaboration/conflict handling, and crash-durable history
remain separate work. `ALGC`/`ALGH`/`ALGM`/`ALGS` authoring grants no firmware
opcode, resource ownership, cache preparation, execution, arming, start,
timing, motion, or safety authority.
