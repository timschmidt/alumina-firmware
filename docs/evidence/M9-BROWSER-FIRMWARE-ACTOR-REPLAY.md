# M9 browser firmware-actor replay evidence

Date: 2026-08-21

Checkpoint state: lowered Realtime graph bytes can now be replayed offline from
native or browser/WASM code through the same portable fixed-memory actor types
used by firmware. The public boundary is explicitly bounded, preserves actual
resource-provider call order and first-cause execution faults, and produces a
canonical evidence digest. The visible TinyBee target-I/O draft accepts a
candidate only after the exact lowered package passes both a four-release truth
table and an ordered unavailable-resource replay. This grants no device,
network, deployment, resource, safety, or output authority.

## Exact source boundary

- Interface implementation:
  `34a9accc39c2a90f198bf7d77e10a524d6ce0b79`
  (`feat: replay lowered graphs in firmware actors`).
- Preceding interface checkpoint:
  `5a5b2d132e34ec6b22f5691e82d34b771f6a250f`.
- Firmware actor/opcode implementation:
  `1e4a0fe43a08af5117d398bb3ee1af5c963b07bc`
  (`feat: execute paired graph resources`).
- Preceding firmware evidence checkpoint:
  `9c3bd224f232b1e45bdc0bab74a6df9c5ba1b5fe`.
- Both repositories remained on `agent/zizmor-ci-hardening`. The interface
  repository was clean immediately after its implementation commit. No
  firmware source, manifest, or lockfile changed in this implementation slice.
- `alumina-firmware` is the canonical firmware repository. The retired
  `aluminafw` path was not used.
- The interface compiled against the live local `csgrs`, `hypercurve`,
  `hypergraphics`, and wider Hyper workspace paths read-only. Hypercurve was
  known to be undergoing concurrent development. No sibling status, diff,
  reset, pin, format, stage, commit, or edit was performed. The hashes below
  are point-in-time evidence against that live stack, not an old published
  CSGRS release.

## Public replay boundary

`alumina-interface-core::graph::replay_realtime_graph_deployment` accepts:

- one already lowered `GraphDeploymentReport` and its exact target identities;
- the firmware image's static opcode and typed resource palettes;
- nonzero transaction/run/start identities;
- caller-owned exact `DeviceCycle`, safety-admission, and typed Boolean
  resource-state frames; and
- an independent replay policy below fixed absolute ceilings.

The boundary rejects Service work and requires at least one Realtime node. An
empty Service actor is still installed and prepared independently so the
production actor handshake is reproduced rather than bypassed. The package is
installed into distinct fixed actors with these host/WASM arenas:

| Fixed replay storage | Bytes |
| --- | ---: |
| Service state | 2,048 |
| Realtime state | 2,048 |
| Service local channels | 4,096 |
| Realtime local channels | 4,096 |
| cross-owner bridge | 4,096 |

Hard policy admits at most 4,096 releases, 256 distinct resource samples per
release, and 64 actual provider calls per release (`2 * MAX_GRAPH_IR_NODES`).
The interactive policy narrows those to 1,024 releases, 128 samples, and 64
calls. Zero or oversized policies, empty input, duplicate typed resources,
nonmonotonic cycles, oversized inputs, and actual read overflow reject before
an evidence result is accepted. Actor admission or lifecycle errors remain
errors. A terminal execution fault is a valid modeled replay outcome: it is
retained with exact cycle evidence and stops later attempts.

Every attempted release records resources in actual firmware-provider call
order, independent of the caller's sample-vector order. Completed releases
retain the complete `GraphReleaseReport`; faulted releases retain the exact
`GraphExecutionError`. No network, storage, device, worker, or physical-I/O
provider is reachable from this API.

`ALGRREP1` is the domain tag for the deterministic SHA-256 evidence identity.
The digest binds the target device/capability/configuration, implementation,
content/package, transaction/run/start identity, all three limits, every
requested cycle/safety/resource state, every actual provider read, and every
completed report or fault. Caller resource samples are sorted only for this
canonical evidence encoding. Consequently, semantically identical frames with
reversed sample storage produce byte-identical replay results and evidence,
while provider-call order remains the executor's observed order.

## TinyBee transactional consumer

The visible target draft still lowers exactly one
`StableBooleanPairAll` followed by one required `BooleanStreamSink`. After
independent pair decoding, every candidate now runs two actor replays before
the in-memory ALGW, ALGR summary, and replay summary commit together.

The success replay starts at cycle 240,000 and releases at 240,000-cycle
intervals. Input storage deliberately names the second resource before the
first, but the actual actor reads the lowered stable-field order on all four
releases:

| `permit` | `interlock` | sink value | actual provider calls |
| ---: | ---: | ---: | --- |
| 0 | 0 | 0 | first, second |
| 0 | 1 | 0 | first, second |
| 1 | 0 | 0 | first, second |
| 1 | 1 | 1 | first, second |

The separate fault replay supplies the second resource as available/true and
the first as explicitly unavailable, again in reverse storage order. The actor
still calls first then second, latches `ResourceUnavailable`, and returns no
completed sink report. Wrong truth values, provider order, release count,
terminal fault, or missing sink evidence reject the complete UI candidate.

The existing canonical ALGW, ALGR, and implementation identities do not change
merely because an offline replay summary is attached. The new evidence
identities are:

| State | ALGW | ALGR | implementation | success `ALGRREP1` | fault `ALGRREP1` |
| --- | --- | --- | --- | --- | --- |
| GPIO22/GPIO32 default | `33a0fcb7e3d35c38f6119c8e173e1a3a0a935a8019f5ff9c617a639177fe58c6` | `8139f4816581007d762fe48e90a1a821d9f350e3508ccebd156b4cc21c9a64d5` | `126e8ff6ec978b3bf1129b4109f2f7e1e5cd909c3c69eac4349425b46dd40a94` | `a26b41965997461d109d2aeec4b562eb0d2c7c3dfe61870139c2bd2293f12147` | `5e122937631516da3b57fd9f3e1f1d39a473ada6bf7dbad9c04b5121a4b89f6c` |
| GPIO22/GPIO35 rebound | `7e4a90577c9ab9864a782ccc1b4a4f738a585746d04728930bce37a6266e866f` | `5a7adc0101bfe25aa9ee26772268dac8e0e7504ddf647fa6956740141d92e14c` | `6138146e6e0e014006155a332e82c6af63e1c8aaa198c5ee375a15dd4f41b123` | `cf2215451f222f8b402b85285ae889ffd67c06e7de8eb3d9c03c8d075dc2a8df` | `e99110e8980e319389b8fe7731a6087375a465e4151289c37edaec0ed133b176` |

Reset restores the complete default tuple, including both replay identities.
The target proof remains deliberately session-local, so a cache-bypassing page
reload reconstructs that same default rather than adding persistence or a
compatibility shim.

## Reproduced tests and static checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
bash scripts/audit-source-policy.sh
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
cargo test -p alumina-interface-core --target wasm32-unknown-unknown \
  --no-run --locked --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run \
  --locked --offline
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 319 executable workspace tests pass: 73 application/coordinator, 82
protocol-client, 163 core, and one public integration test. The compile-fail
rustdoc contract also passes. Both strict Clippy targets, the standalone core
WASM test links, all six workspace WASM test links, strict Rustdoc, formatting,
optimized assembly, WASM validation, and gzip/Brotli integrity pass. The two
new core tests cover actor truth/fault behavior, actual ordered reads, caller
sample-order canonicalization, duplicate/nonmonotonic rejection, invalid/empty
policies, input/read ceilings, and Service-domain exclusion. UI tests pin both
default/rebound evidence identities and prove transactional reset.

The source-policy result is:

```text
source policy: local Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted
```

The unchanged portable actor crate was also checked independently from
`alumina-firmware`:

```console
cargo test -p alumina-runtime --locked --quiet
cargo clippy -p alumina-runtime --all-targets --locked --offline -- -D warnings
```

All 37 runtime tests and warnings-denied all-target Clippy pass.

## Optimized browser artifacts

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `Cargo.lock` | 99,454 | `c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5` |
| `index.html` | 1,295 | `88843f3e655b224253dc9941b819bc9e8dcdd301dbe3736e5153dd731d24e22e` |
| `alumina-interface.js` | 91,816 | `2741e73433f7293515243d3a5276ec817e54e7d653bf5e2ac563d139653b50cb` |
| `alumina-interface_bg.wasm` | 6,498,376 | `9f7ae85c48b00156c054135a70f82e6aef3f5515589512a5a8d216750f7dcf65` |
| `index.html.gz` | 719 | `e78a848780bc8bab396adbe5def893558ea7b0bdd59a2425206a608a5a5737bd` |
| `alumina-interface.js.gz` | 12,981 | `ca5dc8afbe54488fd85a212d5dc9439e8df363f41595499190f1c1d566a76779` |
| `alumina-interface_bg.wasm.gz` | 2,876,219 | `18f14d0a0788d8df538a5630bbbf3bd0d008edbac793026649f06a2195226598` |
| `alumina-interface_bg.wasm.br` | 2,268,053 | `cdbee479d9e7ef72e95cf0dcb478e662c1bc83b06d6c6437db6bf807b1d19ea3` |

## Optimized localhost browser interaction

The exact optimized bundle above was served only on `127.0.0.1:4173` to a
fresh isolated headless Chromium profile with one 1,800-by-914 canvas. The
browser executed actor replay during ordinary WASM UI construction. It visibly
showed `00→0 · 01→0 · 10→0 · 11→1`, GPIO22→GPIO32 actual provider order,
the success evidence prefix, and the ordered `ResourceUnavailable` fault with
both retained reads.

The interaction selected composite leaf `interlock`, kept GPIO35 as the
catalog replacement, and invoked `rebind composite leaf`. ALGW changed from
`33a0fc…` to `7e4a90…`; ALGR changed from `8139f4…` to `5a7adc…`; the first
selector stayed GPIO22; the second became GPIO35; and the success/fault evidence
prefixes changed from `a26b41…`/`5e1229…` to `cf2215…`/`e99110…`. The truth
table, provider order relative to the rebound pair, terminal fault, closed
session authority, and all non-authority warnings remained visible.

`reset target draft` restored the complete default tuple. A cache-bypassing
reload reconstructed it again. Origin storage contained only
`alumina.graph-workspace-bundle.algwb.v1`; its three canonical sections were
3,755, 407, and 825 bytes. The target replay proof did not gain persistent or
compatibility state.

The final captures are:

| Browser capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/alumina-actor-replay-target-small-2.png` | 350,773 | `c9e426884d7d3ed29adb42464b7c080f095571614a67c0f3a23364e4e059d2f8` |
| `/tmp/alumina-actor-replay-final-rebound.png` | 352,232 | `0c418d430e5927b1d639fd2cad6097e35cf9c11e29a3f4ebdffd2ae2c17a931e` |
| `/tmp/alumina-actor-replay-final-reset.png` | 350,414 | `c32887318ac4244a7e91d0a0536395715e1026dd5cefe0b3e4436f3b2a6da961` |
| `/tmp/alumina-actor-replay-final-reloaded-proof.png` | 351,346 | `9d8fb925b97d4ce374724dc2a7d6c6d0d43800aaa1443ea61f6bbdb8212642c4` |
| `/tmp/alumina-actor-replay-final-reload-top.png` | 365,541 | `365f416786bb1b2137a14f191e1128d0974e9cd23f79df6d2ebc8355250ac020` |

The reload log contained only Chromium's preload-integrity limitation and
software-WebGL fallback warnings. No Alumina application, worker, WASM,
firmware, device, or localhost request failed. Chromium and the localhost
server were stopped afterward.

## Closed claims and licensing

This checkpoint does **not** authenticate a device, open Wi-Fi, upload or
install a package, select persistent firmware state, prepare/activate/start a
run on an MCU, acquire or configure a GPIO, read a physical pin, arm an output,
measure executor timing, qualify freshness/interrupt behavior, or validate any
electrical or safety behavior. Modeled `safety_authorized: true` is replay input,
not physical safety authority. Declared period and WCET remain static package
facts, not measurements. Service-domain replay, output opcodes, broader
resource types, mixed-domain browser replay, physical HIL, and live UI/device
execution are still open.

The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, contacted,
or used. No USB serial, probe lead, SLogic16U3, motor, motor power, board AP,
workstation WLAN, or physical network path was used. Browser work remained on
loopback.

The implementation is independently authored under MIT. It makes the existing
local `alumina-runtime` path dependency direct and selects the already locked
`critical-section` 1.2.0 `std` implementation at the native/WASM application
boundary. That crate is `MIT OR Apache-2.0`; no new package version was
introduced. The source-policy audit accepts the complete existing inventory.
No GPL-family source, library, generated asset, protocol implementation, or
tool output was copied, linked, or vendored.
