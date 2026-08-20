# M9 exact multi-rate probe-axis evidence

Date: 2026-08-20

This checkpoint replaces the mixed-signal plot's local integer-tick axis with
exact rational time on the simulation's shared root clock. Canonical probes on
different same-root local rates now align without conflating their local tick
integers, pointer motion selects an existing exact time instead of creating
one, and each visible series has explicit retained sample-and-hold semantics.

## Source boundary

- Interface implementation:
  `53837475453317d728c095fb3514c1cc46045f17`
  (`feat: align graph probes on exact root time`).
- The shared checkout was on `agent/zizmor-ci-hardening`; the implementation
  commit follows unrelated workflow-hardening commit `6d04994`. No branch was
  switched and no workflow-hardening source was changed by this slice.
- Preceding firmware repository checkpoint:
  `b2f5c819ef4bd8c07c735704eff2a16a9fb763fb`, whose only delta after the prior
  functional evidence commit `3c9f4c2f17e4eba41be3c3b4b9d03ac1969b9169`
  is unrelated workflow hardening.
- `alumina-firmware` remains the canonical firmware repository. The retired
  `aluminafw` path was not used.
- Firmware source, protocol, target images, board packages, and physical-device
  state are unchanged by this checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or external analyzer lead is involved.
- Workstation Wi-Fi configuration and the Alumina device AP were not touched.
  Browser qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies were compiled read-only. No sibling
  status, diff, reset, pin, format, stage, or edit was performed.

## Complete output provenance

ALGP already admitted exact graph output endpoints, including external stream
sources, but replay formerly selected only trace records tagged `NodeOutput`.
The projection and trigger resolver now select canonical trace entries by the
exact bound endpoint. A caller-owned source record tagged `ExternalInput` is
therefore observable and triggerable without rewriting or discarding that
provenance tag. Identity, endpoint, type, stride, retention, trigger-clock, and
aggregate-memory validation remain unchanged.

The core mixed-rate regression binds one probe to a 50 Hz source-clock output
and one to a 10 Hz control-clock output. After independent stride and trailing
retention, their local identities differ while exact time agrees:

| Series | Local samples | Exact root ticks |
| --- | --- | --- |
| source clock 2 | 15, 20, 25 | 30, 40, 50 |
| control clock 3 | 3, 4, 5 | 30, 40, 50 |

A separate external Boolean-source regression resolves the first falling edge
at clock-2 tick 20 and source sequence 320, retains local ticks 18–22, preserves
`ExternalInput` on all five projected entries, and maps them exactly to root
ticks 36–44.

## Exact root-time axis

Every nonempty projected Boolean or exact-rational probe now becomes a visible
series keyed by canonical probe ID, name, output endpoint, sample type, and unit
symbol. The former seven-reference-signal whitelist is retained only to give
the reference fixture stable probe/component semantics, colors, and bindings;
it no longer limits plotted ALGP endpoints. Generic probes receive a
deterministic color from their stable probe ID. Exact-rational series may share
an analog scale only when their exact sample type is identical; incompatible
unit/type overlays fail closed before painting.

`TraceTimeAxis` collects the exact rational root tick from every retained
sample plus the matched first/trigger/last window anchors. It then:

- orders and deduplicates the exact rationals;
- keeps the exact minimum, maximum, selectable times, and cursor;
- creates only a named one-way finite `f64` enclosure for egui placement;
- makes lossy fractions monotonic when distinct exact times collapse to the
  same display pixel; and
- bounds grid construction to at most five exact rational divisions.

Pointer coordinates never become a graph time. Hover position is compared only
against the bounded lossy positions of existing exact candidates; the selected
candidate's original `Rational` becomes the cursor. A tie chooses the earlier
exact candidate. Cursor reset uses the exact projected trigger root tick.

At one exact shared cursor time, each series reports its last retained sample
whose exact root tick is less than or equal to the cursor. Cursor labels include
the canonical probe name, exact value/unit, and original `clock:tick`, so slower
and faster sample-and-hold states remain explicit. The reference component's
output indicators use the same exact cursor over a separately projected,
trigger-disabled bounded reference history.

The application mixed-rate regression adds a real eighth ALGP probe at the raw
50 Hz exact source. The matched root window admits clock-2 ticks 5–25 (21
samples, root ticks 10–50) alongside 35 samples from the seven clock-3 control
probes. The resulting 56-sample plot reports source clocks 2 and 3 and retains
the new probe's canonical name, exact sample type, and `mm` unit. A synthetic
rational-axis regression independently proves exact `1/3`, `1/2`, and `2/3`
snapping, slower series hold behavior, deterministic mixed-clock labels, and
rejection of two different exact analog sample types.

## Reproduced checks

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

All 282 executable workspace tests pass: 59 application/coordinator, 82
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
| `alumina-interface_bg.wasm` | 6,383,018 | `acfc871e004f8375d48c3fe2650bf200fb02dc047a1ce1ab3f4d298028cba968` |
| `alumina-interface_bg.wasm.gz` | 2,823,225 | `867ee0c2498ef443074d102752f3752dfe574d849eb4fac375eb462e670a56ea` |
| `alumina-interface_bg.wasm.br` | 2,228,352 | `c88d9fde6a13b4b470c07d66306b5b2fdf389b35887fb41b3eff684e116f3d1c` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and dedicated worker loaded from `127.0.0.1:8765`
in a fresh isolated Chromium profile. Runtime inspection found the unchanged
one-value `algwp1:` carrier with a 3,755-byte ALGW segment, a 407-byte ALGP
segment, and 8,332 total text characters.

The scrolled Control Graph view visibly rendered exact root-axis labels 10, 20,
30, 40, and 50; the matched root trigger and cursor at 30; 35 retained reference
samples; and all four analog plus three canonical-name Boolean lanes. Cursor
readout retained `root tick 30` while every visible value reported its original
`c3:t3` local identity. The y-axis reported the schema-resolved `mm` unit
symbol.

The accepted 1,440-by-813 capture
`/tmp/alumina-probe-metadata-browser.png` is 234,925 bytes with SHA-256
`2253fc1a957b096bd357c86de7d6138ee6276d234b3259813d63d94b3d2decdc`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and background Google
registration errors; no Alumina application, device, or WLAN error appeared.

## Closed claims and licensing

This is exact host replay and presentation evidence. It does not allocate
device capture memory, negotiate telemetry bandwidth, arm a hardware trigger,
read a GPIO, deploy a graph, or grant firmware/output/safety authority. Root
time here is exact within one simulated shared clock tree; it is not proof of
physical multi-MCU clock synchronization or capture timing.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
