# M10 HTTP phase-storage reuse evidence

## Claim

Implementation commit
`b162e086a2ac2b82134887b0a88f4a445b1e140d` makes the sequential
authenticated-body and service-transaction phases explicit so the compiler no
longer retains their complete fixed storage simultaneously.

The bounded firmware adapter still admits two concurrent HTTP connections.
Each authenticated route still parses the same finite headers, reads at most
1,148 exact body bytes, applies the same HMAC-SHA-256 boot-nonce/counter/origin/
route/body proof, mutates the same replay and rate-limit state, decodes the same
owned `ServiceRequest`, waits on the same correlated single-slot service
bridge, and signs the same bounded response.

Previously, the body array was a local in the top-level handler, so it remained
part of that connection future while the already copied `ServiceRequest`
awaited core-0 dispatch. The adapter now uses two explicit phases:

```text
read_and_authorize_request
  headers -> exact body -> auth/rate/replay -> owned ServiceRequest

handle_authenticated_route
  completed ServiceRequest -> service bridge -> signed response
```

The first future ends before `ServiceBridge::transact` begins. Rust's async
state enum can therefore reuse the body phase's fixed storage for the later
transaction phase. No allocation, shared scratch buffer, mutex, extra copy,
streaming interpretation, or concurrency reduction is introduced.

## Preserved rejection and cancellation semantics

The refactor retains the prior order and outcome of every boundary:

- route classification and CORS origin parsing still precede authenticated
  admission;
- malformed/duplicate authentication metadata rejects before a body read;
- exact-length body reads still propagate incomplete-body and transport errors;
- HMAC, counter replay, origin binding, rate limiting, and authentication-state
  mutation occur before operation decode or service dispatch;
- authentication failures still map only to `Unauthorized` or `RateLimited`;
- native-frame admission still maps an overwide body to `BodyTooLarge`;
- no rejected request reaches `ServiceBridge::transact`;
- transaction IDs still prevent a response completed after timeout
  cancellation from satisfying a later request; and
- admitted responses use the same counter, canonical origin, media, body, and
  response HMAC.

Public bootstrap, identity, health, network, authentication-discovery,
preflight, method-not-allowed, and not-found branches are untouched. The
two-connection count, 2,048-byte header allowance per connection, 24-header
count, TCP buffers, request/I/O/keepalive timeouts, 1,148-byte authenticated
body maximum, request/response capacities, and rate policy are unchanged.

## Exact compiler layout

The current board compositions share the same network constants and concrete
handler type. `-Zprint-type-sizes` reports:

| Fixed async owner | Before | After | Change |
| --- | ---: | ---: | ---: |
| one `AluminaHttpHandler::handle` future | 5,456 | 4,456 | -1,000 |
| bounded authorization/decode phase | implicit in handler | 1,784 | explicit |
| authenticated route/bridge phase | implicit in handler | 4,336 | explicit |
| `edge_http::Server<2, 2048, 24>::run` future | 25,136 | 21,136 | -4,000 |
| firmware HTTP task future | 29,440 | 25,440 | -4,000 |
| Embassy `http_task::POOL` | 29,480 | 25,480 | -4,000 |

The server-wide reduction is a compiler-observed layout result for the complete
two-connection state machine, not a multiplication assumption. It is identical
on classic ESP32 and ESP32-S3 release compositions.

## Reproducible verification

Run from the repository at the implementation commit:

```sh
cargo fmt --all -- --check
git diff --check
cargo test --locked --offline
cargo test --locked --offline -- --list
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee-4mb \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings

cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release

llvm-size \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1
sha256sum \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1-4mb \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1
llvm-size -A <board-qualified-artifact>
llvm-nm -S -C <board-qualified-artifact> | \
  rg 'http_task::POOL|service_task::POOL|realtime_task::POOL'
```

With the relevant espup Xtensa GCC `bin` directory on `PATH`, inspect the
concrete future layouts with:

```sh
cargo +esp rustc -p alumina-firmware --bin alumina-firmware --release \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- \
  -Zprint-type-sizes --emit=metadata
```

The default-member listing remains 524 tests, including the portable network,
authentication, service, bridge, CORS, storage, and authenticated browser/HTTP
fixtures. Formatting, all host tests, warnings-denied host Clippy,
warnings-denied rustdoc, diff checks, and warnings-denied Clippy for all four
ESP configurations passed.

The exact optimized images are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,114,648 | 12,432 | 249,712 | `2cf86232fd75597021bcf86a112db31fae07875e83aa370fb645c6d40ffc5928` |
| TinyBee V1.0, 4 MiB variant | 1,114,668 | 12,432 | 249,712 | `0b9e33d189209a22332be692199628f27c05215f7f148ca5ba88dc82b0e59189` |
| T-Deck Pro | 1,045,389 | 13,184 | 525,184 | `98a79a97105913487b28c6b2b50e997790c71b2825f1de94ad2c0c92c36f7f7c` |
| MKS ESP32 FOC V1.0 | 1,054,060 | 11,184 | 250,960 | `139c483d5a8dc79992407274450d09bf26933b434014ba2da0f61c1543ea2094` |

Relative to the exclusive motion-owner checkpoint, text changes by -308 bytes
for each TinyBee variant, -312 bytes for T-Deck Pro, and -84 bytes for MKS
ESP32 FOC after target-specific optimization and inlining. Data and aggregate
BSS remain exact and unchanged. Hashes renew with source/debug identity.

The section and symbol inspection gives:

| Board | live `.bss` | `http_task::POOL` | linker-residual `.stack` | live `.bss` change |
| --- | ---: | ---: | ---: | ---: |
| TinyBee 8/4 MiB | 170,436 | 25,480 | 13,740 | -4,000 |
| T-Deck Pro | 165,940 | 25,480 | 121,884 | -4,000 |
| MKS ESP32 FOC V1.0 | 168,300 | 25,480 | 17,124 | -4,000 |

The service and real-time task pools are bit-for-bit unchanged from the prior
checkpoint. Aggregate BSS reported by `llvm-size` is unchanged because these
linker scripts include the residual `.stack` section in that aggregate. Each
4,000-byte live-static reduction is matched by 4,000 more linker residual.
These remain static-capacity observations, not runtime stack watermarks.

## License, hardware, network, and moving-Hyper boundary

This slice changes no Cargo manifest or lockfile and imports no source. The
workspace all-features Cargo-tree inventory remains 879 nonempty
package/license records, zero missing expressions, and zero
GPL/AGPL/LGPL/SSPL-family entries. New source and documentation are
repository-owned `MIT OR Apache-2.0`.

No network operation, Wi-Fi association, serial contact, reset, flash, GPIO
transition, or peripheral activation was attempted. The connected bare MKS
TinyBee V1.0 remained untouched, and the SLogic16U3 was not used. The HTTP
result is compiler/linker plus portable host-test evidence, not a physical-radio
claim.

Hypercurve and the other moving Hyper crates were not read, edited, formatted,
pinned, reset, or built for this firmware-only slice. `alumina-interface` was
not changed, and firmware still has no Hyper/CSGRS path dependency.

## Open work

- Re-run authenticated AP traffic, timeout cancellation, and two-client load on
  a physically isolated or otherwise non-disruptive network bench.
- Measure core-0 and core-1 stack high-water marks under representative HTTP,
  Wi-Fi, storage, graph, stepper, and servo load before treating linker residual
  as runtime headroom.
- Continue to preserve exact route/body/concurrency limits when optimizing
  async-state ownership.
