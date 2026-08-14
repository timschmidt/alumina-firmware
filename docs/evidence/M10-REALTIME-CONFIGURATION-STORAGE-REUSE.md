# M10 real-time configuration storage-reuse evidence

## Claim

Implementation commit
`20fee330dbde8d4f9c538cc53ba2bd0caeec269a` removes one redundant
permanent core-1 configuration allocation without reducing a protocol,
document, or resource bound.

`RealtimeConfigurationService` previously retained separate inline storage for
its streaming `ConfigurationStreamValidator`, completed inactive candidate,
and active configuration. The first two states are mutually exclusive: a
candidate does not exist until its validator has been consumed by
`finish_with_profile`. They now share one private
`ConfigurationValidationPayload`, whose state is exactly `Validating`,
`Complete`, or `Empty`. The separately active configuration remains outside
that payload.

The classic ESP32 compiler layout for `MAX_BINDINGS = 64` changes as follows:

| Static value | Before | After | Change |
| --- | ---: | ---: | ---: |
| shared validation payload | split 7,512-byte validator plus 4,344-byte candidate | 7,512 | -4,344 |
| `RealtimeConfigurationService<64>` | 16,256 | 11,912 | -4,344 |
| core-1 real-time task future | 50,632 | 46,288 | -4,344 |
| Embassy `realtime_task::POOL` | 50,672 | 46,328 | -4,344 |

This is a linked static-capacity result. It is not a measured stack watermark,
timing result, or physical qualification.

## Preserved lifecycle semantics

The payload-state transition is allocation-free and preserves the existing
wire state machine:

```text
Begin -> Validating -> Data* -> Finish -> Complete -> Activate -> Empty
                                  |                         |
                                  +-- reject/abort ---------+
```

The independent active value is deliberately not part of that sequence. A new
transaction may therefore receive and finish a distinct candidate while the
prior active identity and its authorization remain intact. Only a successful
exact `Activate` moves the completed candidate into the active slot and clears
authorization, as before. `Clear`, `Abort`, rejection, reporting, candidate
inspection, authorization preflight, and command identity checks retain their
prior externally visible behavior.

The new regression constructs two independently valid TinyBee motion
documents with distinct digests and velocity scalars. It validates, activates,
and authorizes document A; streams and finishes document B; proves A remains
active and authorized while B is the inactive candidate; activates B; and
proves B becomes active with authorization cleared. Existing corruption,
sequence, lifecycle, durable activation, and core-0/core-1 equivalence tests
continue to pass.

The private payload enum is also shared with the existing core-0 published-SD
validation actor, which already used this ownership pattern. No public Rust
type, canonical byte, maximum binding count, chunk size, document profile,
digest rule, board fact, or authorization rule changes in this checkpoint.

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
llvm-nm -S -C <board-qualified-artifact> | rg 'realtime_task::POOL'
```

With the espup Xtensa GCC `bin` directory on `PATH`, the compiler layouts can
be inspected directly with:

```sh
cargo +esp rustc -p alumina-firmware --bin alumina-firmware --release \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- \
  -Zprint-type-sizes --emit=metadata
```

The default-member listing contains 524 tests, including 28 configuration, 83
FOC, and 54 simulator tests. Formatting, all host tests, warnings-denied host
Clippy, warnings-denied rustdoc, diff checks, and warnings-denied Clippy for all
four ESP configurations passed.

The exact optimized images are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,115,312 | 12,432 | 249,712 | `a7e03721720d9a05e05cb264bd0bc9ffa0275514b18975b2b3c63e573b4da2b3` |
| TinyBee V1.0, 4 MiB variant | 1,115,332 | 12,432 | 249,712 | `1b2954a16fadcce2190c3572760bca6467bd79dc31bd5128f3977ee281896b47` |
| T-Deck Pro | 1,047,033 | 13,184 | 525,184 | `5cab73b351d1bce6e2254a568e953c828dacff29a65556a69a22dc157dd906dc` |
| MKS ESP32 FOC V1.0 | 1,053,792 | 11,184 | 250,960 | `6b2a6fda4d6da7d7712e441cfe133fe94612f5c45312afa378695d6b4cf8264e` |

Relative to the selected-board authorization checkpoint, text changes by
+2,852 bytes for each TinyBee variant, +716 bytes for T-Deck Pro, and -192
bytes for MKS ESP32 FOC after optimization and inlining. Data and aggregate BSS
remain exact and unchanged. Hashes renew with source/debug identity.

The section and symbol inspection gives:

| Board | live `.bss` | `realtime_task::POOL` | linker-residual `.stack` | live `.bss` change |
| --- | ---: | ---: | ---: | ---: |
| TinyBee 8/4 MiB | 177,724 | 46,328 | 6,452 | -4,344 |
| T-Deck Pro | 172,556 | 44,184 | 115,268 | -4,344 |
| MKS ESP32 FOC V1.0 | 175,252 | 45,400 | 10,172 | -4,344 |

Aggregate BSS reported by `llvm-size` is unchanged because these linker scripts
include the residual `.stack` section in that aggregate. The useful result is
the exact 4,344-byte reduction in permanent live `.bss` and task-pool storage,
matched by a 4,344-byte increase in linker residual on every target. TinyBee's
6,452-byte residual remains small and still requires runtime high-water
measurement before physical promotion.

## License, hardware, network, and moving-Hyper boundary

This slice changes no Cargo manifest or lockfile and imports no source. The
workspace all-features Cargo-tree inventory contains 879 nonempty
package/license records, zero missing expressions, and zero
GPL/AGPL/LGPL/SSPL-family entries. New source and documentation are
repository-owned `MIT OR Apache-2.0`.

No network operation, Wi-Fi association, serial contact, reset, flash, GPIO
transition, or peripheral activation was attempted. The connected bare MKS
TinyBee V1.0 remained untouched, and the SLogic16U3 was not used.

Hypercurve and the other moving Hyper crates were not read, edited, formatted,
pinned, reset, or built for this firmware-only slice. `alumina-interface` was
not changed, and firmware still has no Hyper/CSGRS path dependency.

## Open work

- Measure both core stack high-water marks under representative Wi-Fi, storage,
  graph, stepper, and servo loads before treating the residual as operational
  headroom.
- Continue reducing permanent TinyBee static ownership where bounds and exact
  lifecycle semantics can be preserved.
- Keep all target output/peripheral gates closed until their separate hardware
  qualification evidence exists.
