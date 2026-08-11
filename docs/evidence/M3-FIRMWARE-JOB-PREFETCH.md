# M3 firmware cached-job prefetch evidence

Date: 2026-08-10

Status: authenticated firmware wiring, host simulation, strict target compile,
and optimized link evidence. This does not claim a runnable job, deterministic
commit, canonical board capabilities/configuration, or physical qualification.

This records the firmware-prefetch checkpoint at its commit. The later
[`M3-CANONICAL-CAPABILITIES.md`](M3-CANONICAL-CAPABILITIES.md) supersedes its
zero-capability claim and image/test totals while preserving the closed
configuration, commit, and output gates.

## Implemented boundary

The portable cached-job actors are now instantiated by both first-target
firmware images without extending their authority into motion output.

- `JobPrepare` has one canonical 248-byte `ALMJOBD1` body. It binds a nonzero
  boot-local prepare ID, exact typed publication and manifest, stream ID,
  capability/configuration digests, compile-time axis count, object-derived
  block count, relative tick zero, validation limits, and a SHA-256 identity.
  Decode is exact-length, checks reserved bytes and fixed algorithms, validates
  semantics, and re-encodes to reject alternate representations.
- `JobCancel` is exactly one nonzero eight-byte prepare ID. `JobStatus` is an
  empty request with a fixed 240-byte `ALMJST01` response containing zero-filled
  optional 96-byte service and 128-byte realtime reports. Complete reports must
  agree on prepare ID, block count, terminal stream tick, and terminal digest.
- The former storage-specific POST was removed. `POST /api/v1/control` is the
  sole authenticated native transport and its HMAC transcript binds that exact
  path. `POST /api/v1/storage` is method-not-allowed; no alias or compatibility
  shim exists. `GET /api/v1/storage` remains bounded human-readable status.
- The existing 256-byte command ring carries a complete fixed `ALJC` prepare or
  zero-filled cancel. Core 1 decodes and validates it independently. A malformed
  command, report, or identity transition faults safety rather than degrading to
  a different interpretation.
- The core-0 service task solely owns `ProvisionedCache`, `ServicePrefetch`, and
  the latest correlated realtime report. It reads at most one independently
  hashed storage chunk per 10 ms executor pass, preserves an unsent owned block
  under queue backpressure, and exposes exact queue credits/depth. A prefetch
  fault publishes the urgent emergency-stop mailbox.
- The core-1 1 ms task solely owns `RealtimeJob` and at most one `AdmittedBlock`.
  It revalidates the descriptor and every complete 512-byte block. The first
  block remains owned and outstanding: firmware does not acknowledge it, map
  relative ticks to `DeviceCycle`, install a scheduler, or touch any motion,
  vibration, PWM, I2S, RMT, FOC, or process output.
- Job ownership is included in the freshness-bound safety snapshot. Core 0 also
  applies an immediate local cache-ownership veto before each service request,
  closing the command-to-periodic-telemetry window for upload or provisioning.
  Cancellation clears partial service bytes, invalidates the admitted token,
  and drains the credited work ring before replacement is possible.

The T-Deck Pro executor width is a protocol-only one-axis placeholder; that
board remains non-armable and no block drives its vibration output. TinyBee uses
three axes. Both packages intentionally retain `Digest::ZERO` as their canonical
capability identity. The target endpoint therefore returns `Unsupported` before
opening storage, and core 1 independently rejects such a descriptor. A verified
active-configuration authority is also still absent. This is a fail-closed
linked integration path, not a hidden runnable path.

## Simulation and exactness evidence

`alumina-job` unit tests cover descriptor/command/cancel canonical round trips,
reserved and digest corruption, fixed report encoding, cross-core terminal
divergence, invalid layouts/axis widths/identities, wrong-order blocks,
acknowledgement-token divergence, cancellation/draining, and checked conversion
from `StreamTick` to a future absolute epoch.

The provisioned-cache integration simulation uploads three chained 512-byte
machine blocks in deliberately unaligned 700, 700, and 136-byte storage chunks.
It round-trips the fixed prepare command, forces a two-block ring to
backpressure, and independently validates and acknowledges all blocks through
the portable actor boundary. The combined status round-trips only when service
and realtime terminal facts agree. Separate tests prove storage-valid but
machine-invalid bytes never enter the work ring and that a stalled producer
cannot invent an extended execution horizon.

## Reproduced checks

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
cargo tree -p alumina-firmware --target xtensa-esp32-none-elf \
  --no-default-features --features board-mks-tinybee --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32s3-none-elf \
  --no-default-features --features board-t-deck-pro --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

The complete default workspace has 146 passing unit tests. Host and both ESP
strict Clippy gates pass, and both optimized images link. Final release section
totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 703,192 | 11,872 | 250,272 | 53,948 |
| T-Deck Pro | 653,473 | 12,624 | 525,744 | 160,524 |

Compared with the portable-actor-only checkpoint, instantiating the actors and
wire dispatch adds 49,136 bytes of TinyBee text and 49,012 bytes of T-Deck Pro
text, 104 bytes of data on each, and no additional aggregate BSS. The reported
BSS includes linker stack/reservation sections; these are linked capacities,
not runtime watermarks or deadline evidence.

No dependency or lockfile changed. The deduplicated offline all-workspace,
default, TinyBee, and T-Deck Pro inventories contain 237, 41, 173, and 179
package/license records, respectively, with no missing license and no GPL,
AGPL, LGPL, or SSPL-family license. Repository implementation sources outside
documentation contain no GPL-family SPDX/header or license-manifest match. New
code remains `MIT OR Apache-2.0`; existing Apache-2.0 imports retain their
notices. CI now runs the upstream MIT/Apache-2.0 cargo-deny action over bans,
licenses, and sources, so the allowlist is enforced on subsequent dependency
changes.

## Claim boundary and next gate

No board was connected, flashed, or energized. HTTP behavior has not been
exercised through a physical radio/client, and SD/Wi-Fi concurrency, stack
watermarks, core-1 deadline latency, fault edges, and cancellation need HIL.

The next enabling gate is a canonical byte-level capability document and digest
served to the UI, plus a transactional active-configuration identity known to
both cores. Only after both identities are nonzero and independently verified
may TinyBee accept preparation. Deterministic commit must then bind boot identity,
future local epoch, clock-quality certificate, complete local readiness, safety
state, and a finite lease before a scheduler can acknowledge even the retained
first block.
