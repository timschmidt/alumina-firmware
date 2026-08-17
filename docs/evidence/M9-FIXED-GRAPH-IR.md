# M9 fixed Service/Realtime graph-IR evidence

Date: 2026-08-12

## Scope and source

This checkpoint establishes the first complete browser-compiler-to-portable-
firmware boundary for a deliberately tiny deployed graph subset. It adds a
fixed `no_std` package and independent decoder in `alumina-firmware`, then lowers an
audited structural graph into those exact bytes in `alumina-interface`.

The two reviewed commits are:

- `alumina-firmware`
  `a5a56bd7e34d8f4948f058e1f2fec7cf3ddcf906` (`Define fixed deployed graph IR`);
  and
- `alumina-interface`
  `8d5527cf50a01b5c51f4d3bd1b22f2be041b5ee6`
  (`Compile fixed Service and realtime graph IR`).

The interface qualification used the current sibling CSGRS/Hyper working
trees, never the old published CSGRS release. Hypercurve HEAD was
`dc7aff02fd483fb532765c7e539cfeeddba7d57b` with tracked-diff SHA-256
`3c5765f7c7c7d07935a3aa0a86e95e66597828c6846efc947758e29cdd6e1d9e`.
Hyperphysics HEAD was `a8002f286914356d3ebc5f491695f39f6f1c029e` with tracked-diff SHA-256
`99766a9ad8ccb54b8eac523fcc904db4d2df3aa5eeb4c10f5bcb781d57ad9667`.
Both diff identities were identical immediately before and after the repeated
native, WASM, documentation, and optimized-bundle qualification. They define a
tested dirty development snapshot, not a clean release pin.

## Portable package and independent admission

`alumina-graph-ir` owns one exact 4,096-byte `ALGRIR01` V1 package. It has a
256-byte header, fixed 48-byte node records, fixed 32-byte channel records,
required zero padding, and a final SHA-256 digest over bytes `0..4064`. The
header binds the exact target device, source-graph digest, complete reviewed
implementation digest, capability digest, active configuration digest, both
domain schedules, and every fixed state/channel/bridge total.

The format admits at most 32 nodes, 64 channels, and 4,096 queue items. Its
only value carrier is a canonical Boolean Stream item: five typed-value bytes,
an eight-byte source-clock tick, and an eight-byte monotonic sequence, or 21
bytes total. Retained Boolean latest-state is exactly five bytes.

V1 has only three fixed opcodes:

- a Service Boolean Stream constant;
- a Realtime Boolean latest-at-or-before transition; and
- a Realtime Boolean Stream sink with no modeled side effect.

Queues are owned by Service, Realtime, or the one-way Service-to-Realtime
bridge. Every queue consumed by Realtime must fault on full. The decoder
independently checks exact length, magic/version/flags, all nonzero identities,
reserved bytes, padding, digest, record semantics, topological source/target
order, unique consuming inputs, connected producers, schedule/node agreement,
checked `WCET + executor reserve <= period`, contiguous per-owner arena
offsets, and all header totals. It performs no heap allocation.

Encoding calls the same independent decoder used for untrusted input. Tests
reject ordinary digest corruption, nonzero padding/reserved bytes, semantic
tampering followed by a correctly recomputed digest, all 4,095 strict prefixes,
one trailing byte, invalid topology/schedule/state, and lossy realtime queues.
The portable codec fixture has canonical package SHA-256:

```text
09ba7f443cb6acbd82c436943653fb55ce2d20992f763632f859e4f05fac5876
```

## Browser-side deployment compiler

`GraphDeploymentRegistry` is separate from both the saved `ALGR` document and
the audited semantic registry. It binds an exact node kind/version to one fixed
opcode, execution domain, schedule clock, and nonzero WCET. Construction
canonicalizes binding order and rejects unknown, duplicate, or contradictory
implementations.

The `ALDI` V1 implementation digest commits to the source graph, deployment
limits, executor reserve, analysis limits, every audited schema fact, every
dependency/rate/state contract, and every implementation binding. A test
broadens one audited domain without changing the structural `ALGR` digest and
proves that both implementation and deployed-package identities change.

Lowering first reruns graph analysis, requires every structural node to have a
fixed implementation, and requires every node to target one exact nonzero
device. `HostExact`, foreign-device, unsupported-domain, explicit-state,
Event/synchronous-channel, Realtime-to-Service, and lossy realtime cases are
rejected. Each active domain must derive one positive integer device-cycle
period from the target's `DeviceCycle` root. Nodes and target-owned channels
are deterministically ordered; state and queue offsets are allocated in fixed
Service, Realtime, and bridge arenas under checked policy limits. The emitted
package is immediately decoded and replayed by `alumina-graph-ir`.

The representative compiler fixture uses a 1 MHz device root, a 1,000-cycle
Service period, and a 2,000-cycle Realtime period. Service WCET is 20 cycles;
Realtime WCET is 60 cycles; each executor reserve is 100 cycles. One
two-element bridge occupies 42 bytes, one Realtime queue occupies 21 bytes,
and retained state occupies five bytes, for 68 fixed runtime bytes. The package
has canonical SHA-256:

```text
802d6a2f9b8d2958055532aecdec7b6dbed602c44fb3f80a1755d8f8412aca67
```

The codec and compiler fixtures intentionally use different synthetic
identity fields, so their complete package digests are expected to differ.

## Reproduced checks

At the firmware commit:

```console
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
```

All 337 portable firmware/board/xtask unit tests and all doc-test targets pass;
the new package contributes five focused tests. All portable targets pass
warnings-denied Clippy. `cargo-deny` is not installed in this local environment,
so no local deny result is claimed; the repository CI retains its bans,
licenses, and sources check.

At the interface commit and stable sibling snapshot:

```console
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 89 native tests pass: 8 application/coordinator, 24 client, and 57 exact
core tests, plus the intentional compile-fail documentation test. The five new
compiler tests cover canonical lowering/replay and golden facts, registry-order
identity, complete semantic identity binding, period/WCET/device/arena
rejection, and mandatory fixed opcode binding. Native and WASM warnings-denied
Clippy, warnings-denied rustdoc, the local-source/permissive-license audit,
optimized Trunk build, WASM validation, and compression integrity all pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,263,450 | `1b7b75743551d39e8f29c969daa84dbd552ba7e3f122ce08354edec71687b4ba` |
| `alumina-interface_bg.wasm.gz` | 1,981,677 | `75ae9047e600548dda5beeea5857467ea44bb9665efd798e9abbedc9b5609136` |
| `alumina-interface_bg.wasm.br` | 1,615,952 | `bbe1fdb6efb0525f0b66ab173aced6f7d3e1eca6ccf8bd0a8827614c757dde19` |
| `alumina-interface.js` | 89,162 | `eae4d73ce18c24dea9661456262a13579704efb51c6fe0951ea42b466ab69399` |
| `index.html` | 1,295 | `649e79efcc97bdb96ad4cd3abe9888c874d90243d5d56dc9f086700e5ab280f5` |
| interface `Cargo.lock` | 96,411 | `7662eb1de891f63b7afc075aa3f86304b3f86dc5593aec548882325f771d588d` |

## Closed claims, hardware, and licensing

There is still no authenticated firmware install route, active/candidate
package lifecycle, preallocated arena owner, core-0/core-1 bridge, executor,
deadline monitor, uninstall/rollback path, resource opcode, telemetry, or
physical side effect. The package's WCET values and reserve are declared static
budgets, not measured target timing. Arbitrary graph interpretation remains
absent from firmware.

No target firmware binary changed in the interface commit. The connected bare
8 MiB MKS TinyBee V1.0 was not read, reset, flashed, or otherwise touched; no
motor or motor power was connected. The 8 MiB board remains the primary TinyBee
target and the separately identified 4 MiB package remains opportunistic. The
SLogic16U3 remained disconnected, and no MKS ESP32 FOC hardware was available.
No physical I/O, timing, motion, synchronization, FOC, or safety claim is made.

The new firmware crate is independently authored under `MIT OR Apache-2.0` and
depends only on the existing local `alumina-protocol` crate and permissively
licensed `sha2`. The interface source audit now requires `alumina-graph-ir` to
resolve from the same sibling `alumina-firmware` checkout and confirms the current
native/WASM inventory. No GPL-family source, asset, or dependency was added or
consulted.
