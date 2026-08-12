# M9 fixed split-core graph-runtime evidence

Date: 2026-08-12

## Scope and source

This checkpoint executes the first browser-lowered `ALGRIR01` package in a
portable, allocation-free firmware runtime. It closes the in-memory contract
for exact package admission, compile-time arena capacity, source-first start
priming, split Service/Realtime ownership, fixed Boolean opcodes, and shared
fail-stop behavior. It does not add a live firmware install route or physical
resource operation.

The reviewed commits are:

- `aluminafw`
  `d45939c6f20fd5244e46d7a9aaaa528614804095`
  (`Execute fixed graph IR in preallocated runtime`); and
- `alumina-interface`
  `3e1946de8595a499240915b9abc3646e6a80dd2a`
  (`Replay deployed graph in firmware runtime`).

The interface qualification used the current sibling CSGRS/Hyper working
trees, never the old published CSGRS release. Hypercurve HEAD was
`dc7aff02fd483fb532765c7e539cfeeddba7d57b` with tracked-diff SHA-256
`c7f6c3c567b05646f1b1582b75ce42239539f75004c6721d4f5e6cd848b273b8`.
Hyperphysics HEAD was `a8002f286914356d3ebc5f491695f39f6f1c029e` with tracked-diff SHA-256
`99766a9ad8ccb54b8eac523fcc904db4d2df3aa5eeb4c10f5bcb781d57ad9667`.
Both identities were unchanged immediately before and after the complete
native/WASM/documentation/bundle run. This is a tested dirty development
snapshot, not a clean release pin.

## Canonical runtime item

The 21-byte Boolean Stream item is now independently defined by
`alumina-graph-ir`, rather than left as an unexplained five-byte value prefix:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 4 | little-endian deployment-local Boolean tag `1` |
| 4 | 1 | canonical Boolean `0` or `1` |
| 5 | 8 | source node's zero-based schedule tick |
| 13 | 8 | monotonic sequence; fixed V1 opcodes emit the schedule tick |

The deployment tag is deliberately not an arbitrary `ALGR` document-local type
ID. The browser's `ALDI` identity binds the complete source schema and reviewed
mapping; firmware only handles this one fixed runtime representation. The same
five-byte tag/value representation is retained by `BooleanLatest`. Exact-width,
wrong-tag, and noncanonical-Boolean tests pass. Already admitted packages also
provide bounded O(1) node/channel record access without allocation.

## Transactional static admission

`FixedGraphRuntime<SERVICE_STATE, REALTIME_STATE, SERVICE_CHANNELS,
REALTIME_CHANNELS, BRIDGE_CHANNELS>` owns:

- one exact decoded 4 KiB package;
- separate compile-time Service and Realtime node-state arrays;
- separate compile-time Service and Realtime local queue arrays;
- one compile-time Service-to-Realtime byte arena behind the existing
  cross-core critical-section mechanism;
- fixed queue cursors, input/fanout metadata, and Realtime initialization bits;
  and
- one atomic first-cause fault mailbox.

`install` is allowed only from `Empty` while the fixed safety/job owner permits
mutation. It first copies and independently decodes all package bytes into a
local candidate. Before changing runtime state it requires the caller's
nonzero expected package digest and exact device, capability, active
configuration, and implementation identities. It reconstructs bytes for each
owner and proves that every requirement fits its concrete const-generic array.
Wrong length/digest/identity/capacity or a repeated install leaves the runtime
unmodified.

The install report distinguishes selected state/queue payload bytes from the
complete `size_of::<Self>()` footprint. The latter includes package, cursor,
adjacency, bridge-lock, fault-mailbox, and executor metadata; the representative
68-byte payload is not misreported as the complete runtime cost.

## Source-first start and split ownership

Start preparation is separately safety-gated and fixes one shared
`DeviceCycle` epoch. Before any Realtime endpoint can exist, it runs Service
release tick zero and fills the empty bridge with each constant's initial
sample. This realizes the audited source-first initial condition without
inventing `false` or adding an undeclared validity byte to five-byte latest
state.

`split` can occur only once after successful preparation. Safe Rust then gives
core 0 unique mutable access to Service state/local queues and core 1 unique
mutable access to Realtime state/local queues. Both endpoint types are `Send`
for transfer into their pinned-core start closures. Only immutable package
metadata, the bounded bridge mutex, and the first-cause fault mailbox are
shared. The runtime contains no allocator, arbitrary graph-schema decoder,
trait object, recursion, or user code.

Each endpoint accepts only its exact next release cycle. It computes the next
cycle/tick before execution and advances only after a complete release. Service
constants emit canonical tick/sequence items. Realtime latest nodes consume all
source items whose checked `start + source_tick * source_period` is due, retain
the newest canonical Boolean, reuse it when the target runs faster, and emit
one target-tick item. Sinks drain every due item with no modeled side effect.

All queues in the executable V1 subset are fault-on-full. Queue full or corrupt
bytes/cursors, sequence disagreement, missing initial value, wrong release
cycle, arithmetic failure, runtime-shape disagreement, absent domain, or absent
safety authority publishes the first fault atomically. A racing second core
cannot overwrite its code/detail pair, and every later release on either core
stops at the same observation. A release already in progress is bounded but is
not claimed to be asynchronously preempted; future resource commits must still
recheck fixed safety immediately at their physical boundary.

## Direct compiler-to-runtime replay

The native interface test no longer stops at decoding its own package. It
lowers the representative graph, then passes those exact bytes directly to the
sibling firmware runtime using the independent target and implementation
identities.

The package remains SHA-256:

```text
802d6a2f9b8d2958055532aecdec7b6dbed602c44fb3f80a1755d8f8412aca67
```

For a 1 MHz device root it selects a 1,000-cycle Service period and 2,000-cycle
Realtime period. Installation reports five state bytes, 42 bridge bytes, and
21 Realtime-channel bytes. Preparation emits one Service tick-zero item. The
first Realtime release consumes the bridge item and its same-release local item;
after two Service releases, the next Realtime release consumes two due bridge
items and its local item. Both sinks observe `true`, all sequence/tick facts are
canonical, and neither endpoint reports a fault.

Separate firmware tests prove retained-latest behavior when Realtime runs twice
as fast, bridge overflow when Service outruns a two-item queue, corrupted type
bytes, wrong release cycle with exact expected/received evidence, missing
safety authority, cross-core fault visibility, and endpoint `Send` ownership.

## Reproduced checks

At the firmware commit:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
git diff --check
```

All 347 portable firmware/board/xtask unit tests and all portable doc-test
targets pass. The new graph boundary contributes one deployed-value codec test,
eight graph-runtime tests, and one first-cause mailbox test. Full portable
warnings-denied Clippy and default-member warnings-denied rustdoc pass. A
whole-workspace host rustdoc command is intentionally not the documentation
boundary because ESP-HAL firmware/examples reject a host target.

All four release target commands link. Their artifacts prove that the new local
dependency and generic runtime source remain compatible with both ESP32
families; because no board composition instantiates the graph runtime, they do
not prove its target memory footprint, schedule, or behavior.

| Linked target ELF | Bytes | SHA-256 |
| --- | ---: | --- |
| TinyBee V1.0, 8 MiB primary package | 9,615,856 | `73a1b792f557dfdd0ceaa759fc74e844a7fdf82f77c3d41261f0a89897aa8b73` |
| TinyBee V1.0, 4 MiB opportunistic package | 9,616,308 | `1a48961b8f0353594120165ce969ce01176e2651a96570a4a830f03afaad527f` |
| T-Deck Pro | 9,484,072 | `701d984a3bd75fcd2d0d3c667614e4288699192a7f331801c3b039d137503465` |
| MKS ESP32 FOC V1.0 | 9,111,832 | `fe3fec74cb9d735652ccee380de8f9f94dd6d8ea7c259fd21580eefedda81ebd` |

These are ELF file sizes, not flash payload sizes. Existing board-package/image
fit gates remain authoritative; no conclusion is drawn by comparing an ELF's
file length to 4 MiB or 8 MiB flash capacity.

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

All 90 native tests pass: 8 application/coordinator, 24 client, and 58 exact
core tests, plus the intentional compile-fail documentation test. Native/WASM
warnings-denied Clippy, warnings-denied rustdoc, local-source/permissive-license
audit, optimized Trunk build, WASM validation, and compression integrity pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,260,162 | `dc90ebdab9d8dc0de82f6a52763481da34f5b44be68e3892baa1622118446358` |
| `alumina-interface_bg.wasm.gz` | 1,979,809 | `ead5379089fab0ac0ce7a5e86808d35ad6c47a9cad858be92fab76a75a2b7617` |
| `alumina-interface_bg.wasm.br` | 1,614,455 | `bf3c5303b05fd71aa7eb4bc0053c03f6eb1ace5814675ff4f92b6a009781d671` |
| `alumina-interface.js` | 89,162 | `d9ea0265a6ff69baa94393addd4e41642bac840e67eea388c2dfcb1c084528f8` |
| `index.html` | 1,295 | `d58f4034c6a02a11ba5637e9f61b8d9c44000901f57510704e5f00dee3cc7923` |
| interface `Cargo.lock` | 96,473 | `db63373ddfa61a7f7af2580e506da73db018539a5c1f516d439d408fc44921fa` |

## Closed claims, hardware, and licensing

No authenticated HTTP/native operation uploads, installs, starts, queries, or
removes graph work. No SD publication type, streaming core-1 validator,
active/candidate durable lifecycle, live Embassy task, target static allocation,
deadline measurement, resource claim/opcode, telemetry record, or physical side
effect exists. Declared node WCET and executor reserve remain browser-reviewed
static budgets, not measured ESP32 timing. The linked board images do not call
the executor.

The connected bare 8 MiB MKS TinyBee V1.0 was not read, reset, flashed, or
otherwise touched; no motor or motor power was connected. The 8 MiB package
remains primary and the 4 MiB package opportunistic. The SLogic16U3 remained
disconnected, and no MKS ESP32 FOC hardware was available. No physical I/O,
timing, motion, synchronization, FOC, or safety qualification is claimed.

All implementation is independently authored under `MIT OR Apache-2.0` in
firmware and MIT in the interface. The runtime adds only the existing local
`alumina-graph-ir` dependency; the native interface-only replay uses the local
runtime and permissively licensed `critical-section`. Firmware `cargo-deny` is
not installed locally, so no local deny result is claimed; CI retains its bans,
licenses, and sources check. The interface source audit passes for native and
WASM inventories. No GPL-family source, asset, or dependency was added or
consulted.
