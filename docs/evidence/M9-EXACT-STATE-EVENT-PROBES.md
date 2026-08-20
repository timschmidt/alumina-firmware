# M9 exact state/event probe evidence

Date: 2026-08-20

This checkpoint closes the plot boundary for every literal non-scalar graph
value. Text, bytes, arrays, records, options, results, resource handles, and job
handles now retain canonical typed identity in bounded categorical state/event
lanes instead of failing the complete mixed-signal projection or acquiring an
invented numeric ordering.

## Source boundary

- Interface implementation:
  `fa1c3d0ee8f87dadccb8308c0cb7b6a4b4e0927d`
  (`feat: plot canonical graph state events`).
- Exact event-order follow-up:
  `34b832e7f3376a9e3b6109767032e6dd947f5ac0`
  (`fix: retain exact state event ordering`).
- Qualification-only test split:
  `ac5d119aba0cad09bf42723e97b88376d6c68541`
  (`test: split state lane policy coverage`).
- Preceding firmware evidence checkpoint:
  `5b4202094b3581cae271c409a6d7ec11c22fce2a`.
- Both repositories remained on `agent/zizmor-ci-hardening` throughout the
  checkpoint. `alumina-firmware` is the canonical firmware repository; the
  retired `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and physical-device
  state are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or analyzer lead was involved.
- Workstation Wi-Fi and the Alumina device AP were not touched. Browser
  qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies were compiled read-only. No sibling
  status, diff, reset, pin, format, stage, or edit was performed.

## Canonical typed-value identity

`alumina-interface-core` now exposes
`CanonicalTypedGraphValueEncoding` and
`encode_typed_graph_value(schema, value)`. The carrier owns the exact
schema-relative type-ID-plus-value bytes already used inside canonical `ALGR`
documents and `ALGT` traces, plus SHA-256 over those bytes. It is deliberately
not a second self-describing format: callers must supply the authoritative
schema, and a value valid only under a foreign schema fails before an encoding
is returned.

The UI retains those complete bytes for every displayed state sample. Byte
equality, not digest equality, debug text, a preview, or an assigned category,
decides whether one event changes the preceding state. The digest is a visible
complete identity, and the byte count remains visible with it.

Bounded summaries expose useful shape without duplicating the canonical value:

- text shows at most 32 escaped Unicode scalar values and its exact UTF-8 byte
  count;
- bytes show at most eight leading bytes in hexadecimal and the exact count;
- arrays and records report their item or field counts;
- options and results report their exact branch and nested type shape; and
- resource/job handles expose bounded identity prefixes while the full typed
  digest remains available beside them.

Runtime `Event` and `Stream` wrappers still have no literal graph value and
remain a typed rejection. This checkpoint displays retained literal state
events; it does not invent a runtime wrapper value or firmware event opcode.

## Exact state/event lanes

Every supported non-scalar probe becomes one categorical lane, sorted by stable
probe identity and labelled with probe name, registered type name, and type ID.
The lane shares the exact rational root-time axis, trigger marker, and cursor
with analog and Boolean panes. No state value contributes a Y-scale or numeric
ordering.

Every retained event is represented. Consecutive events at the same exact root
tick share one marker annotated `×N`; the renderer never fabricates a sub-tick
coordinate. A stronger marker appears if any event in that exact-time run is a
byte-exact change. Original local clock, tick, and monotonic sequence survive
through projection and cursor text. At a shared exact time, sample-and-hold
selection returns the final canonical sequence at or before the cursor.

The explicit UI policy admits at most 64 state/event lanes and 16 MiB of
retained canonical state identity bytes. Checked addition rejects byte-count
overflow. The existing limits of 4,096 retained points per series and 131,072
aggregate projected samples remain in force. Excess lanes, excess identity
bytes, malformed schema/value combinations, and unavailable canonical identity
all fail before a partial pane set is painted.

Focused regressions cover all eight supported non-scalar variants, text/byte
preview truncation, exact type ID and SHA-256 identity, foreign-schema
rejection, state-byte budget exhaustion and integer overflow, stable lane
ordering, the 64-lane ceiling, repeated versus changed canonical bytes,
same-root-time multiplicity, final-sequence cursor selection, and a direct
headless `egui` state-lane painter invocation.

## Reproduced checks

Run from `alumina-interface` at the final interface commit:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli --test dist/alumina-interface_bg.wasm.br
git diff --check
```

All 290 executable workspace tests pass: 66 application/coordinator, 82
protocol-client, 141 core, and one public exact-control integration test. Both
warnings-denied Alumina Clippy commands, complete WASM test-target linking,
strict Alumina rustdoc, package-scoped formatting, the local-source and
permissive-license audit, optimized Trunk assembly, WASM validation, and
gzip/Brotli integrity pass.

HyperCurve was being edited concurrently. Early probes observed changing
incomplete helper/method renames, including one transition after a successful
check but before the first full test compile. Each failure originated in the
read-only live HyperCurve path. Qualification resumed only after the current
sibling stack became coherent; the subsequent complete gates above passed.
Nothing in Alumina hid, edited, pinned, copied, or replaced that dependency.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The final optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,326,174 | `4c09f591b7c87c7402c49261686e44c13cae870f82de64264a61b95d2fcb993a` |
| `alumina-interface_bg.wasm.gz` | 2,804,982 | `80f99729922d6dadf2c55b4a9f0532dbdea877ae7661d70bd3e74b278284d2af` |
| `alumina-interface_bg.wasm.br` | 2,217,075 | `5824516b37ad6e5d1d1f1033d307f11b810fec5f70ad06b9b8e57c1da3f7ecdf` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and worker loaded from `127.0.0.1:8765` in a fresh
isolated Chromium profile. Runtime inspection found the unchanged `algwp1:`
carrier with a 3,755-byte ALGW segment, a 407-byte ALGP segment, and 8,332 total
text characters. The 1,440-by-670 capture
`/tmp/alumina-probe-metadata-browser.png` is 225,567 bytes with SHA-256
`05968a28199325f873bcb818e7d1a46bedeced47f2654f1054db7e188dc030c2`.
The localhost Chromium and HTTP server were stopped afterward.

The reference graph still emits only exact-rational and Boolean probe values,
so it deliberately does not manufacture a non-scalar browser fixture. The
direct Rust state painter test and canonical value-family regressions are the
evidence for state/event lane semantics. This checkpoint does not claim live
state acquisition, device-triggered event capture, or firmware Event/Stream
execution.

## Closed claims and licensing

This is exact bounded HostExact replay and presentation evidence. It neither
allocates device telemetry/capture memory nor grants deployment, I/O, safety,
or motion authority. Identity-bearing values are displayed as retained opaque
state; displaying a resource handle does not make that resource accessible.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the current local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
