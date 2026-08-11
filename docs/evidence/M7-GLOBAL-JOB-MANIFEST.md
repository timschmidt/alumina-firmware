# M5/M7 canonical global-job packaging evidence

Date: 2026-08-11

Status: canonical browser-side partition packaging and global multi-MCU manifest
construction are implemented and verified against the shared firmware schemas.
This is native/WASM software and production-bundle evidence, not browser Wi-Fi
transport, physical synchronization, armability, or motion-output evidence.

## Source identity

The firmware schema checkpoint is `aluminafw`
`eecbccd85d1cd424ed33873f044bdb3d76a90138`. It adds canonical allocation-free
`ALMJMF01` construction and decode to `alumina-job`. The coordinated interface
checkpoint is `alumina-interface`
`dfb07770e816b8f24ac5a0767be88ef03901c7e2`. It depends directly on the sibling
`alumina-machine-ir`, `alumina-job`, `alumina-storage`, and `alumina-protocol`
crates; there is no copied browser-only job or storage schema.

The verified exact-source trees were:

| Source | Revision |
| --- | --- |
| Hypergraphics | `31811aeb17bd2dc827db5669558f6251e0c2f2aa` |
| CSGRS | `b34a2f47b90e3d329028d6337d19dfbc9629fbb0` |
| Hypercurve | `6b1dab45149605338c010a8d6a9bd4d75aa1345d` |
| Hyperpath | `e65506279d3cba99a23cf98bbd17be44126ec14d` |
| Hypersolve | `cdac9bf4e5b88aa050d53667bc2c2244db5ee650` |

These selected trees were clean at capture time. The interface source audit
requires the current sibling CSGRS/Hyper and Alumina sources and rejects a
registry substitution. In particular, CSGRS's published release is not the
source used or accepted by this checkpoint.

## Implemented boundary

The interface first converts the certified exact-CAM fixture into real
`ExecutionBlock<2>` values. Block capacity comes from the firmware schema, not
from a repeated UI constant. Every encoded block is independently decoded and
replayed through `MotionStreamValidator`; terminal tick, digest, displacement,
and final lattice position must agree before an artifact is released. The same
bytes then pass through real `alumina-storage` object, chunk-manifest, upload,
verification, finalization, and publication types.

`CanonicalGlobalJob2` retains every owned per-MCU partition that its manifest
names. It erases discovery order by sorting on stable `DeviceId`, rejects
duplicate device/stream identities through the shared schema, and emits the
allocation-free firmware `MachineJobManifest`. An independent decoder replays
all global facts and participant records before publication.

The manifest binds:

- exact source, compiler, interface, policy, machine, coordinate-epoch, safety,
  and synchronization identities;
- each participant's board package, capability, active configuration, cached
  partition object and chunk manifest, terminal block, resources, error
  evidence, and safety envelope;
- exact block bytes/count, axes, timer frequency, stream span, and initial/final
  integer lattice state; and
- one exact global duration rational. Every local duration is proved equal by
  checked `u128` cross multiplication, without rounded seconds or floats.

SHA-256 of the complete canonical manifest is `global_job_digest`. A separate
domain-separated hash over the complete sorted participant records is
`participant_set_digest`. The firmware tests bind those values and the selected
local partition digest directly to the existing `JobCommit` fields.

Every MCU stores identical manifest content. Resumable upload IDs remain
boot/device-local transaction identities, so the interface can construct a
different valid upload plan for each participant without changing job content.
Tests replay the complete manifest through `UploadCoordinator` under a second
transaction identity.

## Deterministic fixture

The exact line/arc/cubic source still compiles to 197 canonical motion segments
ending at `[960, 0]` on tick `1,583,188`. Each participant partition reports:

```text
partition_blocks=20
partition_bytes=10240
storage_chunks=15
maximum_observed_block_ticks=449740
partition_sha256=49e4292876fbf4d0d2c83b1adbf5f6d8069faf3603fd6243bed06786a4f7401e
chunk_manifest_sha256=6aeb0b3fcf14b087d02683d956fd215488a89bf57b191526f189cd379c00376c
terminal_block_sha256=ec0b6b3c82d61f23180c32f709676e93f7402e03e37c4bf177348d95f91b5285
```

The two-participant global object reports:

```text
global_participants=2
global_manifest_bytes=1312
global_manifest_chunks=2
global_job_sha256=acd6bb77c405c770ef4cc75a7d5423f84bcbab440a39c71707d5701d38714eae
participant_set_sha256=bf496a0af361742e00968605947c1fbde338eaf9db0862206cade1fa49739e4e
global_chunk_manifest_sha256=2d84df13947cfb73f0ded35a559c98107ffd9284ab6a49f00cdb1785bf355311
```

The 1,312-byte size is exactly the schema's 320-byte header plus two 496-byte
records. Inputs deliberately arrive in reverse device order and repeated
compilation produces identical sorted bytes and digests. Both participants run
the same harmless fixture; this proves binding and deterministic identity, not
a useful axis/resource split.

## Reproduced checks

Run from `alumina-interface` at the revision above:

```console
cargo fmt -p alumina-interface -p alumina-interface-core \
  -p alumina-interface-client -- --check
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
cargo run --locked --example exact_checkpoint --quiet
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Run from `aluminafw` at the firmware revision above:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

Twenty exact-core tests, two protocol-client tests, one compile-fail doc test,
and all 238 default firmware-workspace tests pass. Strict native and WASM
interface Clippy, strict firmware Clippy, warnings-denied rustdoc,
package-scoped formatting, current-sibling source enforcement, and the
permissive-license inventory pass. The optimized offline production bundle
validates, including every recorded compression stream.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 3,860,782 | `9216b93c5dbe347a0f3ddde7a2990328eb458e8ff373a346fc415a53d705ddb9` |
| `alumina-interface_bg.wasm.br` | 1,482,633 | `aa51ca701091eb2a5c0cf0308ba183f9581e9139285070312187d52d7bddeb10` |
| `alumina-interface_bg.wasm.gz` | 1,806,061 | `3265e3f86357d7313a1a655101abd3d87a950ab96cc6d1cff7dbfdfc588a5450` |
| `alumina-interface.js` | 75,563 | `3e83bfb70fef884a666dfed97ff848b33af24a9b728e9b116b502c8d29b4336b` |
| `index.html` | 1,290 | `94bf0544a6d6fae85f19c52e2b4c1ee94286aa33b2be195fafe211c7c1f05ab6` |
| `Cargo.lock` | - | `edaa0b0255a0b77f073006174bf5c1dc259e4669ce286382a414fe08e1c43797` |

## Claim and authority boundary

- Fixture source/compiler/interface/policy/machine/coordinate/safety/resource/
  error identities are explicit nonzero sentinels. Production compilation must
  derive them from canonical source, authenticated capabilities and active
  configuration, compiler/build identity, and retained evidence objects.
- The caller still supplies complete participant packages. Global machine axes
  and resources are not yet partitioned automatically.
- Browser Wi-Fi upload, retry/reconciliation, cache inventory, authenticated
  prepare receipts, clock-worker fits, commit/confirm/abort orchestration, and
  recovery UI remain open.
- Exact declared duration and content identity do not prove physical clock
  synchronization, start-edge spread, following error, or coordinated safe
  stopping. Those remain simulation, HIL, and hardwired-safety gates.
- Constant-feed certified chords remain upstream of this boundary; full
  capability-driven lookahead, jerk, kinematics, calibration, and physical
  error budgeting remain open.

No board was connected, flashed, armed, or energized. New code is independently
authored under `MIT OR Apache-2.0`; the source-policy audit accepts no missing or
GPL/AGPL/LGPL/SSPL-family implementation license.
