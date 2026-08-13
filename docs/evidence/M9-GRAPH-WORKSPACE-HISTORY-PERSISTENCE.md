# M9 canonical graph history and persistence evidence

Date: 2026-08-12

## Scope and source

This checkpoint makes the editable `ALGW` control workspace durable without
changing the canonical workspace format or granting deployment authority. The
implementation is `alumina-interface` commit
`8ab0d6c9b0f0112e56de2ebd72c7afcae8809f89`, against preceding `aluminafw`
evidence commit `a2471647185f77779a34c835fc81faa98b72fae7`. It changes no
firmware source, board image, target configuration, workstation network
configuration, or physical I/O.

The interface continued to build from the checked-out sibling stack, not an
old CSGRS release. Relevant committed identities were CSGRS
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0`, Hypercurve
`bc58f471d8f123a2002189df1156b0b092027bf7`, Hypergraphics
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve retained a
concurrent tracked edit in `src/bezier_offset.rs`, and Hyperlimit retained
untracked fuzz artifacts/corpora and a local file. Those trees were preserved
unchanged by this work. The successful checks below prove consumption of that
workspace state; they do not turn dirty sibling trees into release pins.

## Replay-backed history

`ALGW` V1 bytes are unchanged. `GraphWorkspaceHistory` retains complete
canonical workspace encodings rather than inverse operations or unchecked UI
deltas. The default interactive policy holds at most 32 undo and 32 redo
snapshots and at most 64 MiB across both stacks. The current document remains
separate. Oldest snapshots are evicted first, and an accepted edit discards an
abandoned redo branch.

Undo and redo preview and replay their target under the normal 20 MiB
workspace and graph admission policies before mutating history. Encoding,
replay, byte-accounting, oversize, and UI admission failures preserve the
current document and both stacks. Each accepted edit records the exact prior
encoding. Importing another valid workspace is therefore undoable; restoring
application state starts a fresh session with empty history.

The UI exposes buttons plus command/control-Z, command/control-Y, and shifted
command/control-Z. Navigation clears transient drag, pending-wire, and
parameter-text state and retains selection only if its node survives.

## Draft semantic admission

The core now distinguishes complete executable `GraphAnalysis` from
non-executable `GraphDraftAnalysis`. Draft analysis collects every unconnected
required input in canonical node/port order but still completes all remaining
shape, domain, storage, channel, exact-rate, and combinational-cycle checks.

This closes an import ambiguity: an early missing input can no longer mask a
later unknown node or other unsafe semantic failure. The editor may retain an
incomplete graph as an explicit blocker, but a hostile structurally valid
`ALGW` with both a missing input and an unreviewed node still rejects without
mutation. A draft report cannot be passed where executable analysis is
required because it is a separate type.

## Browser and file persistence

The WASM application stores only the current canonical workspace in origin
local storage under `alumina.graph-workspace.algw.v1`. Its value is `algw1:`
followed by canonical lowercase hexadecimal. Decode rejects the wrong version
tag, odd length, uppercase/non-hex input, and more than 2 MiB of canonical
workspace bytes before allocating. The smaller persistence ceiling accounts
for hex expansion and common browser quotas. Invalid stored state never
replaces the reviewed reference; if storage remains usable, that reference is
written back.

Browser export constructs a Blob from the exact canonical bytes and triggers
a named `.algw` download through a temporary object URL. Browser import uses a
temporary file input, checks advertised size, checks the materialized
`ArrayBuffer` again, and admits at most the independent 20 MiB interactive
workspace limit. The native application exposes an explicit path, reads no
more than limit plus one byte, and writes and synchronizes the complete
canonical encoding. The platform bridge moves bounded bytes only; canonical
replay, re-encoding equality, layout admission, and draft semantic analysis
remain authoritative in the UI.

History is deliberately not persisted or embedded recursively in `ALGW`.
File and browser restoration therefore preserve document identity while
starting with zero undo/redo entries.

## Reproduced checks

Run from `alumina-interface` at the implementation checkpoint:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 120 executable tests pass: 18 application/coordinator tests, 32 client
tests, 69 core tests, and one cross-crate exact-control integration test, plus
the intentional compile-fail rustdoc test. New coverage exercises bounded
oldest-first history eviction, exact undo/redo replay, abandoned-branch
clearing, corrupt and oversize history failure atomicity, UI navigation,
current-document-only persistence, canonical lowercase encoding, invalid
stored-state repair, undoable import, corrupt import rejection, the
missing-input-plus-unknown-node masking case, bounded native reads, exact
native writes, and empty/missing paths.

Strict native and WASM Clippy, WASM check, warnings-denied project rustdoc,
local-source/permissive-license audit, optimized Trunk build, both WASM
validators, and compressed-artifact checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface.js` | 91,813 | `f7e0b3962bc92c331163f8d008eb9218827e97db66b23fd8c261375b0e8eb327` |
| `alumina-interface_bg.wasm` | 4,778,831 | `79d332043c6d8de883242539d83a5ae637d97801edb3d6f2984044455aed1fcc` |
| `alumina-interface_bg.wasm.gz` | 2,179,405 | `ad2bda173de19854b4f8c54a962921a6b290df5cd43c4fc7da3bccd35f1d4687` |
| `alumina-interface_bg.wasm.br` | 1,758,074 | `bcc79ca4ab32b4b055939de484d8f7beef5ea0a255086f4f90202fa5cce497c6` |

The 96,806-byte `Cargo.lock` remains SHA-256
`5790fedc0da96498fad696e5627e6f1601ceb04dead8c5adb67d1efb3773cfc1`.
No package entered the resolved inventory; only already-present `web-sys`
features were enabled for Blob, file input, object URL, and local-storage APIs.

The optimized bundle was served only on `127.0.0.1:8765` and rendered with
headless Chromium software WebGL at 1,440 by 1,100 pixels. A fresh isolated
browser profile first stored the reference workspace; a second independent
load visibly reported `restored canonical ALGW from application storage` with
zero persisted history. The final 182,991-byte transient screenshot has
SHA-256
`080708297eca0954c8b33d3617eade56a841ebbcaa52f0e67a7b3638a3ab2446`.
Visual inspection confirmed the undo/redo controls, exact-byte download/open
controls, canonical saved-state status, complete graph and plot, and explicit
no-deployment/output warning. The loopback server was stopped after capture.

## Closed claims, licensing, and next gate

This checkpoint proves one bounded HostExact editor's local durability. It is
not collaborative storage, a merge/diff format, a component or front-panel
document, a capability-generated palette, a deployed graph, or firmware
authority. Browser download/upload button interaction was not automated; its
WASM path passed strict compilation/Clippy, while byte admission and native
file behavior are covered by tests. No arbitrary graph document is sent to or
interpreted by firmware.

The bare MKS TinyBee V1.0 remained on its prior disconnected-load HIL image. It
was not reset, flashed, or contacted. No motor, motor driver, process power, or
SLogic16U3 input was connected, and no workstation Wi-Fi setting changed.
Physical AP/HTTP load and analyzer capture remain postponed until Wi-Fi can be
dedicated to the Alumina AP without interrupting the Internet development
session.

The implementation is independently authored under MIT. No dependency package
was added, and the source-policy audit accepted the existing permissive
Alumina/CSGRS/Hyper/native/WASM inventory. No GPL-family source, library, tool
output, or asset was copied, linked, or vendored.

The next promoted offline editor slice is a canonical component/subgraph and
front-panel foundation, followed by capability-derived resource nodes and
broader bounded probes. Physical Wi-Fi and input timing remain retained-capture
gates rather than prerequisites for that offline work.
