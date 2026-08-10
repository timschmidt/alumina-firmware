# M3 SD SPI transport evidence

Date: 2026-08-10

Status: semantic-host-tested and ESP release-linked transport/composition. This
does not claim a card was inserted, a board was flashed, a raw cache region was
provisioned or mounted, hot removal was handled, or any HIL timing/durability
gate passed.

## Implemented claim

- `alumina-sd-spi` is independently authored `no_std` code under
  `MIT OR Apache-2.0`; it copies no SD driver implementation. Its functional
  protocol reference is the SD Association's public Physical Layer Simplified
  Specification. No third-party package entered the dependency lock.
- A board-local wrapper fixes SPI mode 0/eight-bit operation and permits the
  portable driver to select 400 kHz identification and 10 MHz data clocks. The
  portable policy rejects zero clocks, identification above 400 kHz, data above
  25 MHz, and zero retry/poll bounds before touching hardware.
- Initialization drives CS high for at least 80 clocks, issues CRC-7-protected
  CMD0 and CMD8, completes CMD55/ACMD41 with bounded retries, validates CMD58
  power/2.7–3.6 V/capacity bits, requires CMD59 CRC enablement, conditionally
  fixes 512-byte length with CMD16, reads CRC-16-protected CSD with CMD9, and
  derives exact block capacity. Pre-V2 cards which reject CMD8 are intentionally
  out of the first milestone; V2 byte-addressed SDSC and block-addressed
  SDHC/SDXC layouts are checked for consistent OCR/CSD/address ranges.
- Every wait is bounded: 16 reset attempts, 1,000 ACMD41 attempts with 1 ms
  delays, 16 command/token-response bytes, 250,000 read-token bytes, and 500,000
  programming-busy bytes by default. At the 10 MHz data clock the latter two
  bounds correspond to 200 ms and 400 ms of wire time, before HAL/software
  overhead. Expiry is a typed fault, never an infinite service-core loop.
- Reads use explicit full-duplex `0xff` dummy transfers rather than the HAL's
  zero-padded read helper, find a bounded `0xfe` data token, preserve bytes
  prefetched after that token, and verify CRC-16 over all 512 bytes. Writes send
  CMD24, token, exact block, and CRC-16; require an accepted data-response token;
  wait for programming release; deassert CS safely; and require clean CMD13 R2
  status. `sync` also waits ready and validates CMD13. Any block-operation error
  faults the driver until explicit reinitialization.
- Command arguments reject blocks outside CSD capacity and checked byte-address
  overflow. Chip select is flushed and released on protocol/transport failure;
  one trailing `0xff` byte is clocked deselected after each transaction.
- The semantic fake card validates every command CRC/argument, models idle and
  ACMD initialization, OCR/CSD, delayed data tokens, data CRC, write response,
  programming busy time, persistent blocks, transfer failure, and CS state. Ten
  tests cover published CRC vectors, configuration rejection, initialization,
  CSD/addressing, exact read/write/sync, CRC fault/recovery, rejected write,
  cleanup after bus failure, bad CMD8 echo, and a complete real
  `CacheMedia` format/upload/publish/remount sequence over this transport.
- TinyBee now composes SPI2 SCK/MOSI/MISO on GPIO 18/23/19 and SD CS on GPIO 5.
  T-Deck Pro composes GPIO 36/33/47 and SD CS 48 while actively holding EPD CS
  34 and LoRa CS 3 high and LoRa power 46 low. The sole core-0 service task owns
  each card for the life of the image; core 1 receives no SPI/card handle.
- Successful card identification produces an `UnprovisionedStorageBackend` with
  exact total blocks and authenticated status `detached`. Failed identification
  is `faulted`. Both return native `Unsupported` for mutation and report
  `mutation_available: false`; neither chooses, mounts, erases, or formats a
  region. Board storage support is promoted from `Described` to `Compiles`, not
  to bench-qualified.

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

The complete default workspace has 103 passing unit tests. The new SD driver has
ten; `alumina-service` has eight, including the detached-card admission/status
case. Host strict Clippy, both target strict Clippy gates, both board checks, and
both optimized release links pass.

Final release section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 592,716 | 11,560 | 250,576 | 71,980 |
| T-Deck Pro | 543,625 | 12,304 | 526,064 | 178,524 |

The aggregate BSS includes linker-reserved stack and the intentional 65,536-byte
reclaimed radio heap. These are linked capacity baselines, not runtime stack
watermarks, card throughput, or real-time isolation evidence.

The new implementation crate is workspace-owned `MIT OR Apache-2.0`. It uses
only already locked Alumina, `embedded-hal`, and `embedded-hal-async` runtime
crates, all `MIT OR Apache-2.0`; test-only `embassy-futures` is the same. The
offline graph contains no GPL-family implementation dependency.

## Claim boundary and next evidence

No device was connected. SPI electrical behavior, pull-ups, signal integrity,
card power stabilization, CMD timing, real CSD variants, write-busy duration,
brownout, removal, and CRC fault behavior remain unobserved. The TinyBee's
multiplexed GPIO34 TH2/SD-detect route remains owned by the real-time resource
set and is not silently read by core 0; T-Deck has no declared detect input.
Card presence is therefore inferred only through bounded protocol
identification for now.

Only conservative single-block default-speed operation is implemented. There is
no DMA, multi-block CMD18/CMD25, pre-erase, erase/TRIM, UHS/SDUC, SDIO, password,
or filesystem layer. On T-Deck the SD actor currently owns SPI2 exclusively;
EPD and LoRa are held inactive rather than concurrently scheduled. A future
shared-bus actor must retain whole-command CS exclusivity and per-device clock
configuration before those peripherals run together.

Most importantly, an identified card remains detached. Stored configuration,
media identity, region boundaries, explicit format authorization, mount/recovery,
and card-change policy are the next storage slice. Until that exists, firmware
cannot acknowledge uploads or execute cached data. HIL must then test real cards
on both boards, cut power at every raw-media barrier, remove cards during every
phase, measure concurrent Wi-Fi/SD load and core-1 latency, and retain the
non-armable board qualification.
