# M9 exact root-hierarchy canvas evidence

Date: 2026-08-21

Authoritative interface source:
`584a120050f6efd150bc88699ec7f8bdc0606267`
(`feat: author exact root hierarchy canvas`)

This checkpoint closes exact placement and typed root-wire authoring on the
structural `ALGW` canvas embedded in the canonical `ALGH`. It is a host-side
authoring/compiler change only. It changes no firmware protocol, runtime
package, device resource, GPIO, Wi-Fi, cache, motion, arming, start, or safety
authority.

## Transactional hierarchy mutations

`GraphHierarchyDocument` now exposes three bounded clone-and-reconstruct
operations:

- `move_root_instance` accepts only a root placeholder with an exact `ALGH`
  binding, changes its signed integer presentation coordinate, and treats an
  identical coordinate as an exact no-op;
- `connect_root_wire` delegates endpoint ownership, direction, type, fan-in,
  duplicate, and graph-limit checks to the canonical `ALGW`, then validates the
  complete hierarchy; and
- `disconnect_root_wire` removes one exact known root wire without rewinding
  the monotonic wire cursor.

Every changed candidate advances the checked hierarchy revision and is
reconstructed through complete `ALGH` validation. Any unknown occurrence,
wire, endpoint, overflow, invalid type/direction/fan-in, recursive expansion,
or limit failure leaves the original document byte-for-byte unchanged. These
operations bring the directly exposed hierarchy transaction set to eight:
component add/remove/replace, root occurrence add/delete/move, and root wire
connect/disconnect.

Core regression coverage proves exact movement without changing the embedded
graph digest, movement no-op behavior, disconnect/reconnect with fresh wire ID
`3` after cursor `2`, canonical hierarchy replay, and atomic rejection of a
duplicate connection, unknown wire, and unbound ordinary-node movement.

## Structural authoring surface and complete-session admission

The component hierarchy panel now projects the bounded root workspace as a
minimum-height structural canvas. It paints exact integer placements,
placeholder connectors, and existing wires without assigning executable
semantics to `alumina.component.instance` nodes. A bound component header can
be dragged, selecting an output followed by a type-compatible input proposes a
wire, and secondary-clicking a connected input proposes disconnection.

Egui reports per-frame drag deltas and may report zero on the release frame.
Both the ordinary graph canvas and new hierarchy canvas therefore retain a
cumulative gesture delta and round only the complete logical displacement at
commit. The new canvas also reserves 260 logical pixels inside the enclosing
scroll area so its initial content is not clipped to egui's default inner
height.

Each accepted UI proposal performs the complete authoring transaction before
changing visible state:

1. clone and mutate the `ALGH` document;
2. encode and replay the complete hierarchy;
3. recursively flatten it to ordinary `ALGW`;
4. regenerate the total canonical `ALGM` source map;
5. semantically admit the flattened ordinary draft;
6. construct the complete canonical `ALGS` snapshot; and
7. record the prior snapshot in unified undo/redo history and persistence.

Pending connector selection is cleared only after a successful wire mutation.
Rejected and exact no-op proposals retain the exact session, roles, history,
and persistence bytes. UI regression coverage builds a valid one-input leaf,
then proves root movement, duplicate movement no-op, dependency import, root
occurrence creation, typed connect/disconnect/reconnect with monotonic IDs,
unified history, isolation of all unrelated roles, persistence restoration,
and atomic invalid-edit rejection.

## Native and WASM qualification

`cargo test --workspace --offline` passed:

- 85 application/coordinator tests;
- 82 client tests;
- 176 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 345 unit/integration/compile-fail checks. The following stricter checks
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
```

The Alumina crates passed warnings-denied native/WASM clippy and rustdoc. The
live moving Hypercurve path separately emitted its existing unused-function
warning for
`relation_to_supporting_line_with_certified_tangencies` in
`hypercurve/src/bezier_offset.rs`; this checkpoint neither changes nor hides
that upstream diagnostic. `Cargo.lock` remained unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

Gzip and Brotli decompression compared byte-for-byte with the optimized WASM:

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `18ca6234f3b2824dad02df8d32cd36e9f37fb0ae79c0192d99a122479615398b` |
| `alumina-interface.js` | 91,816 | `3a923eae2253efd3c784836764cb7bdbf202db0b68b0aebc054857f60d4e1098` |
| `alumina-interface_bg.wasm` | 6,639,269 | `feddecd54339ac8428c8728cf99d7f20ac7fcbd42787e0d8cd93c5b78d2b3625` |
| WASM gzip | 2,930,709 | `daa4d123712c111c7ed0e4c427890b2c4a4238eb390f5b3c3066e95e3c9afa84` |
| WASM Brotli | 2,308,636 | `d1c1c42bc85b5ff9c230eb60670b512c5273427d89ad25f694eeae72829c3813` |

## Optimized browser root-placement lifecycle

Headless Chromium loaded only the optimized loopback bundle. Through the
visible canvas it dragged bound root wrapper `n1` by exactly `(+130, +60)`
logical pixels, from `(28, 28)` to `(158, 88)`. The test then used the visible
Undo and Redo buttons and performed a fresh page reload.

| State | ALGS bytes / SHA-256 | ALGH SHA-256 | Root ALGW SHA-256 | Placement |
| --- | --- | --- | --- | --- |
| initial | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | `3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6` | `n1 @ (28,28)` |
| moved | 14,770 / `90167e8ee1e404caa24b4827d05bd9b6959f5196168ee980ed75bf1701937ac5` | `02eed6a0b7d8b2c8b29b1af3f25bc37bb9cb612798a4a49ba2087831c680d405` | `01a7ff6369a960824e090e4a8cd7d33093620f3fba8cfb365cbe5b76d58be82a` | `n1 @ (158,88)` |
| visible Undo | exact initial bytes | exact initial bytes | exact initial bytes | `n1 @ (28,28)` |
| visible Redo | exact moved bytes | exact moved bytes | exact moved bytes | `n1 @ (158,88)` |
| fresh reload | exact moved bytes | exact moved bytes | exact moved bytes | `n1 @ (158,88)` |

The moved 2,550-byte `ALGM` had SHA-256
`6713465fc123a2a40f847366e1a9b2046e0ec061a60fb188b7db0610b995317e`.
The control `ALGW`, bound `ALGP`, cached-job `ALGW`, selected `ALGC`, and both
dependency encodings remained byte-identical throughout. Their retained
identities were, respectively,
`bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16`,
`50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223`,
`493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58`,
and selected component
`10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228`.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| initial visible root canvas | 328,425 | `c066c0004cd1dc8fae538aa3944b190e0d084cbb00630e410ae3cc7ac738b334` |
| moved visible root canvas | 328,470 | `e98d53dfe796154cdfefcd3fc333fe6db10e043789b57b9213be0120185a079c` |
| visible history controls after Redo | 367,686 | `d598be8ed5b33da328fe8a9a5ac08a314d4c9a5910ca849088531f71f7a63a5a` |
| exact moved-session reload | 238,648 | `b8a25c6a01529519aff0ea3bfbc6bdf74218993eb2fd5a16b9f81a54fc6b486d` |

The screenshots and browser result document remain transient `/tmp` evidence
and are not repository inputs. Chromium and the loopback server were stopped
after replay. Typed root wiring is proven at the canonical core and complete UI
transaction layers; this browser run was deliberately scoped to the actual
pointer-drag/history/persistence path rather than fabricating an otherwise
unavailable multi-input visual fixture.

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

Exact root placement and typed root wiring are now closed. Nested-definition
canvases, nested package/binding exchange, component rename/version evolution,
connector/front-panel authoring, parameter promotion/overrides, package
signatures and permissions, locked dependency manifests, incremental
flattening, collaborative conflict handling, and crash-durable history remain
separate work. `ALGW`/`ALGC`/`ALGH`/`ALGM`/`ALGS` authoring still grants no
firmware opcode, resource ownership, cache preparation, execution, arming,
start, timing, motion, or safety authority.
