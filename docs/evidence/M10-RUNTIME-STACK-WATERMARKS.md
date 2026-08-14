# M10 runtime stack-watermark evidence

## Claim

Implementation commit
`12e2e6ff1ac4a541b0db11f63321ea4e02585cf2` adds allocation-free,
same-core watermark epochs for the two Embassy executor stacks and exposes
their incrementally observed state through the authenticated native health
service.

This is measurement-mechanism and compile evidence, not a physical high-water
result. The reported current-stack bound is conservative at each sampling
instant, and canary damage persists, but bounded scans discover a newly reached
transient depth only when a later window reaches it. No linker residual is
therefore promoted to usable RAM by this checkpoint.

## Stack ownership and unsafe boundary

The two measured ranges have distinct, explicit owners:

- core 0 uses the linker-owned
  `_stack_end_cpu0.._stack_start_cpu0` range;
- core 1 uses the permanent 32 KiB ESP-HAL `Stack` allocated first from the
  reclaimed internal heap; and
- each core alone initializes, paints, and samples its current stack with
  interrupts masked. Neither core scans the other core's memory.

All linker-symbol, live-stack-pointer, exposed-provenance, and volatile memory
operations are isolated in the target-only
`drivers/alumina-xtensa-stack-watermark` crate. It denies undocumented unsafe
blocks and unsafe operations inside unsafe functions. Firmware and all portable
crates remain under the workspace `forbid(unsafe_code)` policy.

The safe core-1 API does not expose detachable bounds. Its
`start_second_core_with_watermark` wrapper accepts the same permanent
`&'static mut Stack` that it transfers to ESP-RTOS, captures the bounds, and
delivers a non-`Copy`, non-`Clone` initializer only inside the new core's entry
function. Initialization still rejects a live stack pointer outside the exact
range. Sampling records the initializing processor ID and rejects any later
cross-core use before touching memory.

Probe setup or sampling failure is passive. Core 1 still starts if bounds setup
fails; either executor drops its local probe after failure. A missing service
probe makes `HealthSnapshot` explicitly `Unsupported`, while a missing
real-time probe produces the canonical absence marker.

## Bounded measurement policy

Both stacks use one address-dependent 32-bit canary epoch with these fixed
limits:

| Policy | Core 0 service | Core 1 real time |
| --- | ---: | ---: |
| stack ownership | linker range | permanent 32 KiB HAL stack |
| low exclusion | 256 bytes | 256 bytes |
| current-SP reserve charged as used | 2,048 bytes | 2,048 bytes |
| maximum reads per sampling pass | 64 words | 16 words |
| ordinary sampling cadence | about 10 ms | 1 ms management pass |
| passive publication | served on request | at most one lossy frame/second |

ESP-HAL 1.0 places its default stack guard at byte 60. The 256-byte exclusion
preserves that guard and a policy margin. Painting stops at least 2 KiB below
the live initialization stack pointer, so startup before the epoch and the
probe's own call chain are counted as unknown/used rather than free.

The portable tracker:

- accepts only four-byte-aligned layouts and monotonic sample cycles;
- clamps headroom immediately when the sampled current stack pointer is lower;
- scans no more than the caller's fixed word budget;
- only decreases the observed minimum headroom;
- records sample and completed-sweep counters with saturation; and
- validates every report independently before encoding or use.

A complete-sweep flag means that at least one bounded sweep reached the
then-current observed headroom. It is not a synchronous proof that no later
transient occurred. Qualification must run representative loads for long
enough to complete repeated sweeps and must retain this convergence caveat.

## Passive health boundary

Core 1 publishes a fixed 48-byte `ASWM` V1 report in a zero-configuration
`Health` inter-core frame. Core 0 additionally requires:

- the exact real-time domain;
- the compiled 32 KiB allocation and 256-byte exclusion;
- zero configuration digest;
- a nonzero serial sequence newer than the previous report;
- unchanged epoch and layout;
- monotonic sample/sweep counters and sample cycle;
- nonincreasing observed headroom; and
- frame and sample ages no greater than two seconds at observation.

Malformed, stale, substituted, or nonmonotonic health content revokes only the
cached health observation. Its match arm remains valid to the outer authority
dispatcher, so it makes no safety, clock, storage, job, or output-authority
transition and cannot establish the initial safe-output contract. All existing
malformed authority-bearing frame behavior remains fail-closed.

An authenticated, bodyless `HealthSnapshot` request returns one canonical
124-byte `AHLT` V1 body containing:

- exact command, deterministic-work, and lossy-telemetry queue depth/capacity;
- the current service-core stack report; and
- the latest valid core-1 report, with independent present/fresh bits, or its
  canonical absence report.

The exact byte offsets and validation relationships are recorded in
[`PROTOCOL.md`](../PROTOCOL.md). A stale but valid core-1 report remains
present and is explicitly not fresh. Health response construction rejects zero
queue capacities, depth overflow, domain substitution, unknown flags,
noncanonical absence, future samples, and malformed nested reports.

## Reproducible verification

Run from the repository at the implementation commit:

~~~sh
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

llvm-size <board-qualified-artifacts>
sha256sum <board-qualified-artifacts>
llvm-size -A <board-qualified-artifact>
llvm-nm -S -C --size-sort --reverse-sort <board-qualified-artifact>
~~~

Formatting, diff checks, all host tests, warnings-denied host Clippy,
warnings-denied rustdoc, and warnings-denied target Clippy for all four board
configurations passed. The default-member listing is exactly 537 tests: 524
existing tests plus 13 new tests covering stack tracking/wire rules, combined
health encoding, queue bounds, freshness, monotonic observations, substitution,
and unavailable probes.

## Exact release images

The four clean implementation-commit builds are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,124,452 | 12,432 | 249,712 | `2778f8b34ab38e6cfc4584e2ba7162618be685f370f306e184de0315d5df7d12` |
| TinyBee V1.0, 4 MiB variant | 1,124,472 | 12,432 | 249,712 | `7b227385d70b6d5b1ea09cb2ff6bca2da9db0b51e409d55b3efd765f4e1d15cf` |
| T-Deck Pro | 1,054,001 | 13,184 | 525,184 | `31f73c4c65b5fca3713e876fa639f0d02f60bc45747b31b3b9f6160f7ec59202` |
| MKS ESP32 FOC V1.0 | 1,061,736 | 11,184 | 250,960 | `7b7747fc058d15c4cee47ba517113d8560b8026d8d6d1e61abfe98c53b177404` |

Relative to the prior HTTP phase-storage implementation
`b162e086a2ac2b82134887b0a88f4a445b1e140d`, text grows by 9,804 bytes
for either TinyBee variant, 8,612 bytes for T-Deck Pro, and 7,676 bytes for MKS
ESP32 FOC. Data and aggregate BSS remain exact and unchanged on all four
images.

## Exact static layout

`llvm-size -A` and the demangled task symbols report:

| Board | live `.bss` | main pool | service pool | RT pool | HTTP pool | residual `.stack` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| TinyBee 8/4 MiB | 170,892 | 352 | 46,016 | 43,200 | 25,480 | 13,284 |
| T-Deck Pro | 166,396 | 352 | 45,896 | 41,728 | 25,480 | 121,428 |
| MKS ESP32 FOC V1.0 | 168,756 | 344 | 44,480 | 42,608 | 25,480 | 16,668 |

Against the exact prior images recorded by
[`M10-HTTP-PHASE-STORAGE-REUSE.md`](M10-HTTP-PHASE-STORAGE-REUSE.md), each
service pool grows by 224 bytes, each real-time pool by 160 bytes, and each
main pool by 72 bytes. The HTTP pool is unchanged. Live `.bss` therefore grows
by exactly 456 bytes and linker-residual `.stack` falls by exactly 456 bytes on
every composition. Aggregate BSS remains unchanged because these linker scripts
include the residual stack section in that aggregate.

These are compiler/linker ownership facts. They do not establish runtime
headroom, allocator availability, interrupt latency, or a safe reduction in any
configured stack.

## License, hardware, network, and moving-Hyper boundary

The all-features locked/offline Cargo-tree inventory emits 886 package/license
records, zero missing license expressions, and zero
GPL/AGPL/LGPL/SSPL-family expressions. The new target crate and all new source
are repository-owned `MIT OR Apache-2.0`. Its `esp-rtos` edge refers to the
same already locked dependency used by firmware; this checkpoint imports no
external implementation or source.

No network operation, Wi-Fi association, serial contact, reset, flash, GPIO
transition, or peripheral activation was attempted. The connected bare MKS
TinyBee V1.0 remained untouched with no motor power, and the SLogic16U3 was not
used. Every verification and release build was locked/offline.

Hypercurve and the other moving Hyper crates were not read, edited, formatted,
pinned, reset, or built for this firmware-only checkpoint. `alumina-interface`
was not changed, and firmware still has no Hyper/CSGRS path dependency.

## Physical evidence still required

Before any stack or RAM budget is promoted:

- flash a bench image only when doing so is explicitly in scope;
- exercise AP/Wi-Fi, authenticated HTTP, SD cache, graph execution, stepper,
  servo/FOC, diagnostics, emergency-stop, and recovery paths together for
  repeated complete sweeps;
- record both executor reports, their epoch/sweep/sample fields, queue
  occupancies, and the exact image hash throughout the run;
- measure the latency/WCET effect of the 16-word and 64-word interrupt-masked
  windows with the available logic analyzer/DSO;
- independently measure both allocator regions and every vendor-radio RTOS task
  stack; and
- retain the partial-epoch and incremental-discovery limits in any UI display
  or qualification decision.

Until that evidence exists, the mechanism is diagnostic and every current board
package remains non-armable.
