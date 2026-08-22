# M9 exact flattened-source navigation evidence

Date: 2026-08-22

Authoritative interface source:
`0f91a778c73e30be203f82af825153dc981328fa`
(`feat: navigate exact flattened sources`)

This checkpoint closes read-only traversal from any final `ALGM` node or wire
to its exact root or component-definition source. It adds no compatibility
path and grants no firmware, network, storage, GPIO, motion, arming, timing,
start, or safety authority.

## Exact occurrence resolution

`GraphHierarchyDocument::component_at_instance_path` now walks a nonempty
root-to-nested placeholder path through canonical scoped
`GraphComponentInstance` bindings. Each path element must exist in the current
scope; the scope for the next element is the exact component digest resolved
at the current element. Empty, missing, or discontinuous paths return no
component authority.

The source-navigation boundary never trusts a displayed or retained origin by
itself. For each open request it:

1. requires the current fresh flattening to map the selected final ID to the
   same complete node or wire origin;
2. resolves every component occurrence path against the current `ALGH`;
3. requires the resolved component digest to name a current admitted
   dependency;
4. requires the component-local node or wire to remain present; and
5. only then changes transient editor focus.

Root nodes and wires scroll to the structural root canvas and highlight the
exact source. Component nodes select the exact dependency plus local node. A
component wire reports both exact endpoints, opens the exact dependency, and
selects its local target node. An authoritative component opens the main
control canvas; a non-authoritative component opens the selected-definition
canvas. The first occurrence-path item also selects the corresponding root
occurrence.

The reference proof resolves `[1]` to the wrapper digest and `[1, 1]` to the
authoritative PID component. Empty `[ ]`, missing root `[1]` in the independent
core fixture, and discontinuous `[2, 2]` paths return no authority.

## Transient browser state

The visible `Flattened hierarchy source browser` borrows the ordered fresh
node/wire provenance slices. It formats the selected row on an ordinary frame
and materializes other labels only while the corresponding combo is open, so
the 4,096-node/8,192-wire admission ceilings do not cause steady-state label
allocation.

The browser retains selected final IDs, the last valid complete origin, status,
and one pending destination scroll only in memory. A successful open clears
transient pending-wire and drag gestures for the destination editor. It does
not encode a canonical artifact, advance a revision, push an undo/redo state,
or mark origin-local persistence pending. A forged or stale request changes
only visible rejection status and retains the last valid destination. After a
canonical hierarchy edit, reconciliation clears a last-opened final/origin
pair and pending scroll if that exact pair is absent from the regenerated map.

## Native and adversarial coverage

The complete qualification passed:

- 96 application/coordinator tests;
- 82 client tests;
- 183 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 363 unit/integration/compile-fail checks. New regressions prove:

- canonical root-to-nested path resolution and rejection of empty, missing,
  and discontinuous paths;
- final node `[1/1]` traversal into the authoritative control `ALGC` and its
  exact local node;
- final wire `[1/1]` traversal into the same definition and its exact local
  target node;
- traversal into a private wrapper definition after a canonical ordinary node
  is added there;
- byte-identical `ALGS`, complete component package, history stacks, and
  persistence state across each successful open;
- fail-closed rejection of a forged root origin while the last valid source is
  retained; and
- clearing a stale final/origin selection and pending scroll after a private
  node edit recursively remaps the component digest and regenerates `ALGM`.

## Native and WASM qualification

The following checks passed against the live, read-only workspace CSGRS/Hyper
paths:

```text
cargo fmt --package alumina-interface --package alumina-interface-core -- --check
cargo check -p alumina-interface -p alumina-interface-core --all-targets --offline
cargo test --workspace --all-targets --offline
cargo test --doc --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -cd dist/alumina-interface_bg.wasm.gz | cmp - dist/alumina-interface_bg.wasm
brotli -d -c dist/alumina-interface_bg.wasm.br | cmp - dist/alumina-interface_bg.wasm
```

The strict `--no-deps` native and WASM interface lints were warning-free.
Hypercurve was being edited concurrently and emitted its current read-only
dependency warnings for one unused diagnostic closure parameter and three
unused internal helper groups. Those dependency repositories' Git state was
not inspected or changed. `Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

The source-policy audit accepted only sibling Alumina/CSGRS/Hyper paths and the
permissive native/WASM license inventories.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `b4c71d8571a37d7ffab0f68f28c9cbdc34aed0ac6368187825080fd70a2486fb` |
| `alumina-interface.js` | 91,816 | `5e720179915353c60a2b5697b1a3063fdf34613e98ca01ba4293cd6ccfa0184b` |
| `alumina-interface_bg.wasm` | 6,771,924 | `b40b9d9d20b766c2f423701855b94b548fa5c93dbdb4c2c07e8e5ca9cc4e7259` |
| WASM gzip | 2,981,863 | `312dad69759d3b265415e7aa98ad9c688a948aa9c48bec955a58450d2a91c664` |
| WASM Brotli | 2,345,163 | `6526be76d06ed993ae6635d5218200729b9515ad2438d6d74fff1f0beb5cbe3c` |

Independent gzip and Brotli decompression both reproduced the uncompressed
WASM exactly.

## Optimized browser traversal

The application page and proof script addressed only the optimized bundle,
loopback HTTP, and the isolated loopback Chromium debugging port. The proof
cleared origin-local persistence, opened the visible Control graph, and found
the visible source browser. Its default selections were:

- final node `n3 Setpoint source`, exact occurrence `[1/1]`, authoritative
  `ALGC` local node `n1`; and
- final wire `w1`, final endpoints `n3.p1` to `n6.p1`, exact occurrence
  `[1/1]`, authoritative `ALGC` local wire `w1` from `n1.p1` to `n4.p1`.

Clicking `open exact node source` visibly reported the exact occurrence,
scrolled to the authoritative main canvas, and selected `#1 Setpoint source`.
Clicking `open exact wire source` visibly reported both local endpoints,
scrolled to the same canvas, and selected target `#4 Setpoint 50 Hz to 10 Hz`.
After both actions, the browser storage value remained byte-for-byte the exact
14,770-byte reference `ALGS` with SHA-256
`d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d`.
A fresh page reload retained those exact canonical bytes and reset the
navigation status to its transient initial prompt.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| source browser before node open | 297,993 | `4a4eaa0d63dbdfaf0070db876fd019ab7693eb6022f147ec0a106a23b608f997` |
| authoritative node destination | 300,474 | `8d7c07c535ec3d8968f186dadccfc267755d883ab4d305be66a2c052ba0b92ca` |
| visible exact-node success | 307,880 | `b8c5cd7346221b8478a0c15a0fb0ec4d44f29a6ef6890b4dfbb927218d05dd6f` |
| authoritative wire-target destination | 303,339 | `42fdf210dc6fd7d7eae996a951e40b177da02d3a50430c7d2e233010b0c969d7` |
| visible exact-wire success | 309,356 | `1117e40d60aa06c52c189454e7523283e2c0000ec8dc801fbec6721a8031d152` |
| source browser after fresh reload | 294,753 | `5ff3ad79d9fceb98bedd6adc8756c4a1a0ba2bbc4a34b2d631b8da316365fd3f` |

The retained parsed result is 1,849 bytes with SHA-256
`c9cd2c0953e3c723610c5089131700b2dd10ed4eee0eec9584801fd31eb8b1bd`.
Screenshots, bounded OCR locator frames, and the result remain transient
`/tmp/alumina-source-navigation-proof.1ilzil` evidence and are not repository
inputs. The disposable Chromium profile was removed after replay; Chromium and
the loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or device-network path was used. Workstation Wi-Fi and NetworkManager were
not changed.

Hypercurve remained a moving read-only dependency; this work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The interface implementation is independently authored under MIT.
The source-policy audit found no GPL-family dependency, copied external
planner/control implementation, compatibility shim, or retired-repository
path.

Exact final-to-origin revalidation, scoped occurrence-path resolution, root and
component destination focus, private/authoritative node selection, local wire
target selection, one-shot scrolling, stale-origin reconciliation, and
canonical session/history/persistence isolation are now closed. Dedicated
component-local wire selection, child rebinding/replacement, nested binding
import/exchange, general component-library creation, parameter
promotion/overrides, coordinated descendant/control-authority replacement,
runtime panel injection, collaboration/conflict handling, and crash-durable
journals remain separate work.
