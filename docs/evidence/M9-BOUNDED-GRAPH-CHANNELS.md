# M9 bounded graph-channel evidence

Date: 2026-08-11

## Scope and source

This checkpoint gives every audited graph input explicit delivery semantics and
proves a canonical byte ceiling for every connected slot or queue. It remains a
host-side admission and reporting contract. It does not implement a node
evaluator, rate transition, network bridge, firmware queue layout, opcode, or
deployment path.

The implementation is `alumina-interface` commit
`e8938ffab601edb902dd6c9d8e08fababbb64e8f`, above the canonical graph,
audited semantics, and type-storage checkpoints. It resolves CSGRS, the Hyper
stack, and Alumina protocol/storage crates from the current sibling workspace;
no published legacy CSGRS package is selected.

## Explicit input delivery

Every `NodeSchema` input has exactly one canonical
`NodeInputChannelContract`. Registry construction rejects missing, duplicate,
mis-typed, zero-capacity, or over-policy contracts before a graph can be
analyzed. Each contract fixes whether its input is required or optional and
uses one of three delivery forms:

- `Synchronous` is one current typed value slot for a non-runtime value type;
- `EventQueue` is a bounded timestamped queue for an Event payload; and
- `StreamQueue` is a bounded timestamped queue for a Stream sample.

A required input without a structural wire fails at its exact endpoint. An
unconnected optional input is admitted and reserves zero bytes. Stream queue
capacity may not exceed the capacity registered in its Stream type. Every
queued input names `Backpressure`, `Fault`, `DropNewest`, or `DropOldest` as its
full behavior; there is no implicit loss policy.

A synchronous wire may connect only identical concrete execution ownership.
The comparison includes `HostExact`, Service or Realtime family, and the exact
device identity within Service/Realtime. A scalar wire from HostExact to a
Service node therefore fails with the exact `GraphWireId` and both domains.
Event/Stream queues may describe a domain boundary, but acceptance is not a
claim that its transport, clock bridge, delivery protocol, or implementation
exists.

## Exact allocation report

The analyzer starts from the checked maximum canonical bytes already proven for
the input type. A synchronous connected input reserves that one full typed
value. Each queued item reserves its complete typed payload/sample plus 16
canonical analysis bytes: one `u64` source-clock tick and one monotonic `u64`
sequence. Checked multiplication by capacity produces the input total, and
checked addition produces the graph total.

The interactive admission policy currently limits a queue to 4,096 items, one
input allocation to 64 MiB, and all channel allocations to 256 MiB. The report
retains source endpoint, target endpoint, delivery/full policy, maximum item
bytes, maximum total bytes, and the graph aggregate in canonical node/port
order. These are serialized-value bounds, not Rust heap-layout claims; later
firmware lowering must match or conservatively exceed them with a separately
audited fixed representation.

The exact-rational semantic fixture has four connected synchronous inputs. Its
45-byte typed-value ceiling yields exactly 180 channel bytes. A separate Boolean
runtime fixture proves the queue calculation:

| Input | Typed payload/sample | Envelope | Capacity | Total |
| --- | ---: | ---: | ---: | ---: |
| event | 5 bytes | 16 bytes | 3 | 63 bytes |
| stream | 5 bytes | 16 bytes | 4 | 84 bytes |
| aggregate | — | — | — | 147 bytes |

Tests also reject a required unconnected endpoint, while proving an optional
unconnected input allocates zero. They reject a cross-domain synchronous wire,
a Stream queue larger than its type, a queue larger than policy, an 84-byte
queue under an 83-byte per-input policy, and a 147-byte graph under a 146-byte
aggregate policy.

## Reproduced checks

Run from `alumina-interface` at the commit above:

```console
cargo fmt -p alumina-interface -p alumina-interface-client \
  -p alumina-interface-core -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 77 native tests pass: 8 application/coordinator, 24 client, and 45 core,
plus the intentional compile-fail rustdoc test. The core total includes 25 graph
tests. Native and WASM warnings-denied project Clippy, warnings-denied project
rustdoc, the current-sibling/permissive-license audit, optimized Trunk build,
WASM validation, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,257,524 | `53d62629fdbe468aba30690d048db1d0b64bd27918c1cd60c8563d9157afdd22` |
| `alumina-interface_bg.wasm.gz` | 1,978,863 | `840c4147e4e58390abb4207c15b73072094aba8ac0a4bf66e7b94b452eea5f6e` |
| `alumina-interface_bg.wasm.br` | 1,614,669 | `9d08bee79d73022a7a7edec70e4ad4c52a1a48b21fe8ed48344bbdb3572f9ab4` |

The optimized WASM and Brotli bytes remain identical to the preceding storage
checkpoint because this host analysis API is not yet reachable from the
browser shell and release dead-code elimination removes it. The gzip digest is
a build observation and includes compressor metadata.

## Closed claims and licensing

Clock/rate conversion, queue ordering at runtime, timeout/empty semantics,
bridge transport, fixed firmware memory, WCET/deadlines, evaluation, lowering,
and deployment remain open. No graph document or arbitrary graph value reaches
firmware.

No target firmware source or binary changed. The connected bare MKS TinyBee
V1.0 was not read, reset, flashed, or otherwise touched; no motor or motor power
was connected. The SLogic16U3 remained disconnected, and no MKS ESP32 FOC
hardware was available. No physical I/O, timing, motion, synchronization, FOC,
or safety claim is made.

The implementation is independently authored under MIT and adds no dependency.
The native/WASM inventory and current-sibling inverse-path audit admit no
GPL-family or missing-license dependency. No GPL-family source or asset was
copied or consulted.
