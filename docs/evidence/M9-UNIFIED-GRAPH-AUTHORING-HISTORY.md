# M9 unified graph authoring history evidence

Date: 2026-08-21

Authoritative interface source:
`acac3f93351c3486f17ec54886d08fb6e0feaa17`
(`feat: unify exact authoring session history`)

This checkpoint replaces two independently navigable editor histories with one
bounded ephemeral timeline of complete canonical `ALGS` V1 authoring sessions.
It changes host authoring behavior only. No firmware protocol, runtime package,
GPIO authority, network authority, motion command, or safety claim changed.

## Retired split state

The prior control editor retained independent canonical `ALGW`/bound-`ALGP`
pairs, while the cached-job panel retained a separate `ALGW` history. Either
carrier was locally exact, but the two timelines could not express one atomic
authoring transaction containing:

- control `ALGW`;
- bound `ALGP`;
- catalog-bound cached-job `ALGW`;
- selected `ALGC`;
- complete `ALGH`; and
- exact regenerated `ALGM`.

The interface commit deletes `workspace_probe_history.rs`, removes the
cached-job panel's `GraphWorkspaceHistory`, and removes its private undo/redo
controls. There is no compatibility shim, migration, or dual-history mode.
Focused `ALGW` and `ALGP` import remain available, but each successful change
is now one complete-session history transition.

## Complete-session carrier and bounds

`GraphAuthoringSessionHistory` retains only complete canonical `ALGS` byte
strings. History is not nested into `ALGS`, browser storage, or file exchange.
The first interactive policy is:

| Bound | Value |
| --- | ---: |
| snapshots per direction | 16 |
| combined undo + redo bytes | 64 MiB |
| separately held current session | excluded from history budget |

Recording a successful change retains the exact prior session, clears an
abandoned redo branch, and evicts oldest complete snapshots until both bounds
hold. Exact UI no-ops record nothing. A single maximum-size 8 MiB `ALGS` fits
the history policy; no nested artifact can be evicted independently.

Undo/redo first replays the target through
`replay_graph_authoring_session`. Only after complete core replay succeeds does
the UI rebuild layout, rerun its audited graph registry, freshly admit the
selected hierarchy/source map, and validate every cached-job handle leaf
against the reconstructed catalog. The history object is cloned for navigation
and committed only after those application gates pass. Byte corruption,
tighter nested limits, unknown graph semantics, and stale or foreign
cached-job identities leave both current state and both stacks unchanged.

Graph edits now prepare their rebound probe sidecar, selected component,
hierarchy, and source map before committing. Probe and cached-job edits likewise
construct a complete candidate `ALGS` before recording history or changing UI
state. Navigation restores all canonical artifacts together and clears only
transient selection, drag, wire, and text-field state. Complete-session import,
persistence restore, and fresh startup begin with empty undo/redo stacks.

## Native regression qualification

`cargo test --workspace` passed:

- 79 application/coordinator tests;
- 82 client tests;
- 171 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 334 unit/integration/compile-fail checks. New coverage proves:

- mixed graph, probe, and cached-job edits replay in exact chronological order;
- the selected component, complete hierarchy, and source map travel with every
  complete session;
- cached-job reapplication is a no-op and does not dirty persistence or consume
  history;
- abandoned redo branches clear;
- oldest complete snapshots are evicted as one unit;
- corrupt target bytes and tighter nested replay limits fail transactionally;
- a core-valid `ALGS` containing a foreign cached-job identity fails UI/catalog
  admission without changing current bytes or history;
- focused ALGW/ALGP import remains complete-session historical; and
- persistence/import restore exact state with empty ephemeral history.

The following stricter checks also passed against the live workspace Hyper,
Hypercurve, Hypergraphics, and CSGRS paths:

```text
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
```

Gzip and Brotli decompression both compared byte-for-byte equal to the optimized
WASM. `Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `d74e49548de6e7a58517b8d2de3e37fdd9ab64416fd9ee7f8cadcf06d00de74f` |
| `alumina-interface.js` | 91,816 | `bf568b7a16911bc66de0b3ae4acbfd29935e218e16b1712b6d4963d129d3dd1e` |
| `alumina-interface_bg.wasm` | 6,603,641 | `97884b4b3dc941ad2f96b4e9adccbca4f1b25bfc96c24d87c8a58cc4f2645132` |
| WASM gzip | 2,915,742 | `434ffe8977314d606d9620d8d1d8d1abdb38bf384dcb18763b0763e7222f3a03` |
| WASM Brotli | 2,297,728 | `c2939798e5c13ea3b996c4695ce3d274abdedd1c680deb329b7769577035dc96` |

## Optimized browser replay

Headless Chromium loaded only the optimized loopback bundle. The visible
Control graph workflow then:

1. created one reviewed Boolean-conjunction draft node;
2. selected the second simulated cached-job participant and rebound the primary
   leaf;
3. observed `2 undo / 0 redo` and 29,823 retained `ALGS` bytes;
4. undid cached-job then graph state;
5. redid graph then cached-job state; and
6. reloaded the page from origin-local persistence.

A byte parser independently checked each local-storage `algs1:` payload and all
nested section hashes at every transition:

| State | ALGS bytes | ALGS SHA-256 | Control ALGW | ALGP | Cached-job ALGW |
| --- | ---: | --- | --- | --- | --- |
| initial | 14,770 | `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |
| graph edit | 15,053 | `bc6b969d257e7cec71fb71b34b5e458042052a7781fdf43cc5cfa600358eaeb8` | `d3a01e1ea5a8628b65a73a04bff955b93275557625fdca2940baeb968db31129` | `94f4484b1573a9e69272b3a43e176a26e19ca7ce5e26edd7e05c4081edb069a1` | unchanged |
| cached-job edit | 15,053 | `d936977530e5cd4e91fe6f22192cd5c98bbc3fb69e81e2de343f44ea5b6fee81` | unchanged from graph edit | unchanged from graph edit | `d049e2dd519e3f82684394d0142c1ebbd081c6daf90811ffc9c01ab0f30cba19` |

The graph edit also changed selected ALGC, ALGH, and ALGM consistently. The
cached-job edit left all five other artifact roles byte-for-byte unchanged.
Both undo transitions and both redo transitions reproduced the corresponding
complete `algs1:` string exactly. Reload retained the final 15,053-byte
`d936…ee81` session and visibly reopened with `0 undo / 0 redo` and zero retained
history bytes.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| cached-job edit, `2 undo / 0 redo` | 318,023 | `2b7004f2bd2f8ca9a103b7fb37a3564122be91892f732ddb700bacdfbbd2e0bc` |
| cached-job undo, `1 undo / 1 redo` | 315,680 | `320aeb35a08df3c493006c4e9803281a3d1a3d86d4dbd0dd713ffb9972e5a367` |
| final exact redo sequence | 315,814 | `a6b028faa92631ca0908b50d7cb9f415aab53116710a3185b38862dc715574b1` |
| exact reload with empty history | 313,038 | `85487c3122220de2fbd55f82e5944a35aadd19cd4eeca51b63a3a27829e5fc97` |
| parsed browser result JSON | 2,775 | `893095fd1e8b3feaeec73eff826ce192f678d1bf40d293f170e2f17d15586d73` |

The screenshots and parsed result remain transient `/tmp` evidence and are not
repository inputs. Chromium and the loopback server were stopped after replay.

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

Complete-session history grants no graph execution, deployment, firmware,
resource ownership, cache preparation, arming, start, timing, motion, or safety
authority. Editable component libraries/instances, direct hierarchy mutation
commands, nested-canvas traversal, collaboration/conflict resolution, durable
history journals, and signed component manifests remain separate work.
