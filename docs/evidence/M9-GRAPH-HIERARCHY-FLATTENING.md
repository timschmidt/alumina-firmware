# M9 canonical component hierarchy and flattening evidence

Date: 2026-08-12

## Scope and source

This checkpoint makes the preceding canonical component packages reusable as
collapsed authoring instances and deterministically lowers them back to an
ordinary graph workspace. The implementation is `alumina-interface` commit
`51fc8e1ef107a6b65de812803e3c20f3e80dab28`, against preceding `alumina-firmware`
evidence commit `59ec9c3`. It changes no firmware source or wire format.

The build continued to use the checked-out sibling stack: CSGRS
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0`, Hypercurve
`bc58f471d8f123a2002189df1156b0b092027bf7`, Hypergraphics
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Concurrent tracked Hypercurve
edits and untracked Hyperlimit fuzz/local state were preserved unchanged.

## Canonical hierarchy package

`ALGH` V1 embeds one canonical root `ALGW`, a digest-sorted library of complete
canonical `ALGC` dependencies, and root-node-to-component-digest instance
bindings sorted by node identity. The first caller/embedded policy admits at
most 32 MiB, 64 dependencies, 256 instances, 4,096 flattened nodes, and 8,192
flattened wires. Every nested `ALGW`, `ALGC`, and `ALGR` retains its own
independent admission policy.

Replay bounds the outer bytes and counts before allocation, independently
replays every nested canonical envelope, rebuilds library/instance invariants,
rejects trailing data, and requires byte-for-byte re-encoding before assigning
SHA-256 identity. Duplicate dependencies, missing component digests, duplicate
bindings, unbound reserved nodes, and unknown reserved instance versions fail
closed.

Every dependency, used or unused, must share the exact root type registry and
clock set. V1 is deliberately leaf-only: any dependency node using the reserved
`alumina.component.instance` name rejects, so recursion and dependency cycles
cannot be hidden behind an unchecked depth. General bounded nested hierarchy is
not claimed.

## Derived instances and deterministic flattening

The collapsed node is authoring-only. Its inputs and outputs are derived from
the component's canonical public terminals in order, with consecutive
node-local port IDs and graph-resolved exact value types. Its reserved kind is
not a host implementation, firmware opcode, resource, or deployment authority.
Any kind/version/domain/port/parameter mismatch rejects before flattening.

Flattening operates transactionally on a root clone in ascending instance-node
order. It deletes each placeholder without rewinding identity cursors, copies
component nodes/wires in canonical order with fresh monotonic IDs, translates
integer placement relative to the component's minimum x/y, and reconnects
incident root wires from public instance ports to exact internal endpoints.
Public inputs are known-unowned internal targets by `ALGC` construction;
outputs may fan out.

Final node/wire counts are checked before and after expansion. Identifier,
coordinate, type, target-ownership, byte-limit, or workspace edit failure
returns no candidate. The result is a normal canonical `ALGW` plus an audit map
from each component-local node ID to its flattened root ID and a binding to the
source `ALGH` digest.

Tests cover exact incoming/outgoing connector rewiring, a wire whose source and
target are both collapsed instances, canonical dependency order, missing and
duplicate bindings, misshaped nodes, unsupported instance versions,
type/clock-context mismatch, nested-instance rejection, flattened-count limits,
outer corruption/trailing data, canonical replay, and independent audited
analysis of the ordinary flattened graph.

## Visible proof and golden identities

The UI constructs one collapsed instance of `control.reference_pid`. Its
5,008-byte `ALGH` has SHA-256
`d9a0d5cbe0b7694b506711f48d85ac2a904874261fd0d2d86244da06cf2e5f64`.
It expands to 19 ordinary nodes and 22 wires. The resulting 3,396-byte `ALGW`
has SHA-256
`a5e0abfd4f1e8642a78244b7c14f91150665faadef4898c49ddc256c88a98277`
and passes the fixed audited HostExact registry. The UI displays both identities
and counts beside the component front panel; it never claims the collapsed node
itself can execute.

## Reproduced checks

Run from `alumina-interface` at the implementation checkpoint:

```console
cargo fmt -p alumina-interface -p alumina-interface-client \
  -p alumina-interface-core -- --check
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

All 133 executable tests pass: 19 application/coordinator tests, 32 client
tests, 81 core tests, and one cross-crate exact-control integration test, plus
the intentional compile-fail rustdoc test. Strict native and WASM Clippy, WASM
check, warnings-denied documentation, local-source/permissive-license audit,
optimized Trunk build, both WASM validators, and compressed-artifact checks
pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface.js` | 91,813 | `0183d5890db703e34db9ad9b410c792cd83b6867d3b3a1887365a4d9a639e1fb` |
| `alumina-interface_bg.wasm` | 4,845,282 | `11f6fe69081275c767e12eacb3b2e474511aa2dcf3be9b1858664520b19f0099` |
| `alumina-interface_bg.wasm.gz` | 2,205,722 | `62ac2589676ada43f0ea7474526c3e02aaa47fd9563233a5d02b557be352ba3b` |
| `alumina-interface_bg.wasm.br` | 1,779,952 | `a88ddda6e680c3fffe7a5d0c98f8e532a3f412c9860aa683eaa6f7f6221af133` |

The 96,806-byte lockfile remains SHA-256
`5790fedc0da96498fad696e5627e6f1601ceb04dead8c5adb67d1efb3773cfc1`.
No package entered the resolved inventory.

The optimized bundle was served temporarily on loopback and rendered in
headless Chromium software WebGL at 1,440 by 1,600 pixels. Visual inspection
confirmed the source `ALGH` identity, one-instance-to-19-node/22-wire report,
flattened `ALGW` identity, exact component panel, unchanged graph/trace, and
explicit no-deployment/output warning. The 265,882-byte transient screenshot
has SHA-256
`9a18a5bf732ba5b2a0dc0b15b2cce8ecbf3f7caa093c3712e73989e3088ec24f`.
The loopback server was stopped after capture.

## Closed claims, hardware, and licensing

`ALGH` and its reserved instance nodes are authoring/compiler input only. They
grant no semantic implementation, runtime recursion, resource allocation,
timing, safety, firmware, or physical-output authority. Only the ordinary
flattened graph can proceed to the existing independent semantic and deployment
registries; firmware still receives only fixed authenticated graph IR.

The bare MKS TinyBee V1.0 remained on its prior disconnected-load HIL image. It
was not reset, flashed, contacted, or associated with workstation Wi-Fi. No
motor, driver, process power, or analyzer input was connected. Browser evidence
used loopback only; NetworkManager and Wi-Fi were unchanged.

The implementation is independently authored under MIT. No dependency was
added. The existing source-policy audit accepted the permissive
Alumina/CSGRS/Hyper/native/WASM inventory; no GPL-family source, library, asset,
or tool output was copied, linked, or vendored.

The next promoted offline graph slice is capability-derived resource/protocol
nodes plus bounded probe/telemetry bindings. Nested hierarchy and editable
library workflows remain later explicit gates rather than implicit recursion.
