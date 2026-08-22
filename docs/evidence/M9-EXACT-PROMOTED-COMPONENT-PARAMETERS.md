# M9 exact promoted component parameters

Date: 2026-08-22

## Scope and authority boundary

This slice advances the browser-authoring hierarchy from canonical `ALGH` V2
to V3 and adds exact public component parameters with occurrence-local values.
It changes `alumina-interface`; this firmware repository records the shared
delivery-plan evidence and authority boundary.

No `ALGH`, `ALGC`, `ALGW`, `ALGM`, `ALGS`, panel item, or exact authoring value
is firmware input. Firmware receives only independently compiled, replayed,
capability-bound artifacts through its existing protocol boundaries. This work
does not allocate a peripheral, claim WCET, arm a machine, command an output,
or grant runtime front-panel authority.

The connected bare MKS TinyBee V1.0 and workstation WLAN were not contacted.
No serial port, reset line, GPIO, radio, flash, SD, or board power state was
read or changed. Browser qualification used only a loopback HTTP server and a
disposable headless Chromium profile.

## Canonical design

`ALGH` V3 is the only hierarchy format accepted by the current interface source.
There is no V2 decoder, migration shim, or compatibility path. The reserved
authoring-only node `alumina.component.instance` advances to version 2.

For each `GraphFrontPanelBinding::ParameterControl` in a component:

- the stable `GraphFrontPanelItemId` becomes the placeholder parameter ID;
- the stable panel-item name becomes the parameter name;
- the bound ordinary-node parameter supplies the exact registered type; and
- its retained typed value becomes the definition default.

A parent component can bind its own parameter-control panel item to a child
placeholder parameter. Applying the same rule recursively promotes a leaf
parameter through any admitted hierarchy depth. There is no inferred binding,
path string, copied schema, or second override sidecar.

Every occurrence value is already a canonical parameter in its containing root
or parent `ALGW`. `GraphHierarchyDocument::set_root_instance_parameter` clones
that workspace, replaces one stable typed value, reconstructs and validates the
complete hierarchy, and commits only after all limits and invariants pass. An
exact retained value is a byte-for-byte no-op.

Flattening clones the bound component workspace, resolves each occurrence
parameter through the stable panel binding, applies its exact value to the
controlled parameter, and only then copies and recursively expands ordinary
nodes. Two occurrences can therefore flatten to different literals without
cloning or mutating the shared leaf `ALGC`.

Component evolution is stable-ID based:

- an occurrence value equal to the old default follows a new default;
- an explicit non-default value survives when ID and type remain;
- removing an explicitly overridden parameter rejects atomically;
- changing an explicitly overridden parameter's type rejects atomically; and
- a new public parameter begins at its new definition default.

Root occurrence editors use the existing schema-directed exact literal parser.
Accepted values regenerate flattened `ALGW`, `ALGM`, and complete `ALGS`, pass
audited semantic/session admission, enter one unified history state, and become
browser persistence. Invalid text changes no canonical byte. The hierarchy
root canvas reports each placeholder's promoted-parameter count.

`ALCP` V1 requires no new override section. Nested definition-level values are
already inside the package's `ALGC` closure; root-occurrence values remain
machine-hierarchy state and are intentionally excluded from reusable packages.

## Native qualification

The complete live-stack command passed:

```text
cargo test --workspace --offline
```

Results:

- `alumina-interface`: 104 passed;
- `alumina-interface-client`: 82 passed;
- `alumina-interface-core`: 192 passed;
- integration tests: 1 passed; and
- doc tests: 1 compile-fail boundary passed.

New focused coverage proves distinct repeated-occurrence values and flattened
leaf literals, recursive default evolution, compatible explicit-override
retention, removal/type-drift atomic rejection, exact no-op behavior, invalid
literal atomicity, unchanged control/probe/cached-job authority, complete
history, and persistence.

The suite compiled read-only against the current sibling Hyper/CSGRS working
trees. It emitted four warnings from the concurrently edited `hypercurve`
dependency (three unused bindings and one unused function); no sibling
repository was edited, formatted, pinned, or inspected for worktree state.

## Optimized browser qualification

The release bundle was rebuilt offline with locked dependencies, validated with
`wasm-tools`, and checked with `gzip -t`/`brotli -t`. A fresh loopback browser
session then used visible component/root controls to:

1. select root occurrence 1 and set promoted parameter ID 1 to `7/3`;
2. reapply `7/3` as an exact no-op;
3. retain an invalid `1/0` draft while canonical bytes/history stayed exact;
4. add a second occurrence of the same wrapper definition;
5. set occurrence 2 parameter ID 1 to `11/5`;
6. use visible Undo to restore the second occurrence's `200/1` default;
7. use visible Redo to restore `11/5` byte-for-byte;
8. reload from browser storage and recover the same final `ALGS`; and
9. select the rendered second root card, visibly showing one local override and
   Hyperreal's mixed-number `2 1/5` text without changing canonical state.

The proof independently parses `ALGS`, `ALGH` V3, root `ALGW`, embedded `ALGR`,
all version-2 placeholder parameter records, and `ALGM` identities from browser
storage. It does not trust labels as canonical evidence.

| State | ALGS bytes / SHA-256 | ALGH bytes / revision / SHA-256 | Root ALGW bytes / SHA-256 | ALGM bytes / SHA-256 | Flattened ALGW SHA-256 |
| --- | --- | --- | --- | --- | --- |
| V3 reference, one occurrence at `200/1` | 15,750 / `e473839dd8b1708fe699a8749d79b8f37672699e6ea203bff9efae1b58b30c08` | 8,104 / 1 / `ecd9ab11557bf6c3a565af4563cb145bb6f1e28efc73d99ccc74f7354755f198` | 1,196 / `919d0fee34d96e6ebb8a4addd0e93c39722c8dd2cf2e4ad8236095012921ab94` | 2,550 / `cfdd57949bda80538736b4ada6f23062854ee9c6ee8d829035d316f7a241cd43` | `6804b964535d08b9ceead3d43891c3ae4c5aa5ce38b015c34b4b385bfa3257d4` |
| Occurrence 1 = `7/3` | 15,748 / `d25e29c4ddf69ad4fe5661d5c1afc0a870dc1ef45141c5c88c6517487046abf3` | 8,102 / 2 / `863f10388c6c8df2f70e3892eabccb192a1418ac763e4ef05b2d2ff982b14979` | 1,194 / `40208495677172a1e16d197a8e56ee3b6d72387e031b084f4ac1b622d2d9428a` | 2,550 / `19815c5f5c653d7f5ddc8c8957568d0a0d78df17e3b786d4c8ab2e9b342b23e8` | `a598a6d897cd9d946570e7edd747c839f83075481aecd5a72bb52a1fb5eb3190` |
| Second occurrence added at `200/1` | 18,806 / `83d8c6f11019ed4e3b286b296b2d4cdec362a3b4a65dbb9605ae75e3cab31035` | 8,722 / 3 / `212e5666309cf822f15767de3289d8096a03fc55db80ca48f3f207bf9b84a975` | 1,777 / `74c5740722ec0faa418c149306445b3ff4d2bd82846f46818867f0ce931cb8a6` | 4,988 / `d99ff9580ce67439e83b6bf750ed83fa4ad5523fc02b7d0b4f6b3633268d9e80` | `9ddc9a98a155c16eb57e8cd4a80efbfe25984d70a30ab1df0bf58b5014547deb` |
| Final: occurrence 1 = `7/3`, occurrence 2 = `11/5` | 18,805 / `248f2e340eae9ad724253a5bc0f50b7c832da78356d2918abb5acdf3176e19ba` | 8,721 / 4 / `e6545ea05712ce9a86b202086b5654ad1cb9e9f9f5a3a94ed7c203fd39f6a464` | 1,776 / `fd12a50933b6b62d0119d3585ca7f81acfae2024987440a8ec26e208916c3ca2` | 4,988 / `071a261b361c41708d0296bd2787c26b27247335a8e156e64d7559b98f969fe5` | `902396a53f158fd0ac359d8a8874b8809e21dd0b800626b4b8fbfed4d51e45ae` |

The exact control `ALGW`
`bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16`,
probe sidecar
`50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223`,
cached-job `ALGW`
`493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58`,
and selected `ALGC`
`10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228`
remained exact through every accepted edit.

Undo produced the byte-identical second-occurrence-added row above. Redo and
fresh reload both produced the final row byte-for-byte. The retained 69,617-byte
browser result has SHA-256
`bf60d7ca8607eccea7a2d9cb02e15c629c2079bda9e996dce54c0693fb4d6f69`.
The qualified second-occurrence screenshot has SHA-256
`bb21c2522b57e77f5218d20401953d497ea04fbe0a4546cf32fccc2eac3ed711`.

## Release bundle

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `dfd86ba35254658244e7535b4b209b8884775e44eae2aa73a5bb5d9f62268ecd` |
| `alumina-interface.js` | 91,816 | `26b3ddc84f73a596bad6c733817d9dfd1111281b93ee03155fa14351509f037a` |
| `alumina-interface_bg.wasm` | 6,843,206 | `ea6d3264ef263c2768fc15de6b383708dc289af4a2928485d2ebb44b12b6c55f` |
| `index.html.gz` | 714 | `18d4236b4e4d8b378240cb5dd0b7b327b39832eebe291f473cdb4f11b383bc3b` |
| `alumina-interface.js.gz` | 12,979 | `b685b319858c5fb268371391c295d840a37b35e9a922101c5887ad6d5d522f7f` |
| `alumina-interface_bg.wasm.gz` | 3,008,560 | `5b54bee5067e4d7e7f5b5150914b8b2292a1d28480f77ea68e36fe5250c48cfb` |
| `alumina-interface_bg.wasm.br` | 2,362,454 | `2781bda3437e951d7e60bfb8ca81db6184d49ff9e1ecbab306ab732d08f3bc3e` |

## Still closed

- `InputControl` runtime values and executable front-panel programs;
- direct editing of path-addressed nested occurrences outside explicit parent
  definition bindings;
- firmware interpretation of graph/component/hierarchy/session bytes;
- deployment/resource/timing/safety authority from authoring parameters;
- collaboration journals and conflict resolution; and
- physical browser/radio/device qualification for this authoring slice.
