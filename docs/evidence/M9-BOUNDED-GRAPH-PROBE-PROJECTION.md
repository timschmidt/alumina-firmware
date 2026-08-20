# M9 bounded graph-probe projection evidence

Date: 2026-08-20

This checkpoint makes canonical ALGP capture policy operational in the
HostExact reference replay. Per-probe event stride and retention now select the
exact entries consumed by the mixed-signal plot, a matched trigger window is
compared in exact shared-root time, and one caller-owned aggregate ceiling
bounds the complete projection.

## Source boundary

- Interface implementation:
  `7575c5c72904ced24e6a6dbc023177581960a3b8`
  (`feat: project bounded graph probe replay`).
- Preceding firmware evidence checkpoint:
  `10957f40a975f3e4cfd8c4fb6c82f9ed9f8cfb47`.
- `alumina-firmware` is the canonical firmware repository. No evidence or
  implementation was written to the retired `aluminafw` path.
- Firmware source, protocol, target images, board packages, and physical-device
  state are unchanged by this checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or external analyzer lead is involved.
- Workstation Wi-Fi configuration and the Alumina device AP were not touched.
  Browser qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies, including the concurrently edited
  HyperCurve tree, were compiled read-only. No sibling status, diff, reset,
  pin, format, stage, or edit was performed. One focused build observed an
  incomplete live HyperCurve edit and was discarded; the final coherent source
  state passed the complete qualification below.

## Exact bounded projection

`project_graph_probe_replay` accepts the canonical ALGP document, its exact
bound ALGW workspace, a canonical simulation, the simulation registry, and a
caller-owned `GraphProbeProjectionLimits`. It fails before returning any output
unless:

- the aggregate sample ceiling is nonzero;
- the sidecar is bound to the exact workspace identity;
- the simulation graph identity matches that workspace;
- the supplied registry identity matches the simulation authority;
- exact graph/rate analysis resolves the simulation's independent root; and
- every selected sample belongs to that same root clock tree.

For each probe in canonical probe-ID order, event ordinal advances only over
canonical node-output entries for the probe's exact endpoint. Ordinal zero and
then every declared `sample_stride`th entry are eligible. If the Boolean replay
trigger matched, the trigger's first, trigger, and last local ticks are first
converted to exact rational ticks on the independent root clock. Candidate
samples are admitted only when their own exact root time is inside that closed
window. A disabled or waiting trigger instead makes the complete finite replay
eligible.

A trailing ring then retains at most that probe's canonical `maximum_samples`.
This deliberately means disabled/waiting replay shows the latest decimated
history. The implementation caps capacity before allocation and retains at
most one sample beyond the remaining aggregate budget solely to detect and
reject overflow. The interactive aggregate policy is 131,072 samples across
all series. Checked ordinal/count arithmetic and all identity, rate, and memory
failures return a typed error; no partial projection escapes.

Each `GraphProbeProjectedSample` owns the original exact canonical
clock/tick/sequence/value entry and its exact rational root tick. No float or
display coordinate enters selection, ordering, trigger-window comparison, or
retention.

## Plot boundary

The current mixed-signal reference plot converts analog enclosures and Boolean
levels directly from projected entries. It no longer intersects projected
identities with the workspace's complete reference-trace display cache. The UI
regression clears that cache before projection and still obtains all seven
series, proving that the bounded projection is the value source rather than
saved metadata attached to an unbounded plot.

The plot retains its independent 4,096-point per-visible-series ceiling. It
derives the displayed local clock from projected entries and refuses multiple
display clocks or a matched trigger on another local clock. Equal-looking local
tick integers therefore cannot be silently aligned. The core already preserves
exact shared-root time for a later rational-time horizontal axis.

Visible replay status now reports:

- disabled/waiting behavior as latest bounded, decimated samples;
- matched probe/edge/local tick/sequence and available pre/post completeness;
- exact first/trigger/last ticks on the simulation root clock; and
- aggregate retained samples, probe count, and accepted display clock.

The unchanged reference ALGP projects five exact samples for each of seven
probes inside local ticks 1–5: 35 samples in aggregate. Reducing integral-prior
probe `p2` to maximum two with stride two projects only local ticks 2 and 4,
reduces the aggregate to 32, and remains two latest decimated samples after the
trigger is cleared.

## Regression and qualification

Core regressions prove exact cross-rate trigger-window conversion, canonical
series/sample ordering, per-probe stride and retention, latest-history behavior
without a trigger, the zero aggregate rejection, and rejection when actual
retention exceeds a tighter aggregate policy. The application regression proves
direct projected-entry consumption, reference counts/clock, changed stride and
retention, and disabled-trigger trailing retention.

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --offline -- -D warnings
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz
gzip -t dist/alumina-interface.js.gz
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 278 executable workspace tests pass: 57 application/coordinator, 82
protocol-client, 138 core, and one public exact-control integration test.
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
| `alumina-interface_bg.wasm` | 6,368,683 | `067c0c989d53948a607e70d14156dd02180ab9415c89564879e2a3bc4ddc7d8b` |
| `alumina-interface_bg.wasm.gz` | 2,817,990 | `394ee9b51a2ec0898435b780be1d8ca977dfb271cf4613ad092a47b3a14e7747` |
| `alumina-interface_bg.wasm.br` | 2,225,267 | `b36be6041cb032ed5415a5d953e664ca0c535c570d142f928411280c66742856` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and dedicated worker loaded from `127.0.0.1:8765`
in a fresh isolated Chromium profile. Runtime inspection found the unchanged
one-value `algwp1:` carrier with a 3,755-byte ALGW segment, a 407-byte ALGP
segment, and 8,332 total text characters.

The scrolled Control Graph view visibly rendered the matched falling-edge
trigger at local tick 3, the exact root-clock window 10–50 with trigger 30, 35
retained samples across seven probes on display clock 3, and all four analog
plus three Boolean series over local ticks 1–5. Probe metadata controls remained
visible directly above the plot.

The accepted 1,440-by-813 capture
`/tmp/alumina-probe-metadata-browser.png` is 228,735 bytes with SHA-256
`1e134f5e348c885bfc2b47ce49dbb809d72b311531c9e640cdfe57557e6e0462`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and background Google
registration errors; no Alumina application, device, or WLAN error appeared.

## Closed claims and licensing

This is exact host replay and presentation evidence. It does not allocate
device capture memory, negotiate telemetry bandwidth, arm a hardware trigger,
read a GPIO, deploy a graph, or grant firmware/output/safety authority. It is
not proof of physical clock synchronization or capture timing.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
