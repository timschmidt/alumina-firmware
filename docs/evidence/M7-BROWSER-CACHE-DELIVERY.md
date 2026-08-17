# M7 authenticated browser cache-delivery evidence

Date: 2026-08-11

Status: the firmware and browser client share an origin-bound authenticated HTTP
boundary, the browser can reconcile resumable content-addressed cache objects,
and one validated delivery machine orders each participant partition before the
identical global manifest. This is native/WASM, link, and production-bundle
evidence. It is not a live-browser, radio, SD-card, physical-start, armability,
or motion-output claim.

## Source identity

The firmware boundary is `alumina-firmware`
`ee1e0ece0cf19bf9645b43118c8d099007939ecd`. The coordinated browser client is
`alumina-interface` `05e06fb22abe29450bae0a2179f76162584c7be2`.
The interface depends directly on the sibling `alumina-net`,
`alumina-protocol`, `alumina-storage`, `alumina-job`, and
`alumina-machine-ir` crates. There is no browser-only protocol, upload, job, or
manifest schema.

The production WASM build also resolved CSGRS and every Hyper crate from the
current sibling checkout. CSGRS was
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0`; its local manifest's `0.23.0`
package label does not authorize substitution of the old published release.
Hypergraphics was `31811aeb17bd2dc827db5669558f6251e0c2f2aa`, Hyperpath was
`e65506279d3cba99a23cf98bbd17be44126ec14d`, and Hypersolve was
`8a578174c5e4400ec77a1607a95264caf4e47807`.

This remains a development snapshot rather than a release pin. Hypercurve HEAD
was `6cbb7e9f0094654fba39f4401fd64e411a0261fb` with a concurrent tracked diff
whose post-build SHA-256 was
`6fb35d4c2b6d7d534b76daeb28bbc6b2ebb89f8cc4a60fb7a9d70bf569f458a8`.
Hyperphysics HEAD was `a8002f286914356d3ebc5f491695f39f6f1c029e` with tracked-diff SHA-256
`99766a9ad8ccb54b8eac523fcc904db4d2df3aa5eeb4c10f5bcb781d57ad9667`.
Those unrelated working-tree edits were preserved. A release must repeat this
gate against one reviewed clean revision set.

## Exact browser/firmware boundary

The public `/api/v1/auth` response is one fixed 260-byte JSON schema. It reports
the current HMAC-SHA-256 V2 scheme, a fresh nonzero 128-bit boot nonce, origin
binding, replay window, rate limits, and exact proof-header names. The client
rejects changed policy, unknown fields, non-lowercase nonce text, or a missing or
different `Content-Length` before using the challenge.

Every `/api/v1/control` request commits the boot nonce, nonzero HTTP counter,
method, path, calling browser origin, and SHA-256 identity of the exact canonical
native frame. Every response commits the same counter and origin plus HTTP
status, media, and body identity. The client accepts native bytes only after the
response HMAC, native sequence, operation, correlation, and active configuration
digest all agree. Ambient browser credentials are omitted, caching is disabled,
and redirects are rejected.

Firmware parses the `Origin` into a bounded canonical `CorsOrigin`, emits that
exact origin rather than `*`, and admits only route-specific preflight methods
and headers. It retains the older private-network preflight request/response
fields as compatibility metadata, not as its security boundary. Current secure
Chromium contexts instead receive `targetAddressSpace: "local"` so Local Network
Access can request user permission; an insecure device-served HTTP page does not
pretend it can use that secure-context mechanism.

The target MCU origin and calling document origin remain distinct. Device URLs
must be exact canonical HTTP(S) origins, optionally with one trailing slash;
paths, normalization tricks, credentials, queries, and fragments reject.
Immediately before every fetch, the adapter verifies that the browser-generated
document origin still equals the origin bound into the HMAC session.

## Retry and participant ordering

`CacheUploadMachine` always begins or recovers with exact `StorageInspect` of the
desired `(StoredObject, chunk manifest)` publication. A miss proceeds through
the real firmware operations:

```text
StorageInspect -> StorageBeginUpload -> StoragePutChunk* -> StorageFinalize
```

Each local chunk header, index, size, upload identity, and SHA-256 is validated
before framing. Device progress is accepted only when its upload ID, phase,
durable prefix, next chunk, accepted bytes, and total bytes are the unique valid
state for the declared plan. Any transport ambiguity or authenticated semantic
failure spends the request counters and returns to inspection. A lost chunk
response therefore resumes from durable progress, while a lost finalize
response becomes complete only after the exact published identity is observed;
the client does not issue a speculative second finalize.

`ParticipantCacheDelivery` binds both immutable publications before I/O. For
every sorted global-job participant it completes the executable local partition
first, then uploads/reconciles the identical global manifest under a separate
device-local transaction ID. Construction validates every participant before
returning a readiness table, so malformed input cannot produce a partially
initialized multi-device delivery set.

## Reproduced checks

Run from `alumina-interface` at the revision above:

```console
cargo fmt --package alumina-interface --package alumina-interface-client \
  --package alumina-interface-core -- --check
cargo test --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo check --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline -- -D warnings
cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

The interface has 30 passing native tests. Strict native and WASM Clippy, WASM
checking, rustdoc, exact sibling-source/permissive-license audit, Trunk release,
WASM validation, and all compression integrity checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 3,879,477 | `036b2fd6724909b38e4b78889502d86ecabf0253ddb95773138a5a761a975654` |
| `alumina-interface_bg.wasm.br` | 1,487,722 | `eb90a36b60fa938c4ff78740615e1273cac7821b9e992fd5048244de1ab6daa5` |
| `alumina-interface_bg.wasm.gz` | 1,812,558 | `201967bf6221d84a6497f620475ba2830b64c3a576dd487ffde49344f8020b8a` |
| `alumina-interface.js` | 75,566 | `313059b0fd768ab80720c48c616dad95f96cca476df3c2149d1fa05614f2eebb` |
| `index.html` | 1,290 | `3751ce62d375decc3524d0e2894ff713d88ec88225d6e0c9f3d36d20b9c0b8fd` |
| `Cargo.lock` | - | `d4f70c403885669ef4358c58379c70cbad5e980761277360dd140309ff638159` |

Run the split firmware gates from `alumina-firmware`:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
llvm-size target/xtensa-esp32-none-elf/release/alumina-firmware \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware
git diff --check
```

All 242 portable tests, strict portable Clippy, both strict target Clippy gates,
and both optimized links pass. The firmware workspace deliberately tests its
portable default members on the host and builds each mutually exclusive board
feature on its actual target; combining both embedded target families in one
host `--workspace --all-targets` invocation is not a valid gate.

| Board image | text | data | bss aggregate | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| MKS TinyBee V1.x | 875,380 | 11,992 | 250,144 | `1326372f8712adf0263da140435ae8d3ac09ccdcc7e3c7e4305d72e23327971c` |
| T-Deck Pro | 815,969 | 12,752 | 525,616 | `77811b5a5022d9e6fca4f3997937f47aea9b937caff14c860a5acbbf2be5ad06` |

## Claim and license boundary

No real browser or MCU exchanged these requests. Credential enrollment/storage,
device discovery and identity UI, live progress panels, browser integration
tests, concurrent-device backpressure, actual AP/STA behavior, SD durability on
hardware, and HIL captures remain open. The clock-worker and
prepare/install/confirm/abort coordinator are the next browser-side slice. Both
boards remain non-armable.

New code is independently authored under `MIT OR Apache-2.0`. Native and WASM
inventories have no missing license and no GPL/AGPL/LGPL/SSPL-family dependency.
No GPL-family implementation source was introduced or consulted, and no
Synthetos or SimpleFOC implementation source was used.
