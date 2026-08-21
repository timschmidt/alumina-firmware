# M9 cache-derived graph job-handle evidence

Date: 2026-08-20

Checkpoint state: source implementation, exact-commit native/WASM
qualification, optimized bundle, transactional browser interaction, and exact
origin-local reload complete.

This checkpoint adds the first identity-bearing cached-job value to graphical
authoring without turning a digest, path, mutable job name, or UI choice into
execution authority. A graph can retain an inert reference to one exact
participant partition only after the complete canonical multi-MCU job and the
coordinator's cache-ready observations agree exactly.

## Source boundary

- Qualified interface implementation:
  `8bfebea8db8abb40700d251d98f210d8c7214a59`
  (`feat: derive graph job handles from cache evidence`).
- Docs-only interface closure:
  `d0a71319c17e265f5ecba5c62446ede7743e5847`
  (`docs: close cached job-handle planning gap`).
- Preceding firmware evidence checkpoint:
  `2b10e346acee54679c1b02fe903b18cd01b35a2b`.
- Both repositories remained on `agent/zizmor-ci-hardening` throughout the
  checkpoint. `alumina-firmware` is the canonical firmware repository; the
  retired `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and physical-device
  state are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, analyzer lead, USB serial, device AP,
  or workstation WLAN was used.
- Current CSGRS/Hyper sibling path dependencies were compiled read-only. No
  sibling status, diff, reset, pin, format, stage, commit, or edit was
  performed.

## Window-free authority join

`GraphCachedJobCatalog` is derived from two independent inputs:

1. `CanonicalGlobalJob2`, which retains the canonical `ALMJMF02` manifest,
   every complete `MachineJobParticipant`, every independently replayed
   partition package, and their exact `PublishedObject` identities; and
2. `ParticipantCacheReady`, whose fields remain private to the application
   crate and which the Wi-Fi delivery coordinator emits only after that MCU's
   partition upload and its copy of the shared manifest both reach `Complete`.

`derive_graph_cached_job_catalog_from_ready` converts those provenance-bearing
tokens into data-only core evidence and reruns the exact join. Core derivation
validates a nonzero bounded entry policy, requires exactly one item per
participant, canonicalizes by stable `DeviceId`, rejects duplicates, checks
that each package agrees with its manifest participant, and compares the full
partition and shared-manifest publications exactly. Discovery order, upload
transaction IDs, display labels, and UI position do not enter authority.

Every resulting entry retains the complete canonical participant record. The
UI can therefore report its MCU, global-job and partition identities,
capability/configuration digests, execution kind, canonical partition bytes,
block count, and local timer frequency without guessing from a board name.
`GraphCachedJobPublicationEvidence` remains intentionally data-only: its public
constructor validates identity shape and object kind but does not authenticate
a transport, so production callers use the readiness-token adapter.

## Transactional graph selector

The separately registered offline node kind `alumina.job.cached-reference` V1
has one root `TypeKind::JobHandle` parameter. A catalog entry can materialize
that value only for the registered root job-handle type. The value contains the
stable participant MCU, shared global-job digest, and participant-local
partition digest; it contains no file path, mutable name, command, token, or
lease.

`select_graph_cached_job_handle` requires the existing parameter value to be an
exact member of the same supplied catalog before replacement. This closes raw,
stale, and foreign-handle laundering through an otherwise well-typed graph. It
then resolves the selected entry, rejects exact no-ops, edits a clone, reruns
complete audited semantic analysis and canonical ALGW encoding, and commits
only after all checks succeed. Node identity, label, domain, ports, canvas
placement, and both monotonic allocation cursors remain stable.

Multiple nodes may deliberately retain the same handle because an immutable
reference is inert data, not an exclusive execution lease. Future
prepare/start nodes and the coordinator must independently enforce cache,
clock, ownership, safety, arming, and deterministic-start authority.

Core regressions cover canonical participant ordering and retained facts;
missing, duplicate, substituted-partition, substituted-manifest, wrong-kind,
zero-limit, and entry-limit evidence; stable rebind and duplicate inert
references; and atomic rejection of raw current values, wrong parameters,
wrong root types, exact no-ops, and semantically invalid candidates.

## Visible offline workflow and persistence

The Control workspace now contains a separate two-MCU cached-job authoring
proof. It runs the real deterministic partition-then-manifest reconciliation
state machines, derives the catalog from their readiness tokens, and builds a
minimal reviewed HostExact graph. The operator may select a participant, add
another inert reference, rebind the selected managed node, and use canonical
replay-backed undo/redo. No digest, device ID, partition ID, path, or command is
editable.

The panel reports its simulated/offline/non-executing status, shared global job
and participant-set prefixes, canonical ALGW bytes/revision/history, exact
participant cache facts, and the selected node's resolved catalog membership.
Every operation reruns catalog membership, semantic admission, and canonical
encoding. Every history target is replayed and re-admitted before replacement.

Application persistence is deliberately replaced, without a compatibility
shim, by one `algwb1:` value with three lowercase-hex sections: control `ALGW`,
bound `ALGP`, and catalog-bound cached-job `ALGW`. Each section has an
independent 2 MiB ceiling. Decode rejects wrong tags, missing or extra sections,
odd-length, uppercase, nonhex, or oversized input. Restore replays all three,
proves the probe sidecar against the exact control workspace, and proves every
job handle against the catalog freshly derived during that startup before it
changes any in-memory document. History is intentionally ephemeral and clears
after restore.

UI regressions prove initial derivation from the complete reconciled set,
transactional rebind/add/history/replay behavior, exact bundle restore, and
atomic whole-bundle rejection of a persisted foreign raw handle.

## Reproduced checks

Run from `alumina-interface` at the qualified implementation commit against the
then-live workspace Hyper/CSGRS paths:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli --test dist/alumina-interface_bg.wasm.br
git diff --check
```

All 311 executable workspace tests pass: 73 application/coordinator, 82
protocol-client, 155 core, and one public exact-control integration test. Both
warnings-denied Alumina Clippy commands, all six WASM test-target links, strict
Alumina rustdoc, package-scoped formatting, the local-source/permissive-license
audit, optimized Trunk assembly, WASM validation, and gzip/Brotli integrity
pass. The implementation commit remained clean after qualification. The later
interface checkpoint changes only the two stale open-scope documentation
sentences recorded above.

The unchanged control artifacts and new startup cached-job artifact are:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference control `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |
| participant-1 cached-job `ALGW` | 449 | `75eaf24063782f6a56557a1c3917bb4aea4ef89c4814cd121a81481297623bda` |

The canonical startup `algwb1:` text is 9,231 characters. Its exact job
identities are:

- global job:
  `2626778741a7046fd1957371132ed44c54790ce5b7f7b4c145930af4f93ddd9c`;
- participant set:
  `d26ddc63881977582a1a3138c676be9e60a95db24fd923480a0706021977217a`;
- participant 1 partition:
  `ccd0a47153dae2c1f7dff1697f7b2d8d1a229535c02881476251911ab8ab3f98`;
  and
- participant 2 partition prefix: `608653924c648477…`.

Both deterministic fixture partitions are 9,216 bytes in 18 blocks and use a
1,000,000 Hz local timer.

The optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `Cargo.lock` | 99,392 | `e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c` |
| `index.html` | 1,295 | `f1249ce9b695ad23ec85626d0cb2cd3ae347cfe3cc6dc1ae6e3e1ef448c44e89` |
| `alumina-interface.js` | 91,816 | `06d0f20a2310a8ba5faf5df0cd4d508a97f75ebbe7a94b30481b387ef6439b86` |
| `alumina-interface_bg.wasm` | 6,428,408 | `1c8456d240aee8f5e312767f68e3238c1689ef95cfa5e3fa4e510bd3240e3082` |
| `alumina-interface_bg.wasm.gz` | 2,846,350 | `1b3e0fc012a5a4d693b679b75eef35e5e0a0c00a2ff095142e4c8a1a5e86a843` |
| `alumina-interface_bg.wasm.br` | 2,246,201 | `5e9ef28aa28b0948d50f8aebced1d5ae59503633599ee9a7f6b3027eca5f9df9` |

## Browser runtime and reload evidence

The optimized bundle loaded from `127.0.0.1:8765` in an isolated Chromium
profile after clearing that profile's local storage. The actual Control graph
tab rendered the cached-job panel. Its participant selector contained exactly
the two cache-derived entries: MCU `010101…` / partition `ccd0a471…` and MCU
`020202…` / partition `608653…`.

The browser selected MCU 02 and invoked the actual `rebind selected` control.
The visible result retained node `#1`, advanced the cached graph to revision 2,
showed participant 2 and its capability/configuration/partition facts, retained
one undo and no redo entry, and reported
`rebound inert reference #1 to audited catalog entry 2`. The adjacent text
explicitly denied deployment/start authority and exposed no raw identity, path,
or command field.

After the click, the complete persisted `algwb1:` text remained 9,231
characters. The control and sidecar sections remained byte-identical. The
449-byte cached-job ALGW changed to SHA-256
`b3b3dbbf141cdb6a81bddaa3a2d10f20fc270f69dbe1049dce6245ea5c3c02f3`.

The page was then reloaded and returned to the actual panel. Revision 2,
participant 2, the same 449-byte canonical object, and the same complete bundle
were restored. Undo and redo were both zero as required for ephemeral history,
and the status reported
`restored catalog-bound cached-job ALGW from application storage`.

The 1,440-by-913 captures are:

| Browser capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/alumina-job-catalog-browser.png` | 286,350 | `15744667d18e49259b4203c27bfc031c468d8b29612e560884b3f39c6ed24b54` |
| `/tmp/alumina-job-catalog-reloaded.png` | 285,923 | `2ebbe0a5340a6cec63daf696f8318c7d7cc78453c035d41c5ee87f8dc90e013a` |

Chromium and the localhost server were stopped afterward. Chromium emitted
only a preload-integrity limitation, software-WebGL readback/performance
notices, and background Google registration/auth/quota failures; no Alumina
application, device, or WLAN error appeared.

## Closed claims and licensing

This is bounded HostExact authoring and offline simulation evidence. It does not
authenticate a live MCU merely from publication metadata, make simulated
readiness live, upload or install a partition, acquire a resource, prepare or
start a schedule, synchronize clocks, arm an output, command motion, or contact
hardware. Firmware safety, multi-MCU coordination, and physical execution
remain independent authorities.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the current local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
