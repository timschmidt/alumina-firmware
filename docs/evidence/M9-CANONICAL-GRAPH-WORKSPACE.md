# M9 canonical graph-workspace evidence

Date: 2026-08-12

## Scope and source

This checkpoint adds the first canonical editing envelope around an exact graph
and uses it for bounded native/WASM placement and typed-wire editing. The
implementation is `alumina-interface` commit
`e5511fcd630dd038afdfd298130d0bea2776d379`, against preceding `aluminafw`
evidence commit `5e190527e8b412674e4ee6ac4337b25975e5ff5d`. It changes no
firmware source, target image, board configuration, workstation network
configuration, or physical I/O.

The interface continued to build from the current sibling workspace stack,
not the obsolete released CSGRS baseline. Relevant committed source identities
were CSGRS `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`, Hypergraphics
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve was concurrent
development commit `a795e42e69a91ba0fbc0c10cb13a1ee1bc48c587`; the strict
native/WASM compiler and documentation gates were bracketed by identical
tracked-diff SHA-256
`b2eb53033b11de5cb10604a6f0df34767c2e68b534c9d4181c17fdc16e150bb1`.
That external dirty worktree continued advancing after the gates. The snapshot
is recorded rather than represented as a clean release pin, and the produced
artifact hashes below are not a claim that later dirty Hypercurve state is
identical.

## Separate canonical editor authority

Canonical `ALGR` remains the only executable graph authority. The new `ALGW` V1
envelope embeds its complete canonical bytes, then adds:

- explicit embedded workspace admission limits;
- a monotonic workspace revision;
- monotonic next-node and next-wire identity cursors; and
- exactly one signed-integer logical-pixel x/y placement for every graph node.

Canvas coordinates are presentation metadata, not exact physical values. No
API converts them into a graph literal, CAD coordinate, machine lattice, timer,
protocol value, or firmware command. Browser pointer motion is rounded once
into the integer canvas lattice at commit; bounded integers are projected back
to `f32` only for egui painting.

The first policy admits at most 20 MiB of complete workspace bytes, 256
placements, and coordinate magnitude 1,000,000. Replay bounds outer bytes
before length-directed work, rejects any embedded policy larger than caller
admission, independently replays the embedded `ALGR`, verifies placement
coverage and identity cursors, reconstructs the workspace, and requires exact
re-encoding before assigning SHA-256 identity. Every strict byte prefix and
trailing-data case is rejected in regression coverage.

The representative PID/interlock identities are:

| Canonical object | Identity |
| --- | --- |
| embedded `ALGR` graph | `fb173fb30bc5e04269caea439dea8fa455050142fac3a4afc78f5fd16e7ac59a` |
| `ALSI` V2 registry | `6bb6f814941b632ac5c9858fbbfe599fe8febb3a04b4dcc7bf4fbc8ac2f61537` |
| 7,836-byte `ALGT` reference trace | `4d9b63633be3afc658cac8d6475d6ede602568ab084de005ac5dd2dfcb7542a3` |
| 3,396-byte `ALGW` workspace | `d7d4ef9e27359a474b59f48cdbcb604b3d4d16f2a768a65f12c95dde8aee9799` |

## Transactional edits

The core exposes three initial mutations: move a node, connect a typed output
to an unowned input, and disconnect one retained wire. Every edit constructs
and fully validates a candidate before replacing the prior workspace.

Placement changes advance only workspace revision and preserve embedded graph
identity. Structural changes advance both graph and workspace revisions. Wire
IDs come from the monotonic `u64` cursor over the `u32` ID domain; deletion does
not rewind it, and `u32::MAX + 1` is an explicit exhausted sentinel. Invalid
coordinates, missing IDs, wrong port direction/type, duplicate input ownership,
and counter/revision overflow leave the prior core workspace unchanged. The UI
then performs audited dependency layout before replacing its live draft, so an
instantaneous cycle is likewise rejected without live mutation.

A structurally valid draft may temporarily have a required input disconnected.
The UI retains it and displays the exact audited semantic blocker rather than
silently supplying a value or repairing the graph. A candidate current-tick
cycle cannot produce the bounded semantic layout and rejects without mutation.

## Native/WASM editing surface

The control workspace constructs the one canonical `ALGW` from its audited
deterministic initial layout. A node header drag commits an integer position;
clicking one output and then one input attempts a typed connection; a secondary
click on an input disconnects its current wire. Each accepted candidate is
canonically encoded, relaid out, and audited. Reset reconstructs the reviewed
reference graph and layout. The workspace is intentionally in-memory in this
slice; file/browser persistence and history remain open.

The exact reference trace remains visible after placement-only edits because
its bound graph digest is unchanged. Any structural edit detaches and hides the
trace. The UI never presents old `ALGT` bytes as evidence for a topology they
did not simulate.

A headless egui test exercises a complete editor frame. The optimized WASM
distribution was also served only on `127.0.0.1:8765` and loaded by headless
Chromium at 1,440 by 1,000 pixels. The final 163,536-byte transient screenshot
had SHA-256
`dff7b803cf382ad6b65381e870649840ea19d51bd213de58e5c53d77e6e219a4`.
Visual inspection confirmed the complete header, canonical workspace identity
and revision, edit instructions, semantic graph, exact reference plot, and
explicit no-deployment/output warning. The live-MCU panel remained
unconfigured; this was loopback browser rendering, not radio or device HIL.

## Reproduced checks

Run from `alumina-interface` during the implementation checkpoint:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 109 executable tests pass: 11 application/coordinator, 32 client, 65 core,
and one exact-control integration test, plus the intentional compile-fail
rustdoc test. The new tests cover canonical workspace replay, every strict
prefix, trailing data, embedded-policy admission, missing placement, bad ID
cursors, graph-identity-preserving moves, monotonic wire replacement, complete
semantic reanalysis, failed-edit immutability, reference-trace detachment, and a
headless editor frame. Native and WASM warnings-denied Clippy, strict rustdoc,
local-source/permissive-license audit, optimized Trunk build, both WASM
validators, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,614,165 | `4526338db4693ee71ea8c70fd73060b5ba4661cc6a4eeebaa78fdfcdec6b1b28` |
| `alumina-interface_bg.wasm.gz` | 2,115,550 | `33d9473ed387a0071dc1007ed3c285669c641df328e9e496b5f967956efee1c2` |
| `alumina-interface_bg.wasm.br` | 1,713,461 | `76e32e062a534310a42e76150fe4c597a5728cb53b120b8eb6bfff4c4961d44a` |

The unchanged 96,792-byte `Cargo.lock` has SHA-256
`789484967e2659c753722fab8ab5c21b6f2765195d95b17e7c7fa1056846989b`.

## Closed claims, licensing, and next gate

This is an in-memory editing foundation, not a complete graph editor or front
panel. It has no node insertion/deletion, palette, parameter editing,
selection-set operation, grouping/commenting, subgraph/component document,
front-panel binding, undo/redo history, file download/upload, browser
persistence, or collaborative diff. It does not grant semantic admission,
Service/Realtime implementation, firmware opcode, resource, deployment,
safety, motor-control, or physical-output authority.

The bare MKS TinyBee V1.0 remained on its existing disconnected-load HIL image;
it was not reset, flashed, or contacted. No motor, motor driver, or process
power was connected. No workstation Wi-Fi setting changed. Live AP/HTTP load
and SLogic16U3 capture remain postponed until Wi-Fi can be dedicated to the
Alumina AP without interrupting the Internet development session.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted only the local CSGRS/Hyper/Alumina stacks and
the existing permissive native/WASM inventory. No GPL-family source, library,
tool output, or asset was copied, linked, or vendored.

The next offline editor slice is canonical node creation/deletion and a
schema-derived palette/parameter surface, followed by undo/redo and browser/file
persistence. Component/front-panel documents and broader bounded probes remain
separate work. Physical Wi-Fi and input timing remain retained-capture gates.
