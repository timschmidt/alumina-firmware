# M9 capability-derived catalog and diagnostic-probe evidence

Date: 2026-08-12

## Scope

This offline checkpoint turns the existing TinyBee graph-execution capability
facts into concrete editor nodes without treating the broader board inventory
as execution authority. It also adds canonical bounded diagnostic-probe
authoring for host plots. The reviewed interface commit is
`9459dc95cdaf5a8495996b7dbb71966a6db57df2` (`Add capability-derived graph
resources and probes`).

No live identity, configuration, capability, telemetry, or input was read. No
firmware package was installed or started. The connected bare MKS TinyBee V1.0
was not contacted, reset, erased, flashed, or driven.

## Capability and registry intersection

`GraphCapabilityNodeCatalog` accepts the complete bounded `ALMCAP02` bytes, one
nonzero device/capability/configuration target tuple, a reviewed
`GraphDeploymentRegistry`, and caller-owned byte/entry limits. It hashes and
canonically decodes the complete document and requires its digest to equal the
target capability identity. Transport/device authentication remains the
caller's responsibility; the catalog does not mistake a digest for an
authenticator.

V1 derives only the fixed `StableBooleanInput` operation. A concrete entry
exists only when all of these independent facts agree:

- the reviewed implementation and semantic schema select a Realtime stable
  Boolean input with one typed resource-handle parameter;
- the image advertises graph opcode 4 in Realtime with at least `Compiles`
  support;
- opcode and resource records have the same nonzero class and exactly
  `StableBooleanInput` access; and
- the concrete resource record itself has at least `Compiles` support.

Each resulting `GraphNodePrototype` retains the reviewed kind, ports, parameter
name/type, and schedule context. Its immutable handle already binds the exact
target `DeviceId`, capability/board-package digest, resource class, and
canonical four-byte typed selector. Creating a node still performs complete
`ALGW` structural validation; semantic, implementation, WCET/schedule, arena,
configuration, lowering, and firmware-replay gates remain independent.

The visible proof reconstructs the exact primary 8 MiB MKS TinyBee V1 package
through the existing bounded capability range API. Its 3,435-byte document has
SHA-256
`0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5`.
The derived editor catalog contains exactly four entries in canonical typed
resource order: GPIO22, GPIO32, GPIO33, and GPIO35. ADC, UART, timers, I2S
shifted outputs, storage, other GPIOs, and raw pin access remain unavailable
even though the broader board descriptor describes them.

The UI holds these entries in a separate Realtime target-I/O workspace with an
explicit 240 MHz reference cycle clock and derived 1 kHz input clock. It shows
the synthetic offline device (`54` repeated) and configuration (`43` repeated)
prefixes and has no deployment action. The HostExact PID/interlock workspace
keeps its distinct exact type/clock context and never acquires a physical
resource handle.

## Canonical bounded diagnostic probes

`ALGP` V1 binds presentation intent to one exact canonical `ALGW` digest. Each
probe record retains a monotonic stable identity, bounded stable name, exact
output node/port endpoint, resolved `GraphTypeId`, maximum host-retained value
count, and nonzero event-ordinal decimation stride. The first interactive
policy admits at most 256 probes, 1,000,000 values per probe, a 1,000,000-value
stride, and 2 MiB of canonical bytes.

Construction rejects zero/duplicate identities, duplicate names or endpoints,
non-output endpoints, changed exact types, zero/over-limit capture values, bad
monotonic cursors, and malformed names. Replay bounds outer bytes and embedded
limits, requires the external workspace identity, resolves every endpoint and
type, rejects all strict prefixes and trailing bytes, reconstructs through the
normal validator, and requires exact byte-for-byte re-encoding. Add, remove,
and workspace-rebind operations replace state only after complete candidate
validation and never reuse deleted IDs.

The reference PID/interlock sidecar selects error, integral-prior,
clamped-controller, and permit-gated-output series. It is 257 bytes with
SHA-256
`3bbd8ff29e118f3f0a37885adf13e263252ecc01148d50eb88058be1a1b42651`.
Adding/removing a probe changes only the sidecar and plotted series, never the
graph. A graph edit that removes or changes a bound output visibly detaches the
sidecar. `ALGP` grants no GPIO read, firmware telemetry, trigger, transport,
buffer, storage, timing, or deployment authority.

## Verification

The checkpoint reproduced from `alumina-interface`:

```console
cargo fmt -p alumina-interface -p alumina-interface-client \
  -p alumina-interface-core -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
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

All 143 executable tests pass: 21 application/coordinator tests, 32 client
tests, 89 core tests, and one cross-crate exact-control integration test, plus
the intentional compile-fail rustdoc test. Strict native and WASM Clippy, WASM
check, warnings-denied docs, the current-sibling-source/permissive-license
audit, optimized Trunk build, both WASM validators, and compressed-asset checks
pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface.js` | 91,813 | `3a077cb5f3183537933dff9f3f2a0f1f6538714a93bc5fe7878556b52c4ffcaf` |
| `alumina-interface_bg.wasm` | 4,949,251 | `3e6371281eed69d59bf5e72d96ac53c1a13096394a122b25aab8f2bbae716345` |
| `alumina-interface_bg.wasm.gz` | 2,244,422 | `9903f7accd7a26a2054aeb7521ec0a491f8088263cfa5fb3fdaed67c4a3abe8b` |
| `alumina-interface_bg.wasm.br` | 1,805,928 | `02f314c96747be23de5941d1d7e7355a93dd3620430e70bf5036e13211bb793b` |

The 96,851-byte lockfile has SHA-256
`83e84a0ccba91cc657e3259c0a4451937110bcd6661b9d3192ad27bdd9954b72`.
Only existing workspace packages became direct application dependencies; the
resolved package inventory did not gain a package.

The optimized bundle was served temporarily on `127.0.0.1` and rendered in
headless Chromium software WebGL at 1,440 by 2,400 pixels. Visual inspection
confirmed the offline device/configuration labels, four-entry catalog, closed
peripheral warning, separate empty target draft, four canonical probe rows, and
probe-selected exact trace. The 351,125-byte transient screenshot has SHA-256
`1a6f77d346d23261186c7d49b92fb7c6aebef4bc261cf601e28e55d69d468fd6`.
The loopback server was stopped after capture.

## Closed claims, hardware, network, and licensing

This checkpoint does not claim live capability discovery, physical input HIL,
measured graph timing, live telemetry, triggers, bandwidth/loss handling,
annotated-board correlation, output or motion opcodes, or control of any UART,
timer, ADC, raw GPIO, shifted output, storage device, or motor. The SLogic16U3
was not used. No motor, driver, process power, or analyzer input was connected.

Workstation Wi-Fi and NetworkManager were not changed. The TinyBee AP was not
joined. The only network activity created for verification was the temporary
loopback HTTP server used by local Chromium; it made no device request.

The implementation is independently authored under MIT. The source-policy
audit accepted the existing permissive Alumina/CSGRS/Hyper/native/WASM
inventory. No GPL-family source, library, asset, or generated output was
copied, linked, or vendored.

The next offline gate is a capability-led board/resource explorer that can
show the complete descriptive inventory separately from graph-admitted access,
followed by authenticated capability acquisition and live bounded telemetry
only when workstation networking can be arranged without losing the Codex
connection.
