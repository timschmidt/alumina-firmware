# M9 exact replay probe trigger evidence

Date: 2026-08-20

This checkpoint turns the causal interlock lanes into a bounded host-side
logic-analyzer primitive. Canonical `ALGP` V2 retains one typed Boolean-stream
edge trigger and an exact pre/post replay window. The native/WASM UI authors
that trigger and renders its resolved window without changing the graph or
claiming any device capture authority.

## Source boundary

- Interface implementation:
  `1010a932f707a17bf2994695731848d12c41f049`
  (`feat: add exact replay probe triggers`).
- Preceding firmware evidence checkpoint:
  `78f0a42953185ae2c03e8b1dd749c7e1a5bcb3a1`.
- Firmware source, target images, protocol, and board packages are unchanged by
  this checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor power, or motor is involved.
- Workstation Wi-Fi configuration and the Alumina device AP were not touched.
  Browser qualification used `127.0.0.1` only.
- The live CSGRS/Hyper path dependencies, including the concurrently edited
  HyperCurve tree, were compiled as read-only dependencies. No sibling status,
  diff, reset, pin, format, stage, or edit was performed.

## Canonical `ALGP` V2

The V2 sidecar deliberately replaces V1; there is no compatibility decoder or
shim. Its fixed little-endian trigger record follows the bound `ALGW` digest and
contains:

1. edge kind: disabled, rising, falling, or either;
2. stable source `GraphProbeId`;
3. requested pretrigger retained-sample count; and
4. requested posttrigger retained-sample count.

An enabled trigger must name an existing Boolean `Stream` probe. The pretrigger
count, one trigger sample, and posttrigger count must fit both caller/document
policy and that probe's own capture ceiling. Disabled records re-encode all
unused fields as zero; nonzero hidden state is noncanonical. Unknown edge kinds,
unknown probes, analog probes, excessive windows, corrupt/truncated input,
trailing bytes, and workspace substitution all fail before a document is
returned.

Probe identities and endpoints remain unchanged. The reference trigger binds
probe 5, `measurement-in-range`, to a falling edge with two pretrigger and two
posttrigger samples.

## Bounded exact resolution

`resolve_graph_probe_trigger` first requires the simulation's canonical `ALGR`
identity to equal the graph embedded by the sidecar-bound workspace. It then:

- selects only canonical node-output records for the trigger probe;
- applies the probe's explicit event-ordinal stride before edge comparison;
- compares consecutive retained Boolean values;
- keeps at most the requested pretrigger values plus the current value in a
  bounded `VecDeque` while waiting;
- selects the first matching edge; and
- reports exact graph clock, tick, source sequence, first/last available window
  ticks, actual pre/post counts, and completeness flags.

No edge in a finite replay returns typed `Waiting`; no configured trigger
returns `Disabled`. A match with insufficient history or tail remains explicit
and reports truncated pre/post sides rather than inventing samples. Tests cover
stride-two detection, complete and truncated windows, waiting and disabled
states, transactional failures, and exact canonical replay.

The reference traces are:

| Signal | Control ticks 0 through 5 |
| --- | --- |
| external permit | `true, true, true, true, false, false` |
| measurement in range | `true, true, true, false, false, false` |
| combined permit | `true, true, true, false, false, false` |

Probe 5 therefore matches at exact control tick 3 and source sequence 3. The
complete two-before/two-after window is ticks 1 through 5. At the trigger tick,
external permit remains `true`, measurement-in-range and combined permit are
`false`, and the permit-gated output is exactly `0 mm`.

## Transactional authoring and presentation

Trigger installation, replacement, and clearing advance the canonical sidecar
revision only when state changes. Setting an identical trigger and clearing an
already-disabled trigger are exact no-ops. Removing the active source probe
atomically clears the trigger without reusing its probe identity. Invalid edits
leave the prior bytes and graph untouched.

The UI exposes bounded pre/post counts plus readable `rise trigger`, `fall
trigger`, and `either trigger` actions on Boolean-stream probes only. Analog
probes have no trigger action. A matched trigger narrows every selected analog
and Boolean series to the exact common window, draws a distinct trigger marker,
and initializes the shared exact cursor at the trigger tick. Waiting/disabled
states show the complete finite replay with an explicit status.

The first visual pass exposed missing glyph boxes for Unicode arrow button
labels. Those labels were replaced with plain text before the accepted build
and capture.

## Canonical identities

The graph, workspace, component, hierarchy, and trace identities are unchanged
from the preceding cause-trace checkpoint. Only the probe format/bytes and
downstream application artifact change.

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| `ALGR` graph | — | `96a3348264a9b65d267b45f9a6419a44ee60473fd961abcf4436295e10b3735f` |
| `ALSI` V2 registry | — | `fc68d37f279782c5a5368bc0e44aa695a3b2babbfaf967b09cf4fc75287eae83` |
| `ALGT` reference trace | 8,292 | `1a1f7e0e80e24f112787bfcc9d5e04c2012a5122a9013d14c998fd6fbdc95f72` |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| `ALGC` component | 4,815 | `10e6498ec36afc377f138cacb5c6afe2091c40749ea3c9e9d4bba8925a4f0228` |
| source `ALGH` | 5,814 | `232603de0d4a17ff45b7bda1c363745b316405c99fbdbdc44347fa12bd00e0c8` |
| flattened `ALGW` | 3,755 | `e39c5396539689b8b563a7220e1180d7717e70893b71580ebbe51873fa13b68f` |

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt -p alumina-interface -p alumina-interface-core -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 265 executable workspace tests pass: 51 application/coordinator, 82
protocol-client, 131 core, and one public exact-control integration test, plus
the intentional compile-fail rustdoc test. A restricted-runner attempt denied
the existing localhost waveform test's socket with `Operation not permitted`;
the unchanged offline suite passed with loopback permission. Native and WASM
warnings-denied Clippy, strict Alumina rustdoc, the local-source/permissive-
license audit, optimized Trunk assembly, both WASM validators, and compressed-
artifact integrity pass. Dependency builds emitted only pre-existing warnings
from the live concurrently edited HyperCurve tree.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,319,779 | `15877ef3b20dfe263c88f91a6bdb2ba6382689e3d133f9ce97331af621af9d4c` |
| `alumina-interface_bg.wasm.gz` | 2,799,459 | `510f61a6990566a3bf69cb72da617a7e206f35444ebd1ef7d90845e3dddd4070` |
| `alumina-interface_bg.wasm.br` | 2,213,594 | `cb478f0218d4cd2120cb51c33b864a51f9e4782484ffd23063003f62dd0b509c` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

The optimized bundle and dedicated worker loaded from `127.0.0.1:8765` in a
fresh headless Chromium profile using ANGLE/SwiftShader. The accepted
1,440-by-970 capture visibly shows canonical sidecar `50955da7…`, seven probes,
the active probe-5 falling-edge trigger, plain-text trigger actions, the exact
tick-1-through-5 window, a trigger marker at tick 3, all three causal Boolean
lanes, and exact tick-3 cursor values. `/tmp/alumina-trigger-qualified.png` is
266,673 bytes with SHA-256
`6875c2c790aa855dede633a98a630523807ac5353890f9c74c4ccea6cd9fdb88`.
The loopback browser and server were stopped afterward. Chromium logged only
background Google registration errors; no Alumina device or WLAN operation was
attempted.

## Closed claims and licensing

This is bounded `HostExact` replay/editor authority. It does not subscribe to
device telemetry, configure an ESP32 capture peripheral, grant a pin/resource,
arm an output, install or execute firmware graph work, or establish physical
interlock timing. Promotion requires authenticated capability/configuration
binding, bounded loss-aware transport/buffering, device-cycle timing evidence,
and independently qualified capture and safety ownership.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
