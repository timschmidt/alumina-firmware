# M9 authenticated dual-core graph deployment evidence

Date: 2026-08-12

## Scope and source

This checkpoint carries one browser-lowered fixed `ALGRIR01` package from an
immutable SD publication through authenticated Wi-Fi control, independent
core-0 and core-1 admission, explicit selection, and dual-core authorization.
It composes the lifecycle into the real Embassy service and realtime tasks on
all four current board images. It does not yet schedule the selected package's
fixed opcodes in those live tasks or claim a physical graph side effect.

The reviewed implementation commits are:

- `alumina-firmware`
  `cc216f259105df287cee5a6f52d9abf1b6f095b7`
  (`Deploy authenticated graph packages on both cores`); and
- `alumina-interface`
  `12121592af366e3fbb80f5801cbebb7054f29754`
  (`Drive graph package deployment from the browser`).

The interface uses the checked-out sibling CSGRS/Hyper stack, not the old
published CSGRS release. The directly relevant clean HEADs were CSGRS
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0`, Hypergraphics
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperpath
`e65506279d3cba99a23cf98bbd17be44126ec14d`, Hyperreal
`f09c147b0352884f8efe88e875c37d8f0f439ba5`, Hyperlimit
`b0418bddff50183fa782e5caa6da6974a2b969a1`, and Hypersolve
`d8bfa6b113020d1588ce2b0e549235d1bb9bc205`. Hypercurve was the tested dirty
development snapshot at HEAD `f4a58c23065cbb61fd6046ddb5f141684db6bf30`
with tracked-diff SHA-256
`24c9855caf06fe1b76082f500017ff5d2266a9212a35ee1dffdf34f0273245cd`.
Hyperphysics remained at HEAD `a8002f286914356d3ebc5f491695f39f6f1c029e`
with tracked-diff SHA-256
`99766a9ad8ccb54b8eac523fcc904db4d2df3aa5eeb4c10f5bcb781d57ad9667`.
Those existing sibling changes were not modified by this work.

## Stable target and stored-object identity

`DeviceId::from_esp_base_mac` maps the six factory base-MAC bytes into the
16-byte public namespace `ALUM-ESP1:` plus the MAC. This is stable across
firmware updates and is not a secret or an authentication credential. Firmware
derives it once before splitting board resources, gives the same value to both
cores, and returns its 32 lowercase hexadecimal digits from
`GET /api/v1/identity` beside the board capability digest.

Storage object kind `7` is now `DeployedGraph`. A graph object is always exactly
4,096 bytes and inert after publication. The browser client implements the
existing resumable `UploadSource` contract: it hashes every exact chunk,
constructs the canonical ordered manifest, and refuses zero or unrepresentable
chunk geometry before I/O. Its fixture publishes the package as 24 chunks of
173 bytes; replay reconstructs all 4,096 bytes with no trailing or missing
data.

Two SHA-256 identities are deliberately distinct:

- the storage content digest covers all 4,096 bytes, including the embedded
  digest; and
- the package digest covers the canonical padded prefix before the final
  32-byte embedded field.

Neither is substituted for the other. The publication also binds the
graph-specific implementation-registry digest that the browser compiler
derived from the complete audited semantics.

## Authenticated and inter-core wire contracts

Protocol V1 adds frame family `Graph` (`14`) and four assigned operations:
`GraphGet` `0x0e01`, `GraphInstall` `0x0e02`, `GraphActivate` `0x0e03`, and
`GraphClear` `0x0e04`. They use the existing origin-bound HMAC control route;
there is no unauthenticated mutating alias or compatibility decoder.

`GraphInstall` carries one canonical 168-byte `ALGRPQ01` body:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | magic |
| 8 | 2 | exact graph-IR version `1` |
| 10 | 6 | reserved zero |
| 16 | 8 | nonzero boot-local transaction ID |
| 24 | 80 | exact `PublishedObject` identity |
| 104 | 32 | embedded package digest |
| 136 | 32 | implementation-registry digest |

Activate and clear use an 88-byte `ALGRPS01` selection containing version,
transaction, storage content digest, and package digest. Decode always validates
and re-encodes the complete body.

The fixed inter-core command has a 128-byte prefix and at most 208 initialized
data bytes, exactly fitting the existing 336-byte runtime command payload. Its
`Begin`, contiguous `Data`, `Finish`, `Activate`, `Authorize`, `Clear`, and
`Abort` actions repeat transaction, both package identities, and implementation
identity. Offset and initialized length are checked; unused bytes are zero.

Core 1 reports one canonical 128-byte `ALGR` lifecycle observation after every
command and periodically. Core 0 combines it with service-reader phase,
independent byte/chunk progress, fault family, candidate identity, and active
identity in one canonical 256-byte `ALGS` response. The browser state machine
accepts only its selected transaction and digests, reconciles ambiguous network
I/O without inventing progress, polls read-only status between mutations, and
requires both cores to report the exact authorized active package. A periodic
report from the preceding transaction cannot reject or replace the new
transaction.

## Independent admission and live ownership

Core 0 alone owns Wi-Fi, HTTP authentication, the SD backend, and
`ServiceGraphValidation`. It opens only the exact typed published object through
`PublishedReader`, re-verifies every cache chunk, reconstructs a complete
4,096-byte local copy, decodes it, checks device/capability/configuration/
implementation authority, derives arena use, and emits at most one inter-core
command per cooperative service step.

Core 1 owns `RealtimeGraphDeployment` with distinct 4,096-byte staging and
active arrays. Receiving or rejecting a replacement cannot mutate an already
authorized active image. `Finish` independently hashes, decodes, checks all four
authorities, and enforces the concrete arena limits. `Activate` selects but
does not expose package bytes; only the later exact `Authorize` command makes
`authorized_package_bytes` visible. Clear zeroes active storage. Abort zeroes
staging and restores the retained active observation or the canonical empty
state.

The first image limits are 2 KiB each of Service and Realtime node state, 4 KiB
each of Service and Realtime local channels, and 4 KiB of bridge storage. They
are enforced on both cores but are not yet capability-published. The selected
package itself does not allocate these executor arenas yet.

Configuration, cached-job, graph, and external storage mutation now serialize:
an active or in-flight graph blocks configuration replacement and job
admission; an in-flight reader blocks unrelated storage mutation; and graph
installation requires current safe/idle mutation authority and one nonzero
configuration already authorized by both cores. Core 1 independently rejects
graph mutation while a job is active, work remains queued, or safety is not
quiescent. A selected graph also rejects job/configuration commands.

The fixed images increased static internal-memory ownership. To restore the
classic ESP32 link margin, the secondary general heap reservation changed from
36 KiB to 32 KiB; the separate 64 KiB reclaimed radio/runtime heap remains.
This is a link-time allocation decision, not a measured heap low-water result.

## Simulated replay and failure coverage

Portable tests execute the real provisioned-cache and graph deployment types.
One fixture uploads the exact package to a simulated raw SD cache in 24
173-byte chunks, atomically publishes it, then has core 0 and core 1
independently reconstruct the same identity through 26 commands: Begin, 24
Data, and Finish. Separate tests cover:

- complete transfer, activation, authorization, and byte-for-byte active replay;
- immutable old active bytes throughout replacement staging;
- storage-content, package, device, capability, configuration,
  implementation, arena-capacity, offset, length, and action failures;
- a complete 4,096-byte Receiving observation before Finish;
- fail-closed rejection followed by exact Abort recovery; and
- browser chunk manifests, lifecycle phases, lost-response retry, malformed or
  foreign identity, and prior-transaction telemetry.

The portable `FixedGraphRuntime` executor tests from the prior checkpoint still
pass against the same package. The live firmware tasks do not yet instantiate,
split, prime, or run that executor; this checkpoint proves selected package
ownership and lifecycle, not live graph execution.

## Reproduced checks

At the firmware implementation commit:

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

All 353 default-member portable firmware/board/driver/xtask tests and doc-test
targets pass. Warnings-denied Clippy and warnings-denied default-member rustdoc
pass. A whole-workspace host command is not the portable gate because the
ESP-HAL firmware package deliberately rejects x86_64; one exploratory
whole-workspace invocation stopped at that upstream target guard before any
project test ran.

All four current release images link from the live graph composition:

| Linked target ELF | Bytes | SHA-256 |
| --- | ---: | --- |
| TinyBee V1.0, 8 MiB primary package | 9,933,696 | `65ad66eb246a06bbc504e4150e585a089e9e15d60be37907ea2a58f900e89e32` |
| TinyBee V1.0, 4 MiB opportunistic package | 9,933,944 | `61d0bb6802864fbb59ecf9e4f4c3ec3fdac6d9c4846424d9615e94694bc38de6` |
| T-Deck Pro | 9,807,240 | `69374d8adf276a9f0f608d061b2c3bec45561391f6bc4a41b80b7e0e6107b3c2` |
| MKS ESP32 FOC V1.0 | 9,419,160 | `16123871015df15121218195f7149864b3e8f9952b54bbf36941822889c18c90` |

These are ELF file sizes, not flash payload sizes. Existing board-package and
image-fit gates remain authoritative; an ELF file length is not compared with
the 4 MiB or 8 MiB flash capacity. Firmware `Cargo.lock` is SHA-256
`5e8fa5c4549e581ddcdf7e18b8d758bbcd953c55e6f7f33d56c2a9aa4dfaef3b`.

At the interface implementation commit and tested sibling snapshot:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 95 native tests pass: 8 application/coordinator, 29 headless client, and 58
exact core tests, plus the intentional compile-fail documentation test.
Native/WASM warnings-denied Clippy, warnings-denied rustdoc, local sibling-source
and permissive-license audit, optimized Trunk build, WASM validation, and both
compression integrity checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,260,593 | `a5cfbaa95e6345cd1bc58a2ce58b3b877c0acc0dddf043ce93fd965678e40b7a` |
| `alumina-interface_bg.wasm.gz` | 1,980,001 | `5e3aa068f4e0f2128094917b3e870855745a88e8be11ff5f722fc31dbb0a1479` |
| `alumina-interface_bg.wasm.br` | 1,614,794 | `0dfed2fa89777f193cb2ca6bd4609921fd48110774bd08eba7009125609a987d` |
| `alumina-interface.js` | 89,162 | `d529fb7741708a8c86d1aa3e97d5b19bee2a14ca6ea2891d6fdb378155ef6aab` |
| `index.html` | 1,295 | `9629d2b3d76ec5ea6d406a9fd17a93b13f884d786f4588266a4333d8f555d2f2` |
| interface `Cargo.lock` | 96,554 | `f7b0bf4bb9c54ecc536ad697513bde6a3c0f037058eebc027f9d6a22fd0a79ae` |

## Closed claims, hardware, and licensing

Active graph selection is boot-ephemeral. Firmware does not yet journal the
selector, recover it after reset, or instantiate the selected package in a live
`FixedGraphRuntime`. There is no graph start/stop epoch, Service executor task,
Realtime executor release, resource-bearing opcode, capability-published arena
limit, measured WCET/deadline, heap/stack watermark, Wi-Fi HIL transcript, or
physical graph operation. The headless browser/WASM delivery API exists, but
the visible graph editor does not yet drive it.

The connected bare 8 MiB MKS TinyBee V1.0 was not opened, read, reset, flashed,
or otherwise touched. No motor or motor power was connected. The 8 MiB package
remains primary and the 4 MiB package opportunistic. The SLogic16U3 remained
disconnected. No MKS ESP32 FOC hardware was available. No physical I/O, timing,
motion, synchronization, FOC, safety, heap, or RF qualification is claimed.

All new implementation is independently authored under `MIT OR Apache-2.0` in
firmware and MIT in the interface. The lockfile changes add only existing local
Alumina dependencies and the already-locked permissive `embassy-futures` test
helper; no new third-party package entered either lock. Firmware `cargo-deny`
was not installed locally, so no local deny result is claimed; CI retains its
bans/licenses/sources gate. The interface native/WASM source-policy and license
inventories pass. No GPL-family implementation source, asset, or dependency was
added or consulted.
