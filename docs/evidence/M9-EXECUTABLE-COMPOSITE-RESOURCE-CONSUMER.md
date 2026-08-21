# M9 executable composite-resource consumer evidence

Date: 2026-08-21

Checkpoint state: one bounded two-resource graph node is executable from the
browser's capability-derived authoring model through canonical `ALGW` lowering,
the fixed 4 KiB `ALGR` package, independent firmware admission, and the portable
Realtime actor. Ordered sampling, exact capability binding, transactional UI
rebinding, reset/reload behavior, and host/ESP compile evidence are complete.
Every physical-device, Wi-Fi, deployment, electrical, and timing qualification
claim remains closed.

## Exact source boundary

- Firmware implementation:
  `1e4a0fe43a08af5117d398bb3ee1af5c963b07bc`
  (`feat: execute paired graph resources`).
- Preceding firmware checkpoint:
  `5093ce7611e1232db1185f6b0b678184a273c836`.
- Interface implementation:
  `5a5b2d132e34ec6b22f5691e82d34b771f6a250f`
  (`feat: execute paired capability resources`).
- Preceding interface checkpoint:
  `a4f71c20d099adb02b50ed683066513af53f2d27`.
- Both repositories remained on `agent/zizmor-ci-hardening` and were clean
  immediately after their implementation commits.
- `alumina-firmware` is the canonical firmware repository. The retired
  `aluminafw` path was not used.
- The release build consumed the then-live local `csgrs`, `hypercurve`,
  `hypergraphics`, and wider Hyper workspace paths read-only. No sibling
  status, diff, reset, pin, format, stage, commit, or edit was performed. This
  is point-in-time build evidence against the changing workspace Hyper stack,
  not an obsolete published CSGRS release.

## Fixed wire and runtime contract

`GraphIrOpcode::StableBooleanPairAll` is fixed opcode 5 in the existing
`ALGRIR02` package. Its `u64` immediate contains two complete typed
`ResourceId` wire values: the first in the low 32 bits and the second in the
high 32 bits. Decode independently validates both selectors, rejects a
duplicate physical identity, and retains order even though conjunction is
commutative.

The node has no input channel or state, executes only in the Realtime domain,
and owns one Boolean Stream output. Admission requires one exact opcode
descriptor with `StableBooleanInput` access and a resource class, then requires
both decoded resources to appear in the capability-derived palette with the
same class/access and at least `Compiles` support.

At every release the executor calls the resource provider first for the low
field and then for the high field. It performs both calls even when the first
sample is unavailable. If both samples exist it emits their conjunction with
the release tick as source tick and sequence. If either sample is absent it
latches `ResourceUnavailable` before emitting anything. Existing queue/fault
ownership remains unchanged.

Portable tests independently prove:

- ordered canonical round trip and swapped-order identity;
- duplicate and malformed-low/malformed-high rejection;
- package-shape rejection for an invalid pair;
- admission failure when only the first capability resource is present;
- ordered `[GPIO33, GPIO35]` provider calls for false and true releases;
- conjunction values `false` and `true`; and
- both provider calls, no output, and a first-cause fault when GPIO33 is absent
  while GPIO35 remains available.

## Capability identities

Adding opcode 5 changes canonical capability content without widening the
TinyBee resource palette. The four graph-readable resources remain GPIO33,
GPIO32, GPIO22, and GPIO35.

| Capability package | Bytes | SHA-256 identity |
| --- | ---: | --- |
| MKS TinyBee V1.x, 8 MiB primary | 3,607 | `24c011c210b7a7efc3a027c926053b9ac1090f49b79b510487328887ceae5cfd` |
| MKS TinyBee V1.x, 4 MiB variant | 3,620 | `bbffb55926e95aefbbae6f39c369f7e6376dc06a531447bfa5ae57508877dc6a` |
| deterministic TinyBee simulator | 4,040 | `cd79743abb05c28500dedca7fb075433922528ea4e269dfbe53287f7aa77026c` |

The two physical-board documents still report two cores, service core 0,
Realtime core 1, `Compiles` qualification, and `armable: false`. The primary
document still reports 8 MiB flash and the opportunistic variant 4 MiB.

## Browser lowering boundary

The interface adds one reviewed deployment kind,
`StableBooleanPairAll { output, resource_parameter, first_field, second_field
}`. Registration accepts exactly one two-field record parameter whose two
stable field IDs are distinct and whose fields are `ResourceHandle` values of
one class. Lowering requires both complete opaque handles to match the selected
device and capability digest, requires distinct selectors, checks both against
the capability-derived palette, and writes them in stable field-ID order.
The implementation digest binds the reviewed behavior and both field IDs.

The separate session-local TinyBee target draft now contains exactly:

1. one Realtime pair node with stable fields `permit` and `interlock`;
2. one required Realtime Boolean Stream sink; and
3. one typed wire from the pair output to that sink.

The scalar stable-input implementation remains registered only to derive the
four exact catalog entries. It is not inserted into this executable proof.
Every candidate edit first validates the complete graph and catalog provenance,
checks whole-draft uniqueness, derives limits from the complete TinyBee
capability, lowers the fixed package, and independently decodes its pair
immediate. Only then do the in-memory `ALGW` and lowering summary commit.

The exact default GPIO22/GPIO32 state is:

| Identity/fact | Value |
| --- | --- |
| `ALGW` SHA-256 | `33a0fcb7e3d35c38f6119c8e173e1a3a0a935a8019f5ff9c617a639177fe58c6` |
| `ALGR` package SHA-256 | `8139f4816581007d762fe48e90a1a821d9f350e3508ccebd156b4cc21c9a64d5` |
| implementation SHA-256 | `126e8ff6ec978b3bf1129b4109f2f7e1e5cd909c3c69eac4349425b46dd40a94` |
| lowered ordered pair | GPIO22, GPIO32 |
| package bytes | 4,096 |
| declared period | 240,000 device cycles |
| declared total WCET | 160 device cycles (120 pair + 40 sink) |

Rebinding only `interlock` from GPIO32 to GPIO35 produces:

| Identity/fact | Value |
| --- | --- |
| `ALGW` SHA-256 | `7e4a90577c9ab9864a782ccc1b4a4f738a585746d04728930bce37a6266e866f` |
| `ALGR` package SHA-256 | `5a7adc0101bfe25aa9ee26772268dac8e0e7504ddf647fa6956740141d92e14c` |
| implementation SHA-256 | `6138146e6e0e014006155a332e82c6af63e1c8aaa198c5ee375a15dd4f41b123` |
| lowered ordered pair | GPIO22, GPIO35 |

Native interface-core coverage rejects wrong target/capability identity,
wrong shapes/classes, duplicate selectors, missing opcodes/resources, and
noncanonical lowering without mutation. A native cross-repository test then
runs the lowered TinyBee pair through the actual firmware actor types and
resource provider. Application tests prove initial lowering, sibling and
placement/cursor preservation, changed `ALGW`/`ALGR`/implementation identities,
atomic duplicate rejection, and exact reset.

## Reproduced firmware checks

Run from `alumina-firmware` at the implementation commit:

```console
cargo fmt --all -- --check
cargo test --locked --quiet
cargo clippy --all-targets --locked --offline -- -D warnings
cargo xtask check --board mks-tinybee
cargo xtask check --board mks-tinybee-4mb
cargo xtask check --board t-deck-pro
cargo xtask check --board t-lora-pager
cargo xtask check --board mks-esp32-foc-v1
cargo xtask capabilities --board mks-tinybee --json
cargo xtask capabilities --board mks-tinybee-4mb --json
git diff --check
```

All 574 portable default-member tests, warnings-denied all-target Clippy,
formatting, whitespace, and all five actual Xtensa board checks pass. Both
capability commands recalculate and verify the exact identities above. The
unchanged 69,478-byte firmware `Cargo.lock` has SHA-256
`b356c38c2fe8188c98288e1015bebeefc8d8865037174848d80a0eb87e8a7c16`.

A fresh locked Cargo-metadata audit covered 275 packages and reported zero
missing license declarations, zero GPL-family expressions, and zero external
non-crates.io sources. No firmware manifest or lockfile changed in this slice.

## Reproduced interface checks

Run from `alumina-interface` at the implementation commit against the same live
workspace paths:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
bash scripts/audit-source-policy.sh
cargo test --workspace --locked --quiet
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
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

All 317 executable workspace tests pass: 73 application/coordinator, 82
protocol-client, 161 core, and one public integration test. The compile-fail
rustdoc contract also passes. Both strict Clippy targets, all six WASM test
links, strict Rustdoc, formatting, optimized assembly, WASM validation, and
gzip/Brotli integrity pass. The policy result is:

```text
source policy: local Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted
```

The interface adds only a direct dependency on the already-local workspace
`alumina-graph-ir` crate. It introduces no external package.

## Optimized browser artifacts

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `Cargo.lock` | 99,413 | `35ee60035ed97e220888dbc0bffcb8e2fce9f8bbfa344277f399f9338fc1d697` |
| `index.html` | 1,295 | `cbece72fbd81159193d512b61f95e62bf688acbfa848149c6718f5e375ce594e` |
| `alumina-interface.js` | 91,816 | `a6e427ab00d61dfc6fe97b8746ea33f1907240cfea7300753ce7ca69320a4717` |
| `alumina-interface_bg.wasm` | 6,456,908 | `abb158a94e3a4ba3824efd5ef9a8077ff011c2253072c492e2450180da6010f9` |
| `index.html.gz` | 717 | `6758ba5a4c583cccd1bea939e07c2607d9ed32c6bf105bca1ba29923bd0f1626` |
| `alumina-interface.js.gz` | 12,981 | `2a784c27b9904f0adae7dde58b11c55ef913bc5ef994c6085c11d6b118c91bb5` |
| `alumina-interface_bg.wasm.gz` | 2,858,468 | `d3732da85d34a98d33c2471f7638d61892130a5e45c19d181f93b802ae87ef96` |
| `alumina-interface_bg.wasm.br` | 2,254,414 | `1dbb7482339b790dd4677e133ef66adc11c6362190f07081c121175dbc8db5ea` |

## Optimized localhost browser interaction

The exact optimized bundle above was served only on `127.0.0.1:4173` to a
fresh isolated headless Chromium profile with a 1,800-by-1,057 canvas. The
browser opened the actual egui UI, selected stable field `interlock`, retained
the already-selected GPIO35 catalog entry, and invoked `rebind composite leaf`.
The visible `ALGW` prefix changed from `33a0fc…` to `7e4a90…`; `ALGR` changed
from `8139f4…` to `5a7adc…`; the decoded first selector stayed GPIO22 while the
second became GPIO35. The UI continued to state that the edit was offline and
that firmware session authority remained closed.

`reset target draft` restored the exact default identities and GPIO22/GPIO32
pair. A cache-bypassing full page reload reconstructed the same default again.
Origin storage contained only
`alumina.graph-workspace-bundle.algwb.v1`; this target draft is deliberately
session-local and did not gain a persistence or compatibility shim.

The 1,800-by-1,057 captures are:

| Browser capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/alumina-exec-pair-final-target.png` | 357,952 | `3bd197859d2ca7f8eb8af9d8876deb680ff448fc501070a737a2922496d05d63` |
| `/tmp/alumina-exec-pair-final-rebound.png` | 359,747 | `09193155e5f9178d171c6b2c7053cf14437c3546b89dfb13d7ad5f15096e6ca4` |
| `/tmp/alumina-exec-pair-final-reset.png` | 357,661 | `0f7441625087d9a6b3dd1e6bde3d70d6fb7700fbc96d9915b32aff53cd9803c4` |
| `/tmp/alumina-exec-pair-final-reloaded.png` | 357,865 | `aa8744cee2cba0dfd076eb8c24e3d9bce1c6182fa2135b263268fdba651564c3` |

Chromium and the localhost server were stopped afterward. Chromium printed
only headless software-WebGL/readback notices and unrelated background Google
registration failures; the local server recorded successful interface, WASM,
worker, and favicon requests. No Alumina application, firmware, device, or
WLAN error appeared.

## Closed claims and licensing

This checkpoint does **not** authenticate or open a live MCU session, upload,
install, activate, start, stop, or persist an `ALGR` package, acquire a GPIO,
configure an electrical mode, read a physical pin, arm an output, command
motion, validate an interrupt, measure WCET/latency, or qualify a board. The
displayed period and WCET are declared static lowering facts, not measurements.
All ADC, UART, timer, storage, shifted-output, raw-GPIO, broader composite,
motion, and job-execution graph opcodes remain closed until separately
published and qualified.

The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, contacted,
or used. No USB serial, probe lead, SLogic16U3, motor, motor power, board AP,
workstation WLAN, or physical network path was used. Browser work remained on
loopback.

The implementation is independently authored under MIT and adds no external
dependency. The source-policy and metadata audits accept the existing
MIT/Apache-compatible inventory. No GPL-family source, library, generated
asset, protocol implementation, or tool output was copied, linked, or vendored.
