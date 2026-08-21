# M9 exact hierarchy source-map evidence

Date: 2026-08-21

## Scope and source

This checkpoint makes recursive component flattening traceable without making
imported provenance authoritative. The implementation is `alumina-interface`
commit `b73a1b321eabbe65fc9ae89f04fc032a9d1bc367`, following the bounded
recursive hierarchy at `3d7dff39295bab7810b7528cc9d030221796c468`. This
evidence is based on preceding `alumina-firmware` commit
`e98ecef86ee95d9ff4aa6871908404cb02a6eba0`. It changes no firmware source,
wire format, task ownership, board package, or hardware authority.

The build used the current checked-out sibling CSGRS/Hyper paths. Hypercurve
was explicitly treated as a moving read-only dependency: it was not pinned,
modified, formatted, staged, or assigned a reproducibility claim. Final native
and WASM qualification compiled against the live workspace state. No
compatibility layer targets an older CSGRS/Hyper release.

## Total recursive provenance

Every node and wire in the final ordinary flattened `ALGW` now has exactly one
origin:

- an ordinary root item retains its root-local node or wire ID; or
- a copied item retains its nonempty occurrence source path, exact source
  `ALGC` digest, and component-local node or wire ID.

The source path begins with the root placeholder ID and continues with each
nested component-local placeholder ID. Removed placeholders have no final-node
record. Provenance is sorted by final identity and supports both
final-to-origin and origin-to-final lookup. Missing final items, stale items,
duplicate origins, duplicate final identities, empty paths, and noncanonical
ordering fail before a flattening result is returned.

Wire provenance moves with a wire whenever public connector replacement gives
it a fresh monotonic final ID. Tests prove repeated root-wire reconnection
through the wrapper and leaf and separately prove that a wire authored in a
parent component retains the parent's digest, source path, and local wire ID
after its nested child is replaced.

## Canonical bounded ALGM V1

`ALGM` V1 serializes the total map as a separate host-side artifact. Its
little-endian canonical bytes contain:

1. `ALGM`, version 1, and zero flags;
2. embedded maximum bytes, node records, wire records, and path depth;
3. the exact complete source `ALGH` digest;
4. the exact flattened `ALGW` digest;
5. node-origin records in strictly increasing final-node order; and
6. wire-origin records in strictly increasing final-wire order.

Origin tag 0 names a root-local item. Tag 1 names a bounded source path, exact
component digest, and component-local item. The interactive policy admits at
most 4 MiB, 4,096 node records, 8,192 wire records, and depth 32. Encoding
precomputes the complete byte length with checked arithmetic, validates every
`u32` count, and rejects the byte ceiling before allocating the exact output
vector.

Imported bytes never supply provenance independently. Replay first bounds the
outer bytes and header, admits every embedded policy under caller limits, and
requires the digest of the caller-supplied complete hierarchy. It then freshly
flattens that hierarchy, requires the exact resulting workspace digest, bounds
and validates all records, rejects duplicates/trailing bytes, regenerates the
complete map under the embedded policy, and requires byte-for-byte equality.
Only that fresh flattening and regenerated encoding are returned.

Tests cover both origin tags, total node/wire counts, stable nested paths,
bidirectional lookup, parent and root wire reconnection, exact golden replay,
wrong source/workspace identities, bad magic/version/origin tags, trailing
bytes, caller byte admission, path-depth failure, and
pre-allocation byte-ceiling rejection.

## Visible exact correlation

The unchanged visible hierarchy remains the wrapper-to-PID proof:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| source `ALGH` V2 | 7,124 | `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` |
| flattened `ALGW` | 3,755 | `6804b964535d08b9ceead3d43891c3ae4c5aa5ce38b015c34b4b385bfa3257d4` |
| total `ALGM` V1 | 2,550 | `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |

The hierarchy expands two occurrences at depth two to 21 final nodes and 25
final wires. Its leaf path is `[1/1]`; leaf-local node `n8` maps to final `n10`
and endpoint `n8.p1` maps to `n10.p1`. The component panel displays the map
identity and total origin counts and offers bounded `.algm` download/open
controls. Open replays against the current complete hierarchy without mutating
graph, component, hierarchy, probes, trace, or deployment state. The
selected-node inspector and exact trace cursor prefix local endpoints with up
to four occurrence/final mappings and explicitly count any additional
occurrences.

The independent core golden includes two ordinary root nodes, one ordinary
root wire, and a nested leaf. Its 23 node and 26 wire records encode to 2,577
bytes with SHA-256
`885eaed4357bdc7c3d97d2eff76321732260d6147afbacdb5e004bbd23c291e3`.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

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

All 325 executable tests pass: 74 application/coordinator tests, 82 client
tests, 168 core tests, and one cross-crate exact-control integration test, plus
the intentional compile-fail rustdoc test. Strict native and WASM Clippy, WASM
check, warnings-denied documentation, source-policy/license audit, optimized
Trunk build, both WASM validators, and gzip/Brotli checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `cc4384410e31568e0a4d2dddacc615c17453056d6bf77283d1f9e74b335d673d` |
| `alumina-interface.js` | 91,816 | `d178b732214ffce898a22f73bd0bd7b62a3ba810306d34a392cfc827b3efe7d2` |
| `alumina-interface_bg.wasm` | 6,578,435 | `79473b88fbb321fdcf5a78d477142fe78205f67c329f809482c27d5f5c78b24c` |
| `alumina-interface_bg.wasm.gz` | 2,905,401 | `a118c8d612d31e936de5a4095e465787ee6bc9ec0ba1709bfc494485a8b84edc` |
| `alumina-interface_bg.wasm.br` | 2,288,266 | `8be9c85b90331a893bcf31c4df3c1f40ff33d11f64809cc5104e7edce9d17fbe` |

The unchanged 99,454-byte lockfile has SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`;
no dependency entered the resolved inventory.

## Localhost browser proof

The optimized bundle was served only on `127.0.0.1:4173` and rendered in
headless Chromium software WebGL at 1,600 by 1,000 pixels. Non-loopback host
resolution and background network features were disabled. The application
reconstructed the exact 3,755-byte editable control workspace, 407-byte probe
sidecar, and 825-byte catalog-bound job workspace from its bounded `algwb1:`
record. The only logged browser entries were Chromium's preload-SRI warning;
there was no application error.

Visual inspection confirmed:

- the sidebar `ALGM dbfaf692…` identity and 2,550-byte count;
- the component-panel `ALGM` identity, 21 node/25 wire origins, and
  `download bytes` / `open .algm` controls; and
- exact trace cursor prefixes including `[1/1]:n7.p3 → flat n9.p3` and the
  remaining PID/interlock occurrence-to-final endpoints.

The browser's real download control produced
`alumina-dbfaf69255a1a415.algm`. It was exactly 2,550 bytes, began with header
`414c474d01000000` (`ALGM`, V1, zero flags), and had SHA-256
`dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77`.

The 309,302-byte component-panel screenshot has SHA-256
`3cedae0418f693a0c0b181941279713ef5a820ea7f0765aded906bcd2306da21`.
The 265,895-byte trace-correlation screenshot has SHA-256
`06641bc886d8524946b0b8df7a03661d2066b14d1454cbbb5726b7f63850b1d2`.
These transient screenshots and the downloaded proof file remain in `/tmp` and
are not repository inputs. The browser and loopback server were stopped after
capture.

## Closed claims, hardware, and licensing

`ALGM` is UI/compiler audit metadata only. It grants no graph semantics,
implementation choice, resource allocation, timing, firmware installation,
runtime recursion, safety authority, or physical output. Firmware continues to
receive only separately lowered, authenticated fixed graph IR.

Editable nested canvases, hierarchy-aware persistence/history, interactive
traversal from final items into nested component editors, signed dependency
manifests, and live-device trace correlation remain separate work.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read,
or configured. No GPIO, motor, driver, motor/process power, serial port,
analyzer, device AP, or device network path was used. Workstation Wi-Fi and
NetworkManager were not changed; browser qualification used only loopback.

The implementation is independently authored under MIT. No dependency was
added, and the source-policy audit accepted the existing permissive inventory.
No GPL-family source or dependency was introduced.
