# M9 canonical graph type-storage evidence

Date: 2026-08-11

## Scope and source

This checkpoint proves finite canonical byte ceilings for the exact registered
graph value domain and uses them to validate explicit state storage. It remains
host-side analysis. It does not define a Rust heap layout, a fixed firmware
value representation, queue envelopes, an evaluator, or deployment IR.

The implementation is `alumina-interface` commit
`89d119492bbeea3181d09c2003cfb0aa84568af2`, above canonical document commit
`6c07c2f` and audited semantic commit `9f8462a`. It uses the current sibling
`hyperreal`, `alumina-protocol`, and `alumina-storage` source selected by the
local-path/source-policy audit.

## Checked type ceilings

Storage analysis walks the already cycle-checked type registry using checked
`u64` arithmetic and retains one result per canonical type ID. Literal ceilings
match the `ALGR` V1 typed-value representation, including the four-byte root
type ID. The recursion accounts for:

- boolean and signed/unsigned canonical integer widths;
- rational sign, two length fields, and the configured maximum decimal digits
  for both numerator and denominator;
- both exact endpoints of a measurement interval;
- length fields plus maximum UTF-8/blob/array content;
- record count and every field identity/body;
- option/result tags and the larger result branch;
- device, digest, class, selector, global-job, and partition handle fields; and
- checked multiplication/addition through nested composite maxima.

Runtime Event and Stream types remain distinct. Their reports retain the exact
clock, and Stream retains its declared capacity, but the byte ceiling covers
only one complete typed payload/sample. No timestamp, sequence, queue slot, or
transport envelope is invented by this slice. An Event or Stream nested inside
a saved literal composite rejects rather than gaining an ambiguous layout.

The focused 16-digit exact-rational fixture proves these maxima:

| Type | Maximum canonical bytes |
| --- | ---: |
| exact rational scalar | 45 |
| eight-byte bounded text | 16 |
| record of that scalar and text | 69 |
| event typed payload | 69 |
| stream typed sample | 69, capacity 7 retained separately |

## State proof

Semantic analysis computes type ceilings before examining node instances. Each
`NodeStateContract` must name a literal type. Its declared storage must be at
least the full typed-value ceiling, and both declared and required totals remain
in the report.

The representative read-before-write delay declares 64 bytes for its 45-byte
maximum exact value and passes. The otherwise identical 44-byte contract fails
at the exact node with `StateStorageTooSmall { declared: 44, required: 45 }`.
The independent total-declared-state policy remains enforced as well.

This establishes a canonical retained-value bound. It does not assert that the
current arbitrary-precision `hyperreal::Rational` in-memory object occupies 45
bytes, nor that a future firmware implementation may operate on it directly.

## Reproduced checks

Run from `alumina-interface`:

```console
cargo fmt --all --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- \
  -D warnings
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

All 75 native tests pass: 8 application/coordinator, 24 client, and 43 core,
plus the intentional compile-fail rustdoc test. The core total includes 23
graph tests. Native and WASM warnings-denied project Clippy, warnings-denied
project rustdoc, current-sibling/permissive-license audit, optimized Trunk
build, WASM validation, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,257,524 | `53d62629fdbe468aba30690d048db1d0b64bd27918c1cd60c8563d9157afdd22` |
| `alumina-interface_bg.wasm.gz` | 1,978,863 | `61448d2e474df23bef1959766c9e2882d22dcd1b96016e678a95607408a45982` |
| `alumina-interface_bg.wasm.br` | 1,614,669 | `9d08bee79d73022a7a7edec70e4ad4c52a1a48b21fe8ed48344bbdb3572f9ab4` |

## Closed claims and licensing

Queue/channel capacity, full/empty/backpressure policy, runtime object layout,
fixed firmware storage, rate transitions, WCET/deadlines, resource allocation,
node evaluation, protocol bridges, and service/realtime lowering remain open.
No graph file or arbitrary value reaches firmware.

No target source or binary changed. The connected bare MKS TinyBee V1.0 was not
read, reset, flashed, or otherwise touched; no motor or motor power was
connected. The SLogic16U3 remained disconnected, and no MKS ESP32 FOC hardware
was available. No physical claim is made.

The implementation is independently authored under MIT and adds no dependency.
The native/WASM inventory and current-sibling inverse-path audit admit no
GPL-family or missing-license dependency. No GPL-family source or asset was
copied or consulted.
