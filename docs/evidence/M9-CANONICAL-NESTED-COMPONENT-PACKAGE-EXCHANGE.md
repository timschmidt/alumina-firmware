# M9 canonical nested component-package exchange

Date: 2026-08-22

Canonical repositories:

- interface: `/home/tim/Documents/GitHub/workspace/alumina-interface`
- firmware and coordinated evidence:
  `/home/tim/Documents/GitHub/workspace/alumina-firmware`

Interface source commit: `7f823a1825eb1cc033e5b1bf85faaeac8a580c58`

## Claim closed by this checkpoint

The browser can exchange one reusable nested graph component without losing or
guessing the child bindings that are intentionally absent from standalone
`ALGC` bytes.

`alumina-interface-core` now owns canonical `ALCP` V1:

- one exact root-component digest;
- the root's complete transitive `ALGC` dependency closure, sorted by digest;
- only component-scoped `(parent digest, local node ID, child digest)`
  bindings, sorted canonically;
- embedded/caller byte, count, depth, expansion, flattened-node, and
  flattened-wire limits;
- independent bounded replay of every nested `ALGC`/`ALGW`/`ALGR` envelope;
- total placeholder/binding, one-context, derived-shape, DAG, and exact-closure
  validation; and
- byte-for-byte re-encoding plus SHA-256 identity before replay succeeds.

`ALCP` is immutable authoring data with no package revision. It grants no graph
semantics, implementation, resource, timing, memory, safety, firmware,
deployment, or physical-output authority. Firmware does not decode it.

## Additive transaction

`GraphHierarchyDocument::export_component_package` begins at one selected
dependency and emits exactly the reachable closure. Root occurrences,
ancestors, siblings, and unrelated library definitions are omitted.

`GraphHierarchyDocument::import_component_package` never replaces an existing
component or scoped binding:

- an existing digest must retain byte-identical canonical `ALGC` bytes;
- a stable name owned by a different digest rejects;
- an existing parent/node binding to the same child is a duplicate;
- an existing parent/node binding to a different child rejects; and
- every new component and binding enters one complete candidate `ALGH`, whose
  revision advances exactly once only after full destination validation.

An exact duplicate returns before revision and canonical-state mutation. All
component, binding, name, context, cycle, shape, count, and byte failures leave
the destination hierarchy exact.

The component-library UI exposes independent `.algc` leaf and `.alcp` closure
controls. A changing `.alcp` import regenerates flattened `ALGW` and `ALGM`,
audits every ordinary library draft, constructs complete `ALGS`, records one
unified undo state, and only then commits. An exact duplicate may follow the
transient library selection but creates no history or persistence write.

## Native regression evidence

The new core regressions prove:

- a nested two-component closure excludes an unrelated dependency;
- canonical encode/replay preserves the exact document and identity;
- import adds two definitions and one scoped binding in one hierarchy revision;
- importing the same package again changes no byte or revision;
- the imported root can subsequently be instantiated and recursively
  flattened to the package's proved node/wire counts;
- same-parent/node child disagreement and stable-name collision reject
  atomically;
- invalid magic, unsupported version, unknown root, trailing bytes, tighter
  caller policy, unreachable dependency, root binding, and duplicate component
  reject at their typed boundaries; and
- destination state remains byte-identical after every merge conflict.

The UI regression constructs a private empty leaf/root closure, imports it,
proves the complete-session authority roles unchanged, verifies duplicate
no-op behavior, injects a same-parent/different-child conflict, and replays the
accepted state through Undo, Redo, persistence, and fresh restore.

The complete native workspace run passed:

- `alumina-interface`: 103 tests;
- `alumina-interface-client`: 82 tests;
- `alumina-interface-core`: 189 tests;
- cross-crate integration: 1 test; and
- doc tests: passed.

Total: 375 tests.

Strict native and WASM Clippy, WASM check, strict rustdoc, source/license audit,
and package-scoped formatting also passed. The live Hypercurve dependency
emitted two unrelated warnings while being edited concurrently; this
checkpoint neither inspected nor changed that repository.

## Optimized browser proof

Only localhost HTTP and the optimized production WASM bundle were used. The
connected bare TinyBee V1.0, its GPIO, USB serial/reset, Wi-Fi radio, and the
workstation WLAN were not contacted.

The qualified production artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `a62aaa9d9a46385ab099940a6fb3b51abfb4f06357814757a9719d86530f052d` |
| `alumina-interface.js` | 91,816 | `bb4e7b08c223687bfa0baef1f99c6b5e7742a000c345adfc3751c471bd5bebc0` |
| `alumina-interface_bg.wasm` | 6,830,461 | `ecb6a15a89a8bdca55207e394bdd4bf7c2fa7bd363ba46e75839d66fedb72028` |
| `index.html.gz` | 718 | `53f14994a256a2e60df5667413809412af830b4e024fd68300535ea4253b3e53` |
| `alumina-interface.js.gz` | 12,983 | `4fc716f8b5801cdad16b17c2a79c7b2941be836bb9cb9842ab64740abc515d83` |
| `alumina-interface_bg.wasm.gz` | 3,002,987 | `eaa55f303a3890c051acbf453fbef05e334c3fa9bb430e8472cc61f46e5be201` |
| `alumina-interface_bg.wasm.br` | 2,358,757 | `d3ec4bcf24d227da0cce90b3b573af7ae1b28168ef3650df152a4e3d7a9f3f89` |

`wasm-tools validate`, gzip integrity, and Brotli integrity all passed before
the browser run.

The browser visibly created:

- `user.browser_package_leaf`: 739-byte revision-1 `ALGC`
  `21095733ea4c610fa059dcf3b81e605ed5d2e938970c8345e05b58d1af1f39cc`;
- `user.browser_package_root`: 842-byte revision-2 `ALGC`
  `f0499ffabb76e8b7c9735419fb05818093b1ba75fa43c8b8fa7bdae028efd2b6`;
  and
- root-local node 1 bound exactly to the leaf.

The selected closure downloaded as 1,761 exact bytes with SHA-256
`71bc56d7e52c1dcd54eb1f2258ad70ed44be1fd32f95a21310eeafdd56c2e4b0`.
Independent parsing found exactly two components and one binding, with the
downloaded package root and binding matching those identities.

The harness then cleared browser authoring storage and restored the canonical
reference:

- 14,770-byte `ALGS`
  `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d`;
- 7,124-byte revision-1 `ALGH`
  `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a`;
- two dependencies and two bindings.

Opening the downloaded `.alcp` added its two definitions and one binding in
exactly one transaction:

- 16,428-byte `ALGS`
  `0fd9bad9b1000d2701afc2fa54c26bc98af2a63328a4abcd639a8bc036631701`;
- 8,782-byte revision-2 `ALGH`
  `f6a006ac527225db6aa717f27ca512c6db6adff6fd45bb198c56a1c62d7f0cc1`;
- four dependencies and three bindings; and
- 2,550-byte regenerated `ALGM`
  `6b6d5d949d9de0793dd08e75a275a6a21cbb00aeef1d0e39feb9c54103d3c193`.

The following remained byte-identical to the reference:

- 3,755-byte control `ALGW`
  `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16`;
- 407-byte `ALGP`
  `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223`;
- 825-byte cached-job `ALGW`
  `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58`;
- control-authority `ALGC`
  `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228`;
- 889-byte structural root `ALGW`
  `3d7775b323bfa9442ba5eee800cf0020eac3e3f046f67693dc7154359fbe0fd6`;
- flattened `ALGW`
  `6804b964535d08b9ceead3d43891c3ae4c5aa5ce38b015c34b4b385bfa3257d4`;
  and
- 2,478-byte source-map provenance body
  `552549c14a78850b01d9c6c3e68dafc4d1ba62d3dfee5746c7e54b5309ae8e85`.

A second visible open of the same `.alcp` retained every imported `ALGS` byte.
Visible Undo restored the exact reference, visible Redo restored the imported
session, fresh reload restored those same bytes, and the imported root remained
available for visible library selection.

Retained parsed proof:

- `/tmp/alumina-alcp-final.dqzkDG/result.json`
- 14,333 bytes
- SHA-256
  `2294ba6c706945093bf4c335bbf9ec09d7adebb4084b7de08af15caeef78b4f6`

Representative retained screenshots:

| State | Bytes | SHA-256 |
| --- | ---: | --- |
| create leaf | 444,256 | `59d10712b96647234f562aafc72da14971a6fe5a846fd9cbcc4fc99432c8609b` |
| create root | 438,230 | `9695c20ffffac77715bea1ca3f7e0a1ed8bb41f3201619e8dd5b20cb611b97d0` |
| authored nested closure | 386,885 | `f06546b96419e823c3c5235a8aee80b15cb73c9613a66407dd550e1781c2ce1b` |
| downloaded closure | 386,785 | `6f1d77c909fd9f30c87001052ac347f8cccb94da6bccc94549cb225252d2a694` |
| accepted import | 449,911 | `8938adcb08674660dbff469f48a155f1e1d39a35ab2a33eef961131f27cb51a9` |
| duplicate no-op | 450,911 | `6121eee1a886ab780f1c673a57ceefdfaf2976532eb2c2610a65714a98bbfeea` |
| history controls | 441,300 | `2e44fb9b2e31fe61eb36c54a49b81084e0d467d697f857f245a533aa0061b84d` |
| after Undo | 439,163 | `984aec22d402b2205f4706fc446ba22fc86dfcca85c9ee58e03f34fa73d27ac6` |
| reloaded imported root | 451,930 | `b1490ec0bf3d82bdbc395c1a80c479cb49705c58f1cba178a495f46527abe2f6` |

The disposable 142 MiB Chromium profile was deleted after the browser and
localhost server stopped. The parsed result, screenshots, and exact downloaded
`.alcp` remain under the retained proof directory.

## Licensing and next boundary

The implementation is original project code under the interface repository's
MIT license. The native/WASM dependency inventory remains permissive and the
source-policy audit found no GPL-family dependency.

This closes nested definition/binding exchange, not instance parameter
promotion/overrides, signed or permissioned packages, locked dependency
manifests, coordinated descendant/control-authority replacement,
collaboration/conflict journals, or executable front-panel inputs. Those remain
separate future authority and format boundaries.
