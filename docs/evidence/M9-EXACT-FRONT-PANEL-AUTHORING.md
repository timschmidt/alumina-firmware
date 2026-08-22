# M9 exact front-panel authoring evidence

Date: 2026-08-21

Authoritative interface source:
`32925ddc894ec6fcef8d7dc76e30ab4d4f64d692`
(`feat: author exact component front panels`)

This checkpoint closes the first general canonical front-panel metadata editor
for a selected `ALGC`. It adds/removes exact bindings, updates stable
name/binding/integer-rectangle metadata, and moves visible items on the logical
panel canvas. It is host-side authoring/compiler behavior only. It changes no
firmware protocol, runtime package, device resource, GPIO, Wi-Fi, cache,
motion, arming, start, or safety authority.

## Transactional panel-item authority

`GraphComponentDocument` now provides three bounded clone-and-reconstruct
operations:

- `add_panel_item` allocates one fresh monotonic `GraphFrontPanelItemId` and
  advances the cursor only after complete component validation;
- `update_panel_item` replaces one item's stable name, exact binding, and
  signed-origin/unsigned-extent integer rectangle together, while an identical
  replacement is an exact no-op; and
- `remove_panel_item` removes one known item without rewinding or reusing its
  identity.

Every mutation advances the checked component revision and reconstructs the
complete document through existing name, identity, binding, endpoint, type,
rectangle, count, coordinate, and byte limits. Exhausted identity space,
unknown items, duplicate names or bindings, unresolved inputs/outputs/node
parameters, negative or empty rectangles, arithmetic overflow, and policy
excess leave the prior `ALGC` byte-for-byte unchanged.

Core regression coverage proves add, update, exact no-op, duplicate-binding and
invalid-rectangle rejection, remove, unknown-remove rejection, fresh identity
`4` after deleting `3`, unchanged embedded workspace authority, value-type
resolution, and exact canonical replay.

## Visible editor and complete-session transaction

The front-panel section now derives one selectable binding catalog from the
selected component's public inputs, retained typed parameters, and public
outputs. A binding already owned by another item is excluded. The visible
controls support:

- a validated stable name plus one unbound exact binding for item creation;
- selection and deletion by stable item identity;
- exact name, binding, x, y, width, and height replacement;
- draft reset; and
- header drag with one retained cumulative egui delta, rounded only when the
  complete gesture commits.

New items receive a bounded default rectangle below the retained layout.
Canvas interaction never turns rectangle coordinates into a graph value,
clock, machine coordinate, timer, or firmware quantity.

Each accepted proposal performs one complete authoring transaction:

1. mutate a cloned `ALGC`;
2. canonically encode its new exact identity;
3. replace and remap the selected dependency in the complete recursive `ALGH`;
4. audit every component-library draft;
5. flatten to ordinary `ALGW`;
6. regenerate the total `ALGM`;
7. rerun ordinary-node semantic admission;
8. construct the complete canonical `ALGS`; and
9. record the exact prior session before component/history/persistence commit.

Panel-only changes preserve connector shape, so all root and nested
placeholders remain valid. The selected component's embedded workspace,
connector pane, root workspace, unrelated wrapper dependency, probes, and
cached-job workspace remain exact. A significant prior limitation is also
closed: later ordinary workspace edits now call `replace_workspace` on the
current selected component, preserving authored panel metadata instead of
reconstructing the reviewed initial fixture.

The complete UI lifecycle regression proves delete/add/update, monotonic IDs
`16` then `17`, exact no-op behavior, atomic duplicate-binding rejection,
unchanged flattened workspace, isolation of every unrelated authoring role,
unified Undo/Redo, authored-panel retention through a later ordinary workspace
move, and exact persistence restoration with empty restored history.

## Native and WASM qualification

`cargo test --workspace --offline` passed:

- 86 application/coordinator tests;
- 82 client tests;
- 177 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 347 unit/integration/compile-fail checks. The following stricter checks
also passed against the live, read-only workspace Hyper, Hypercurve,
Hypergraphics, and CSGRS paths:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Gzip and Brotli decompression compared byte-for-byte with the optimized WASM.
`Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `fa3fd8090f966a4660517f409890139da444bcab1083614cdb9a9a569706bf39` |
| `alumina-interface.js` | 91,816 | `6e62c5e867c7b48067c534812caf1c192a730392317b2d4cc932425d817dc53d` |
| `alumina-interface_bg.wasm` | 6,673,016 | `46012f07b931d95b06291acd403d75908bf3a605ecdb410a580380d1b1320dda` |
| WASM gzip | 2,943,483 | `d6bcfe56908a5d95254bf48a9c6e7ea2fb9932b8ceccf4f73c82406987e8c898` |
| WASM Brotli | 2,318,281 | `b66e700c15dc23ba7acd358cac97cbf165e86cb99fa23778c611de606dfc4176` |

An early incremental compile observed the live moving Hypercurve path's
existing unused helper
`relation_to_supporting_line_with_certified_tangencies`; no Alumina warning was
accepted or suppressed, the final warnings-denied Alumina checks passed, and
this work did not modify that sibling dependency.

## Optimized browser panel-placement lifecycle

Headless Chromium loaded only the optimized loopback bundle. The test cleared
origin-local storage, opened the visible Control graph view, and used OCR only
to locate the rendered words `combined permit indicator` on the egui canvas.
It then performed a real pointer gesture on item `#14`; no test-only
application API proposed or committed the edit.

The header moved by exactly `(+80, +30)` logical pixels:

| State | Item `#14` rectangle | ALGS bytes / SHA-256 | Selected ALGC SHA-256 | ALGH SHA-256 | ALGM SHA-256 |
| --- | --- | --- | --- | --- | --- |
| initial | `(460,404,240,54)` | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` | `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| moved | `(540,434,240,54)` | 14,770 / `530db6a4d7cec74fcb3d36736c8de51cd024b4ed70b3b9c282234c48b5eaf666` | `c607de51199369cc1ff7fb40b377309511d5fcee07df1822475efe4c0383c2fd` | `93567a3cf1b27c6c14acb6ef95eeb37d16fc1aab8a62e69900ad5ccca214bcb6` | `2bd200edb3f2be3a84e0c6b5251f0043a2be0a0f4da8dce293b02c262e4ffbd8` |

The selected component revision advanced from `1` to `2`; its byte length
remained 4,815 and its next input/output/panel cursors remained `1`, `8`, and
`16`. Its 3,755-byte embedded `ALGW`, all inputs and outputs, and the other
fourteen panel records remained byte-identical. The 1,222-byte wrapper
dependency retained SHA-256
`941a11bb7a0a34f1ff64f510d474d7df9a3e6bfa5a0323090ffa6715a2b396a9`.
The 889-byte hierarchy root `ALGW` retained SHA-256
`3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6`.

All noncomponent authoring roles also remained byte-identical:

| Role | Bytes | SHA-256 |
| --- | ---: | --- |
| control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |

The test scrolled to the actual visible history controls. Visible Undo restored
the exact initial session; visible Redo restored the exact moved session; a
fresh page reload retained the exact moved bytes.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| parsed result document | 16,794 | `0b5f2ea02347f1362478a2187e0116aea42bf73778f686d60254fdd24bbc8419` |
| initial visible panel | 309,405 | `a322894c2c888c9adcdee6cc923d10daa80a15366384ef19ba3e0bba4f6e32c2` |
| moved visible panel | 314,520 | `a3234f296b88e28daf227530dd89bd2868e3510d5b29d990910875137bf2b98a` |
| visible history controls | 368,559 | `32af07f8ae2578e8166a077f0be345ccf2a959763eac60378b60a6b206904518` |
| exact moved-session reload | 238,673 | `cf98d4ea2f3866ddb9df0e81892ec14f26abc2ee5d70944e192056ff89ca8a2d` |

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

Canonical front-panel item creation, metadata/layout update, removal, history,
and persistence are now closed. Connector-pane authoring and recursive
placeholder/wire shape remapping, editable nested-definition canvases,
component rename/version evolution, nested package/binding exchange,
overlap/group/responsive layout policy, runtime input injection, panel
execution, collaboration/conflict handling, and crash-durable history remain
separate work. `ALGC`/`ALGH`/`ALGM`/`ALGS` authoring grants no firmware opcode,
resource ownership, cache preparation, execution, arming, start, timing,
motion, or safety authority.
