# M9 direct root-instance authoring evidence

Date: 2026-08-21

Authoritative interface source:
`970cbdee3c63deea330959c908dd5700a8837db3`
(`feat: author exact root component instances`)

This checkpoint adds the first direct component-library operation to the
greenfield graph editor: selecting an exact dependency already retained by one
canonical `ALGH` V2 hierarchy, adding it as a root occurrence, and deleting a
selected root occurrence. It also preserves those authored occurrences across
compatible edits to the selected control component. The change is host
authoring/compiler behavior only. No firmware protocol, runtime package, GPIO
authority, Wi-Fi authority, motion command, or safety claim changed.

## Transactional core boundary

`GraphHierarchyDocument` now exposes three clone-and-validate operations:

- `add_root_instance` resolves one existing exact `ALGC` dependency, derives
  the reserved typed placeholder, and allocates its node through the root
  `ALGW` monotonic cursor;
- `remove_root_instance` deletes one exact root binding, its placeholder, and
  every incident root wire while retaining the dependency library and never
  rewinding node or wire cursors; and
- `replace_component` substitutes one exact dependency and remaps every child
  reference and every parent component scope from the old digest to the new
  digest.

Each method builds a complete candidate with the next checked hierarchy
revision and reruns the existing root/dependency/binding, connector-shape,
clock/type context, DAG, expansion, and flattened-size invariants before
replacing the prior document. Unknown identities, digest collisions,
incompatible placeholder shapes, cycles, limits, and integer overflow leave the
original hierarchy unchanged. Replacing a component with the exact encoding
already retained is a no-op and does not advance state.

Core regression coverage proves that deleting root node 4 leaves the next-node
cursor at 5 and that the next addition receives node 5. It also proves exact
replay of the edited hierarchy, transactional unknown-component and
unknown-instance failures, selected-dependency replacement, root-workspace
preservation, binding remap, and exact-replacement no-op behavior.

## Complete-session UI transaction

The visible `Component library / root instances` panel lists only canonical
dependencies already admitted by the current hierarchy. It has no raw digest,
path, package, device, or command field. The first panel can:

1. select the PID leaf or wrapper dependency;
2. add the selected dependency to the root with a bounded generated label and
   deterministic integer placement to the right of the retained layout; and
3. select and delete any existing root occurrence.

An action edits a cloned `GraphHierarchyDocument`, encodes the complete `ALGH`,
freshly flattens it to ordinary `ALGW`, regenerates the total `ALGM`, admits the
flattened draft through the audited semantic registry, constructs a complete
candidate `ALGS`, and records the exact prior complete session. Only then does
the UI commit the component/hierarchy/source-map package, history, persistence
dirty state, and transient selection. Every failed stage leaves the current
canonical authoring session and history unchanged.

Compatible control-workspace edits no longer reconstruct the representative
hierarchy and discard direct authoring. The UI constructs the replacement
selected `ALGC`, calls `replace_component` against the existing hierarchy, and
regenerates the derived artifacts. Root IDs, placements, root wires, root
instances, and unrelated dependency encodings remain exact; bindings that
named the former selected-component digest move to the replacement digest. An
edit that invalidates the selected component's public bindings still follows
the explicit detached-hierarchy path and remains recoverable through unified
complete-session undo.

## Native and WASM qualification

`cargo test --workspace --locked` passed:

- 82 application/coordinator tests;
- 82 client tests;
- 173 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 339 unit/integration/compile-fail checks. New application coverage
proves hierarchy-only artifact changes, monotonic root IDs 2 then 3 after
delete/re-add, exact unified undo/redo, exact persistence with empty restored
history, selected-component remap across a control parameter edit, stable
wrapper bytes, and transactional invalid selections.

The following stricter checks also passed against the live workspace Hyper,
Hypercurve, Hypergraphics, and CSGRS paths:

```text
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --locked -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
```

Gzip and Brotli integrity checks passed, and independently decompressed WASM
bytes compared exactly with the optimized artifact. `Cargo.lock` remained
unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `b7d0bc9f6b400790dbe3e625ec1dd9325b1a8c8fd3c144088850663442554c19` |
| `alumina-interface.js` | 91,816 | `2ac41da927b64281f43c39dc9a2326ef5898b299fdd3284bf67e3013d22e7c81` |
| `alumina-interface_bg.wasm` | 6,618,422 | `6fce04602961a6e20940a88acb02cf12937d52b943d52dc31fa9166302b457ae` |
| WASM gzip | 2,921,839 | `9b7e627540f26db163af62e017fd38da1fecc3135092edfa040870b915e5bcaf` |
| WASM Brotli | 2,302,419 | `6e02401fd54a1aff066321fe873f0d6bf58d8b1b77859c6a78251f028bd9962e` |

## Optimized browser lifecycle

Headless Chromium loaded only the optimized loopback bundle. Starting from the
canonical two-level wrapper/PID hierarchy, the visible panel added PID root
instance 2. The UI then reported:

- 3 expanded occurrences at depth 2;
- 42 ordinary flattened nodes and 50 wires;
- root node cursor 3;
- selected root occurrence `#2 Component instance 2`; and
- `1 undo / 0 redo` with the exact 14,770-byte prior `ALGS` retained.

Deleting that selected occurrence returned the reachable hierarchy to 2
occurrences, 21 nodes, and 25 wires while preserving root node cursor 3. The UI
reported `2 undo / 0 redo` and 32,107 retained bytes: the complete 14,770-byte
initial plus 17,337-byte added sessions.

An independent byte parser checked the origin-local `algs1:` payload after
each state:

| State | ALGS bytes | ALGS SHA-256 | ALGH bytes / SHA-256 | ALGM bytes / SHA-256 |
| --- | ---: | --- | --- | --- |
| initial | 14,770 | `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | 7,124 / `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | 2,550 / `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| root 2 added | 17,337 | `5d03ce08d112aac78efde705481db7001ed232d744cbe0b4792225a2af734d33` | 7,437 / `0180aebed342eae319df073af96121fe5e24c498ff563335b1558b3873dc31a4` | 4,804 / `b27abcb4ae01b0596275e79de8f7b05ee6f451323b4925b74d0296dca1c0de07` |
| root 2 deleted | 14,770 | `58a4afd9b116d0e60376d07cdc2b94699812eb0cbcb876889f75f649dda08e1f` | 7,124 / `08e4707c39d0ed65115c297a0f36e2ba3897aa07dbffd80184c17d9b5219547f` | 2,550 / `e38c70e0459bdcedfe8cef8d00b78a9ad444eb30b2355e6a4eced50b3e74c702` |

All three states retained byte-identical non-hierarchy roles:

| Role | Bytes | SHA-256 / identity |
| --- | ---: | --- |
| control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |
| selected `ALGC` | — | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |

The visible undo button restored the added session and then the initial session
exactly. The visible redo button reproduced the 17,337-byte `5d03…d33` added
session and then the 14,770-byte `58a4…e1f` deleted session exactly. Reload
retained that final payload byte-for-byte and visibly reopened with `0 undo / 0
redo` and zero retained history bytes.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| root 2 added, 3 occurrences / 42 nodes | 368,727 | `7a8b7c320552694db7afb67277829598bcb2fa60398004aeece30a8961984532` |
| root 2 deleted, monotonic next node 3 | 368,527 | `6d45fbbb6a41d801ba782a7ff6d95fc5c8d93495bbfb93c016fee95bddaca7a5` |
| exact reload with empty history | 364,324 | `c215ad57ee746aba4f3546dcc73b03f38cea2734eed2d6c876dafe0133c5a2bd` |

The screenshots remain transient `/tmp` evidence and are not repository inputs.
Chromium and the loopback server were stopped after replay.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or device-network path was used. Workstation Wi-Fi and NetworkManager were
not changed. Browser qualification used loopback HTTP only.

Hypercurve remained a moving read-only dependency; this work did not edit,
format, pin, stage, or commit any Hyper/CSGRS repository. The implementation is
independently authored under MIT. The source-policy audit accepted the existing
permissive native/WASM inventories and found no GPL-family source, dependency,
compatibility layer, or copied external planner/control implementation.

Root hierarchy authoring grants no graph execution, deployment, firmware,
resource ownership, cache preparation, arming, start, timing, motion, or safety
authority. Main-canvas instance movement/wiring, nested component-definition
editing, general library import/creation/removal, connector and front-panel
authoring, interactive traversal into nested canvases, signed/locked dependency
manifests, collaboration/conflict handling, and crash-durable history journals
remain separate work.
