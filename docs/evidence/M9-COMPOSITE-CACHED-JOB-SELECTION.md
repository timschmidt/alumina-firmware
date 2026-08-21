# M9 composite cached-job selection evidence

Date: 2026-08-20

Checkpoint state: schema-aware nested value selection, composite cached-job
authoring, exact-commit native/WASM qualification, optimized browser
undo/redo, and origin-local reload are complete.

This checkpoint makes inert cached-job references composable without widening
them into command or execution authority. A reviewed graph parameter may retain
several exact participant references inside records, options, results, and
bounded arrays. Selection identifies one already-existing typed leaf by schema,
not by editable identity text, then preserves and revalidates the complete root
value before any workspace commit.

## Source boundary

- Qualified interface implementation:
  `cd0a1389bc8c70e1e7503a4724e03d26b58a3f7b`
  (`feat: select cached jobs in composite values`).
- Preceding qualified interface checkpoint:
  `d0a71319c17e265f5ecba5c62446ede7743e5847`.
- Exact firmware documentation checkpoint:
  `ff9134319874e989e9fd2a6b37d940a76898d9d3`
  (`docs: record composite cached job selection`).
- Preceding qualified firmware checkpoint:
  `d9097a7382bb56a1022d7aed077686495ec53f8f`.
- Both repositories remained on `agent/zizmor-ci-hardening`.
  `alumina-firmware` is the canonical firmware repository; the retired
  `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and device state
  are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed,
  contacted, or used. No USB serial, GPIO, analyzer lead, motor, motor power,
  device AP, or workstation WLAN was used.
- Current CSGRS/Hyper sibling path dependencies were compiled read-only. No
  sibling status, diff, reset, pin, format, stage, commit, or edit was
  performed. Qualification therefore describes the live workspace stack that
  the exact interface commit compiled against, not an old CSGRS release.

## Bounded schema-aware value paths

The core now publishes `GraphValuePathSegment` with five explicit structural
steps:

- `ArrayIndex(u32)`;
- `RecordField(RecordFieldId)`;
- `OptionSome`;
- `ResultOk`; and
- `ResultError`.

`TypedGraphValue::value_at_path` validates the complete registered root before
walking it. A path is bounded to at most `maximum_value_depth - 1` segments.
Record selection uses stable field IDs, arrays require an in-bounds existing
index, and option/result traversal requires the exact active branch. An empty
path remains the ordinary root selection in the same greenfield API.

`TypedGraphValue::replacing_value_at_path` first validates the old root and
replacement leaf, reconstructs only the selected branch, and then validates
the complete new root. It returns a new value; an invalid segment, unknown
field, out-of-bounds index, inactive branch, wrong type, or excessive depth
leaves the original value untouched. It never creates an option/result branch,
grows an array, looks up a display name, or treats raw identity shape as
authority.

The public error family distinguishes depth, schema, segment, bounds, stable
field, and inactive-branch failures. This keeps callers from guessing which
structural precondition failed while retaining one bounded implementation for
root and nested selectors.

## Catalog-bound nested replacement

`select_graph_cached_job_handle` now takes one borrowed value path. Before
replacement it requires:

1. the node and parameter to exist in the cloned workspace;
2. the path to resolve through the registered parameter schema;
3. the leaf schema and current value to be exactly `JobHandle`;
4. the current leaf to be an exact member of the supplied cache-derived
   catalog; and
5. the requested replacement to be an exact member of that same catalog and
   not an exact no-op.

Only then does it replace the selected leaf, revalidate the reconstructed root,
run complete audited graph analysis, encode canonical `ALGW`, and return the
candidate. The caller commits that candidate only after all checks succeed.
Node identity, label, domain, canvas placement, allocation cursors, unrelated
parameters, and every sibling composite value remain exact.

Duplicate inert handles remain legal. A catalog entry is immutable descriptive
evidence, not an execution lease. The selector creates no cache readiness,
ownership, clock, prepare, start, safety, or output authority.

## Visible composite reference set

The offline two-MCU proof now registers
`alumina.job.cached-reference-set` V1. Its one `references` parameter is an
exact record with stable fields:

- field 1, `primary: JobHandle`;
- field 2, `fallback: Option<JobHandle>`, initially `Some`; and
- field 3, `mirrors: Array<JobHandle>`, bounded to four and initially holding
  one item.

The visible selector offers exactly three reviewed leaves:
`references.primary`, `references.fallback.some`, and
`references.mirrors[0]`. It displays the chosen path and segment count, then
resolves the selected node's current participant at that exact leaf. Add
creates a complete reference set; rebind changes one leaf. History snapshots
and the persisted third `algwb1:` section retain the complete composite
`ALGW`.

Startup restore replays all three bundle sections, proves the probe sidecar
against the exact control workspace, traverses every reviewed composite
job-reference leaf, and requires each handle to belong to the newly derived
catalog before replacing any application state. Persisted foreign nested
handles reject the complete bundle. No compatibility decoder or shim was
added.

## Hostile and transactional coverage

Core regressions exercise record, array, option, and result traversal and
replacement; empty root paths; stable sibling preservation; wrong replacement
types; invalid segments; unknown field IDs; array bounds; inactive branches;
and the path-depth ceiling.

Cached-job regressions exercise exact nested record/option/array rebinding,
stable node placement and allocation cursors, duplicate inert references,
exact no-ops, a wrong container type, a bad field, an inactive option, and a
well-shaped raw foreign leaf. Every rejection is atomic.

Application regressions exercise the full composite reference set, selected
leaf behavior, rebind/add/history replay, all three persisted artifacts, exact
restore, and all-or-nothing rejection of a foreign nested leaf.

## Reproduced interface checks

Run from `alumina-interface` at the qualified implementation commit against
the then-live workspace CSGRS/Hyper paths:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
bash scripts/audit-source-policy.sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 313 executable workspace tests pass: 73 application/coordinator, 82
protocol-client, 157 core, and one public exact-control integration test. The
compile-fail rustdoc test also passes. Both warnings-denied Clippy commands,
all six WASM test-target links, strict rustdoc, package-scoped formatting,
source policy, optimized Trunk assembly, WASM validation, and gzip/Brotli
integrity pass. The source-policy result was:

```text
source policy: local Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted
```

The qualified interface repository remained clean afterward.

## Canonical and optimized artifacts

The initial three-section application bundle was 9,983 characters:

| Canonical section | Bytes | SHA-256 |
| --- | ---: | --- |
| reference control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| initial composite cached-job `ALGW` | 825 | `493b293ac9f83f96b6b91d4fba05597f7c7e280cdfd4f462c82d443e1f38bf58` |

After only `references.fallback.some` changed to participant 2, the bundle
remained 9,983 characters, the first two sections remained byte-identical, and
the 825-byte cached-job `ALGW` became
`7ae5b465604806411483311eeedb3472e956644aa7eebae0f631fe9d36dfd612`.

The deterministic fixture retained global job
`2626778741a7046fd1957371132ed44c54790ce5b7f7b4c145930af4f93ddd9c`
and participant set
`d26ddc63881977582a1a3138c676be9e60a95db24fd923480a0706021977217a`.
Both participant partitions remain 9,216 bytes in 18 blocks with a 1,000,000 Hz
local timer.

The exact optimized artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `Cargo.lock` | 99,392 | `e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c` |
| `index.html` | 1,295 | `fcd30bf6bfd0761b79ff1012dc7482e34a837fb0c07feec402211d6d1bfa09bb` |
| `alumina-interface.js` | 91,816 | `0fb2501dafd0bdc4459bde3d6a1ea910602cec09bb32275133ea5eb56310672e` |
| `alumina-interface_bg.wasm` | 6,436,557 | `096ba5e74443d929d6c943de187f8661dad6acc438e8586ef3a3bc5b7d492632` |
| `alumina-interface_bg.wasm.gz` | 2,850,465 | `737e749a496a5097706bba08d3c3c6a87a1b4685b537a6677f95d98ba954de15` |
| `alumina-interface_bg.wasm.br` | 2,247,217 | `72ee8a67d4c16fedd5115c1155e594a69855192222be2581adfe95eb0aa0f384` |

## Optimized browser interaction and reload

The release bundle was served only from `127.0.0.1:8765` into an isolated
Chromium profile after clearing origin storage. The actual canvas UI showed
the three composite leaf choices. The browser selected MCU 02, selected
`references.fallback.some`, and invoked the actual `rebind selected`
control.

The visible result retained node 1, advanced only the cached-job workspace to
revision 2, reported one undo and zero redo entries, displayed a two-segment
path, resolved fallback to participant 2, and stated:

```text
rebound inert reference #1 at references.fallback.some to audited catalog entry 2
```

The actual undo control restored revision 1 and fallback participant 1 with
zero undo/one redo. The actual redo control restored revision 2 and fallback
participant 2 with one undo/zero redo. The canonical bundle returned to the
same post-edit bytes and digest.

After reload, the panel first showed primary participant 1, proving that the
untouched sibling survived the nested edit. It reported revision 2, zero
undo/redo, and restored catalog-bound cached-job `ALGW` from application
storage. Selecting `fallback.some` then showed participant 2 and the same
two-segment path. Local storage still held exactly one application key and the
same 3,755/407/825-byte sections with the same post-edit digests.

The 1,280-by-757 captures are:

| Browser capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/alumina-composite-qualified.png` | 249,486 | `7c3cbd172bce367c282fbbcbfd2aa8b155b6e7d50ac06c2862fa308cb14f0808` |
| `/tmp/alumina-composite-undone.png` | 249,293 | `e982279c7ecb9c9d03817cb06d540d603d1dc8a9fd2ac8c611f263eb130630b3` |
| `/tmp/alumina-composite-restored-qualified.png` | 247,584 | `bb34d5fe7d211c5cbd1c6aa89a385708ecea0fc4b0cbd26dbe9c9f6557e81dae` |

Chromium and the localhost server were stopped afterward. Captured page logs
contained only the browser's preload-integrity limitation and software-WebGL
fallback warning. Chromium also printed software-render readback notices and
background Google registration failures. No Alumina application, firmware,
device, or WLAN error appeared.

## Firmware documentation qualification

The exact documentation checkpoint above was clean before qualification. Run
from `alumina-firmware`:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee-v1
cargo xtask board check mks-tinybee-v1-4mb
cargo xtask board check t-deck-pro
cargo xtask board check mks-esp32-foc-v1
cargo xtask board check t-lora-pager-current
cargo xtask capabilities --board mks-tinybee --json | jq -e '
  .id == "mks-tinybee-v1" and .armable == false and
  (.clocks | length == 2) and (.hil_requirements | length == 8)'
cargo xtask capabilities --board mks-tinybee-4mb --json | jq -e '
  .id == "mks-tinybee-v1-4mb" and .flash_bytes == 4194304 and
  .armable == false and .capability_digest_verified == true'
cargo xtask capabilities --board t-deck-pro --json | jq -e '
  .id == "t-deck-pro" and .armable == false and
  (.devices | length == 12) and (.hil_requirements | length == 7)'
git diff --check
```

Formatting, the complete portable firmware test suite, warnings-denied Clippy,
all five dual-core board definitions, all three capability assertions, and
repository whitespace checks pass.

The local host still has no `cargo-deny` subcommand, so no local
`cargo deny` result is claimed. A fresh locked Cargo-metadata audit reported
zero missing license declarations, zero GPL-family license expressions, and
zero non-crates.io external sources. The slice changed only `README.md`,
`docs/PLAN.md`, the evidence index, and this evidence file; no manifest or
lockfile changed. Checked-in CI remains configured to run
`cargo deny check bans licenses sources` against the repository's permissive
allowlist.

## Closed claims and licensing

This is bounded HostExact graph-authoring and offline-simulation evidence. It
does not make simulated cache readiness live, upload or install a partition,
authenticate a live MCU, acquire a resource, prepare or start a schedule,
synchronize clocks, arm an output, command motion, or contact hardware.
Executable multi-job workflows, prepare/start graph nodes, and nested physical
resource selectors remain separate open work.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the current local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
