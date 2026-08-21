# M9 bounded recursive graph hierarchy evidence

Date: 2026-08-21

## Scope and source

This checkpoint replaces the leaf-only hierarchy compiler with a bounded
component dependency DAG and deterministic recursive flattening. The
implementation is `alumina-interface` commit
`3d7dff39295bab7810b7528cc9d030221796c468`, against preceding
`alumina-firmware` evidence commit
`2628b752beb6a85a15e689d229b4d51dfa10e8e6`. It changes no firmware source or
firmware wire format.

The build used the current checked-out sibling CSGRS/Hyper paths without
pinning, modifying, formatting, staging, or asserting revisions for those
moving repositories. No compatibility layer targets an older CSGRS or Hyper
release.

## Canonical ALGH V2 package

`ALGH` V2 is the only accepted hierarchy version. There is no V1 decoder or
shim. It embeds an exact root `ALGW`, a SHA-256-digest-sorted library of complete
canonical `ALGC` definitions, and scoped instance bindings. A scope is either
the root workspace or the exact digest of a containing component. Each binding
then names one local placeholder node and one exact child-component digest.
Canonical order is scope, local node, then child identity; duplicate
`(scope, node)` records reject.

The first caller/embedded policy admits at most 32 MiB, 64 dependencies, 4,096
binding records, nesting depth 32, 4,096 root-reachable expanded occurrences,
4,096 flattened nodes, and 8,192 flattened wires. Every embedded `ALGW`,
`ALGC`, and `ALGR` retains its independent limits. Replay checks outer bytes and
counts before allocation, rejects unknown scope tags, independently replays all
nested envelopes, reconstructs the complete hierarchy, rejects trailing bytes,
and requires byte-for-byte re-encoding before assigning identity. V1 bytes fail
with an unsupported-version result.

Every root and component placeholder must have exactly one binding. The parent
scope and child identity must both exist, all dependencies must share the root's
exact type and clock context, and each reserved node's kind, version, domain,
ports, names, exact types, and empty parameter set must match the derived child
connector shape.

## Dependency proof and recursive flattening

A memoized checked-arithmetic traversal derives each component's expansion
depth, occurrence count, surviving node count, and wire count. It validates
every retained definition, rejects a dependency cycle at the exact digest it
re-enters, and admits the root-reachable result only when it fits hierarchy,
graph, and placement limits.

Flattening clones the root and walks canonical root bindings depth first in
pre-order. Each occurrence is deleted, copied with fresh monotonic root node
and wire identities, translated from its exact integer placement, reconnected
through its derived public terminals, and then recursively expanded through a
private map of transient nested placeholders. Sibling and cross-instance wires
remain ordinary typed workspace edits. The result must match the proved
occurrence/node/wire counts and contain no reserved placeholder before its
canonical `ALGW` is returned.

The audit report now records every expanded occurrence in depth-first order.
Each record carries the transient removed node, exact component digest,
surviving local-to-root node mappings, and a stable source path consisting of
the root instance ID followed by nested component-local instance IDs. A pure
wrapper can correctly retain an empty surviving-node map while its path remains
unambiguous.

Tests cover canonical dependency and scoped-binding order, exact nested replay,
depth-two flattening and semantic analysis, stable source paths, missing and
duplicate nested bindings, unknown parent scopes, connector shape and version
failure, dependency cycles, nesting-depth and expanded-occurrence ceilings,
flattened limits, corrupt scope tags, V1 rejection, root-to-root instance
wiring, and independent canonical replay of the flattened workspace.

## Visible recursive proof

The control UI constructs a root `control.reference_pid_wrapper` occurrence
whose component definition contains one exact `control.reference_pid` leaf.
The canonical 7,124-byte `ALGH` has SHA-256
`f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a`.
It expands two occurrences at depth two to the same 21 audited semantic nodes
and 25 wires. The depth-first paths are `[1]` and `[1, 1]`; the wrapper's public
connectors map through the transient leaf placeholder.

The resulting 3,755-byte `ALGW` has SHA-256
`6804b964535d08b9ceead3d43891c3ae4c5aa5ce38b015c34b4b385bfa3257d4`.
It replays canonically and passes the existing audited HostExact semantic
registry. The UI displays the source hierarchy prefix, two expanded
occurrences, depth two, final counts, and flattened workspace prefix beside the
reusable front panel.

## Reproduced checks

Run from `alumina-interface` at the implementation checkpoint:

```console
cargo fmt -p alumina-interface -p alumina-interface-core -- --check
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

All 322 executable tests pass: 74 application/coordinator tests, 82 client
tests, 165 core tests, and one cross-crate exact-control integration test, plus
the intentional compile-fail rustdoc test. Strict native and WASM Clippy, WASM
check, warnings-denied documentation, the local-source/permissive-license audit,
the optimized Trunk build, both WASM validators, and compressed-artifact checks
pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `bb560d1fb6cf1547d435afb7161828918259d30cf4f17df9a6bb55969fe922e7` |
| `alumina-interface.js` | 91,816 | `4546dcd9448a3df7bd6db9d7fbc13464a0cd2ee1604c7e46e710ca37f7d6621c` |
| `alumina-interface_bg.wasm` | 6,537,519 | `433b1d9a5da0bb2bdc7229896cdb97c849151c1524a828359f726c152542350d` |
| `alumina-interface_bg.wasm.gz` | 2,890,614 | `2b06e49fa999babf3de990fdb119c1f979c0c0834e82338f6fa727f4c011160d` |
| `alumina-interface_bg.wasm.br` | 2,279,076 | `43fec35639c5ec48e2d882f5a29b08c048d57e90360b10a8baee93d62ded120a` |

The unchanged 99,454-byte lockfile has SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`;
no dependency entered the resolved inventory.

The optimized bundle was served only on `127.0.0.1` and rendered in headless
Chromium software WebGL at 1,440 by 1,600 pixels. Browser state reconstructed
the exact 3,755-byte workspace, 407-byte probe sidecar, and 825-byte cached-job
catalog from its bounded `algwb1` record. Visual inspection confirmed the
depth-two hierarchy line, both canonical prefixes, component panel, ordinary
graph, trace, and explicit no-deployment/output warning. The 385,286-byte
transient screenshot has SHA-256
`e459d39064652cb07361e042fff59b35d50e60285428781839d05b4abb49ab62`.
The loopback server and browser were stopped after capture.

## Closed claims, hardware, and licensing

`ALGH` and its instance nodes remain UI/compiler structure only. They grant no
runtime recursion, semantic implementation, resource allocation, timing,
safety, firmware, or physical-output authority. Firmware still receives only
the separately lowered and authenticated fixed graph IR.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No GPIO, motor, driver, motor/process power, serial port, analyzer,
or device network path was used. Workstation Wi-Fi and NetworkManager were not
changed; browser qualification used the loopback application endpoint.

The implementation is independently authored under MIT. No dependency was
added, and the source-policy audit accepted only the existing permissive
inventory. No GPL-family source or dependency was introduced.
