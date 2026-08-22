# M9 selected-component definition canvas evidence

Date: 2026-08-22

Authoritative interface source:
`032eb1fc30154866f805b2fad1bee3f7eff9fef8`
(`feat: edit selected component definitions`)

This checkpoint gives the exact `ALGC` dependency chosen in the existing
`ALGH` library panel its own structural `ALGW` authoring canvas. It closes
selected non-authoritative definition editing; it does not add a second path
to the complete-session control workspace, create nested occurrence records,
or grant firmware, network, storage, GPIO, motion, arming, start, timing, or
safety authority.

## Separate selected-definition surface

The selected-definition editor owns state distinct from the main exact-control
canvas:

- exact selected dependency digest;
- audited-palette cursor;
- stable selected node;
- pending typed-wire source;
- integer drag origin and delta;
- bounded parameter and UTF-8 label drafts; and
- visible transaction status.

Changing library scope clears transient state. A successful recursive
replacement resolves the prior digest through the complete hierarchy
replacement report, so the logical selection follows the newly encoded
dependency without aliasing an old digest.

The visible surface supports audited node creation/deletion, exact label,
domain, and schema-directed parameter edits, integer placement drag, typed
output-to-input wiring, secondary-click disconnect, and stable monotonic node
and wire allocation. Collapsed `alumina.component.instance` placeholders stay
visible, selectable, movable, relabelable, and wireable. `ALGH` owns their
scoped occurrence records, so this canvas disables ordinary-node deletion for
them.

If the selected dependency is the `ALGC` whose embedded `ALGW` is the complete
session's control authority, the entire definition surface is visibly
read-only and directs editing to the main canvas. A descendant edit that would
indirectly replace that authority is rejected atomically by the existing
recursive guard.

## Complete transaction and exact no-ops

Each proposed edit clones only the selected component document and its
embedded workspace. It then:

1. validates the complete candidate workspace and structural presentation;
2. replaces the component's embedded `ALGW` and validates every public and
   panel binding;
3. encodes the proposed `ALGC`;
4. recursively replaces that digest through `ALGH`;
5. regenerates the complete hierarchy package, flattened workspace, and total
   `ALGM` provenance;
6. reruns component-library and flattened ordinary-graph semantics;
7. admits one complete canonical `ALGS`; and
8. records the exact prior session before committing history and browser
   persistence.

There is no partial component or hierarchy commit. Missing/stale scope,
invalid coordinates, incompatible or duplicate wires, invalid labels,
domains, or parameter values, broken component bindings, semantic failures,
placeholder deletion, and indirect control-authority replacement leave the
complete session unchanged.

`GraphWorkspaceDocument::move_node` and `set_parameter` now return an exact
no-op without advancing revisions when the requested coordinate or typed value
already matches. The component transaction also compares the complete
candidate workspace before replacement. Exact placement, label, domain, and
parameter no-ops therefore add neither a graph/workspace revision nor an
`ALGS` history snapshot.

## Native lifecycle and adversarial coverage

`cargo test --workspace --offline` passed:

- 92 application/coordinator tests;
- 82 client tests;
- 181 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 357 unit/integration/compile-fail checks. New regressions prove:

- selected wrapper node creation, exact label metadata, integer movement,
  recursive digest selection retention, unchanged control/probe/cache/root
  authority, complete Undo/Redo, and persisted-session restoration;
- exact workspace placement and typed-parameter no-ops without revision
  advance;
- atomic rejection of `ALGH` placeholder deletion and selected-control
  definition editing; and
- typed wire creation, duplicate rejection, disconnect, and monotonic
  reconnection as wire `#1` then wire `#2` without identity reuse.

## Native and WASM qualification

The following checks passed against a coherent snapshot of the live, read-only
workspace Hyper, Hypercurve, Hypergraphics, and CSGRS paths:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --offline
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

The first whole-workspace Clippy attempt observed Hypercurve between live
coordinated edits and stopped on its temporarily incomplete internal types.
No sibling state was inspected or changed. The next coherent snapshot passed
both strict native and WASM Clippy. Hypercurve emitted only its two current
dependency `dead_code` warnings; the `--no-deps` interface lint remained
warning-free.

The source-policy audit accepted only the sibling Alumina/CSGRS/Hyper paths and
the permissive native/WASM license inventories. `Cargo.lock` remained unchanged
at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `24ef32400cf033df4368cf9a32a93419bf4b434887f5022ae9d9fbb664027084` |
| `alumina-interface.js` | 91,816 | `fc7d26c86fe076d90debb2c6db823b49ada7ebefd533179c6f798a1184c769f1` |
| `alumina-interface_bg.wasm` | 6,752,014 | `e4ebcc25bfcfdd0d5ffb78bd91e41a3cd223136e0857d21670fecb250f4191bf` |
| WASM gzip | 2,971,817 | `bb4c2b24b12860c39fe83547131ce59b1c8b8ad59617d2faa4fdff95fff1afaa` |
| WASM Brotli | 2,336,590 | `57e7556767df61b6639e91302bf6a6422cf45334558c5ac234cd945f61ae0dcd` |

Independent gzip and Brotli decompression both reproduced the uncompressed
WASM SHA-256 exactly.

## Optimized browser definition lifecycle

Headless Chromium loaded only the optimized bundle over loopback HTTP. The
proof cleared origin-local persistence, opened the visible Control graph,
selected `control.reference_pid_wrapper` through the visible library selector,
and used the visible definition palette to create `control.bool.and` node `#2`
at canonical canvas `(328, 28)`. It used the visible metadata editor to rename
that node from `New bool.and` to `nested_logic_exact`. No application test API
proposed or committed either edit.

| State | ALGS bytes / SHA-256 | Wrapper ALGC revision, bytes / SHA-256 | Embedded ALGW revision, bytes / SHA-256 | ALGH bytes / SHA-256 | ALGM bytes / SHA-256 |
| --- | --- | --- | --- | --- | --- |
| initial | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | 1, 1,222 / `941a11bb7a0a34f1ff64f510d474d7df9a3e6bfa5a0323090ffa6715a2b396a9` | 1, 886 / `edc6393f928b20ebadb4aa39dc182096b45ca00ab955c1891bf9b31a214ce04c` | 7,124 / `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | 2,550 / `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| node added | 14,934 / `e83e0256a79ccb5a708a80cb31061bbda343495bd7d8d2b0792060bf3dff3692` | 2, 1,337 / `e7687e9f008245484d5fb2ad84d0cc5fc9651bb10c7265933d08db239235e70e` | 2, 1,001 / `d40a9db7e76298bb2afef8a2e90068266638e0e4adf21af127c10207bd738b53` | 7,239 / `d09afd8fc2e66048f5834157bea9f9bb9900fc9498a41ead2bfcc979f5112469` | 2,599 / `d5034da785771f27947919aafbd0a68ebda2e2f9556d06cdff2189d9fa648266` |
| node renamed | 14,940 / `8b20462c4c518ae91a4ec0eb8d9a268ea0e600e8e5a75c1820990067331dff70` | 3, 1,343 / `229bbf10c3d61ed65e75d7a92473fd3d84aac984230b8c95528a77c278d470a5` | 3, 1,007 / `77d9c7b6e62ea54f9bfec6a8eebd29e194457f28f34dc7a00edc60e6ee35eb75` | 7,245 / `82a87059d9fb6b5c4021f4d48bd654f19ed54587e9e8083aeea276fa31eccf23` | 2,599 / `c16cceb19483d8b7578bcd22372fb561d5ec8c83f2ba8849f7560d66ba62dc58` |

Node creation advanced the node cursor from `2` to `3`, retained the wire
cursor at `1`, and appended exact placement `{ node: 2, x: 328, y: 28 }`.
Label replacement changed graph identity but retained both cursors, the
complete placement list, and every workspace admission limit exactly.

Every authority outside that private definition stayed byte-identical through
both edits:

| Retained authority | Bytes | SHA-256 |
| --- | ---: | --- |
| complete-session control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| selected control `ALGC` | 4,815 | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |
| root `ALGW` | 889 | `3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6` |

The exact root workspace is an important negative result: changing private
implementation without changing the wrapper's public shape must not churn root
node, wire, label, placement, or cursor facts.

Repeating the exact label proposal produced no new bytes or history state.
The visible Undo and Redo controls reproduced the added and renamed `ALGS`
bytes, and a fresh reload restored the renamed node on the selected-definition
canvas. The retained parsed result is 47,633 bytes with SHA-256
`f16bd905bdabc3afd47c74cd14b5887eacb6385754a238695e6cfc3dbfc9f204`.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| selected wrapper definition | 297,911 | `c1a2a44d7adae9bc0e8c6393a86f06112d781a138d1c1132d158663f9eb2efa6` |
| added definition node | 327,761 | `4ad3f74924bf0f11817ba64338946e5ab950bbf91592058ad5a5fc8630671c8c` |
| renamed definition node | 333,828 | `f7496786cd399882abd3f036a816c1a1d9a22bfd09ccc56cd0b949a6d96139bd` |
| exact label no-op | 333,957 | `1e7ff6541c361ae344235114bf86949ba8c4ed3252d87ba584e6f604541a8e1f` |
| visible history controls | 369,362 | `d930a38d51820e5684837584a9cadb6116bb571d2e08e2ab50fcbbfa6dcba889` |
| state after visible Undo | 368,233 | `fb92f4773211bc062018a11b3861c0b483e8e18d07353ed0aba5d4d434ae77d2` |
| reloaded selected definition | 305,650 | `c4f8dcf4e0e5b93eb93c6e54a17d245a639a721154b6581ddc07df260eec1d8a` |

The screenshots and parsed result remain transient
`/tmp/alumina-definition-proof.AtBNQr` evidence and are not repository inputs.
The disposable Chromium profile was removed after replay; Chromium and the
loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read,
or configured. No serial port, GPIO, motor/driver/process power, analyzer,
board AP, or device-network path was used. Workstation Wi-Fi and
NetworkManager were not changed. Browser qualification used loopback HTTP
only.

Hypercurve remained a moving read-only dependency; this work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The interface implementation is independently authored under MIT.
The source-policy audit found no GPL-family dependency, copied external
planner/control implementation, compatibility shim, or retired-repository
path.

Selected non-authoritative component-definition node/metadata/placement/wire
authoring, placeholder-deletion isolation, recursive digest selection,
complete-session history, and origin-local persistence are now closed. Scoped
child-occurrence creation/deletion is separately closed by its
[`authoring checkpoint`](M9-SCOPED-CHILD-OCCURRENCE-AUTHORING.md), and
flattened-to-source traversal is closed by the
[`navigation checkpoint`](M9-EXACT-FLATTENED-SOURCE-NAVIGATION.md). Nested
binding import/exchange, component rename/version evolution, parameter
promotion/overrides, coordinated descendant/control-authority replacement,
runtime panel injection, collaboration/conflict handling, and crash-durable
journals remain separate work. Deterministic named empty component creation is
separately closed by the
[`general component-library creation`](M9-GENERAL-COMPONENT-LIBRARY-CREATION.md)
checkpoint.
