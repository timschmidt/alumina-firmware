# M3 explicit cache provisioning evidence

Date: 2026-08-10

Status: deterministic host-tested and ESP compile-linked locator, mount, and
authenticated provisioning path. This does not claim a card was inserted, a
board was flashed, safe outputs were physically established, or any destructive
request was exercised on hardware.

## Implemented claim

- `alumina-storage::provisioning` is workspace-owned allocation-free code under
  `MIT OR Apache-2.0`. It adds no package or implementation dependency.
- Physical blocks 2046 and 2047 contain alternating 512-byte `ALMLOC01` locator
  records. This fixed placement is immediately below the required block-2048
  (1 MiB) minimum cache start, stays outside conventional primary partition
  metadata, and does not consume card-tail sectors used by GPT backup metadata.
  Formatting remains destructive to the two explicitly named locator blocks and
  requested region.
- Each locator commits its slot, V1 schema/block size, monotonic generation,
  exact observed device capacity, region start/count, nonzero media ID, cache
  limits, independent header/footer recognition magic, zero canonical padding,
  and SHA-256 digest. Generation parity binds a record to its alternating slot;
  two valid records must be adjacent generations.
- Discovery reads only those two fixed blocks before selecting a region. Bytes
  without Alumina locator magic are foreign/unprovisioned and stay `detached`.
  Recognizable but invalid Alumina locators are `faulted`; they are not silently
  treated as blank. A valid locator is accepted only for the current card size,
  exact compiled limits, minimum start, and in-device interval.
- After locator selection, the existing `CacheMedia` mount must replay the
  complete committed hash chain and return the same media ID. Missing anchors,
  corruption, policy mismatch, or identity mismatch fails closed. One damaged
  older locator is reported as degraded only when another trusted generation
  remains usable.
- `StorageProvision` is assigned native operation `0x0909`. Its exact 112-byte
  `ALMPRV01` body requires the destructive-format bit and binds observed device
  size, expected old locator generation/media ID, exact new region, fresh media
  ID, explicit untrusted-locator recovery intent, zero padding, and a canonical
  SHA-256 confirmation. Replaying an old request after a committed generation is
  a conflict rather than another format.
- Initial or damaged-locator provisioning formats and synchronizes the exact
  cache region, clears/synchronizes both locator slots, then writes and
  synchronizes generations one and two for immediate redundancy. A cut after
  generation one can still mount it while reporting the missing peer degraded.
  Reprovisioning formats first and replaces only the alternating next locator.
  Any failed write/sync faults the manager. A reboot therefore observes a
  complete old/new generation or an explicit state that can be recovered only
  by another confirmed request.
- The core-0 service backend now reports physical device blocks separately from
  selected-region blocks, region start, locator generation, media ID, locator
  and anchor degradation, coarse fault, upload state, and distinct upload versus
  provision availability. The native status body is 112 bytes; authenticated
  JSON exposes the bounded human-readable subset.
- TinyBee and T-Deck Pro construct `ProvisionedCache<SdSpiCard<...>>` after SD
  identification and run discovery/mount in the sole core-0 service task. Core 1
  receives no storage handle. Foreign cards are never formatted at boot.
- The destructive operation uses the same safety admission as every durable
  mutation. Current board images have not yet established/reported physical safe
  outputs and remain in `SafetyState::Boot`, so a real request returns
  `ForbiddenState`. This checkpoint wires the route without weakening that gate;
  physical provisioning becomes usable only with the safe-output milestone.

## Deterministic tests

The nine new provisioning tests cover:

1. canonical request round-trip plus confirmation and reserved-byte tampering;
2. read-only discovery of foreign media;
3. exact preservation of every block outside the two locators and selected
   region, followed by reboot mount;
4. armed-state and stale-generation rejection before device writes;
5. exact old-ID confirmation and alternating locator generations;
6. damaged recognizable locator rejection plus explicit recovery intent;
7. locator/cache media-ID mismatch; and
8. a cut at every initial format/locator write or sync position, followed by
   reboot and proof that state is either ready or explicitly recoverable; and
9. forced cancellation while the media formatter is pending, proving the
   manager retains physical-device ownership and reports a fail-closed state.

The service test additionally proves that `StorageProvision` is forbidden in
boot, accepted after a synthetic safe observation, returns the expanded status,
and rejects a tampered confirmation before backend dispatch. Together with the
numeric-extrema JSON-bound coverage and prior suites, the default workspace has
114 passing tests.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask check --board mks-tinybee
cargo xtask check --board t-deck-pro
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo tree --locked --offline --prefix none --format '{p}\t{l}'
git diff --check
```

Host tests, strict host Clippy, both target checks, both strict target Clippy
gates, and both optimized release links pass. Final section totals from the
reproduced ELFs are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 647,804 | 11,704 | 250,432 | 63,676 |
| T-Deck Pro | 598,665 | 12,448 | 525,920 | 170,204 |

Relative to the preceding SD-transport checkpoint, text grows by 55,088 bytes
on TinyBee and 55,040 bytes on T-Deck Pro; data grows by 144 bytes on each. The
linked BSS aggregate falls by 144 bytes and the linker-reserved `.stack` section
falls by 8,304/8,320 bytes respectively due to the changed compiled async future
layout. These are linked capacity observations, not runtime stack-watermark or
latency evidence.

The workspace runtime dependency graph remains unchanged by this slice. Its
licenses contain no GPL-family implementation dependency; all new code is
`MIT OR Apache-2.0`.

## Claim boundary and next evidence

No SD CID/serial is currently read. The logical media ID binds locator and cache
anchors and detects ordinary card replacement, but a complete byte-for-byte card
clone intentionally retains the same logical identity. Hot removal/reinsertion,
card detect, physical write protection, wear, real power-loss persistence, GPT
or filesystem coexistence, and card-specific busy behavior remain HIL work.
The manager retains its device if a transition future is dropped, but the
underlying SPI transaction is not cancellation-safe with respect to chip select;
the sole firmware actor therefore always awaits each operation to completion.

The front guard and fixed locators are policy, not a partition editor. The UI
must inspect the user's intended layout and request an exact non-overlapping raw
interval; firmware cannot prove that unrecognized data inside an explicitly
authorized region is dispensable. There is no list/read/delete/scrub service,
compaction, filesystem, multiblock/DMA transfer, or verified real-time prefetch
in this checkpoint.

Most importantly, no current board build may physically format while the
real-time side remains in `Boot`. The next enabling dependency is board-specific
safe-output establishment and an authenticated, freshness-bound core-1 safety
observation. HIL must then provision sacrificial cards on both boards, cut power
at every device barrier, verify untouched sentinel sectors and GPT backup data,
exercise removal/replacement, and measure core-1 deadlines under Wi-Fi plus SD
load before board qualification changes.
