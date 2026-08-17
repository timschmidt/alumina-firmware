# M9 audited graph state and cycle semantics evidence

Date: 2026-08-11

## Scope and source

This checkpoint adds host-side semantic admission above the canonical `ALGR`
V1 structural document. It does not make graph documents executable and does
not add a firmware graph parser, opcode, deployment route, or real-time
implementation.

The coordinated source points are:

- `alumina-interface` commit
  `9f8462ada9a9fd8e8df4d849115cdb9a1bc52c05`;
- the preceding canonical document checkpoint at interface commit `6c07c2f`;
- `alumina-firmware` protocol/storage source at `089cd65`; and
- the current sibling CSGRS/Hyper stack selected by the interface's local-path
  source audit. No published legacy CSGRS release or GPL-family source is used.

## Separate semantic authority

The saved document continues to round-trip every opaque node name and version.
`GraphNodeRegistry` is a distinct audited authority. One `NodeSchema` resolves
an exact name/version and must declare:

- the complete input and output port identities, names, and registered types;
- every required parameter identity, name, and registered type;
- exactly which of HostExact, Service, and Realtime placement families are
  allowed; and
- one complete current-tick input dependency list for every output.

Registry construction canonicalizes schema/kind/port/parameter/dependency
order and rejects duplicate kinds, invalid identities or names, incomplete
output coverage, missing/duplicate/non-input feedthrough references, absent
types, empty domain sets, and contradictory state paths.

Document-local IDs cannot be reinterpreted. The registry retains the exact
canonical unit/type schema and complete clock set from the document that
created its authority. Analysis compares both before resolving any node. A
different type under the same integer ID, a changed clock definition, or a
missing clock fails as `SemanticContextMismatch`.

## Explicit state boundary

`NodeStateContract` names:

- one registered state type;
- one exact required run-start parameter;
- one next-state input;
- one prior/current-state output;
- one explicit update clock; and
- one bounded declared storage ceiling.

The parameter/input/output types must be identical to the state type. The
prior-state output must have an empty current-tick dependency list. Its
semantics are read-before-write: the output exposes state captured before the
named clock update, while the next input is consumed for the subsequent value.

Declared state bytes are bounded per node and in aggregate and retained in the
analysis report. This is deliberately a declaration, not yet proof that every
value of the registered type fits a fixed representation.

## Bounded combinational analysis

After exact kind, shape, domain, and context admission, analysis builds a
port-level directed graph:

- every document wire is one output-to-input edge; and
- every audited current-tick input dependency is one input-to-output edge.

The graph and exact witness lengths have independent limits. Cycle detection is
iterative, so an admitted document cannot force recursive call-stack growth.
A deterministic cycle witness retains the precise `GraphWireId` and every
node/input/output feedthrough link in traversal order.

The representative pure self-feedback case returns exactly two links: its
structural feedback wire and the audited input-to-output dependency. The
representative source/add/delay/sink graph contains a feedback topology but is
accepted because the delay's prior-state output has no current-tick path from
its next-state input. Its report retains four admitted nodes, six dependency
links, and one 64-byte state declaration on the named clock.

Additional cases reject an unresolved node at its exact instance, a missing
input contract, a Service placement for a HostExact-only node, a changed
type/clock context, registry construction against a missing state clock,
duplicate kind authority, state-output feedthrough, dependency-link overflow,
cycle-witness overflow, and total declared-state overflow.

## Reproduced checks

Run from `alumina-interface`:

```console
cargo fmt --all --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- \
  -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 73 native tests pass: 8 application/coordinator, 24 client, and 41 core,
plus the intentional compile-fail rustdoc test. The core total includes 21
graph tests, 5 of them focused on semantic admission/state/cycles. Native and
WASM warnings-denied project Clippy, WASM compilation, warnings-denied project
rustdoc, local-stack/permissive-license audit, optimized Trunk build, WASM
validation, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,257,539 | `cd16b3dfafc0e1bd75f306860a8d8fddeb5590be0644a55151e9be6ac31aac07` |
| `alumina-interface_bg.wasm.gz` | 1,978,866 | `fda2449f6dd8b0a4f2ce191ced4557f27a10dc06077698feb13872431dd09909` |
| `alumina-interface_bg.wasm.br` | 1,613,712 | `cbc981fd9979c587c4c3def35c8093587784034b7327875176ad763f82bc83c3` |

These hashes are reproducibility observations, not execution or browser
workflow evidence.

## Closed claims and licensing

An admitted schema is not a node implementation registry. There is still no
node evaluator, memoization, bounded queue/backpressure or rate-transition
semantics, static type-to-memory proof, WCET/deadline analysis, capability or
resource allocation, bridge lowering, service/real-time IR, firmware opcode,
deployment, editor, front panel, or trace simulator. In particular,
`ExecutionDomain::Realtime` plus `ExecutionDomainSet::REALTIME` cannot execute
anything; later fixed opcode and safety admission remain mandatory.

No target firmware source or binary changed in this slice. The connected bare
MKS TinyBee V1.0 was not read, reset, flashed, or otherwise touched; no motor
or motor power was connected. The SLogic16U3 remained disconnected, and no MKS
ESP32 FOC hardware was available. No physical I/O, timing, motion, FOC,
synchronization, or safety claim is made.

The implementation is independently authored under the interface's MIT
license and adds no dependency. The native/WASM inventory and current-sibling
inverse-path audit pass with no GPL-family or missing-license dependency. No
GPL-family source or asset was copied or consulted.
