# M9 exact scoped child-occurrence rebinding evidence

Date: 2026-08-22

Authoritative interface source:
`3f0768e040994a50d7270ffe8ffe3bcfaf3aaf02`
(`feat: rebind exact child occurrences`)

This checkpoint closes transactional replacement of one existing child
occurrence inside a selected non-authoritative component definition. It
extends the canonical `ALGH` V2 hierarchy and complete `ALGS` V1 authoring
transaction; it does not add a compatibility identifier, alias table, second
hierarchy format, firmware command, or raw digest editor.

This is host authoring functionality. Firmware does not receive or interpret
`ALGC`, `ALGH`, `ALGM`, `ALGW`, `ALGR`, or `ALGS`. The work grants no firmware,
network, storage, GPIO, motion, arming, timing, start, or safety authority.

## One exact scoped transaction

`GraphHierarchyDocument::rebind_nested_instance(parent, node, component)`
identifies one occurrence by its exact component scope and parent-local node.
The target must already be an admitted library dependency. The transaction:

1. resolves the exact old child, replacement child, and parent definition;
2. derives the replacement placeholder from the target's public connectors;
3. remaps every live parent-local wire and public connector endpoint by stable
   child connector ID;
4. retains the placeholder node, label, integer placement, and parent node/wire
   allocation cursors;
5. replaces only the named scoped `GraphComponentInstance` binding;
6. revalidates dependency cycles, context, depth, expansion, and all byte/count
   limits; and
7. commits only after complete hierarchy construction succeeds.

A target identical to the current child is an exact no-op: no hierarchy
revision, history state, or persistence write is created.

If the old and new child derive the same public placeholder shape, the parent
`ALGC` document remains byte-identical. Only the scoped binding and complete
`ALGH` revision change at that parent boundary, so the component replacement
report intentionally contains no digest remaps. This is distinguishable from
the exact same-target no-op because canonical hierarchy bytes change.

If the public shape changes, stable connector IDs preserve compatible live
endpoints and the existing recursive replacement path assigns a new parent
identity and cascades every affected ancestor/root binding. Removing a live
connector, changing its type incompatibly, naming an unknown component or
instance, introducing a cycle, exceeding a limit, or failing context replay
rejects the complete candidate without mutation.

## Visible selected-definition workflow

The placeholder inspector reports the exact bound child name and digest. The
existing `Child component` selector supplies the replacement target, and the
new `rebind child to selected component` action enters the same complete
transaction used by all hierarchy authoring:

1. clone and rebind the canonical `ALGH`;
2. reject a recursive replacement of the complete-session control `ALGC` until
   a coordinated control-workspace transaction exists;
3. freshly flatten the whole candidate hierarchy;
4. regenerate total `ALGM` provenance;
5. rerun audited ordinary-node semantics and complete `ALGS` admission; and
6. record one prior complete session before committing persistence.

The selected definition and logical node follow any recursive digest report.
Compatible pending-wire state is reconciled, stale exact source focus is
cleared, and transient editor selection remains outside canonical artifacts.
A same-shape child may therefore change flattened behavior and provenance even
though the immediate parent `ALGC` stays exact; the complete `ALGH`, flattened
workspace, and `ALGM` are always derived afresh.

## Native and adversarial coverage

The complete qualification passed:

- 102 application/coordinator tests;
- 82 client tests;
- 186 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 372 unit, integration, and compile-fail checks. New regressions prove:

- same-shape occurrence rebinding changes only the exact scoped binding at the
  parent boundary;
- a same-target request is byte-for-byte a no-op;
- placeholder node, label, placement, allocation cursors, and stable-ID-
  compatible live endpoints survive;
- a changed public shape recursively replaces the parent while stable output
  IDs remap changed local port ordinals and public endpoints;
- root and nested bindings follow every recursive digest replacement;
- fresh flattening and canonical replay succeed after each accepted path;
- removed live connectors, incompatible shapes, cycles, unknown targets,
  unknown instance nodes, and authoritative-definition editing reject
  atomically;
- the complete session, history stacks, and pending persistence remain exact
  after every rejection;
- accepted UI rebinding records one complete `ALGS` state and reconciles the
  selected node and compatible pending output; and
- Undo, Redo, and persisted restore cannot separate the placeholder from its
  scoped binding or regenerated source map.

## Native, WASM, source, and release qualification

The following checks passed against the live, read-only workspace CSGRS/Hyper
paths:

```text
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip and Brotli integrity checks
git diff --check
```

The owned native and WASM interface crates were warning-free under strict
`--no-deps` Clippy. Hypercurve was being edited concurrently and emitted only
its then-current read-only unused-variable/helper warnings during different
gates. No Hyper/CSGRS repository state was inspected or changed. `Cargo.lock`
remained SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

The source-policy audit accepted only sibling Alumina/CSGRS/Hyper paths and the
permissive native/WASM license inventories.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `0032b3f0e5fe664cc04070157a22cf227c6c0c44727f240979007b51541571c4` |
| `alumina-interface.js` | 91,816 | `8477f05b90f1f9cc20e299e698c1c35e5b45b0452defe7690836534c26fa4f1a` |
| `alumina-interface_bg.wasm` | 6,800,492 | `d19efd173b1ec238e18d82d58b017816b48e55f04e8e3c0a8b630e44208e325e` |
| WASM gzip | 2,991,844 | `eeedf67912d8de8daa4496f3831d55bbf15fdf3a55076e8fadb37cd393b90ba7` |
| WASM Brotli | 2,350,426 | `a1f9fe9495ce817ba7bcb4833ba338061615db253be73d016c874a792059d31f` |

`wasm-tools` accepted the optimized module. Gzip and Brotli integrity checks
accepted every produced compressed artifact. The table is a tested moving-
workspace snapshot, not a coherent CSGRS/Hyper release pin.

## Optimized browser lifecycle

The proof addressed only the optimized bundle through loopback HTTP and an
isolated Chromium debugging port. It cleared only disposable origin-local
persistence and operated visible controls located from rendered pixels.

It created two version-1 empty same-shape dependencies:

- `user.browser_child_a`, digest
  `bddfa32bafd4479c58641acd6aa24d8a87a60ae4b592c9d5f597d79987fbd853`;
  and
- `user.browser_child_b`, digest
  `3d1b0dd9bf5147fe6215345418c903f177c6d3485ee72f01dd88c8e160dea5ae`.

The prepared four-dependency session was 16,246 bytes with identity
`0ae8eda28d37a9655b5def1df40f2e8bbaa0c87fbbdbaebe897a2973efd10a43`.
The UI selected `control.reference_pid_wrapper`, chose A, and added scoped node
`2` at exact canvas `(328, 28)`. The resulting A-bound state was:

| Artifact | Exact A-bound state |
| --- | --- |
| complete `ALGS` | 16,413 bytes, `7579d044c131d3098b3bfd60a8455becda9e1d98f17c4a57ce5f1f7e6d9bc5bb` |
| complete `ALGH` | 8,767 bytes, revision 4, `fd37e63ee1d59194864c61e7e3a7de3c510f90439158942a039c4ec12064dcab` |
| wrapper `ALGC` | 1,320 bytes, revision 2, `42c0e5af13e27a1085bcc52ab574ba105c9d351a021b79e09a6629af85a74423` |
| wrapper `ALGW` | 984 bytes, revision 2, `aff3004bf0f15ed4ee1a535b57c502b8d3268580f1310009cee287aa61b4ed6e` |
| `ALGM` | 2,550 bytes, `0854b6b155bce215339f7619a35b4a2342f828b658f3c2ff7b61a5ec43a28e20` |

The UI then selected B and used the visible placeholder-inspector action. The
complete session remained 16,413 bytes and changed to
`129c1617f9f33af2374db8e294703699ad455cdd8a71dc50d0dc9fb4851f9ff4`.
`ALGH` remained 8,767 bytes, advanced exactly once to revision 5, and changed
to
`c389dbd41338f2a258c341836d3d397f91fb140d9cbf9d0c0583f438f0859774`.
Only scoped node `2` changed from A to B. The wrapper `ALGC` and `ALGW`,
node-`3`/wire-`1` allocation cursors, placement, complete dependency library,
root workspace, control workspace, probes, and cached-job workspace remained
byte-identical.

The freshly flattened workspace remained
`d7f39dac860a59935385d381ccb1226342d3919a8f6f4ac217ae6df8750e13af`.
The 2,478-byte provenance body remained
`98d2431fbb8aaaf8b774842ff3d5f09f7d4d4c971fbafc3f68fd995a56afdd55`,
while source-bound `ALGM` changed to
`7fa57889808eb3219f5d3335834ce79b935b9034922f4255526d6cdf21ff25dc`.

A second visible rebind to B retained every B-bound `ALGS` byte. Visible Undo
restored the exact A-bound session. Redo and a fresh reload restored the exact
B-bound session. The selected placeholder correctly reset across reload because
selection is transient; fresh parsing still proved scoped node `2` was bound
to B.

| Browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| create child A | 371,668 | `b2e5247cec16e6d5757f8665426b289b87fec0cd64b657b0ad04b72a28984de9` |
| create child B | 376,348 | `779cc23029f0ab784bb88dfc999e261ca5bde7dd56c27de207b4cbb478135792` |
| selected wrapper | 348,769 | `a8af045d40828a1a5ffaabcdee42e16b7d17cb6ab443fa7eb0b34bd7247c8ac6` |
| rebound child B | 332,848 | `6331132039e4b77227a5d5c7611fa52487c48ccd3803a2363c88419cee044585` |
| same-target no-op | 331,084 | `a9817a5e6c161da4afe5493a32a38a7e5cb4e4877362d78428a126cc222e0a73` |
| history controls | 411,862 | `06f7ba1f79817928ea3e63d1f240d20a9150bf3b6d37279eb4530d1be4226e75` |
| after visible Undo | 404,439 | `6671e76e3fe9897593bf52f7586e8af91195fb4008d5d75a47005b84aacd892d` |
| reloaded wrapper | 336,657 | `c4ababbeee95ce29b4804a4cef1bd24f8c3b9c4b72cc274fcd9a2c8f6ca71309` |

The retained parsed result is 122,143 bytes with SHA-256
`1b277b9487cd57d509f69accd754fcac69b167d93d2f97bdf5b7e882a3474389`.
Screenshots, bounded locator frames, and the result remain transient
`/tmp/alumina-child-rebinding-proof.RBMXlM` evidence and are not repository
inputs. Failed/intermediate evidence directories and the disposable Chromium
profile were removed; Chromium and the loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or Alumina device-network path was used. Workstation Wi-Fi and
NetworkManager were not changed.

Hypercurve remained a moving read-only dependency. This work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The implementation is independently authored under the existing
MIT terms. The source-policy audit found no GPL-family dependency, copied
external planner/control implementation, compatibility shim, or retired
repository use. `alumina-firmware` is the sole canonical firmware repository;
no file or command targeted the retired `aluminafw` name.

Exact single-occurrence rebinding, stable-ID-compatible endpoint retention,
same-shape parent preservation, shape-changing recursive refresh, same-target
no-op, typed rejection, complete-session history/persistence, and visible
optimized-browser lifecycle are now closed. Nested binding import/exchange,
parameter promotion/overrides, coordinated descendant/control-authority
replacement, collaboration/conflict handling, and crash-durable journals
remain separate work.
