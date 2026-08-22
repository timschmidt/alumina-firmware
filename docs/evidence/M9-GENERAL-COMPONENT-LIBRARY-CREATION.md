# M9 general component-library creation evidence

Date: 2026-08-22

Authoritative interface source:
`f767f5003609e8deb0decab945afc5d2c836d73b`
(`feat: create exact library components`)

This checkpoint closes the first greenfield, file-free component creation
workflow in the exact graph editor. The visible component-library panel accepts
one bounded stable name, deterministically constructs a version-1 empty
`ALGC`, admits it into the current exact hierarchy, and immediately selects it
for ordinary definition authoring. It adds no compatibility path and does not
interpret an old interface or firmware format.

This is host authoring/compiler behavior only. It grants no firmware opcode,
network session, storage mutation, GPIO ownership, motion, arming, timing,
start, or safety authority.

## Deterministic empty package

`empty_library_component` derives the new package only from the requested name
and the current control workspace's admitted authority:

- the embedded `ALGR` is revision 1 and retains the exact current schema and
  clock definitions, with no nodes or wires;
- the embedded `ALGW` is revision 1, has node and wire cursors 1, and has no
  placements;
- the outer `ALGC` is revision 1 and component version 1;
- input, output, and panel-item cursors are 1; and
- public connectors and panel items are empty.

No timestamp, random value, UI position, machine identity, file metadata, or
display projection enters those canonical bytes. The package is therefore
content-addressed and repeatable for the same admitted graph context and name.
It remains a host component definition with no execution semantics beyond the
ordinary audited nodes later authored inside it.

The constructor delegates stable-name syntax, UTF-8, length, graph schema,
clock, limit, cursor, revision, and canonical ordering checks to the existing
`ALGC`/`ALGW`/`ALGR` boundaries. An invalid or overlong name therefore fails
before a hierarchy candidate exists.

## Name and identity admission

The creation boundary encodes the complete candidate `ALGC` and searches every
current dependency before mutation. A dependency with the requested stable
name and a different digest rejects the candidate atomically, even if another
byte-identical candidate is also present. Stable names therefore cannot become
ambiguous through this UI operation.

The canonical hierarchy remains digest-addressed. If the exact candidate digest
is already present, `add_component` is an exact no-op and the UI changes only
its transient selected-library scope. The complete `ALGS`, both history stacks,
and pending-persistence state remain unchanged. This is not a compatibility
alias or a second name registry.

## Complete-session transaction

An accepted creation follows the same fail-closed transaction as every other
hierarchy edit:

1. construct and canonically encode the empty `ALGC` candidate;
2. reject every invalid, overlong, or conflicting stable name;
3. add the exact component identity to a cloned `ALGH` dependency library;
4. validate every dependency's ordinary-node draft through the audited UI
   registry;
5. freshly encode and flatten the complete `ALGH`;
6. regenerate total source-bound `ALGM` provenance;
7. admit the flattened ordinary graph through the same semantic registry;
8. construct and replay one complete canonical `ALGS`; and
9. record the exact prior session before committing history and browser
   persistence.

Only after that transaction succeeds does the UI select the new dependency and
clear the accepted name field. A rejection retains the user's name draft and
all canonical state.

The selected-definition canvas can immediately add the first audited ordinary
node. That edit allocates node identity 1, advances only the new workspace's
node cursor, replaces the component by exact digest, refreshes recursive
hierarchy identities and source binding, and commits another complete session.
It cannot indirectly rewrite the selected control authority.

## Native and adversarial coverage

The complete native qualification passed:

- 98 application/coordinator tests;
- 82 browser/client tests;
- 183 exact-core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 365 unit, integration, and compile-fail checks. The new focused
regressions prove:

- exact revision/version/cursor initialization and empty connector, panel,
  placement, node, and wire sets;
- byte-identical inheritance of the current graph schema and clocks;
- isolation of the control workspace, probes, cached-job workspace, selected
  control component, root workspace, and existing dependencies;
- exact duplicate creation as selection-only state with no history or pending
  persistence;
- immediate first-node definition authoring with recursive digest selection;
- exact complete-session Undo, Redo, persistence, and restore; and
- atomic rejection of an empty name, malformed name, 65-byte name, and a name
  already bound to the nonempty control component.

## Native, WASM, source, and release qualification

The following checks passed against the live, read-only workspace CSGRS/Hyper
paths:

```text
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
env RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip/brotli integrity and byte-exact decompression checks
```

The strict `--no-deps` native and WASM interface lints were warning-free.
Hypercurve was being edited concurrently and emitted its current read-only
warnings for one unused diagnostic closure parameter and three unused internal
helper groups. No Hyper/CSGRS repository state was inspected or changed.
`Cargo.lock` remained SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

The source-policy audit accepted only the sibling Alumina/CSGRS/Hyper paths and
the permissive native/WASM license inventories.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `ca0a1d852a544547cb75111adf876ea7de8b60be93c62cbe8e27cfcf9b00e5d3` |
| `alumina-interface.js` | 91,816 | `b65dc66c7a9b5d98bfb0818880d5efb9541e4c246c393a3dd8aecde2c1795da0` |
| `alumina-interface_bg.wasm` | 6,774,800 | `ad8677bdfa3932ec1694c313ead1a9786ef4ff64c6ed1f6835afeed8bb96ba51` |
| WASM gzip | 2,982,139 | `b904442c11e27e244fe107dcc1014b0bf07db0852f1af372f679c287a3d8a438` |
| WASM Brotli | 2,345,267 | `b2540937899573cd2929ea8f6f3b2d52b7b98401715bf56eab677e426ef34b79` |

Independent gzip and Brotli decompression reproduced every uncompressed
artifact byte-for-byte.

## Optimized browser lifecycle

The application and proof script addressed the optimized bundle through
loopback HTTP and an isolated Chromium debugging port. The proof cleared
origin-local storage, opened the visible Control graph, entered
`user.browser_component`, clicked `create empty component`, used the visible
selected-definition editor, and exercised the visible history controls.

| Canonical state | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGS` | 14,770 | `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` |
| empty-component `ALGS` | 15,510 | `991bb737edcd0a3ad1f45b5f0e05d657c4e2557deddec46288c6512f168fe0fe` |
| one-node `ALGS` | 15,625 | `88bf2e0c9bc04d11112b3a8de865e242fd5beadbf57ab09cd4b82d235122ffa1` |
| empty revision-1 `ALGC` | 736 | `264505540b35ca004697783b0136ec2fb8f09230768355d46627af3e3c07f36f` |
| populated revision-2 `ALGC` | 851 | `b46e9dc604882ce77eebec1ebb8195fa7f4d7159558b4e4f00dcace6e6225198` |

The empty component embeds a 612-byte revision-1 `ALGW`
`4291c6fd4e9e94ee5d5e2ce93391db7b12e3d98bdf4353973cf637cce4d986cf`
and a 548-byte revision-1 `ALGR`
`bac86eedd541722a56758622b0623977ff29cb8ca9c83bd8ea68fd6436cb9877`.
Its node/wire and component connector/panel cursors are all one; every
collection is empty. Comparing its complete graph prefix through the clock
section with the current control graph proved byte-identical schema and clock
inheritance.

Adding `New bool.and` as node 1 produced a 727-byte revision-2 `ALGW`
`b6f5349a916c7f17199ac2f2b0b1befc9baaadb3be12c8752dfa5c039a459ce6`
and a 651-byte revision-2 `ALGR`
`3f6d043daf26d6744048331e63bd092536d78a5ce2777eb4ea701d0d3ba21d53`.

The complete `ALGH` identities moved from the 7,124-byte reference
`f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a`
to the 7,864-byte empty-component state
`98a27e8e59bb0c5173ac1e16f4c19bc7431a8ec782dd75fcde0ee1d970c015f2`,
then to the 7,979-byte one-node state
`30293eff2e855f686ee36d5b6ba297ddc9671d8fba528fe32f13217ed0e90220`.
`ALGM` correctly rebound its source-hierarchy digest at each step, while the
flattened workspace identity remained
`6804b964535d08b9ceead3d43891c3ae4c5aa5ce38b015c34b4b385bfa3257d4`
and its 2,478-byte, 21-node/25-wire provenance body remained exactly
`552549c14a78850b01d9c6c3e68dafc4d1ba62d3dfee5746c7e54b5309ae8e85`.

The control `ALGW`, `ALGP`, cached-job `ALGW`, selected control authority,
root `ALGW`, flattened workspace, and existing dependencies remained exact.
Re-entering the same empty component name before editing left the 15,510-byte
session byte-identical and added no history. Visible Undo restored that exact
empty state, visible Redo restored the exact one-node state, and a fresh reload
retained the exact one-node bytes. After reload, the component was visibly
selectable from the library with its `New bool.and` node rendered.

| Browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| visible creation row | 358,285 | `22502b9e213e4950be8bac906ae384180739b69612649c8144075826ce7e8d60` |
| empty component selected | 299,565 | `3a2442c63870519d2d242ca80a513f9511c8abc2c33caa84b5c7a32b91a222cc` |
| duplicate selection-only no-op | 366,494 | `a83e47f5b417e47071740890aafcc165d51dba0ad065a465ba94e23c424f59b8` |
| populated component definition | 299,305 | `1192e247df919a815d9a1d7c9adcc4b9c614041ee829f439a6684ff9af7e7842` |
| visible two-state history controls | 389,146 | `ac10a6e9d15049e9f51f7249655b50436ddac6cd9638a7b2c065c7203bae3fee` |
| exact state after visible Undo | 385,179 | `8b4a0eed0257a7ee1c7f0ad87be3e9ffb96575a49de2f8bf60b58afde50e9b02` |
| reloaded component library popup | 367,660 | `2cf6cda3ce4b2e1c63015feee818ba30e1c23098eb3c6bb33d29792fe27bfc3e` |
| reloaded populated component | 302,066 | `8f30f346aa01468715fc8ddc69e5f05d62348c98990f4a381c6fb8531411a4a5` |

The retained parsed result is 58,356 bytes with SHA-256
`f01e23a0018b8ea86d9442884019f8fab19777bdd71173537871085d0c1e3332`.
Screenshots, bounded OCR locator frames, and the result remain transient
`/tmp/alumina-component-creation-proof.CjnxWH` evidence and are not repository
inputs. Failed intermediate proof captures and the disposable Chromium profile
were removed; Chromium and the loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or Alumina device-network path was used. Workstation Wi-Fi and
NetworkManager were not changed.

Hypercurve remained a moving read-only dependency. This work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The implementation is independently authored under MIT. The
source-policy audit found no GPL-family dependency, copied external
planner/control implementation, compatibility shim, or retired-repository
path.

Deterministic named empty-component construction, exact schema/clock
inheritance, initialized monotonic cursors, stable-name conflict rejection,
selection-only duplicate handling, immediate definition editing,
complete-session history/persistence, and optimized browser reload are now
closed. Component rename/version evolution, nested package/binding exchange,
parameter promotion/overrides, coordinated descendant/control-authority
replacement, runtime panel injection, collaboration/conflict handling, and
crash-durable journals remain separate work.
