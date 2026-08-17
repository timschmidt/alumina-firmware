# M9 canonical graph component and front-panel evidence

Date: 2026-08-12

## Scope and source

This checkpoint adds the first reusable component/front-panel authoring
boundary without changing `ALGR` V1, `ALGW` V1, firmware graph IR, or any
firmware source. The implementation is `alumina-interface` commit
`d326bd42dac1c608e1594f0f9069a0b575fdd2e9`, against preceding `alumina-firmware`
evidence commit `fb5f0cdeb2fa0fc8cf46084a2f1b8027edd4044b`.

The interface continued to compile against the checked-out workspace stack:
CSGRS `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`, Hypercurve
`bc58f471d8f123a2002189df1156b0b092027bf7`, Hypergraphics
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve had concurrent tracked
edits in `src/bezier_offset.rs` and `src/bezier_region.rs`; Hyperlimit retained
untracked fuzz artifacts/corpora and a local file. This work did not modify or
clean those sibling states.

## Canonical `ALGC` V1

`GraphComponentDocument` embeds one complete canonical `ALGW` and adds only a
stable connector pane and presentation bindings. Its first interactive policy
admits at most 24 MiB, 128 public inputs, 128 public outputs, 256 front-panel
items, and front-panel edges at or below 1,000,000 logical pixels. Embedded
limits are part of identity and cannot exceed caller admission. Stable IDs are
nonzero, collections sort canonically, and monotonic next-ID cursors retain an
explicit exhausted sentinel.

A public input maps to one declared internal graph input and is valid only when
no internal wire owns that target. A public output maps to one declared
internal output and may tap an internally used signal. Duplicate connector
names and endpoint aliases reject. Exact terminal types are resolved from the
embedded graph rather than copied into a second schema.

Front-panel rectangles use bounded integer logical pixels and cannot become a
graph literal, clock, machine coordinate, or firmware value. Three tagged
bindings exist: runtime public-input control, exact retained-parameter control,
and public-output indicator. Every binding resolves against the embedded
workspace and may occur only once. Parameter types and terminal types are
queried from their exact graph authority.

Untrusted replay bounds outer bytes and counts before allocation, independently
replays the embedded `ALGW`/`ALGR`, rebuilds every connector/panel invariant,
rejects trailing data, and requires byte-for-byte canonical re-encoding before
returning SHA-256 identity. Coverage includes connected or wrong-direction
inputs, unknown outputs/parameters, duplicate bindings, invalid rectangles and
cursors, caller-limit escalation, corruption, and an otherwise-valid alternate
record ordering.

`replace_workspace` constructs and validates a complete candidate before
mutation. A valid placement change advances component revision and identity;
deleting a bound output node rejects while preserving the prior component byte
for byte.

The initial 19-node `control.reference_pid` component is 4,099 bytes with
SHA-256
`20759fd476c435eca5318204c1048eed8156244f739bb2df5448a7f580d359d1`.
It embeds the existing 3,396-byte canonical `ALGW` unchanged.

## Visible workflow

The native/WASM editor constructs a component around the current reference
workspace with four public Stream outputs, six exact parameter controls (P/I/D
gains, clamp minimum/maximum, and safe output), and four exact replay
indicators. Controls use the existing bounded Hyperreal parser, typed parameter
replacement, canonical graph/workspace encoding, history, and persistence
path. No display float enters an exact value.

Every accepted workspace edit reconstructs and encodes the component. An edit
that removes a bound node remains a valid incomplete `ALGW` draft but visibly
detaches `ALGC`; undo reattaches it only after full validation. Output indicators
read the canonical reference trace only while its graph digest matches. They
show a detached state rather than relabeling stale samples.

The reviewed component metadata is reconstructed by this first UI fixture.
General component/panel authoring and `ALGC` file/browser persistence remain
open.

## Reproduced checks

Run from `alumina-interface` at the implementation checkpoint:

```console
cargo fmt -p alumina-interface -p alumina-interface-client \
  -p alumina-interface-core -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 127 executable tests pass: 19 application/coordinator tests, 32 client
tests, 75 core tests, and one cross-crate exact-control integration test, plus
the intentional compile-fail rustdoc test. Strict native and WASM Clippy, WASM
check, warnings-denied project documentation, local-source/permissive-license
audit, optimized Trunk build, both WASM validators, and compressed-artifact
checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface.js` | 91,813 | `e6a805566cd1b4c3eb9097b151019762046268b2807c4c510cd28696556cfb95` |
| `alumina-interface_bg.wasm` | 4,820,737 | `e7d1f5b465e5aadaa99157f5a5a8ef905ccbd4cef47aaac074a61ad42e04f3e5` |
| `alumina-interface_bg.wasm.gz` | 2,195,690 | `4f4d255db374ad475543d057910bb59575cef1cbc0ec60abe93919ab451f12fd` |
| `alumina-interface_bg.wasm.br` | 1,771,949 | `f00deb4f808d64bd99d121d76ebbb6826018b90347d32dc8da6ab8c71e24f720` |

The 96,806-byte `Cargo.lock` remains SHA-256
`5790fedc0da96498fad696e5627e6f1601ceb04dead8c5adb67d1efb3773cfc1`.
No package entered the resolved inventory.

The optimized bundle was served temporarily on `127.0.0.1:8765` and rendered
in headless Chromium software WebGL at 1,440 by 1,600 pixels. Visual inspection
confirmed all six exact controls, four tick-zero exact indicators, canonical
component identity/status, unchanged graph canvas, attached trace, and the
explicit no-deployment/output warning. The 249,747-byte transient screenshot
has SHA-256
`b0e44e60e2db05d846ebb195c1589546a91396256a10957d11dec6d514f3b0bb`.
The loopback server was stopped after capture.

## Closed claims, hardware, and licensing

`ALGC` is an authoring package only. It grants no node semantics,
implementation, hierarchy lowering, resource allocation, WCET, safety,
firmware, or physical-output authority. No arbitrary `ALGC`, `ALGW`, or `ALGR`
document is sent to or interpreted by firmware.

The bare MKS TinyBee V1.0 remained on its prior disconnected-load HIL image. It
was not reset, flashed, contacted, or associated with workstation Wi-Fi. No
motor, driver, process power, or SLogic16U3 input was connected. The temporary
browser check used loopback only and did not change NetworkManager or interfere
with Internet connectivity. Physical AP/HTTP and analyzer work remains
postponed.

The implementation is independently authored under MIT. No dependency was
added, and the source-policy audit accepted the existing permissive
Alumina/CSGRS/Hyper/native/WASM inventory. No GPL-family source, library, tool
output, or asset was copied, linked, or vendored.

The next offline hierarchy gate is component instantiation and deterministic
flattening with recursive dependency/cycle bounds. Capability-derived resource
nodes and broader probe/front-panel workflows remain subsequent M9 work.
