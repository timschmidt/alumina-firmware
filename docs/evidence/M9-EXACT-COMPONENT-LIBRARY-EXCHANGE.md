# M9 exact component-library exchange evidence

Date: 2026-08-21

Authoritative interface source:
`ae2860cfb0d70fb3d5e03de65f06030e4bf48d0d`
(`feat: exchange exact component library leaves`)

This checkpoint closes the first standalone component-library file workflow in
the greenfield graph editor. The UI can export the exact selected canonical
`ALGC`, import one bounded canonical leaf as an unreferenced `ALGH` dependency,
recognize an exact duplicate as a no-op, and remove the dependency only while
it remains unreferenced and is not the complete-session control authority.

This is host authoring/compiler behavior only. It changes no firmware protocol,
runtime package, device resource, GPIO, Wi-Fi, cache, motion, arming, start, or
safety authority.

## Transactional component-library core

`GraphHierarchyDocument` now adds two clone-and-validate operations beside the
existing root-instance and replacement operations:

- `add_component` canonically encodes a complete `GraphComponentDocument`,
  derives its SHA-256 identity, and reconstructs the entire hierarchy with the
  new dependency and next checked hierarchy revision;
- a byte-identical dependency already in the exact library returns its existing
  identity without changing the document or revision; and
- `remove_component` accepts only a known identity that is named by neither a
  child binding nor a component parent scope, then reconstructs the complete
  hierarchy without it.

Unknown, referenced, noncanonical, colliding, over-limit, or context-invalid
candidates leave the original hierarchy byte-for-byte unchanged. Removal does
not alter the root workspace, reachable expansion, or monotonic root identity
cursors. Core regression coverage proves add/replay/duplicate/remove, revision
progress, unchanged root and flattened counts, and atomic referenced/unknown
removal failures.

## File admission and complete-session transaction

The visible component-library panel now supplies one bounded browser/native
file bridge for the selected dependency. Export writes its retained canonical
bytes directly. Import performs, in order:

1. bounded `ALGC`, embedded `ALGW`, and embedded `ALGR` replay;
2. byte-for-byte canonical re-encoding and digest recovery;
3. audited UI semantic admission of every ordinary node in the imported draft;
4. exact current hierarchy type/clock/limit/context validation through
   `add_component`;
5. complete-library ordinary-node semantic admission, including unreferenced
   definitions;
6. fresh `ALGH` encoding and deterministic flattening;
7. fresh total `ALGM` regeneration and flattened-draft semantic admission;
8. complete candidate `ALGS` construction; and
9. exact prior-session history recording before the hierarchy package,
   persistence state, and transient selection commit together.

Structural `alumina.component.instance` placeholders remain the responsibility
of `ALGH` validation. The per-definition semantic audit removes those
structural nodes and their incident draft wires before checking all remaining
ordinary nodes; it grants the placeholders no executable behavior. The same
whole-library audit runs during complete-session restore, so an unreferenced
unknown ordinary node cannot be smuggled through persisted or imported `ALGS`.

A standalone `ALGC` contains no scoped `ALGH` binding records. This first file
workflow therefore deliberately admits leaf definitions only. Nested
definition exchange remains a later package/binding workflow rather than an
implicit or guessed compatibility path.

The panel disables removal while any child or parent-scope binding refers to
the selected dependency. It separately protects the selected `ALGC` whose
embedded workspace is complete-session control authority. Exact duplicate
import updates only transient selection/status, records no history, and does
not dirty persistence. Successful add/remove changes use the same complete
`ALGS` undo/redo and origin-local persistence timeline as graph, probe,
cached-job, component, and root-occurrence edits.

## Native and WASM qualification

`cargo test --workspace --locked` passed:

- 84 application/coordinator tests;
- 82 client tests;
- 175 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 343 unit/integration/compile-fail checks. New UI coverage proves a
hierarchy-only leaf import, stable control/probe/cached-job/selected-component
roles, exact duplicate no-op behavior, referenced removal rejection, root
delete then dependency removal, unified undo/redo, persistence/reload with empty
restored history, corrupt encoding rejection, unreviewed ordinary-node semantic
rejection, and authoritative-component removal rejection. Every rejected case
retains exact session bytes, history, and persistence state.

The following stricter checks also passed against the live workspace Hyper,
Hypercurve, Hypergraphics, and CSGRS paths:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --locked -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
```

Gzip and Brotli integrity checks passed, and independently decompressed WASM
bytes compared exactly with the optimized artifact. `Cargo.lock` remained
unchanged at SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `81ae76b16cd27cad646a837e856508e626a0d1b3f3cb06e72e08e3b38239c301` |
| `alumina-interface.js` | 91,816 | `39c66954ac5915ca2f74a3a454eafa8db38923482412827621eee72fc33283a1` |
| `alumina-interface_bg.wasm` | 6,626,971 | `2810c4204f9da680008947fedd8a50f23dd3a5c150d8caf08cd704e59376efd9` |
| WASM gzip | 2,926,085 | `1868e37e2b26d103e514abe89a350f83145aac711863dbb752a894793613ba65` |
| WASM Brotli | 2,306,055 | `7e118def8ce2e74ccf65acb03d86ee18a702b3ac095db9818449649686fcf74f` |

## Optimized browser component lifecycle

Headless Chromium loaded only the optimized loopback bundle. The authoritative
run began from the unchanged canonical reference session, used the visible
component-library download/open/remove controls, used the visible Undo/Redo
buttons, and ended with an exact persistence reload.

The selected PID leaf exported as 4,815 canonical bytes whose SHA-256 exactly
equalled the selected component identity
`10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228`.
The test changed only component node 1's presentation x coordinate from 28 to
29 logical pixels. The resulting same-length canonical leaf had distinct
identity
`e442f80eb5d5d158f6de5d341f72706e07f91884cf8a66720bcd4221503e5b2b`.
The application itself replayed and admitted that temporary file; no test-only
application API bypassed the visible file bridge.

| State | ALGS bytes / SHA-256 | ALGH bytes / revision / dependencies / SHA-256 | ALGM bytes / SHA-256 |
| --- | --- | --- | --- |
| initial | 14,770 / `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | 7,124 / 1 / 2 / `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | 2,550 / `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` |
| imported | 19,589 / `fc3bc7a2f215759599f776cad454d4f17af627c6ada303c110f08dcf4e79b428` | 11,943 / 2 / 3 / `e6ce0051de48fa06404f3ee0b5dca05cfceda40e7d5eb6ebc4b4d576893ae63d` | 2,550 / `0c99acaf27276e044bbfee18df169e1e5302a54ec3806bd92e3857479bf93808` |
| removed | 14,770 / `7716d0f84f083f7da97c3346c18353471808742fe653162f51c76d6325a0911a` | 7,124 / 3 / 2 / `a985648a68434bdbe435e72bb435fb2cd9d21dbbd879c6d257b7f7b495b93035` | 2,550 / `e1c2b668ea639f8dc8f9582391a630ff404a29d4e48b5ede30d23f6db118f1b2` |

The imported hierarchy retained both original dependency encodings and added
exactly the new leaf. Its root `ALGW` remained 889 bytes with SHA-256
`3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6`;
the reachable two occurrences, 21 flattened nodes, and 25 flattened wires did
not change because the new definition was unreferenced. The source map identity
changed as required because `ALGM` binds the complete source `ALGH` identity.

All three states retained byte-identical non-library authority roles:

| Role | Bytes | SHA-256 / identity |
| --- | ---: | --- |
| control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| bound `ALGP` | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |
| selected control `ALGC` | — | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |

Importing the same temporary leaf a second time retained the complete 19,589
imported-session bytes exactly and did not extend history. Removal retained the
two original dependencies byte for byte while advancing the hierarchy revision
instead of pretending to restore the earlier identity. Visible Undo twice
reproduced the imported and initial sessions exactly; visible Redo twice
reproduced the imported and removed sessions exactly. Reload retained the
removed session byte for byte and visibly reopened with `0 undo / 0 redo` and
zero retained history bytes.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| initial two-dependency library | 363,270 | `443bd6ecaa0febcf0754680659cdba9cdae25d9ec0947ff1bf58d50d871d2243` |
| imported three-dependency library | 363,786 | `3bb6f0dfde22b2185bdc1bbab673a140b7b44e89501d2bcc9fa2fba050cd6771` |
| duplicate-import exact no-op | 366,275 | `fca3012546f5517e540143bb3a5d8f30362d1b42490d584d8a17610e4792422b` |
| removed leaf / two retained dependencies | 364,584 | `0393d0e8fd3c102f8ab726f50ffcd6b8afc0e4ec2d04d33ff2df386a877515be` |
| exact reload with empty history | 363,311 | `adf5e32335e994166e2beaa2fb6bbe3c71aff4fa637ee6740b100f4941f38cdb` |

The screenshots and temporary ALGC files remain transient `/tmp` evidence and
are not repository inputs. Chromium and the loopback server were stopped after
replay.

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

Standalone leaf exchange and exact unreferenced removal are now closed.
Nested-component exchange with explicit scoped bindings, component
rename/version evolution,
connector/front-panel authoring, main-canvas instance movement/wiring, locked
or signed dependency manifests, editable nested-definition canvases,
collaboration/conflict handling, and crash-durable history journals remain
separate work. `ALGC`/`ALGH`/`ALGM`/`ALGS` import grants no semantic shortcut,
firmware opcode, resource ownership, cache preparation, execution, arming,
start, timing, motion, or safety authority.
