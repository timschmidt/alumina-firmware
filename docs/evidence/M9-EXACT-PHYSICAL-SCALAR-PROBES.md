# M9 exact physical-scalar probe evidence

Date: 2026-08-20

This checkpoint extends the exact mixed-signal inspector beyond rational and
Boolean samples. Registered measurement intervals and signed or unsigned
canonical integer lattices now use the same bounded per-type analog panes
without weakening their exact source values into display floats.

## Source boundary

- Interface implementation:
  `7be14a0a8b42c54b4310043c27ba6d500bf4eb72`
  (`feat: plot exact physical scalar probes`).
- The implementation follows interface type-pane checkpoint
  `48d4cd7d71ba9dfe8ee53f77dc612cf3996d7f6a` on the unchanged
  `agent/zizmor-ci-hardening` branch.
- Preceding firmware evidence checkpoint:
  `78db02eaa30da102bb0f215a81d485a88fcc52c8`.
- `alumina-firmware` is the canonical firmware repository. The retired
  `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and physical-device
  state are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or analyzer lead was involved.
- Workstation Wi-Fi and the Alumina device AP were not touched. Browser
  qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies were compiled read-only. No sibling
  status, diff, reset, pin, format, stage, or edit was performed.

## Exact scalar projection

The plot admits four physical scalar shapes from the registered graph schema:

| Registered type shape | Retained plot authority | Display-only enclosure |
| --- | --- | --- |
| `ExactRational` | original `Rational` text | its outward binary64 bounds |
| `MeasurementInterval` | exact closed lower and upper `Rational` endpoints | lower projected downward, upper projected upward |
| `CanonicalI64` | signed count and exact `count × quantum` value | outward bounds of that exact product |
| `CanonicalU64` | unsigned count and exact `count × quantum` value | outward bounds of that exact product |

The quantum and unit come from the exact registered sample type. Integer counts
are never cast to a float before multiplication. Cursor text includes both the
exact physical product and original lattice count, the schema unit, and the
original clock/tick. Measurement cursor text retains both exact endpoints.
Component replay indicators now use the resolved series unit instead of a
hard-coded millimetre suffix.

All four families are classified as analog only after the registered
`TypeKind` and retained `GraphValue` variant agree. Reversed intervals,
type/value mismatches, missing physical-unit metadata, non-finite display
enclosures, and inverted enclosure results fail before painting. Text,
structured, identity-bearing, and other non-scalar values are not implicitly
converted into analog data.

The preceding deterministic grouping policy remains unchanged: an identical
`GraphTypeId` may share one Y scale, distinct types receive separate panes in
type-ID order, no unit conversion occurs, and at most 32 analog panes are
admitted. All panes continue to share the exact rational root-time axis,
trigger marker, and sample-and-hold cursor.

The focused regressions prove exact `1/3`, the measured interval
`-1/3..2/3`, signed count `-3` at quantum `1/8`, and unsigned count `7` at
quantum `1/4`. They compare the exact expected outward enclosure bit patterns,
retain `-3/8 [-3 lattice counts]` and `1 3/4 [7 lattice counts]` cursor values,
and reject a signed value under an unsigned type, a reversed interval, and a
text value. The existing grouping, mixed-rate cursor, and complete headless
frame regressions remain green.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz
gzip -t dist/alumina-interface.js.gz
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 285 executable workspace tests pass: 62 application/coordinator, 82
protocol-client, 140 core, and one public exact-control integration test. One
initial final-suite run encountered a transient sandbox `EPERM` in the unchanged
localhost-socket client test; that test passed immediately in isolation and the
subsequent complete 285-test run passed.

Both warnings-denied Alumina Clippy commands, strict Alumina rustdoc,
formatting, the local-source/permissive-license audit, optimized Trunk
assembly, WASM validation, and gzip/Brotli integrity pass. The concurrently
edited read-only HyperCurve dependency emitted two ordinary dead-code warnings
for one unconstructed variant and two unread fields. Cargo's dependency lint
cap left those as warnings while all Alumina workspace sources remained under
`-D warnings`. This checkpoint did not hide, edit, pin, or otherwise interfere
with that live dependency state.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The final optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,372,262 | `17d2694a9d834e5c0b5fae478000541603266531361588c979d61ade56f269ac` |
| `alumina-interface_bg.wasm.gz` | 2,821,749 | `edbd288b6d87d26626a231d448ed413b61e20e1c7c54e9d8de1ccca4104c85ba` |
| `alumina-interface_bg.wasm.br` | 2,227,918 | `1885c81156a1c7faab4d0fb612d3a2fdbffed6783f272dfffb1ef8b2b76c231d` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and worker loaded from `127.0.0.1:8765` in a fresh
isolated Chromium profile. Runtime inspection found the unchanged one-value
`algwp1:` carrier with a 3,755-byte ALGW segment, a 407-byte ALGP segment, and
8,332 total text characters.

The reference graph contains exact-rational and Boolean output samples, so its
visible result is intentionally unchanged: one `mm` / `exact.mm` / `t2` analog
pane, exact root ticks 10–50, trigger/cursor root tick 30, four analog series,
and three Boolean lanes. The accepted 1,440-by-670 capture
`/tmp/alumina-probe-metadata-browser.png` remains 212,066 bytes with SHA-256
`0a1787dd5d643d61785b760da0526fd49dac1391ccdfe91c48f21126fd82f25c`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and a background Google
registration error; no Alumina application, device, or WLAN error appeared.

The Rust scalar-family regressions, not the unchanged reference browser graph,
are the evidence for measured-interval and integer-lattice display semantics.
This checkpoint does not claim live analog acquisition or a browser fixture
that emits those additional graph types.

## Closed claims and licensing

This is exact host replay and presentation evidence. It does not allocate
device capture memory, negotiate telemetry bandwidth, arm a hardware trigger,
read a GPIO, deploy a graph, or prove physical multi-MCU synchronization. A
canonical graph integer is an exact typed lattice value, not proof that a
physical ADC, encoder, timer, or counter produced it.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
