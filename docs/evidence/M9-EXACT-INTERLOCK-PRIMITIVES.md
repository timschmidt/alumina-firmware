# M9 exact interlock primitive evidence

Date: 2026-08-20

## Scope and source

This checkpoint extends the bounded `HostExact` graph simulator and editor with
two independently reviewed primitives: an exact inclusive-range predicate and
same-clock Boolean conjunction. The implementation is `alumina-interface`
commit `ca3690d9ddfc76f4f0b938354f0eafa71e5fe0de`, against preceding
`alumina-firmware` evidence commit
`408dbe695da47a793755396c26857fa32c84da06`.

No firmware source, target image, board configuration, workstation network
configuration, or physical I/O changed. The bare MKS TinyBee V1.0 was not
contacted, reset, flashed, or probed. This is host simulation/editor evidence,
not a Realtime opcode, deployment, safety, timing, or physical-input claim.

The interface continued to compile from the live path-based CSGRS/Hyper stack,
not the obsolete released CSGRS baseline. Hypercurve changed concurrently: one
release-build attempt observed a transient incomplete source edit, and an
unchanged retry later compiled successfully. No Hyper repository was modified,
formatted, staged, pinned, or treated as a reproducible release snapshot by
this checkpoint.

## Audited primitives

`GraphSimulationNodeKind::ExactWithinInclusive` accepts one exact-rational
Stream, same-sample-type exact lower and upper parameters, and produces one
Boolean Stream on the same clock. Admission requires exactly one required
bounded input queue, the complete input-to-output dependency, no rate
transition, and no hidden state. Evaluation compares Hyperreal rationals
directly. Both endpoints are inclusive; a lower bound greater than the upper
bound returns `InvalidParameterValue` rather than swapping, approximating, or
inventing a result.

`GraphSimulationNodeKind::BooleanAnd` accepts two identical same-clock Boolean
Streams and produces the identical Boolean Stream type. Both inputs must be
required bounded queues and both must appear in the audited output dependency.
The evaluator reads and validates both inputs before conjunction, so ordinary
language short-circuiting cannot mask a missing right-hand sample.

Both bindings are part of the canonical `ALSI` V2 implementation identity with
new unambiguous behavior tags. Malformed output ports, aliased Boolean inputs,
wrong types/clocks, missing dependencies, parameters, state, or rate
transitions fail registry construction. Focused tests separately exercise
`true && false`, `false && true`, `true && true`, inverted range rejection, and
malformed binding rejection.

## Representative PID/interlock and editor

The shared fixture is now 21 nodes and 25 wires. The 10 Hz measurement Stream
feeds an exact inclusive `[0, 2] mm` interlock; that Boolean result is conjoined
with the independently resampled external permit before the existing fail-safe
exact permit gate. The visible range and combined-permit traces are both
`[true, true, true, false, false, false]` for the canonical input. The retained
controller traces remain:

- integral prior state: `[0, 3, 5, 6, 6, 6]` mm;
- inclusive-clamped controller: `[5, 5, 4, 2, 3, 3]` mm; and
- final fail-safe output: `[5, 5, 4, 0, 0, 0]` mm.

The fixed-schema editor palette expands from 11 to 13 node kinds. Both new
kinds derive their ports, parameters, queues, dependencies, domain, and reviewed
defaults from the audited registries; the UI invents no implementation or
resource authority. The reusable component front panel now exposes eight exact
controls: P/I/D gains, clamp minimum/maximum, safe output, and interlock
minimum/maximum. Four existing exact replay indicators remain unchanged.

Canonical identities intentionally advance together; no compatibility shim is
provided:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| `ALGR` graph | — | `96a3348264a9b65d267b45f9a6419a44ee60473fd961abcf4436295e10b3735f` |
| `ALSI` V2 registry | — | `fc68d37f279782c5a5368bc0e44aa695a3b2babbfaf967b09cf4fc75287eae83` |
| `ALGT` trace | 8,292 | `e2f8a0f20b3e5f9fdfc12c394e1e325d7b65243efad8c9c7f558f8845c965fe3` |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| four-series `ALGP` | 257 | `712e68be8902d3c87ca67f58b426670e8f7f99ac79923c4c38e6485b35f3b03a` |
| `ALGC` component | 4,554 | `1745af2a70981fcd61c87361e62ba9cb236eccf9ce4fbe498b33e6d278fc29a6` |
| source `ALGH` | 5,463 | `96d01a2427303bffa3a722b190a8433b047a435364298e45d404b7fda5b0f161` |
| flattened `ALGW` | 3,755 | `e39c5396539689b8b563a7220e1180d7717e70893b71580ebbe51873fa13b68f` |

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt -p alumina-interface-core -p alumina-interface -- --check
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

All 260 executable workspace tests pass: 50 application/coordinator, 82
protocol-client, 127 core, and one public exact-control integration test, plus
the intentional compile-fail rustdoc test. The application suite includes a
complete headless exact-control egui frame. Final post-commit native and WASM
warnings-denied Clippy passes cover the changed core package. Strict rustdoc,
the local-source/permissive-license audit, optimized Trunk build, both WASM
validators, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,292,397 | `34c7a9a9a019e8abc7b4132f0b6280a2c231e7af404b630ed2f232c671ebf513` |
| `alumina-interface_bg.wasm.gz` | 2,787,927 | `488adb978dc53ae150f0c021d37ad225e8919824134b186dc4a68517eb4975cd` |
| `alumina-interface_bg.wasm.br` | 2,204,624 | `ea0b1cac97f0b9d36674fb9d5cef90c9b6f3e82e9b7b65758e0a5a36a2126abf` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.
The final optimized bundle and dedicated worker loaded over
`127.0.0.1:8765` in headless Chromium with software WebGL. The transient
1,440-by-1,200 screenshot is 256,505 bytes with SHA-256
`2f8d1461c05b6d762b59fa78031ad29941be2c795ac869cc26f3654b9f410848`.
The loopback server was stopped afterward.

## Closed claims and licensing

These behaviors remain `HostExact` simulation/editor primitives. They have no
firmware opcode, resource claim, WCET, fixed-memory lowering, Service/Realtime
placement, live telemetry, or safety authority. The existing fixed firmware
graph IR remains unchanged and cannot execute either new behavior. Promotion
requires a separately reviewed lowering and split-core runtime checkpoint.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
