# M4 firmware configuration-lifecycle evidence

Date: 2026-08-10

Status: authenticated configuration routing, dual-core activation, durable job
authorization, clear, and boot recovery are implemented and compile for MKS
TinyBee V1.x and T-Deck Pro. This is software evidence, not physical HIL or an
armability claim.

## Implemented boundary

The core-0 firmware task is now the sole owner of a bounded
`ConfigurationService`, the selected SD backend, and the authenticated
configuration operations on `POST /api/v1/control`:

- `ConfigurationGet` returns status without mutation;
- `ConfigurationValidate` names one already published, typed, immutable
  `MachineConfiguration` object and incrementally validates at most one storage
  chunk/command per service pass;
- `ConfigurationCommit` advances an exact validated candidate through durable
  prepare and activation; and
- `ConfigurationRollback` aborts an uncommitted candidate or clears the exact
  active publication.

Every response carries the canonical fixed 264-byte `ALMCST01` status. The
status joins core-0 progress and summary, committed/pending facts, the full
latest core-1 report, and an explicit job-authorization bit. A compile-time
assert keeps that body plus both native headers within the fixed 384-byte service
response capacity.

Core 1 owns `RealtimeConfigurationService` and independently hashes and
semantically validates the same bytes against the compiled board package. Its
128-byte report is periodically replayed, and `Cleared` persists until another
operation so loss of the immediate telemetry frame cannot strand core 0.
Malformed, noncanonical, or identity-unknown configuration telemetry invalidates
the safety observation and sends the urgent emergency-stop signal.

## Ordered authority handoff

Activation has four distinct authority boundaries:

1. both cores independently reach the same candidate identity;
2. core 0 appends and syncs `ConfigurationPrepare` without changing the replayed
   active selector;
3. core 1 activates the candidate with `active_authorized = false`, revokes the
   real-time job digest, and enters `Configured`;
4. core 0 observes that exact report, commits the selector, then sends an exact
   `Authorize` command. Only its acknowledged report installs the digest in both
   job actors.

Thus core-1 activation before media commit is not job authority. Failure between
steps 2 and 3 retains the old committed selection. Failure after step 3 but
before step 4 leaves the real-time identity unauthorized and makes job admission
busy/zero. Failure after media commit but before authorization replays the new
selector at boot and still requires both validators before authorization.

Candidate rollback sends `Abort` when transfer occurred and appends the exact
`ConfigurationAbort` record when a durable prepare exists. The abort record has
the same payload/commit/anchor sync barriers as other media records and never
changes the old active selector. Active clear is ordered prepare → exact core-1
clear → durable commit. Clear is idempotent only when core 1 is already empty,
so an invalid committed boot selector remains recoverable without accepting a
mismatch against a nonempty active identity. `SafetyEvent::Unconfigure` admits only `Safe` or
`Configured`; armed/running clear rejects. Active configuration prevents
destructive cache reprovisioning, while candidate/commit/clear work serializes
all external media mutation.

Boot recovery first replays the media journal and durably aborts an orphaned
transition. It then reopens any committed typed publication, independently
streams and validates it on both cores, activates it unauthorized, and sends
authorization only after the durable and reported identities agree. Until that
sequence finishes, configuration mutation and new job preparation fail closed.

## Fault injection and verification

The portable end-to-end test exercises publication, both validators, durable
prepare, unauthorized activation, durable commit, authorization, media/device
reconstruction, boot revalidation, authorization after reboot, prepared clear,
persistent cleared telemetry, and durable clear commit. Raw-media tests inject a
partial-write or sync failure at every barrier around activation, clear, and
transition abort. In every recovered state the old/new committed selector and
pending intent are complete, and abort always retains the old active selector.

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree --locked --offline --prefix none --format '{p}|{l}'
git diff --check
```

The complete default workspace has 168 passing unit tests. The focused
configuration/safety/storage/service run has 75. Host and both ESP strict Clippy
gates pass, and both optimized images link. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 784,284 | 11,968 | 250,176 | 46,548 |
| T-Deck Pro | 733,297 | 12,720 | 525,648 | 153,116 |

Relative to `M3-CANONICAL-CAPABILITIES.md`, instantiated configuration routing
adds 65,392/64,640 bytes of text and 88/72 bytes of data. Aggregate BSS is
80/64 bytes lower, while the linker-residual `.stack` region is 6,896/6,888
bytes smaller because long-lived task futures now retain the validators and
coordinator. These are linked capacity observations, not runtime stack
watermarks, deadline results, or proof of safe Wi-Fi/SD concurrency.

## License and claim boundary

This change adds only the existing workspace `alumina-config` dependency to the
firmware. Deduplicated offline package/license inventories contain 318 records
for the all-feature workspace, 58 for default members, 228 for TinyBee firmware,
and 235 for T-Deck Pro firmware. None has a missing license or a
GPL/AGPL/LGPL/SSPL-family license. An implementation/import-tree scan likewise
finds no GPL-family source header or manifest. New work is repository-owned
`MIT OR Apache-2.0`; permissive BSD/ISC-style dependencies remain acceptable
under review. GPL-family code, assets, source checkouts, and implementation
dependencies remain excluded.

No board was connected for this checkpoint. Neither package is armable, so
`JobPrepare` remains `Unsupported` even after a valid configuration commit. No
physical SD, Wi-Fi client, reset/brownout, interlock, output, stack-watermark, or
core-latency measurement is claimed. The next software gate is the deterministic
cached multi-MCU prepare/commit/start protocol; physical configuration and safe-
output lifecycle tests remain required before either first board can arm.
