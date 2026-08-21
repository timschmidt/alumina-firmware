# M9 canonical graph authoring-session evidence

Date: 2026-08-21

## Scope and source

This checkpoint replaces the provisional three-section browser graph bundle
with one canonical, bounded, replayable host-authoring artifact. The
implementation is `alumina-interface` commit
`b0a32df74ec3259286ac1ed09dde9ff498fd901a`, following exact hierarchy source
maps at `b73a1b321eabbe65fc9ae89f04fc032a9d1bc367`. This evidence is based on
preceding `alumina-firmware` commit
`f5254bbcd893bebd58e901672a7914375395e9a7`.

No firmware source, protocol, target package, task ownership, flash layout,
GPIO authority, or hardware claim changed. `ALGS` is browser/native authoring
state and is never interpreted by firmware. The interface compiled against the
current checked-out sibling CSGRS/Hyper paths. Hypercurve remained a moving,
read-only dependency: it was not pinned, edited, formatted, staged, or assigned
a separate reproducibility claim.

## Canonical ALGS V1

`ALGS` V1 binds these artifacts in fixed order:

1. editable control `ALGW`;
2. exact `ALGP` bound to that control-workspace digest;
3. inert cached-job `ALGW`; and
4. either no hierarchy, or one selected `ALGC` digest, complete `ALGH`, and
   complete `ALGM`.

The little-endian encoding begins with `ALGS`, version 1, and zero flags. Six
embedded `u64` limits cover the outer document, control workspace, probes,
cached-job workspace, hierarchy, and source map. Three length-delimited
canonical artifacts follow, then a one-byte hierarchy tag. Tag 0 ends the
document. Tag 1 carries the selected 32-byte component digest and
length-delimited ALGH and ALGM. No padding, alternate tag, unknown flag, or
trailing byte is canonical.

The first interactive policy is 8 MiB for the complete session; 2 MiB each for
control ALGW, ALGP, and cached-job ALGW; and 4 MiB each for ALGH and ALGM.
Nested artifacts retain all of their own graph/workspace/probe/component/
hierarchy/source-map limits. Embedded limits may only narrow caller admission.
Encoding computes the checked total before allocation and requires the final
vector to have exactly that length.

Core replay bounds and parses the outer artifact, independently replays both
workspaces and the probe sidecar, replays the complete hierarchy, freshly
flattens it, regenerates the complete source map, locates the selected
component by digest, proves that component's embedded workspace equals the
control workspace, reconstructs a new session, and re-encodes every byte.
Corruption, policy widening, stale provenance, probe/workspace substitution,
missing selected components, alternative valid encodings, and trailing bytes
fail before a session document is returned.

The core hierarchy fixture is 14,794 bytes with SHA-256
`09161f22343464aef962c513be9778b0bbe5b465ebef7423006349b56accc07c`.
It is a protocol golden, distinct from the visible application fixture below.

## Atomic application admission

The application builds its complete ALGS only from its current validated
documents. A hierarchy branch names the actual selected component and retains
the exact complete ALGH and ALGM; it is not reconstructed later from UI
convention. A detached component is encoded by the absent tag.

After core replay, but still before mutation, the UI:

- recomputes layout and admits the control graph against its reviewed registry;
- admits the freshly flattened hierarchy graph against the same semantic
  registry;
- requires every nested cached-job handle leaf to resolve in the freshly
  reconstructed exact catalog; and
- reconstructs the component panel from the selected ALGC dependency retained
  in ALGH.

Only then are the control workspace, probe package, cached-job workspace,
selected component, complete hierarchy, and source map replaced together.
History and transient selection/drag/text drafts are cleared. A hierarchy-absent
session remains hierarchy-absent; restore does not invent the representative
component. Any failure leaves the entire prior authoring state unchanged.

Native tests cover hierarchy-present and hierarchy-absent replay, exact
component/hierarchy/source-map preservation, wrong probe binding, foreign
cached-job identity at a composite leaf, selected-component substitution,
corrupted ALGH, corrupted ALGM, upper-case/noncanonical storage text, and the
retired prefix. Each rejection is atomic.

## Greenfield browser and file boundary

Browser local storage now uses only
`alumina.graph-authoring-session.algs.v1`. Its value is `algs1:` followed by one
lowercase-hex encoding of the complete canonical ALGS bytes. The storage
wrapper is not part of the artifact identity. The former
`alumina.graph-workspace-bundle.algwb.v1` key and `algwb1:` three-section value
are not read, migrated, or decoded; there is no shim or fallback parser.

Native and browser shells expose complete `.algs` import/export at the 8 MiB
ceiling through the existing byte-only bridge. The bridge supplies no trust.
Import uses the same complete replay, semantic, catalog, and atomic-commit
path. Focused `.algw`, `.algp`, and `.algm` exchange remains independent and
does not substitute for complete-session persistence.

## Visible reference identities

The canonical browser session is 14,770 bytes with SHA-256
`d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d`.

| Bound artifact | Bytes | SHA-256 / identity |
| --- | ---: | --- |
| control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |
| selected `ALGC` | — | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |
| complete `ALGH` | 7,124 | `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` |
| total `ALGM` | 2,550 | `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |

The embedded policy tuple is exactly 8,388,608 / 2,097,152 / 2,097,152 /
2,097,152 / 4,194,304 / 4,194,304 bytes in wire order.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --locked -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps \
  --document-private-items --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
gzip -cd dist/alumina-interface_bg.wasm.gz | \
  cmp - dist/alumina-interface_bg.wasm
brotli -d -c dist/alumina-interface_bg.wasm.br | \
  cmp - dist/alumina-interface_bg.wasm
git diff --check
```

All 330 native unit/integration tests pass: 76 application/coordinator tests,
82 client tests, 171 core tests, and one cross-crate exact-control test. The
intentional compile-fail rustdoc test also passes, for 331 executable/replay
checks in the complete run. Strict owned-source native and WASM Clippy, WASM
check, warnings-denied private documentation, local-source/license audit,
optimized Trunk build, both WASM validators, gzip/Brotli integrity, and exact
decompression comparisons pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `ed1271229e0699f25147117a20606a8410dda12e28f4c9595bcff977ab259134` |
| `alumina-interface.js` | 91,816 | `222823135e64d3803b64e1d2d1dcd01251f1795e9a596d80b973b36b63170519` |
| `alumina-interface_bg.wasm` | 6,607,329 | `8299ce66c54cd0d5de6023c1cb68ed87ac53b04ed4ad76ff2cac038871f949f9` |
| `alumina-interface_bg.wasm.gz` | 2,916,771 | `226633265245e1d8313f241e43fd93db142e31cb5a09e1dd7b547eb628c00215` |
| `alumina-interface_bg.wasm.br` | 2,298,580 | `56535667322206cd1c686ce0ab7c90b363bb986f97e39677a1501414103787a9` |

The unchanged 99,454-byte lockfile has SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`;
no dependency entered the resolved inventory.

## Localhost browser proof

The optimized bundle was served only on `127.0.0.1:4175` and rendered in a
fresh headless Chromium profile at 1,440 by 1,000 pixels; the application canvas
was 1,440 by 857. The app used only the loopback origin. Chromium emitted its
preload-integrity and software-WebGL warnings; no application error was logged.

The browser scenarios proved:

1. clean startup wrote one `algs1:` payload under only the new storage key;
2. the payload parsed to the exact limits and artifact identities above;
3. replacing it with the valid hierarchy-absent 5,056-byte ALGS, SHA-256
   `93b65ea6b313cb37f759c105dd0f8feaa33a22b6b8ca5f879844d734a2fcd975`,
   survived reload byte-for-byte and visibly left the component absent;
4. substituting all 32 selected-component digest bytes was rejected, the
   complete canonical fallback was rewritten exactly, and the rejection was
   visible without partial state;
5. a synthetic retired-key value was ignored and left unmigrated while the new
   canonical session was generated independently; and
6. the actual `download bytes` control produced
   `alumina-session-d7a5fba83da9f254.algs`, exactly equal to storage, with
   header `414c475301000000`, 14,770 bytes, and the canonical session digest.

| Transient browser proof | Bytes | SHA-256 |
| --- | ---: | --- |
| hierarchy-absent restore screenshot | 293,736 | `00d333ade7bee6f558803b9afd234b9c72919a84c049b54606e7114829e8e4fe` |
| corruption-fallback screenshot | 312,041 | `729c41827388851ab1d9893529cad15b5b8318e09921293e8e256f1de1ec808f` |
| downloaded `.algs` | 14,770 | `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` |

The screenshots and downloaded file remain transient `/tmp` evidence and are
not repository inputs. Chromium and the loopback server were stopped after
capture.

## Closed claims, hardware, and licensing

ALGS grants no node semantics, deployment lowering, resource allocation,
firmware install, schedule authority, arming, motion, timing, or safety claim.
Firmware continues to receive only separately authenticated/lowered runtime
artifacts. Hierarchy-aware undo/redo, editable nested canvases, signed component
manifests, multi-user persistence, and durable browser job-cache state remain
separate work.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or device network path was used. Workstation Wi-Fi and NetworkManager were
not changed; application qualification used loopback HTTP only.

The implementation is independently authored under MIT. The source-policy
audit accepted the existing local permissive inventory. No GPL-family source,
dependency, compatibility implementation, or copied external planner/control
code was introduced.
