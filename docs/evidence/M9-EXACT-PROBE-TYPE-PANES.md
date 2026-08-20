# M9 exact probe type-pane evidence

Date: 2026-08-20

This checkpoint removes the mixed-signal inspector's remaining single analog
sample-type restriction. Exact-rational probes now occupy deterministic panes
by registered graph type, while every pane and Boolean lane retains the same
exact root-time axis, trigger marker, and cursor.

## Source boundary

- Interface implementation:
  `48d4cd7d71ba9dfe8ee53f77dc612cf3996d7f6a`
  (`feat: split exact probe scales by type`).
- The shared interface checkout remained on `agent/zizmor-ci-hardening`; the
  implementation follows the exact multi-rate axis commit
  `53837475453317d728c095fb3514c1cc46045f17`. No branch was switched.
- Preceding firmware evidence checkpoint:
  `d610f48601c7af22f13371f506f45804457d6c8f`.
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

## Deterministic type panes

Every projected exact-rational series retains the canonical probe ID/name,
endpoint, registered `GraphTypeId`, schema type name, and unit symbol. The UI
groups those series in ascending type-ID order. Series with the identical type
share one pane and one display-only Y enclosure; different types receive
separate panes even when their values or unit symbols happen to resemble one
another. Each pane displays its canonical unit, type name, and type ID.

This grouping performs no unit conversion and does not change any exact sample,
ALGP replay fact, capture stride, retained window, or source-clock identity.
The only numeric projection remains the named one-way conversion of certified
finite enclosures into egui coordinates. Each type pane derives its own finite
Y bounds, while all X positions continue to come from the shared exact
`Rational` root-time axis. Hover position selects one retained exact time; it
never synthesizes a time from a float. The shared cursor and trigger lines span
the complete first-to-last pane extent, including the analog-only layout.

The browser display policy admits at most 32 distinct analog types. A 33rd type
fails explicitly before painting rather than creating an unbounded surface or
hiding a series. The existing 4,096-points-per-visible-series and aggregate
131,072 projected-sample bounds remain in force.

The new regressions prove:

- two `exact.mm` series share the `t2` / `mm` pane;
- one `exact.percent` series receives a separate `t3` / `%` pane;
- groups remain sorted by type ID regardless of input series order;
- inconsistent type-name/unit metadata and missing exact-unit metadata fail;
- 33 distinct exact types fail the 32-pane display policy; and
- a real external-source probe resolves and retains the canonical
  `exact.mm` type name as well as its `mm` unit.

The existing mixed-clock regression still proves exact `1/3`, `1/2`, and `2/3`
cursor snapping and slower-series sample-and-hold. The reference four analog
series all use `exact.mm`, so its canonical ALGP and visible waveform shape are
unchanged; the browser now labels that pane `mm`, `exact.mm`, and `t2`.

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

All 283 executable workspace tests pass: 60 application/coordinator, 82
protocol-client, 140 core, and one public exact-control integration test.
Native and WASM warnings-denied Clippy, strict rustdoc, formatting, the
local-source/permissive-license audit, optimized Trunk assembly, WASM
validation, and gzip/Brotli integrity pass.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The final optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,382,410 | `33faf796ed090d08288069206c0a8ceff546a6e81d2376b0156cfcb4af8732e2` |
| `alumina-interface_bg.wasm.gz` | 2,823,705 | `dc3d32275c881d0e959a0f7697905970208a4672518a3dd3a2cd7abb781fd06d` |
| `alumina-interface_bg.wasm.br` | 2,229,938 | `234269a2b6a6de5b5b13966cd5614aa841cc7ff78657155ff9b7c535d1d32aa4` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and worker loaded from `127.0.0.1:8765` in a fresh
isolated Chromium profile. Runtime inspection found the unchanged one-value
`algwp1:` carrier with a 3,755-byte ALGW segment, a 407-byte ALGP segment, and
8,332 total text characters.

The scrolled Control Graph view visibly rendered the schema-derived `mm`,
`exact.mm`, and `t2` pane label; exact root ticks 10, 20, 30, 40, and 50; the
matched trigger and cursor at root tick 30; four analog series; and three
Boolean lanes. The accepted 1,440-by-670 capture
`/tmp/alumina-probe-metadata-browser.png` is 212,066 bytes with SHA-256
`0a1787dd5d643d61785b760da0526fd49dac1391ccdfe91c48f21126fd82f25c`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and background Google
registration errors; no Alumina application, device, or WLAN error appeared.

The synthetic Rust regression, rather than the single-type reference graph, is
the evidence for simultaneous `mm` and `%` pane separation. This checkpoint
does not claim a browser fixture with two rational types yet.

## Closed claims and licensing

This is exact host replay and presentation evidence. It does not allocate
device capture memory, negotiate telemetry bandwidth, arm a hardware trigger,
read a GPIO, deploy a graph, or prove physical multi-MCU synchronization. The
32-pane policy is a browser display bound, not a firmware telemetry guarantee.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
