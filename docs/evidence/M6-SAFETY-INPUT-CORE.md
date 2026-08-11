# M6 portable safety-input core evidence

Date: 2026-08-11

Status: exact configuration retention, digital-input debounce, sampling
watchdogs, arming facts, and typed conservative reactions are implemented in
portable `no_std` code. This is not an ESP GPIO sampler, interrupt-latency,
electrical-integrity, E-stop, interlock, motion-stop, or HIL qualification claim.

## Implemented boundary

Configuration V1 now treats limits, probes, motor/FOC faults, E-stops, and
safety interlocks as executable core-1 facts. Each requires a finite nonzero
sample-gap watchdog. E-stop and safety-interlock bindings additionally require
the explicit `required-interlock` flag, while that flag is rejected on ordinary
bindings. The independently derived profile retains stable role/instance,
typed GPIO or board-safety resource, active polarity, pull mode, assert/release
debounce cycles, arm requirement, and watchdog for at most 32 per-MCU inputs.
Slots must preserve canonical `(instance, role)` order. All conservative
fault-class roles are arm-blocking; only a probe may be active unless explicitly
marked required, pending later operation-specific homing/probing policy.

`SafetyInputMonitor` accepts samples only in strictly increasing local
`DeviceCycle` order per slot. It applies active-high/active-low polarity and
separate exact assert/release debounce bounds, reports the initial stable state
explicitly, and retains known, active, required, and stale masks. A sample is
healthy through exactly `last_sample + maximum_gap`; the following cycle is the
first watchdog failure. A later sample cannot erase that missing interval and is
rejected without updating state. Arming is ready only when every configured
input is known and fresh and every required input is inactive.

Transitions preserve probe, minimum/maximum limit, motor fault, FOC fault,
E-stop, and interlock identity. The conservative policy requests a hold for a
probe and selects distinct latched fault codes for emergency stop, opened safety
interlock, hard limit, and driver fault. Operation-specific homing/probing logic
may consume a transition before this fallback only after its own later safety
qualification. A new additive safety-snapshot fault value `11` represents an
opened required interlock.

Construction independently rejects zero watchdogs, non-input resources,
duplicate physical resources, duplicate `(role, instance)` identities, absent
required E-stop/interlock gates, empty sets, and capacity overflow. Tests cover
exact debounce boundaries, bounce rejection, reversed timestamps without state
mutation, polarity, arming masks, reaction mapping, watchdog boundary, and the
new canonical snapshot fault. Compile-time regression tests bound the complete
32-input monitor and executable configuration profile to 2 KiB each.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo +esp clippy -p alumina-config \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-config \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
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

The default workspace has 202 passing unit tests. Focused configuration and
safety suites each have 15 tests. Strict host Clippy, both ESP configuration
targets, both complete ESP firmware targets, and both release links pass.
`llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 834,472 | 11,984 | 250,160 | 39,412 |
| T-Deck Pro | 784,209 | 12,728 | 525,632 | 145,988 |

The core-1 transactional validator/candidate/active profiles consume 3,096
bytes of the prior linker stack reserve on each image; BSS aggregate remains
constant because `.stack` is the linker-computed remainder. These are linked
capacity observations, not runtime stack watermarks. A later target sampler and
motion coordinator must repeat the size and runtime-watermark gates.

Default, all-feature workspace, TinyBee, and T-Deck Pro offline cargo-tree
inventories contain 64, 323, 231, and 238 nonempty package/license records.
None has a missing license or a GPL/AGPL/LGPL/SSPL-family license. An
implementation/header/manifest scan outside documentation likewise has no
GPL-family match. New code is repository-owned `MIT OR Apache-2.0`; the only new
dependency edges connect existing Alumina crates. `cargo-deny` remains enforced
in CI but is not installed locally, so no local `cargo deny` result is claimed.

## Closed physical claims

Neither board was connected, flashed, or energized. No target input is yet
configured or sampled from this profile, and no monitor transition is wired to
a physical output backend. Pull behavior, contact conditioning, noise,
interrupt/poll latency, watchdog cadence, simultaneous edges, E-stop/limit
reaction time, and safe-output application require board-specific HIL. Both
board packages remain non-armable.
