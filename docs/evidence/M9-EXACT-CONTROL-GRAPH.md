# M9 exact control-graph evidence

Date: 2026-08-12

## Scope and source

This checkpoint extends the bounded `HostExact` graph simulator with a small,
reviewed control palette and uses it to construct a visible multi-rate discrete
PID plus safety permit interlock. The implementation is `alumina-interface`
commit `e35f57b853f667092a4a04122075ea2824dde38c`, against `alumina-firmware`
`75af1ca1b78834dcc18f1e18e1a24eff32d2a856`. It changes no firmware source,
target image, board configuration, network configuration, or physical I/O.

The later [exact control-inspector checkpoint](M9-EXACT-CONTROL-INSPECTOR.md)
records the shared fallible UI fixture, label-dependent current graph/trace
identities, and optimized native/WASM rendering. The identities below remain
the historical identities of this implementation commit.

The interface continued to build against the current sibling workspace stack,
not a released CSGRS crate. Direct source identities at the final artifact
audit included CSGRS `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`,
Hypergraphics `31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve was a tested
development snapshot at `b1bd9008c505d7fffe49af454cb1c23db862672b` with tracked
diff SHA-256
`d8859f65895be9afebcbd53376741ac5c2c1f042a8511146901e6445f4c4b65e`.
That dirty dependency identity is recorded rather than represented as a clean
release pin.

## Fixed exact-control authority

`GraphSimulationRegistry` now admits nine behaviors: external Stream source,
the audited latest-at-or-before rate transition, Stream sink, exact add,
subtract, dimensionless scale, inclusive clamp, explicit unit delay, and an
exact-value permit gate. These are composable graph primitives rather than a
hidden PID implementation. Every binding still requires an exact audited node
kind/version and matching port, queue, dependency, parameter, clock, and state
contracts.

The unit delay exposes its prior value before current-tick combinational
evaluation and captures its next value only after the graph settles. A state
path may be a literal or a Stream carrying that literal on the declared state
clock, but storage remains one bounded literal; the Stream envelope and history
do not become implicit state. Every other control behavior is combinational.
Inputs must provide one exact same-clock sample at every evaluated tick.

Arithmetic uses `hyperreal::Rational`. Scale parameters include their
registered exact dimensionless unit scale, and every computed value is rebuilt
through the graph schema so its rational-magnitude policy remains authoritative.
The permit gate's false branch selects its declared exact safe parameter.

The simulation-registry encoding is now `ALSI` V2. Its identity includes the
complete unit/type registry and clock context in addition to analysis limits,
audited schemas, and implementation bindings. A regression proves that changing
the clock context changes the registry digest. Canonical `ALGT` remains V1 and
therefore continues to bind the graph plus whichever exact registry identity
was used to generate it; older V1-registry traces fail identity replay rather
than receiving a compatibility shim.

## Representative exact PID/interlock

The fixture samples setpoint, measurement, and permit at 50 Hz, then uses three
explicit source-first transitions to a 10 Hz control clock. The controller is
assembled from subtract, scale, add, two unit delays, clamp, and permit nodes.
Its proportional, integral, and derivative factors are exact percentage values
with unit scale `1/100`; the integral and derivative factors are explicitly
pre-discretized for the 10 Hz clock, so there is no hidden or floating-point
time step.

Across six control ticks, the integral prior-state trace is
`[0, 3, 5, 6, 6, 6]`, the clamped controller trace is
`[5, 5, 4, 2, 3, 3]`, and the fail-safe permit output is
`[5, 5, 4, 0, 0, 0]`. Reversing the caller's complete input vector produces an
identical simulation. Independent `ALGT` replay regenerates every entry.

| Canonical object | Identity |
| --- | --- |
| graph | `cd99124ff57d181830c71e0a79ed0d1f030e319f73e8bfe93569d41b2cb5a921` |
| `ALSI` V2 registry | `6bb6f814941b632ac5c9858fbbfe599fe8febb3a04b4dcc7bf4fbc8ac2f61537` |
| 7,836-byte `ALGT` trace | `9ad6e174717880b9c7e522c8f4b1cf69c905444dc1c19bd4c1518253762dddb9` |

The pre-existing 658-byte multi-rate trace now has SHA-256
`99677284550e7465541096c675ddd360416a3f3655653af3c96e6c6d96ffa2f4`
because it binds the strengthened V2 registry identity.

## Reproduced checks

Run from `alumina-interface` at the implementation commit and recorded sibling
snapshot:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 101 unit/integration tests pass: 8 application/coordinator, 32 client, 60
core, and the new exact-control integration fixture, plus the intentional
compile-fail rustdoc test. Native and WASM warnings-denied Clippy, strict
rustdoc, local-source/permissive-license audit, optimized Trunk build, WASM
validation, and compressed-artifact integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,260,699 | `f0f07539d390ed7984b3ca8b0861728071cf1175f1fec37b7f19520688917b10` |
| `alumina-interface_bg.wasm.gz` | 1,980,132 | `8c4755c1c31faffcac85640af9e0c55df26807abe0a2b416756e87cc169e809d` |
| `alumina-interface_bg.wasm.br` | 1,615,125 | `6cc33e26df282a4f1310222b2795c948b062d5a0d30762adfeb8af41770f583c` |

The 96,792-byte `Cargo.lock` has SHA-256
`789484967e2659c753722fab8ab5c21b6f2765195d95b17e7c7fa1056846989b`.

## Closed claims, licensing, and next gate

This is host simulation evidence only. The browser shell does not yet expose
the fixture as an editable front panel, and these new behaviors have no
Service/Realtime lowering, firmware opcode, resource claim, WCET, deadline,
physical output, or safety authority. It neither closes the M9 deployment and
capture exit gate nor qualifies motor control.

The bare MKS TinyBee V1.0 remained on its existing disconnected-load HIL image;
it was not reset, flashed, or used for this checkpoint. No workstation Wi-Fi
setting was changed. Live AP/HTTP load and SLogic16U3 capture remain postponed
until a separate Internet path permits the workstation Wi-Fi interface to be
dedicated to the Alumina AP without interrupting the development session.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted only the local CSGRS/Hyper/Alumina stacks and
the existing permissive native/WASM inventory. No GPL-family source, library,
tool output, or asset was copied, linked, or vendored.

The immediate browser-inspector slice is now recorded by the linked follow-up.
Remaining offline M9 work includes editable construction, component/front-panel
state, broader bounded probes, and reviewed lowering of selected control
primitives into fixed-memory Service/Realtime IR. Physical Wi-Fi/input timing
remains a separate retained-capture gate.
