# M6 target safety-input and local-stop evidence

Date: 2026-08-11

Status: configuration-derived safety inputs now reach a target-owned ESP GPIO
polling bank and local fail-safe coordinator on core 1. Exact input facts cross
to core 0. This is compile and portable-behavior evidence, not electrical,
latency, interrupt, safe-output, motion-stop, or HIL qualification.

## Implemented boundary

The ESP composition retains a fixed allocation-free input bank for the full
core-1 lifetime. TinyBee converts GPIO 33, 32, 22, and 35 to inputs during the
pre-Wi-Fi safe-output transaction. GPIO 33/32/22 admit floating, pull-up, and
pull-down configuration; input-only GPIO35 admits floating mode only. T-Deck
Pro deliberately supplies an empty machine-safety bank instead of treating its
GPIO2 vibration-transistor route as an interlock.

Configuration activation constructs the portable 32-slot monitor, verifies the
nominal scan cadence against every finite sample-gap bound, resolves every
resource, and validates every pull mode before applying any
configuration-specific bias. Clearing a configuration restores all retained
routes to floating input mode. A nonempty T-Deck safety profile and an
unsupported TinyBee route or pull fail closed before authorization.

The real-time loop polls every active slot once per nominal 1 ms pass using one
shared `DeviceCycle`. Exact polarity, assert/release debounce, ordered-time, and
no-late-healing semantics remain owned by `SafetyInputMonitor`. The V2 safety
snapshot is a canonical 72-byte payload carrying input count, known/active/
required/stale masks, input transition generation, and optional next watchdog
deadline. Core 0 independently validates mask width, active-versus-known shape,
stale/deadline consistency, global transition generation, freshness, and the
board-safe contract. A rolling healthy deadline may advance without pretending
that an input fact changed.

An asserted E-stop/interlock/limit/driver fault or expired sampling promise
first synchronously reapplies the board safe-output transaction on core 1.
TinyBee rewrites the complete 24-bit static safe image while all five retained
GPIOs remain in input mode; configuration-owned weak bias never enables an
output driver. T-Deck reasserts GPIO2 input mode. Only then does core 1
invalidate the unique admitted block, fault the cached schedule with a canonical
local-safety reason, drain queued work, latch the first fault code, and publish
through both the urgent fault mailbox and safety telemetry. Operator Hold and
Stop use the same local safe transaction; Hold intentionally degrades to Stop
until a constrained-deceleration backend is qualified. A network reset request
never satisfies the physical-reset condition. The terminal fault state stops
further monitor mutation, so a deliberately non-healing sample-gap latch cannot
repeat the safe transaction on every 1 ms pass.

TinyBee's safe-contract semantic identity advances to version 2 because the
four digital routes now enter explicit high-impedance input mode before core 0
starts Wi-Fi. Neither board package is made armable by this wiring.

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
git diff --check
```

The default workspace has 207 passing unit tests. Focused safety, job, and
configuration suites have 18, 13, and 15 tests. They cover input-status wire
canonicality, deadline-only refresh, first-fault retention, admitted-token
invalidation, and local-safety schedule faults before and after start. Strict
host Clippy, both complete ESP firmware Clippy targets, and both optimized links
pass. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 845,184 | 11,976 | 250,160 | 37,492 |
| T-Deck Pro | 792,517 | 12,728 | 525,632 | 144,084 |

The target monitor/status state consumes 1,920 bytes of TinyBee's and 1,904
bytes of T-Deck Pro's previous linker stack reserve; BSS aggregate remains
constant because `.stack` is the linker-computed remainder. These are linked
capacity observations, not runtime stack-watermark or deadline measurements.

No dependency was added. Default, all-feature workspace, TinyBee, and T-Deck
Pro offline cargo-tree inventories therefore remain 64, 323, 231, and 238
nonempty package/license records, with no missing or GPL/AGPL/LGPL/SSPL-family
license. New code is repository-owned `MIT OR Apache-2.0`.

## Closed physical claims

Neither available board was connected, flashed, or energized. No ESP input
level, internal pull, contact, or safe shifted image was observed. The nominal
1 ms poll is not a measured worst-case response bound. Contact conditioning,
noise, metastability, simultaneous edges, scheduler/flash/radio interference,
watchdog cadence, first-fault ordering, safe-image rewrite latency, and output
de-energization require board-specific HIL and logic-analyzer/DSO capture. There
is no qualified constrained hold, step serializer, arming transition, or motion
workflow. Both board packages remain non-armable.
