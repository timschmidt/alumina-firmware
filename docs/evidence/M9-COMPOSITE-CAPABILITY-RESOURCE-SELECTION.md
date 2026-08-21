# M9 composite capability-resource selection evidence

Date: 2026-08-20

Checkpoint state: schema-aware nested physical-resource authoring,
whole-workspace uniqueness, exact sibling preservation, exact-commit
native/WASM qualification, and optimized localhost reset/reload are complete.

This checkpoint extends the capability-derived selector without turning an
opaque `ResourceHandle` into editable identity text. A reviewed graph parameter
may retain physical-resource references inside bounded records, arrays,
options, or results. Selection addresses one already-existing typed leaf by its
registered schema path, proves both its old and new values against one exact
catalog, and rejects reuse of the same physical identity anywhere else in the
workspace before committing a reconstructed root value.

## Source and physical boundary

- Qualified interface implementation:
  `a4f71c20d099adb02b50ed683066513af53f2d27`
  (`feat: select capability resources in composite values`).
- Preceding qualified interface checkpoint:
  `cd0a1389bc8c70e1e7503a4724e03d26b58a3f7b`.
- Preceding qualified firmware checkpoint:
  `cf84d7a6da0e4b309a5ffb0639561f354c08a947`.
- Both repositories remained on `agent/zizmor-ci-hardening`.
  `alumina-firmware` is the canonical firmware repository; the retired
  `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and device state
  are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed,
  contacted, or used. No USB serial, GPIO, analyzer lead, motor, motor power,
  device AP, or workstation WLAN was used.
- Browser qualification used only `127.0.0.1` with an isolated Chromium
  profile.
- Current CSGRS/Hyper sibling path dependencies were compiled read-only. No
  sibling status, diff, reset, pin, format, stage, commit, or edit was
  performed. This records the live workspace stack that the interface commit
  compiled against, not an obsolete published CSGRS release.

## Exact nested selector boundary

`GraphCapabilityNodeCatalog::entry_index_for_handle` recognizes a handle only
when the complete opaque value exactly matches a reviewed catalog entry. The
greenfield `select_graph_capability_node_resource` API takes the catalog,
reviewed semantic registry, workspace, stable node and parameter IDs, one
borrowed `GraphValuePathSegment` slice, and a catalog entry index. An empty path
is the ordinary root case; nested traversal reuses the bounded registered-schema
path machinery.

Before replacement the transaction requires:

1. the node and parameter to exist without fallback;
2. the path to resolve through stable record-field IDs, checked retained-array
   indices, and explicit already-active option/result branches;
3. the selected leaf schema and current value to be exactly the catalog's
   registered resource-handle type;
4. the current complete handle to be an exact member of that catalog;
5. the requested replacement to be another exact member of the same catalog
   and not an exact no-op; and
6. the replacement's physical identity to occur in no other root or recursively
   nested parameter leaf in the complete workspace, including a sibling in the
   same node.

Only then does the selector reconstruct and validate the complete root, edit a
workspace clone, run complete audited graph analysis, and encode canonical
`ALGW`. The caller receives the candidate only after every check succeeds.
Node identity, label, execution domain, integer placement, allocation cursors,
unrelated parameters, and every unselected composite sibling remain exact.

The path never creates an option/result branch, grows an array, resolves a
display label, or accepts a numeric GPIO, device ID, digest, class, field name,
or raw handle as authority. Catalog content identity remains authoring evidence,
not authentication, a resource lease, deployment admission, or physical
ownership.

## Visible TinyBee reference-set proof

The separate TinyBee target-I/O draft now has a mixed HostExact/Realtime
schema. Its deliberately non-deployable HostExact node kind
`alumina.io.capability-reference-set` V1 owns parameter 1, `references`, with
three exact leaves:

- `references.primary = GPIO22`;
- `references.fallback.some = GPIO32`; and
- `references.mirrors[0] = GPIO33`.

GPIO35 is initially the one free member of the exact four-entry TinyBee stable
Boolean input catalog. The UI offers exactly the three reviewed composite
paths, reports the stable path and segment count, and can rebind one selected
leaf. It also retains the existing concrete Realtime stable-input workflow.

The exercised transition selected `references.fallback.some` and replaced only
GPIO32 with GPIO35. Primary remained GPIO22 and the array mirror remained
GPIO33. The released GPIO32 then created concrete Realtime node 2, proving that
the recursive uniqueness rule neither leaks nor permanently consumes the old
identity. The non-deployable reference-set node remained node 1. The target
draft advanced from one to two nodes and its displayed canonical `ALGW` prefix
changed from `a8ac0afa62e82620…`, through `79758941266565c…`, to
`ed975b6643ba7b36…`.

`reset target draft` restored the original one-node reference set, GPIO35 as
the free catalog selection, and the original `a8ac0afa62e82620…` identity.
This bounded proof is intentionally session-local and has no compatibility or
persistence shim. A fresh page reload reconstructed the same initial state; it
did not replay the transient target edit.

## Hostile and transactional coverage

Focused capability-catalog tests cover root selection and nested
record/option/array selection. They prove fallback-only GPIO32-to-GPIO35
replacement, exact primary/mirror preservation, stable node placement and
cursors, and changed canonical identity. They reject exact no-ops, unknown
nodes or parameters, invalid root/segment combinations, unknown record fields,
out-of-bounds array indices, inactive options, wrong leaf/root types, a raw but
well-shaped GPIO2 handle, a foreign catalog, duplicate root ownership,
same-node sibling duplication, and semantic-registry rejection without
mutation.

The general typed-value tests independently exercise all path variants,
bounded depth, reconstruction, and exact rollback. Application tests prove the
initial composite/free-resource state, the successful nested transition,
whole-draft uniqueness, released-resource concrete-node creation, exact reset,
and separation from the simulator board explorer.

## Reproduced interface checks

Run from `alumina-interface` at the exact qualified commit against the then-live
workspace CSGRS/Hyper paths:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
bash scripts/audit-source-policy.sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 315 executable workspace tests pass: 73 application/coordinator, 82
protocol-client, 159 core, and one public exact-control integration test. The
compile-fail rustdoc contract also passes. Both warnings-denied Clippy commands,
all six WASM test-target links, strict rustdoc, package-scoped formatting,
source policy, optimized Trunk assembly, WASM validation, and gzip/Brotli
integrity pass. The source-policy result was:

```text
source policy: local Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted
```

The qualified interface repository remained clean afterward.

## Optimized artifacts

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `Cargo.lock` | 99,392 | `e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c` |
| `index.html` | 1,295 | `325d28818ed1b8c5dde661942dce034247809640057aa7fcea611a91e1bb6925` |
| `alumina-interface.js` | 91,816 | `f71618a087fa6364b1a97b84b6a57404f6dd62fc1403a9a1d011c2ab84cfa6da` |
| `alumina-interface_bg.wasm` | 6,422,074 | `9585fc745788a1a835a4ae7255f427f507d5fc7caff0caef3b2e0c00c5427a39` |
| `alumina-interface_bg.wasm.gz` | 2,843,625 | `d80454ec03557a570395ba536285da871c88f34c8a5e47ea1fb975929fb580de` |
| `alumina-interface_bg.wasm.br` | 2,242,352 | `90348ae73670762a90fa6afb60bbb20d1f4d2058538f4b88391352b718ade64f` |

## Optimized browser interaction

The release bundle was served only from `127.0.0.1:8765` into a fresh isolated
Chromium profile. Origin storage was cleared before loading the actual egui
canvas. The browser selected `references.fallback.some`, invoked `rebind
composite leaf`, selected each sibling to verify GPIO22 and GPIO33 remained
exact, selected the newly free GPIO32 catalog entry, and invoked `add concrete
input node`.

The visible status reported:

```text
rebound non-deployable resource reference #1 at references.fallback.some from GPIO 32 to GPIO 35; firmware authority remains closed
```

After concrete creation the UI showed two draft nodes, Realtime node 2 bound to
GPIO32, and `references.mirrors[0] = GPIO33`. Reset then showed one node and
`references.fallback.some = GPIO32`; fresh reload showed the original primary
GPIO22 and free GPIO35 state.

The separate persisted control/probe/cached-job bundle intentionally remained
unchanged throughout this session-local target proof. Local storage held only
`alumina.graph-workspace-bundle.algwb.v1`, 9,983 characters with exact
3,755/407/825-byte sections and SHA-256 values:

- `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16`;
- `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223`;
  and
- `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58`.

The 1,280-by-757 captures are:

| Browser capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/alumina-resource-rebound.png` | 250,173 | `442624f6f0b2923c59fbdf3a53ed91aa6896dcd0121b86ec50293c8871b3af19` |
| `/tmp/alumina-resource-primary-preserved.png` | 255,139 | `1cd5156687147cc27b3a990717ed26507e374e0f9b2eabd1c871042ad71d3815` |
| `/tmp/alumina-resource-mirror-preserved.png` | 249,751 | `1f6f86397e8da7896d5ea22d3292a7c0ff2459651f3031a6c3e4fd7a879e96d1` |
| `/tmp/alumina-resource-concrete-added.png` | 250,569 | `969655644064bd03cb1af91f2ae1e76e910ebd6cc1a4b5c7ee9be8340e319cd9` |
| `/tmp/alumina-resource-reset-fallback.png` | 253,803 | `680852d277a66c85f6c97ac2c07e4cf2d4f7421d2be32d537e28647f2861b98d` |
| `/tmp/alumina-resource-reload-target.png` | 253,439 | `686055c1ac5ccad6486a5161c47f196345cf15d4a7b62dbb7ea41e2053fb21fc` |

Chromium and the localhost server were stopped afterward. Captured page logs
contained only the browser's preload-integrity limitation and software-WebGL
fallback warning. Chromium also printed software-render readback notices and
background Google registration failures. No Alumina application, firmware,
device, or WLAN error appeared.

## Firmware documentation qualification

The exact documentation checkpoint will be qualified with the complete locked
portable firmware suite, warnings-denied Clippy, all five dual-core board
definitions, the three bounded capability assertions, whitespace checks, and a
fresh full Cargo-metadata license/source audit. No firmware manifest or lockfile
changed in this documentation-only slice.

## Closed claims and licensing

This is bounded offline graph-authoring and localhost-simulation evidence. It
does not authenticate a live MCU, prove that an offline catalog matches an
active configuration, persist the target draft, create an executable composite
resource consumer, lower or upload a graph, acquire a resource, read a pin,
configure an electrical mode, arm an output, command motion, or contact
hardware. Firmware safety and deployment admission remain independent
authorities.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the current local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
