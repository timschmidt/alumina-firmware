# M9 scoped child-occurrence authoring evidence

Date: 2026-08-22

Authoritative interface source:
`5978e46c6e4dd088cf5f4e6f92d3830970fa12fa`
(`feat: author scoped child occurrences`)

This checkpoint closes creation and deletion of child-component occurrences
inside the selected non-authoritative component definition. It extends the
existing selected-definition canvas; it does not create a compatibility path,
change the complete-session control workspace, or grant firmware, network,
storage, GPIO, motion, arming, timing, start, or safety authority.

## One placeholder and one binding lifecycle

`GraphHierarchyDocument::add_nested_instance` now accepts an exact parent
`ALGC`, an exact child already present in the `ALGH` dependency library, a
bounded label, and integer placement. It builds one cloned candidate that:

1. creates a typed `alumina.component.instance` placeholder with the parent's
   monotonic `ALGW` node allocator;
2. appends the exact `(parent scope, node, child digest)`
   `GraphComponentInstance` binding;
3. replaces the parent component and recursively refreshes every affected
   ancestor digest, root binding, and placeholder shape; and
4. validates the complete component DAG, bounds, flattening, and canonical
   order before replacing the live hierarchy document.

`remove_nested_instance` performs the inverse as one candidate. It requires an
existing exact scoped binding, deletes the parent-local placeholder and all its
incident wires, removes the binding, and then runs the same recursive
replacement. `GraphComponentDocument::replace_workspace` rejects first if a
public connector or front-panel binding still names the node. The mutation is
therefore never observable as a placeholder without a binding or a binding
without its placeholder.

The internal replacement path now accepts the candidate instance set in
addition to the replacement component. An exact replacement digest is a no-op
only when both component bytes and the complete instance set match. Digest
collisions, cycles, unknown parent/child scopes, missing occurrence nodes,
depth/expanded limits, shape mismatches, and canonical-order failures reject
without changing revision, dependency, binding, root, or flattened state.

Deleting a child retains the parent workspace's next-node cursor. Re-adding at
the same placement therefore allocates a new node rather than resurrecting the
deleted identity.

## Visible selected-definition authority

The selected-definition editor now retains one exact child-dependency choice.
Its `add child occurrence` action creates the placeholder and scoped binding
together at the next deterministic canvas position. Selecting a collapsed
child exposes `delete child occurrence + wires`; ordinary node deletion still
rejects placeholders as defense in depth. The inspector states that live public
connector or panel bindings must be rebound before deletion.

Every proposed hierarchy edit is made against a clone. Before commit, the UI:

1. resolves the complete-session control `ALGC` through the recursive
   replacement report and rejects if it would change indirectly;
2. rebuilds canonical `ALGH`, flattened ordinary `ALGW`, and total `ALGM`;
3. reruns audited component-library and flattened-graph semantics;
4. constructs and validates one complete `ALGS` over the unchanged control,
   probes, and cached-job branches; and
5. records the exact prior complete session for unified history and marks the
   accepted result for origin-local persistence.

After a successful digest remap, the selected-definition scope follows the new
parent digest and selects the newly allocated child node. Rejection changes
only visible status text.

## Native lifecycle and adversarial coverage

`cargo test --workspace --offline` passed:

- 94 application/coordinator tests;
- 82 client tests;
- 183 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 361 unit/integration/compile-fail checks. New core regressions prove:

- atomic child node `2` creation, parent/ancestor identity remap, unchanged root
  `ALGW`, increased recursive occurrence and flattened-node counts, and exact
  canonical replay;
- deletion without cursor rewind followed by node `3` creation;
- rejection of a publicly bound child, a component dependency cycle, unknown
  parent and child digests, and an unknown scoped node with the complete prior
  hierarchy retained; and
- exact revision and binding effects for every accepted transaction.

New application regressions prove the same add/delete/re-add lifecycle through
complete `ALGS` history and persistence. They require the control `ALGW`,
selected control `ALGC`, `ALGP`, cached-job `ALGW`, and root `ALGW` to remain
exact; verify visible scope selection follows each new wrapper digest; replay
Undo and Redo byte for byte; and restore node `3` plus its binding from persisted
canonical bytes. Separate adversarial coverage proves live public bindings,
self-cycles, and selected-control authoring leave the session, component
package, history, and persistence-dirty state unchanged.

## Native and WASM qualification

The following checks passed against the live, read-only workspace CSGRS/Hyper
paths:

```text
cargo fmt --package alumina-interface --package alumina-interface-core -- --check
cargo check -p alumina-interface -p alumina-interface-core --all-targets --offline
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

The strict `--no-deps` native and WASM interface lints were warning-free.
Hypercurve was being edited concurrently and emitted its current read-only
dependency warnings for one unused diagnostic closure parameter and three
unused internal helper groups. No sibling repository state was inspected or
changed. `Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

The source-policy audit accepted only sibling Alumina/CSGRS/Hyper paths and the
permissive native/WASM license inventories.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `4bd9f4009c9a50219b7e34fba0e5bddf85e0dec01b5b5bba5d9ec53a9a26ae92` |
| `alumina-interface.js` | 91,816 | `3093f7b98f657b07c86cb483a53577b6fdd447bd8cb27d5832104a9e4033a027` |
| `alumina-interface_bg.wasm` | 6,761,639 | `347540b6ef2cd5fce24affc16426d682cafed131bb5f53037114be549c27c567` |
| WASM gzip | 2,976,326 | `f32b9bc3a341756cb787ed5ce3dba631faaf7789f440ed8c2714ff559d3b17eb` |
| WASM Brotli | 2,339,009 | `d2cb15458e52e1f85deceda8ef8c036f1e98f6404d597be599899c7d80022e32` |

Independent gzip and Brotli decompression both reproduced the uncompressed
WASM SHA-256 exactly.

## Optimized browser occurrence lifecycle

Headless Chromium loaded only the optimized bundle over loopback HTTP. The
proof cleared origin-local persistence, opened the visible Control graph,
selected `control.reference_pid_wrapper` through the visible library selector,
and clicked the visible `add child occurrence` action. It then used the new
visible inspector action to delete that occurrence and clicked add again. No
application test API proposed or committed any hierarchy edit.

| State | ALGS bytes / SHA-256 | Wrapper ALGC revision, bytes / SHA-256 | Embedded ALGW revision, bytes / SHA-256 | ALGH revision, bytes / SHA-256 | ALGM bytes / SHA-256 |
| --- | --- | --- | --- | --- | --- |
| initial | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | 1, 1,222 / `941a11bb7a0a34f1ff64f510d474d7df9a3e6bfa5a0323090ffa6715a2b396a9` | 1, 886 / `edc6393f928b20ebadb4aa39dc182096b45ca00ab955c1891bf9b31a214ce04c` | 1, 7,124 / `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | 2,550 / `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| node `2` added | 17,567 / `0175e289c430f729f1ffc0ab83874f0f49dce359cdcea3472cc172357f4d3104` | 2, 1,512 / `39860430fb0ae7af6fcda43e9f994a65c6c609c4efc4980f20dd11001e7f59f4` | 2, 1,176 / `82aee2b114ccc4da779d1b55e736083599e68366f74e0e2c0a31337ec1806310` | 2, 7,483 / `043d047064c1f724402ed169c511f0e11e81408591936922290c7b7c6ab01493` | 4,988 / `6ac7f91f0d3fdef6b5ee23ffb0f1198dd16e18754349e8dd8dcf8866e575af5c` |
| node `2` removed | 14,770 / `ab07bdc48f62769d2b9c835a890d8d6b6b320155b4d9bcccaafa2aa5cb42d21c` | 3, 1,222 / `5000911a0b701e25287f84db6ee003a8909ff9a32ad43a616cf3e714b75286d1` | 3, 886 / `545deb60250b8bae11b596ae2e6ed4e7b05d6d1653cd031238bfeeeff1fe19f4` | 3, 7,124 / `ddfbcb652c496dd6c382871c3c5404ddf9386a55c51b7cb3106a6b996d22e398` | 2,550 / `176306c0de4433aa13ed21c77030111a100f378b36867a5c3563038d59e7a3a6` |
| node `3` added | 17,567 / `a37f43fc8fb2b74aea845bdfa58fdd064b15cf2355d7e47b532cfb56182cc27d` | 4, 1,512 / `307e9712e40195e17d6e2be0864bcaad4cf9564a6012c0579b1c9e4fccdea668` | 4, 1,176 / `a82f0fde3f715cb64f8e855d95cf8fc2fba823998db5af639fbd68e25dfd0a1c` | 4, 7,483 / `2782073c2d322c0a2a3f777a2a182296fffd4431515c870b8e843d75455283c2` | 4,988 / `34dd2ae1ec057b1b41f22ba795b93783aab7ebc632a0be7a9afbcb75cdc9d39d` |

The first and final child placements are exactly `(328, 28)`. The first
workspace advances its next-node cursor from `2` to `3`; deletion retains `3`;
the second add allocates node `3` and advances the cursor to `4`. Instance
bindings progress from two to three, back to two, then to three. The final
bindings name root wrapper
`307e9712e40195e17d6e2be0864bcaad4cf9564a6012c0579b1c9e4fccdea668`
and nested nodes `1` and `3`, both bound to unchanged control component
`10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228`.

Every external authority remained byte-identical through all three edits:

| Retained authority | Bytes | SHA-256 |
| --- | ---: | --- |
| complete-session control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| selected control `ALGC` | 4,815 | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |
| root `ALGW` | 889 | `3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6` |

The visible Undo control restored the exact removal `ALGS`; Redo restored the
exact node-`3` `ALGS`; a fresh page reload retained the same node-`3` binding,
cursor `4`, and complete bytes. The retained parsed result is 65,815 bytes with
SHA-256
`486b1d2e3851d474df4e5c8fc51c350b393600de882c5d6aee5186d18add37f9`.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| selected wrapper definition | 268,091 | `64f73fe416c18ba444f7d701b3ee0f9980d0b33a469cbca65398f92e66d329b3` |
| added child node `2` and delete action | 303,954 | `0d23414af80362762d78b30fca8fc24c8d919b2ebadd21c5defbe04e43bab240` |
| removal state | 282,137 | `bfb95bcaf331f9e542fb5a2d02b9e790c43a09c69c5fc88d6663760f212814c9` |
| re-added child node `3` and delete action | 304,053 | `d8df00cf2f379ea472a40ff7678c62f208f618013470171065208546ac321ce3` |
| visible history controls | 318,040 | `3f8a29da91b8d806d2760e83047173a9873c645b1d6f912a4bd5a3cee73eff18` |
| state after visible Undo | 314,467 | `e4d76439baa9f771c0024395f79e4413eea02a93b17a2ea9a3aac8f74c926d78` |
| reloaded wrapper definition | 270,364 | `9e316d1b4dd161e1cac331b495ac05b92c9b2042eb1a7e2856f5b08d048d3ead` |

The screenshots, bounded locator frames, and parsed result remain transient
`/tmp/alumina-child-occurrence-proof.hY47D8` evidence and are not repository
inputs. The disposable Chromium profile was removed after replay; Chromium and
the loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or device-network path was used. Workstation Wi-Fi and NetworkManager were
not changed. Browser qualification used loopback HTTP only.

Hypercurve remained a moving read-only dependency; this work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The interface implementation is independently authored under MIT.
The source-policy audit found no GPL-family dependency, copied external
planner/control implementation, compatibility shim, or retired-repository
path.

Atomic scoped child-occurrence creation/deletion from existing library
dependencies, public-binding vetoes, monotonic child identities, recursive
digest selection, complete-session history, and origin-local persistence are
now closed. Child rebinding/replacement, nested binding import/exchange,
general component-library creation, parameter promotion/overrides, coordinated
descendant/control-authority replacement, runtime panel injection,
collaboration/conflict handling, and crash-durable journals remain separate
work. Interactive flattened-to-source traversal is separately closed by the
[`exact flattened-source navigation`](M9-EXACT-FLATTENED-SOURCE-NAVIGATION.md)
checkpoint.
