# M3 authenticated service-admission evidence

Date: 2026-08-10

Status: host-tested/release-linked admission foundation. This evidence does not
claim secret provisioning, TLS/confidentiality, physical SD durability, AP+STA,
browser integration, live HTTP interoperability, flood timing, or HIL.

This records the authentication milestone at its commit. The later
[`M3-DURABLE-CACHE-MEDIA.md`](M3-DURABLE-CACHE-MEDIA.md) supersedes its status
body and test/image totals while preserving the admission claims below.

## Implemented claim

- `alumina-net` implements domain-separated HMAC-SHA-256 for exact request and
  response transcripts. The request commits a fresh 128-bit boot nonce, nonzero
  counter, canonical method/path, length, and SHA-256 body. The response commits
  that counter, status, media, length, and body identity. Tags and nonce are
  lowercase hex; golden vectors were independently checked against a second
  HMAC/SHA-256 implementation.
- A 64-counter replay bitmap accepts bounded out-of-order browser requests once.
  A global 32-token bucket refills at 50 valid requests per second. Invalid tags
  consume neither replay state nor tokens; valid rate-limited counters are spent.
- The portable raw-header accumulator rejects duplicate recognized headers,
  transfer coding, noncanonical or overflowing lengths, missing/wrong native
  media, non-UTF-8/malformed proof fields, zero/leading-zero counters, and bodies
  beyond the 1,148-byte native-storage limit before service dispatch.
- `/api/v1/auth` is public. Authenticated `GET /api/v1/storage` returns a signed
  bounded status; authenticated `POST /api/v1/storage` accepts exactly one native
  V1 frame and returns a signed JSON rejection or native correlated response.
  No CORS headers or compatibility aliases are served.
- `alumina-service` owns host-testable native dispatch. It validates frame and
  operation sizes/directions, upload plans, chunk layout/limits, and chunk
  SHA-256. The core-0 firmware task remains the sole `StorageServiceState` owner.
- A single-slot, serialized same-executor bridge carries fixed requests and
  responses. Private transaction IDs discard stale results after handler timeout
  cancellation. HMAC and the 1,148-byte bridge copy use no cross-core critical
  section, so they do not mask real-time-core interrupts.
- There is intentionally no SD success path yet. Status returns
  `backend_available: false` and `mutation_available: false`; structurally valid
  storage operations return native `Unsupported`. The boot safety state also
  keeps direct coordinator mutation fail-closed.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run -q -p xtask -- check --board mks-tinybee
cargo run -q -p xtask -- check --board t-deck-pro
cargo run -q -p xtask -- build --board mks-tinybee-v1 --profile release
cargo run -q -p xtask -- build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo tree --locked --offline --prefix none --format '{l}'
cargo tree -p alumina-firmware --target xtensa-esp32-none-elf \
  --no-default-features --features board-mks-tinybee --locked --offline \
  --prefix none --format '{l}'
cargo tree -p alumina-firmware --target xtensa-esp32s3-none-elf \
  --no-default-features --features board-t-deck-pro --locked --offline \
  --prefix none --format '{l}'
git diff --check
```

The focused additions comprise 11 `alumina-net` and six `alumina-service` tests;
the complete default workspace has 79 unit/documentation tests. Both chip
families pass strict ESP Clippy and optimized release linking.

The three offline dependency inventories contain no GPL-family license. The new
HMAC dependency is `MIT OR Apache-2.0`; its `subtle` transitive dependency is
BSD-3-Clause. Existing firmware-only 0BSD and Unicode-3.0 obligations are also
explicitly admitted by `deny.toml` under the reviewed permissive-license policy.

Final section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 569,824 | 10,120 | 252,016 | 75,196 |
| T-Deck Pro | 521,317 | 10,936 | 527,428 | 181,668 |

The aggregate BSS includes linker-reserved stack and the intentional 65,536-byte
reclaimed radio heap. These numbers are linked capacity baselines, not runtime
watermarks or deadline evidence.

## Claim boundary and next evidence

No device was connected or flashed. No browser exercised header behavior,
timeouts, response proofs, slow bodies, duplicate headers, replay, rate limits,
or cancellation through a real TCP stack. HMAC does not encrypt status or native
frames; deployment remains restricted to the protected AP or trusted LAN/VPN.
The repository fallback key is shared development material and cannot arm a
production device.

Next, a board-specific async SD adapter must reproduce the already tested
durability protocol under power cuts before any operation returns `Ok`. HIL must
then measure HMAC/header/flood load, stack/heap watermarks, stale-response
cancellation, service-core watchdog behavior, and core-1 deadline/interrupt
latency concurrently on TinyBee and T-Deck Pro.
