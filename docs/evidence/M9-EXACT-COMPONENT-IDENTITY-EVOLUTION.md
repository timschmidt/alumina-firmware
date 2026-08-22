# M9 exact component-identity evolution evidence

Date: 2026-08-22

Authoritative interface source:
`50ba36ead9842ac06410b0e9b484148589709fb9`
(`feat: evolve exact component identities`)

This checkpoint closes selected-library stable-name and declared behavior-
version evolution. It extends the existing canonical `ALGC` replacement path;
it does not introduce an alias table, compatibility identifier, second
hierarchy format, or firmware command.

This is host authoring functionality. It grants no firmware, network, storage,
GPIO, motion, arming, timing, start, or safety authority.

## Canonical metadata mutation

`GraphComponentDocument::update_identity_metadata` transactionally replaces
the stable component name and declared behavior version on a cloned `ALGC`.
The behavior version is distinct from the `ALGC` wire-format version:

- canonical replay continues to accept every nonzero `u32`, so a historical
  artifact remains meaningful without migration;
- authoring may retain or increase the current behavior version but may not
  decrease it;
- a stable-name-only change may retain the behavior version;
- every accepted change advances component revision once; and
- an identical name/version pair is byte-for-byte a no-op.

Zero and regressing versions return typed errors before mutation. Existing
stable-name validation still admits only 1–64 ASCII bytes beginning with an
alphabetic byte and containing only alphanumerics, `_`, `-`, or `.`.

## One recursive authoring transaction

The visible component-library panel now exposes one selected-component
identity row. Its behavior-version field accepts only the canonical decimal
spelling of a nonzero `u32`: no sign, whitespace, leading zero, alternate
radix, fraction, or overflow is accepted. A name already owned by another
dependency rejects before mutation.

An accepted draft enters the existing complete recursive replacement boundary:

1. clone and update the selected `ALGC`;
2. replace its old digest and every affected parent digest;
3. rewrite exact parent-local and root bindings from the complete replacement
   report;
4. freshly validate and flatten `ALGH`;
5. regenerate total `ALGM` provenance;
6. rerun audited ordinary-node semantic and complete-session admission; and
7. commit one new canonical `ALGS` history/persistence state.

No old stable-name or digest alias survives. Logical library, definition, and
connector selections follow the exact replacement report. Identity text drafts
are keyed by current component digest and disappear when that digest is no
longer present. A retained flattened-source origin is not guessed across the
identity boundary: if the exact final/origin pair disappears, focus and pending
scroll clear with a visible stale-origin status.

The authoritative lifecycle regression deliberately changes the component
whose embedded workspace supplies the complete session. It proves that control
`ALGW`, probes, cached-job `ALGW`, root `ALGW`, unchanged wrapper `ALGC`, and
freshly flattened ordinary `ALGW` remain exact while the selected `ALGC`, one
nested binding, complete `ALGH`, and source-bound `ALGM` change together.

## Native and adversarial coverage

The complete qualification passed:

- 100 application/coordinator tests;
- 82 client tests;
- 184 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 368 unit, integration, and compile-fail checks. New regressions prove:

- exact name/version no-op behavior;
- stable-name-only evolution without a behavior-version increase;
- behavior-version increase by more than one;
- atomic zero, regression, and malformed-name rejection;
- exact canonical decimal parsing through `u32::MAX`, with zero, signs,
  whitespace, leading zeroes, fractions, and overflow rejected;
- authoritative `ALGC` replacement with component revision advancing once;
- exact recursive nested-binding, selected-session, definition-scope, and
  connector-scope digest remapping;
- byte-exact control workspace, probes, cached-job workspace, root workspace,
  unrelated wrapper, and flattened-workspace retention;
- old dependency and transient draft removal without an alias;
- stale exact-source focus and pending-scroll clearing;
- exact no-op history and persistence isolation;
- atomic version regression, noncanonical `02`, conflicting stable name, and
  malformed stable name; and
- complete-session Undo, Redo, and persisted restore of the evolved identity.

## Native, WASM, source, and release qualification

The following checks passed against the live, read-only workspace CSGRS/Hyper
paths:

```text
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip and Brotli integrity checks
git diff --check
```

The strict `--no-deps` native and WASM interface lints were warning-free.
Hypercurve was being edited concurrently and emitted only its current
read-only unused-internal-helper warnings during different gates. No
Hyper/CSGRS repository state was inspected or changed. `Cargo.lock` remained
SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

The source-policy audit accepted only sibling Alumina/CSGRS/Hyper paths and the
permissive native/WASM license inventories.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `defff2a495852fbc6b8f20260234e84bdefa572e6c2419ec1ecefa11e96ee8d1` |
| `alumina-interface.js` | 91,816 | `a9f914c838319617c70dc13fe60f0f5563115105825c559dd4160bcadb736517` |
| `alumina-interface_bg.wasm` | 6,795,340 | `f3f3c88c0c6d61c1bfa6e24dfb30fee2c9ca13dbebe16d10af00bdabbf223000` |
| WASM gzip | 2,989,549 | `98ed489a1f61abb1cf7ab70b3bcb53912e95c06a84be30d318f9e81288e384da` |
| WASM Brotli | 2,348,944 | `71991b0e041d558028f60123534c9c2b7acc1f25dd690f94a86c025655423af0` |

`wasm-tools` accepted the optimized module. Gzip and Brotli integrity checks
accepted every produced compressed artifact. The artifact table is a tested
moving-workspace snapshot, not a coherent CSGRS/Hyper release pin.

## Optimized browser lifecycle

The application and proof script addressed only the optimized bundle through
loopback HTTP and an isolated Chromium debugging port. The proof cleared only
disposable origin-local persistence, opened the visible Control graph, and
operated the selected-component identity row.

It renamed `control.reference_pid` version 1 to
`control.browser_pid_v2` version 2:

| Canonical artifact | Before | After |
| --- | --- | --- |
| `ALGC` | 4,815 bytes, revision 1, `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` | 4,816 bytes, revision 2, `63d2873a02f832bac32a524ec57c68ce9218c22e0b327243b8d8074b2e0cdc80` |
| `ALGH` | 7,124 bytes, revision 1, `f9751073015828f20154a5536d8b217a7d3843e2d63d3b5689fcfb8c2379806a` | 7,125 bytes, revision 2, `17baef8eae8b5ba3287b4b8968502ea627b34f8fc2aa9355f0ec954e94da447e` |
| `ALGM` | 2,550 bytes, `dbfaf69255a1a4159329761523fbe120d8b956548adb4bbf1958e73ab014bb77` | 2,550 bytes, `71126d248b87c27b6fca5c3cdae27f129d1b39dbb525bdaae2d63f91040a5a7b` |
| complete `ALGS` | 14,770 bytes, `d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d` | 14,771 bytes, `78afc205232830829c5e4d5db54dacfc09e5e4e5e3fb4e7dce3b636bef7a21c4` |

The unchanged wrapper digest remained
`941a11bb7a0a34f1ff64f510d474d7df9a3e6bfa5a0323090ffa6715a2b396a9`.
Its scoped node-1 binding changed only its component digest from the old
authority to the replacement. `ALGM` retained flattened-workspace identity
`6804b964535d08b9ceead3d43891c3ae4c5aa5ce38b015c34b4b385bfa3257d4`
and its 21-node/25-wire cardinality.

A second visible Apply retained every evolved `ALGS` byte. Entering behavior
version `1` visibly reported:

```text
component identity edit rejected without mutation: graph component behavior version regressed from 2 to 1
```

and retained the evolved session exactly. Visible Undo restored the exact
reference session; Redo and a fresh reload restored the exact evolved session.

| Browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| initial identity row | 340,889 | `9549c3a2717635c455ff2a47ba7a6dd994e7fbed560051842c8a2923dca94dbd` |
| evolved identity row | 336,751 | `e4a1bd50dcc429b07ddda2429bd3c446973f4479902de86c34c9965a9a4c5792` |
| exact no-op | 337,925 | `682aca598da7c2ec47b9a13ef013ee031fae95a192b4d256c2fd874e03351c6d` |
| rejected regression | 324,023 | `663765d7dfade7b3f270214589d73a1250426f9d30cc796ee6dcc82521e8c171` |
| history controls | 358,787 | `2ec3d0efaf2e9d9663a6874e24e5792ef1f986ae3d6f6d414a4aa07dc1142744` |
| after visible Undo | 356,478 | `ef497dba142dae268acf6f4ec1b040ef5cf2c5c92e43ab7e971580e63b34645a` |
| reloaded identity row | 342,663 | `b5c5678fbbe5579199ca59e4ff5e701f7fa85260dfdc460d0501adefc007bd7b` |
| reloaded evolved identity | 339,013 | `52d31f6d8e5949fa0325535ea961d0f71de4f12c7316fd67cc7038e226f6cd65` |

The retained parsed result is 38,577 bytes with SHA-256
`ae6e4c6e4edc4eda6c21e5922ad301204693d0bf6971a430a64f6ea015696092`.
Screenshots, bounded locator frames, and the result remain transient
`/tmp/alumina-component-identity-proof.r63VaK` evidence and are not repository
inputs. Failed/intermediate evidence directories and the disposable Chromium
profile were removed; Chromium and the loopback server were stopped.

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

Canonical selected-component stable-name and monotonic behavior-version
evolution, recursive digest/binding/source-map replacement, authoritative
workspace isolation, exact no-op, invalid/regressing/conflicting metadata
atomicity, stale-source reconciliation, complete-session history/persistence,
and visible optimized-browser lifecycle are now closed. Child
rebinding/replacement, nested binding import/exchange, parameter
promotion/overrides, coordinated descendant/control-authority structural
replacement, runtime panel injection, collaboration/conflict handling, and
crash-durable journals remain separate work.
