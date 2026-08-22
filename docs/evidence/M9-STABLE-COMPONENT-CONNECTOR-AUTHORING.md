# M9 stable component-connector authoring evidence

Date: 2026-08-21

Authoritative interface source:
`ad14951ceaf02abaeed1f889e9f87018a0fbfe3d`
(`feat: author stable component connectors`)

This checkpoint closes transactional public-input/public-output authoring for
the complete-session selected `ALGC` and closes recursive, stable-identity
refresh of every affected `ALGH` placeholder. It is host-side
authoring/compiler behavior only. It changes no firmware protocol, runtime
package, device resource, GPIO, Wi-Fi, cache, motion, arming, start, or safety
authority.

## Transactional connector authority

`GraphComponentDocument` now exposes six bounded clone-and-reconstruct
operations:

- `add_input` and `add_output` allocate fresh monotonic stable identities;
- `update_input` and `update_output` replace one stable name and one exact
  internal endpoint together, with byte-identical proposals as exact no-ops;
  and
- `remove_input` and `remove_output` remove one known connector without
  rewinding or reusing its identity.

Complete component reconstruction retains the existing name, endpoint,
direction, type, uniqueness, count, byte, identity-cursor, workspace, and
front-panel invariants. A public input may expose only an internal input with no
wire owner. A retained front-panel control or indicator now returns an explicit
`ComponentInputInUse` or `ComponentOutputInUse` removal error. Exhaustion,
unknown identities, malformed/duplicate names, duplicate or directionally
invalid endpoints, internally owned inputs, retained panel bindings, and any
other complete-document rejection leave the prior component unchanged.

The component regression proves input and output add/update/no-op/remove,
duplicate-endpoint rejection, explicit panel-binding rejection, monotonic
output identities `2` then `3`, monotonic input identities `1` then `2`,
unchanged workspace authority during connector-only edits, and exact canonical
replay.

## Stable-ID hierarchy refresh

Instance-node port numbers remain a derived presentation of each component's
stable connector identities: sorted inputs occupy the first ports and sorted
outputs follow them. They are not durable connector identity. The prior
component-replacement path could validate a same-sized positional pane while
silently reinterpreting an incident endpoint after connector insertion or
removal. That path is retired.

`GraphHierarchyDocument::replace_component` now performs one atomic recursive
refresh:

1. encode the proposed selected component;
2. find every occurrence of each changed child in its exact root or parent
   scope;
3. map each incident wire target through the old stable
   `GraphComponentInputId` and each incident wire source through the old stable
   `GraphComponentOutputId`;
4. resolve that same identity in the replacement pane and derive its new port;
5. rebuild changed placeholder shapes while preserving node identity, label,
   integer placement, wire identity, and allocation cursors;
6. remap parent public-input/public-output endpoints by the same stable IDs;
7. rebuild a changed parent `ALGC` once, then propagate its new digest toward
   the root until the bounded acyclic dependency graph converges;
8. remap every affected instance child and scope digest; and
9. reconstruct and validate the complete candidate hierarchy before commit.

Removing a stable input or output still referenced by a root/parent wire or a
parent public connector returns `RemovedInstanceInput` or
`RemovedInstanceOutput`; the old positional port can never alias a surviving
connector. Type changes are rechecked by complete graph validation. Digest
collisions, revision overflow, invalid new shapes, cycles, context changes,
limits, and flattening failures remain atomic.

An internal, panel-only, or other child change whose public placeholder shape
is byte-identical does not churn its parent's `ALGC` or the root workspace.
Conversely, a real public-shape change rebuilds only affected parents. Core
regressions prove a new input shifting a used output from instance port `2` to
`3`, atomic rejection of referenced input/output removal, recursive wrapper
digest/scope/endpoint refresh, unchanged root bytes when the wrapper's own
public pane is unchanged, exact replay, and successful flattening.

## Visible connector-pane editor and complete-session commit

The Control graph's reusable-component section now contains a visible
canonical connector-pane editor for the complete-session selected `ALGC`:

- the public-input chooser contains only unowned internal input endpoints;
- the public-output chooser contains only internal outputs not already exposed;
- input and output creation use validated stable names and fresh monotonic IDs;
- one combined selector displays stable ID, derived instance port, stable name,
  and exact internal endpoint;
- selected input/output name and endpoint metadata can be applied or reset; and
- selected connectors can be removed through the same transactional boundary.

Endpoint labels retain graph node/port IDs, node and port names, registered type
names, and exact type IDs. The UI does not accept raw connector IDs, raw bytes,
or positional instance ports from the user.

Every accepted proposal performs the complete existing authoring transaction:

1. mutate a cloned selected `ALGC`;
2. encode its canonical identity;
3. recursively refresh the complete `ALGH` by stable connector identity;
4. audit every component-library draft;
5. flatten to ordinary `ALGW`;
6. regenerate total `ALGM` provenance;
7. rerun ordinary semantic admission;
8. construct the complete canonical `ALGS`; and
9. record the exact prior session before component/history/persistence commit.

The UI lifecycle regression proves output ID `8`, deletion and fresh ID `9`,
input ID `1`, deletion and fresh ID `2`, endpoint/name updates, exact no-ops,
explicit rejection of a panel-bound output removal, recursive wrapper
replacement, isolation of control/probe/cached-job/root authority, unified
Undo/Redo, and exact persisted-session restoration.

## Native and WASM qualification

`cargo test --workspace --offline` passed:

- 87 application/coordinator tests;
- 82 client tests;
- 181 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 352 unit/integration/compile-fail checks. The following stricter checks
also passed against the live, read-only workspace Hyper, Hypercurve,
Hypergraphics, and CSGRS paths:

```text
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

Gzip and Brotli decompression both reproduced the optimized WASM SHA-256
exactly. `Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `4fbc447a5f150589616c0cbe8ffe791a93ae193b15e633619af6c8f57656e8fd` |
| `alumina-interface.js` | 91,816 | `11a33f4d17549a727b71a72caf7658d74728e264666b354d0310424590600786` |
| `alumina-interface_bg.wasm` | 6,734,684 | `6f629825c294d8c31e1e5efefba85565de3fcacb2d04db97c92b490d47b4abec` |
| WASM gzip | 2,964,946 | `ba47c32771c3f6076abaa0a542109003a38fb9842fe2df78abd37133c49365ab` |
| WASM Brotli | 2,330,888 | `a425b1f29e32cd9cf20635645de4ad5e46dd35a5167d923fa39fa5398366b5bd` |

## Optimized browser connector lifecycle

Headless Chromium loaded only the final optimized loopback bundle. The test
cleared origin-local storage, opened the visible Control graph view, scrolled
to the actual connector editor, typed `browser_diagnostic`, clicked the visible
`add public output` control, edited the selected stable name to
`browser_diagnostic_exact`, and clicked the visible metadata control. No
test-only application API proposed or committed either edit.

The selected component began with no public inputs, seven public outputs, and
next output ID `8`. The browser created stable output ID `8` at exact internal
endpoint `#1.1`; with zero inputs, its derived instance port was `8`. The next
output cursor advanced to `9` and did not advance for the metadata update.

| State | Output `#8` | ALGS bytes / SHA-256 | Selected ALGC SHA-256 | ALGH SHA-256 | ALGM SHA-256 |
| --- | --- | --- | --- | --- | --- |
| initial | absent | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` | `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| added | `browser_diagnostic` ← `#1.1` | 14,832 / `54f38bbb4b7bea640dfeedb76b48e063063d46403f80e3468b06f54e72113fcb` | `052307a2592cb16773c02c1b3aac9e33ed422c8f78dc954675887b6be1dd97d4` | `3a7190ed2f2c8af7d5358d3853554eef72a38ba367577ca61bba9f7790a82c40` | `d286893e72d14734319434ae6ad784c3fb1bacfaa06614e017033c4b59769695` |
| updated | `browser_diagnostic_exact` ← `#1.1` | 14,844 / `62ab14a95032bf6f84afe320cab8f051a4606d791698650d5c03074e24c1ab35` | `81490d0746f95dda8d7264b00200ac679baff26802b720c8c4487487d5edd290` | `f9c40d41395bdd47d6508740f67e9fe6523c13f2f4caef5042c1785819b4b84c` | `de2deb014f4b2e0233197550899d4f8003f1d0ac2a6268da61d30ebd2ba29c17` |

The nested wrapper changed recursively while retaining its independent seven
public outputs:

| State | Wrapper bytes | Wrapper SHA-256 |
| --- | ---: | --- |
| initial | 1,222 | `941a11bb7a0a34f1ff64f510d474d7df9a3e6bfa5a0323090ffa6715a2b396a9` |
| added | 1,252 | `6ec7a8df7124cb70e5c51ee4e366090e0ec5c11e8d614048466cfd5c43d0b65d` |
| updated | 1,258 | `b4b1865dbfe6bd93afa2fa32c036f59d97d9d2e9c6a67999d264f717cd0c1834` |

The hierarchy root remained the exact 889-byte workspace with SHA-256
`3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6`.
The selected component's 3,755-byte embedded workspace, zero-input list,
fifteen panel records, input/panel cursors, and all seven prior outputs remained
exact. All noncomponent authoring roles also remained byte-identical:

| Role | Bytes | SHA-256 |
| --- | ---: | --- |
| control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |

The test scrolled to the actual visible history controls. Two visible Undo
operations reproduced updated → added → initial bytes exactly; two visible Redo
operations reproduced initial → added → updated bytes exactly. A fresh page
reload retained the exact updated session. OCR independently retained the
visible connector-editor heading and authored final name.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| parsed result document | 25,568 | `1b23129f1bba817d682c3f55344b6925b95c21c76deafbf750147b26d3ea6e13` |
| initial visible connector editor | 331,991 | `6d08b56f3e6286acdd05c7b7b2b160407ee95be69afb0560f9fd1525d63ac29d` |
| added output | 337,301 | `fa5f420366a9af06fa2e5832fa56f29a9018aafa1e2c51529bebda5129ef212e` |
| updated output | 337,711 | `1283edcf62e295360f8df5a753decf5b85028ea0156914ccdb7d088a9de159c0` |
| visible history controls | 409,324 | `5514fda594c53e60f6e7f952851cc866ebf594341f38bb811f044b3eca23eac8` |
| exact updated-session reload | 246,744 | `898dc4d0cbb60da6e0267df90e92a14cef49f9855a2ad0a03a75aa8ac3ec9e5e` |

The screenshots, OCR input, and parsed result remain transient `/tmp` evidence
and are not repository inputs. Chromium and the loopback server were stopped
after replay.

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

Selected-component public input/output creation, exact metadata update,
reference-safe removal, monotonic identities, recursive stable-ID placeholder,
wire, and parent-connector remapping, complete-session history, and persistence
are now closed. Editing arbitrary nonselected library dependencies, creating
new component definitions from an arbitrary workspace selection, nested
definition canvases, runtime input injection, connector protocol lowering,
collaboration/conflict handling, and crash-durable history remain separate
work. `ALGC`/`ALGH`/`ALGM`/`ALGS` authoring grants no firmware opcode, resource
ownership, cache preparation, execution, arming, start, timing, motion, or
safety authority.
