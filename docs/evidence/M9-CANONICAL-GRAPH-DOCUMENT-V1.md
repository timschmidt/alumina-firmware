# M9 canonical exact graph document V1 evidence

Date: 2026-08-11

## Scope and coordinated source

This checkpoint establishes the first saved structural document for the
greenfield typed graphical-control system. It is browser/native core code, not
firmware graph execution. No old Alumina graph file, endpoint, renderer value,
or compatibility schema is accepted.

The coordinated implementation points are:

- `alumina-interface` commit
  `6c07c2f729d9511faa8e973102beed637543a107`;
- `alumina-firmware` protocol/storage source at
  `95df11968ecd28f7e1915258c42bc060556c2439`; and
- the current sibling CSGRS/Hyper repositories selected by local path and
  checked by the interface source-policy audit. In particular, exact graph
  rationals use `hyperreal` commit
  `f09c147b0352884f8efe88e875c37d8f0f439ba5`; no published legacy CSGRS
  release is selected.

## Exact registered values

`alumina-interface-core::graph` owns stable IDs and bounded canonical
registries for physical units and value types. A unit retains seven SI
base-dimension exponents and an exact positive rational SI scale. The V1 value
families are:

- Boolean and exact rational values;
- exact closed rational measurement intervals;
- signed/unsigned canonical integer lattices with exact positive quanta;
- bounded UTF-8, bytes, homogeneous arrays, and required-field records;
- options and typed success/error results;
- runtime-only events and bounded streams naming an explicit graph clock;
- resource handles bound to device, board-package digest, class, and selector;
  and
- job handles bound to device, global-job digest, and local-partition digest.

There is no float literal or implicit unit conversion. Schema construction
sorts by stable identity and rejects zero/duplicate identities, malformed or
duplicate names, unknown references, recursive/excessively deep types, invalid
unit scales or lattice quanta, and all collection/capacity bounds. Literal
construction independently enforces type shape, record order, interval order,
text/blob/array sizes, total nodes, depth, exact-rational decimal magnitude,
and nonzero handle authorities. Events and streams cannot be saved as literal
values.

## Structural document and placement

`GraphDocument` retains revision, clocks, opaque versioned nodes, display
labels, requested execution domains, typed input/output ports, exact typed
parameters, and output-to-input wires. Node behavior names are intentionally
opaque: an unknown name/version round-trips unchanged and remains unresolved
until a later audited compiler registry admits it.

The three placement values are `HostExact`, `Service(device)`, and
`Realtime(device)`. A placement value is not executable authority. It carries
no opcode, arbitrary code, WCET proof, resource claim, or permission to run on
core 1.

Clock definitions are host monotonic, physical-device cycle, or an exact
reduced rational derivation. Missing, zero-rate, nonreduced, or recursive
clocks reject. Event/stream clock references resolve only against the complete
document. Ports are canonical local identities. A wire must start at a declared
output, end at an identical registered input type, and uniquely own that input.

## Canonical wire and identity

`ALGR` V1 is independent of serde, JSON numbers, and platform ABI. It uses
fixed-width little-endian integers, bounded length-prefixed UTF-8/bytes, and
signed reduced decimal numerator/denominator magnitudes. All allocation,
nesting, rational-digit, graph, and total-document limits are embedded in and
therefore covered by the graph identity.

Untrusted replay first applies an independent caller admission policy. An
embedded limit may be smaller but cannot grant itself a larger budget. Every
count and byte sequence is bounded before allocation, then the normal schema
and document constructors independently validate the reconstruction. Trailing
bytes reject. Finally, the decoder re-encodes the complete document and accepts
only byte-for-byte equality before returning its SHA-256 digest.

The representative all-shapes golden digest is:

```text
d5b886c8d655fed11d0fa54fd7a37f97cb16a2bc979ee126aa09fbf98598ceb9
```

That fixture covers all 15 type variants, both option and result branches, all
three clock kinds, all three execution domains, resource/job identities, an
unknown node at version 37, and one exact typed wire. A revision-only change
changes the digest.

Hostile tests reject every strict byte prefix of the golden, trailing bytes,
bad magic/version/flags, a self-enlarged embedded policy, a `u32::MAX` count
bomb before allocation, an overlong admitted document, invalid UTF-8, an
unreduced rational alternative, and reordered otherwise-valid unit records.
Separate tests reject recursive types, missing references/clocks/nodes/ports,
wrong wire types, duplicate input ownership, invalid domains, invalid handles,
runtime literals, value-tree overflow, and rational magnitudes beyond policy.

## Reproduced checks

Run from `alumina-interface`:

```console
cargo fmt --all --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
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

All 68 native tests pass: 8 application/coordinator, 24 client, and 36 core,
plus the intentional compile-fail rustdoc test. The 36 core tests include 16
graph tests. Native and WASM warnings-denied Clippy, WASM compilation,
warnings-denied rustdoc, current-sibling/permissive-license source audit,
optimized Trunk production build, WASM validation, and compressed-artifact
integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,257,540 | `4a891f9b01737e61b8a4269477541a814c50afd42586ca36f1afd91bc06bea23` |
| `alumina-interface_bg.wasm.gz` | 1,978,656 | `6466ed2d2efaf695f805918b083ab189760d6ff79016565a5208ad29012d11af` |
| `alumina-interface_bg.wasm.br` | 1,613,900 | `ff53ec8fff747ecf28512898ab5d3433b53a472241721eb6f64e8700c3214089` |

These hashes are reproducibility observations for this source/dependency state,
not a browser workflow or firmware qualification.

## Closed claims and licensing

This slice has no node behavior registry, subgraph/component semantics,
explicit state/feedback, queues/backpressure, rate transitions, cases/loops,
resource allocation, capability-generated palette, combinational-cycle
analysis, fixed-memory/WCET/deadline analysis, protocol bridges, graph
simulation, front panel, editor, plot UI, or firmware graph IR. It deploys
nothing and gives no arbitrary code a service or realtime path. Fixed firmware
safety remains authoritative.

The connected bare MKS TinyBee V1.0 was not read, reset, flashed, or otherwise
touched. No motor or motor power was connected. The SLogic16U3 remained
disconnected, and no MKS ESP32 FOC hardware was available. This checkpoint
makes no physical I/O, timing, synchronization, motion, FOC, or safety claim.

The implementation is independently authored under the interface's MIT
license and adds no external dependency. `alumina-protocol`,
`alumina-storage`, and `hyperreal` remain permissively licensed sibling source.
The native/WASM inventory and inverse-path audit pass with no GPL-family or
missing-license dependency admitted. No GPL-family source or asset was copied
or consulted.
