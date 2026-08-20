# M9 exact graph/probe pair history evidence

Date: 2026-08-20

This checkpoint replaces application-level ALGW-only undo/redo with bounded,
transactional history over the exact current ALGW/ALGP pair. Graph, probe,
trigger, and canonical sidecar-import edits now share one navigation timeline.
Undo and redo restore both independent canonical artifacts, including the
sidecar's exact workspace binding, rather than reconstructing probe state from
the restored graph.

## Source boundary

- Interface implementation:
  `08b1b3ecca03bd037b6802a469b77a3ec48e49bc`
  (`feat: make graph history probe-aware`).
- Preceding firmware evidence checkpoint:
  `dd9a7e9a20cb20789093423c5076b7f3df35c9d9`.
- Firmware source, protocol, target images, board packages, and physical-device
  state are unchanged by this checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or external probe lead is involved.
- Workstation Wi-Fi configuration and the Alumina device AP were not touched.
  Browser qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies, including the concurrently edited
  HyperCurve tree, were compiled read-only. No sibling status, diff, reset,
  pin, format, stage, or edit was performed.

## Bounded exact pair snapshots

Each ephemeral history snapshot owns two independent canonical encodings:

1. one complete ALGW workspace; and
2. one complete ALGP sidecar whose external identity must match that ALGW.

The snapshot records their checked combined byte length. The interactive policy
retains at most 32 complete pair snapshots in each direction and at most 64 MiB
of combined ALGW-plus-ALGP bytes across both stacks. The separately held current
pair is excluded from that budget. Both artifacts in an evicted snapshot leave
together; there is no graph-only or probe-only entry. A snapshot larger than the
complete history budget and any checked-accounting overflow reject before
history mutation.

History is deliberately not a canonical interchange or persistence format. It
is absent from ALGW, ALGP, and the browser's `algwp1:` current-pair carrier. A
fresh application, restored pair, or newly opened editing session begins with
empty undo and redo stacks.

## Transactional navigation

Recording a changed pair first independently canonicalizes the prior ALGW and
ALGP and replays the sidecar against the exact prior workspace. Only a valid
pair can enter history. A successful new edit clears its abandoned redo branch;
an exact canonical no-op records nothing.

Undo/redo preview and commit perform the same ordered admission:

1. bounded canonical ALGW replay under the interactive workspace and graph
   policies;
2. bounded canonical ALGP replay against that reconstructed workspace;
3. audited UI layout and semantic admission of the candidate graph;
4. canonical capture and workspace-binding validation of the current pair; and
5. only then a paired stack transition and installation of both replayed
   documents and exact encodings.

A corrupt, noncanonical, unregistered, over-limit, or unbound target leaves the
visible ALGW, ALGP, and both stacks unchanged. Navigation clears transient drag,
wire, and parameter-text state, retains a selection only if its node still
exists, and resets the exact trace cursor from the restored trigger resolution.

The application records pair history for graph changes, probe creation/removal,
trigger set/clear, and changed `.algp` import. A graph edit that deletes a probed
endpoint still installs an empty sidecar bound to the revised workspace; undo
now restores the exact former graph, all probes, trigger, revisions, and binding,
while redo restores the exact revised graph/empty-sidecar pair.

## Regression coverage

New core regressions prove:

- byte-for-byte bound-pair undo and redo;
- rejection of a workspace/sidecar mismatch before recording;
- redo-branch removal after a new pair edit;
- oldest-snapshot eviction as one complete pair;
- rejection of zero and undersized history policies; and
- transactional failure when a later replay policy cannot admit the target.

Application regressions additionally prove:

- a node move changes the ALGP workspace binding and undo/redo restores the
  exact initial and moved sidecar bytes;
- probe-trigger clearing is pair-historical, while clearing it again is a no-op;
- graph-driven probe invalidation can be undone to the exact original sidecar
  and redone to the exact empty revised sidecar;
- a changed canonical `.algp` import is undoable/redoable without changing the
  workspace; and
- a foreign-workspace ALGP fails closed without changing either document or
  pair-history state.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt -p alumina-interface-core -- --check
cargo fmt -p alumina-interface -- --check
cargo test --workspace --all-targets -- --format terse
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps \
  --document-private-items --locked
./scripts/audit-source-policy.sh
env NO_COLOR=true trunk build --release --locked
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 272 executable workspace tests pass: 54 application/coordinator, 82
protocol-client, 135 core, and one public exact-control integration test.
Native and WASM warnings-denied Clippy, strict Alumina rustdoc, package-scoped
format checks, the local-source/permissive-license audit, optimized Trunk
assembly, both WASM validators, and gzip/Brotli integrity pass. Dependency
builds emitted only warnings from the live concurrently edited read-only
HyperCurve tree.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,343,025 | `31f49e8314d3e8638f3171f1bd6d04a3382a2df515b7291681e2df6f6e9b2159` |
| `alumina-interface_bg.wasm.gz` | 2,809,458 | `51e8b225ba65ef07edfb30f6d1fd7b52d510f52af8b16702857365e503589a78` |
| `alumina-interface_bg.wasm.br` | 2,220,967 | `a9be8330954b6d8b599209c8880aab589e8bdc406ee10036890f01f2354b61e7` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The optimized bundle and dedicated worker loaded from `127.0.0.1:8765` in a
fresh isolated Chromium profile. Runtime inspection found the one current-pair
storage value with the `algwp1:` prefix, a 3,755-byte ALGW segment, a 407-byte
ALGP segment, and 8,332 total text characters. After explicit reload, the
Control Graph view reported
`restored exact canonical ALGW/ALGP pair from application storage`,
`Pair history: 0 undo / 0 redo · 0 bytes`, and
`Browser persistence: exact ALGW/ALGP pair saved`, as required for ephemeral
history after restoration.

The accepted 1,440-by-757 capture also shows the paired-history controls and
the unchanged exact trigger/probe sidebar.
`/tmp/alumina-control-pair-browser.png` is 284,864 bytes with SHA-256
`20f2cbfcb4eaa63aa1ab93511b3d0eb2b213279ef432912237095daef59143d4`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and background Google
registration errors; no Alumina application, device, or WLAN error appeared.

## Closed claims and licensing

This is ephemeral HostExact editor navigation. It does not provide durable
history, collaboration/conflict handling, firmware-side graph/probe history,
live telemetry, device-side triggers, device deployment, or physical
output/safety authority. It does not change either canonical format or add a
compatibility carrier.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
