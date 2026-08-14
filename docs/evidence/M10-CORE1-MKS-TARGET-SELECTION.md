# M10 core-1 MKS target-selection evidence

## Claim

Implementation commit
`37b96aa0dda792cd6604ceda7aea6aad52c7447c` makes selected-board target
fact preparation and retention part of the permanent core-1 configuration
lifecycle.

On MKS ESP32 FOC V1.0, a pure prepared value accepts either a resource-free
configuration or exactly the canonical two-axis FOC topology. The FOC case
replays the existing `StoredFocHardwareBankSelection`, including both fixed
power-stage identities, all six phase-output bindings, both ADC1 current pairs,
both stopped-MCPWM contracts, one common exact loop grid, and the complete
cached-servo admission profile. The prepared value and its cached servo facts
must carry the active validated document digest. Commit retains that immutable
selection beside the still-closed target resources; configuration clear drops
the selection without releasing any hardware singleton.

TinyBee and T-Deck Pro implement the same selected-board hook with zero-sized
acknowledgements because neither adds a second target fact layer at this
boundary. Their optimized section totals are unchanged.

This checkpoint does not initialize or operate I2C, ADC1, or MCPWM, attach an
operator to a phase pin, write a compare value, construct a FOC/PWM owner,
start a timer, enable a power stage, authorize a configuration, or start a job.
Every arm and output-qualification gate remains false. It is static selection,
ownership, compile, and linked-image evidence, not physical qualification.

## Core-1 lifecycle

The selected-board boundary has three operations:

1. `prepare_target_configuration(&self, configuration)` reads only an already
   validated `RealtimeConfiguration` and returns an owned prepared fact value.
2. `commit_target_configuration(&mut self, prepared)` retains that value only
   after portable motion configuration and configuration-derived safety-input
   setup have succeeded.
3. `clear_target_configuration(&mut self)` drops only the retained facts while
   every target peripheral remains in its established closed type state.

For an activation command, the permanent actor now orders its local work as:

```text
validated candidate becomes Active in the configuration service
  -> pure selected-board preparation
  -> portable motion configuration
  -> target safety-input configuration
  -> selected-board fact retention
  -> safety Configure transition
```

Configuration authorization remains a later, separate command. Job admission
still requires that authorization and the existing arm/output gates.

This is a two-phase transaction for target facts, not a claim that the whole
configuration actor rolls back to its prior service state. The configuration
service has already reported `Active` when target preparation begins. Any
target, portable-motion, safety-input, retention, or later safety-transition
error returns failure to the permanent loop; that loop latches a real-time
identity fault and executes its existing complete safe-output path. No failure
is converted into authorization or motion authority.

## Exact MKS selection

MKS preparation rejects:

- any stepper axis;
- one FOC axis, more than two FOC axes, or mixed stepper/FOC topology;
- a capability digest other than the compiled MKS ESP32 FOC V1.0 package;
- a missing, duplicated, reordered, or substituted motor instance;
- any phase output, power-stage device, ADC1 route, or phase-pair substitution;
- ADC calibration outside the fixed 12-bit ADC1 range;
- inconsistent PWM/current synchronization, MCPWM compare contracts, or
  servo-loop grids; and
- any cached-servo identity that does not equal the active configuration
  digest.

The only accepted shapes are:

| Validated shape | Retained MKS target facts |
| --- | --- |
| zero stepper, zero FOC axes | digest plus `None` |
| zero stepper, exactly two canonical FOC axes | digest plus complete `StoredFocHardwareBankSelection` |

`PreparedTargetConfiguration` is 272 bytes in the MKS release image according
to DWARF. It is carried as an `Option` in `EstablishedRealtimeResources` and is
preserved across the existing AS5600, ADC1, and closed-MCPWM type-state
transitions. Preparation borrows those resources immutably and does not call
any transition. Commit performs only validation of the sealed prepared value
and assignment into the retained option.

The equivalent prepared types in both the TinyBee and T-Deck Pro release
images have a DWARF byte size of zero. Their prepare/commit/clear operations do
not mutate target state.

## Closed physical boundary

The MKS target continues to declare:

```text
PACKAGE.armable             = false
MOTION_OUTPUT_IMPLEMENTED   = false
MOTION_OUTPUT_QUALIFIED     = false
SERVO_OUTPUT_IMPLEMENTED    = false
SERVO_OUTPUT_QUALIFIED      = false
```

The selected MKS hardware remains owned as six no-pull phase inputs, two sealed
MCPWM singletons, four typed current-input routes, dormant encoder resources,
and an empty safety-input bank. The separately compiled state-transition
methods remain unreachable from the configuration hook.

The optimized MKS symbol table contains:

- a 667-byte `StoredFocAxisHardwareSelection::from_configuration` selector;
- a 95-byte `commit_target_configuration` retention path; and
- a 205-byte `force_safe_outputs` path.

The complete bank selector and preparation path are inlined into the permanent
real-time task. No linked symbol matches `PwmCommitBankTargetOwner`,
`ClosedPwmCommitBank`, `ServoFocBank`, `activate_closed_mcpwm`,
`activate_unqualified_adc1`, or `activate_as5600_encoders`. Symbol elimination
is evidence that no such target path is linked; it is not evidence of physical
electrical behavior.

## Reproducible verification

Run from the `aluminafw` repository at the implementation commit:

```sh
cargo fmt --all -- --check
cargo test --locked --offline
cargo test --locked --offline -- --list
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

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
llvm-nm -S -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg 'PreparedTargetConfiguration|StoredFocHardwareBankSelection|StoredFocAxisHardwareSelection|prepare_target_configuration|commit_target_configuration|PwmCommitBankTargetOwner|ClosedPwmCommitBank|ServoFocBank|activate_closed_mcpwm|activate_unqualified_adc1|activate_as5600_encoders|force_safe_outputs'
llvm-dwarfdump --name='PreparedTargetConfiguration' \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-tinybee-v1 \
  target/xtensa-esp32s3-none-elf/release/alumina-firmware-t-deck-pro \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1
llvm-size -A \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg '^\.(bss|stack|data|noinit|text|rodata)'
llvm-nm -S -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg 'realtime_task::POOL|__realtime_task_task_inner_function'
```

The default-member listing remains 523 tests. Relevant suites include 83 FOC,
54 simulator, and 27 configuration tests. Formatting, all host tests,
warnings-denied host Clippy, warnings-denied rustdoc, diff checks, and
warnings-denied Clippy for all four ESP configurations passed.

The exact optimized images are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,976 | 12,432 | 249,712 | `e876d1a5c7f2c063e3980197c48b3d5eb18bff5d0852008f15c51dfcb6540a1c` |
| TinyBee V1.0, 4 MiB variant | 1,111,996 | 12,432 | 249,712 | `7c98cbab35a364c1a97a4011d550b58e932ee634d1c649c1b16a1ce451e4bfb3` |
| T-Deck Pro | 1,046,225 | 13,184 | 525,184 | `bcace0a44234ac009180295efe3163532b9c2e9b413c31497c515fa8860816dc` |
| MKS ESP32 FOC V1.0 | 1,054,264 | 11,184 | 250,960 | `2948ea1d801536f9fcb5cc424a27be61fc928a1467864ccb3c35c93f93ba66c2` |

Relative to the preceding closed-backend checkpoint, both TinyBee variants and
T-Deck Pro retain identical text, data, and BSS totals. MKS text grows by 6,152
bytes as exact target selection becomes reachable. MKS data and the
`llvm-size` aggregate BSS total remain unchanged, but that aggregate includes
the linker-residual `.stack` section and must not be interpreted as free static
storage. An isolated rebuild of the immediately preceding implementation shows
the exact internal transfer:

| MKS release section/symbol | preceding | current | delta |
| --- | ---: | ---: | ---: |
| `.bss` | 179,324 | 179,596 | +272 |
| `realtime_task::POOL` | 49,472 | 49,744 | +272 |
| linker-residual `.stack` | 6,100 | 5,828 | -272 |

The pool increase equals the 272-byte MKS prepared value exactly. The residual
stack measurement is a linked capacity observation, not a runtime stack
watermark; it remains an explicit optimization and HIL-measurement constraint.
Hashes renew with source/debug identity on every image.

The preceding values were reproduced from an isolated `git archive` of
`dd6aa1a2d07d4c92f7b4c5fa19aa6cbaef2816e6`, built with the same MKS
release command and inspected with the same `llvm-size -A` and `llvm-nm`
commands. The archive lived outside the working tree; no checkout, reset, or
moving sibling repository was involved.

## License and moving-Hyper boundary

This slice changes no Cargo manifest or lockfile and imports no source. The
workspace all-features Cargo-tree inventory has 879 nonempty package/license
records, zero missing expressions, and zero GPL/AGPL/LGPL/SSPL-family entries.
New code and documentation are repository-owned `MIT OR Apache-2.0`; no
SimpleFOC, Synthetos/g2core, FluidNC, Klipper, or other external implementation
was copied or used.

Hypercurve and the other moving Hyper crates were not read, edited, formatted,
pinned, reset, or built for this firmware slice. `alumina-interface` was not
changed, and firmware still has no Hyper/CSGRS path dependency. Continuing
sibling Hypercurve edits cannot perturb this exact implementation commit.

## Open physical and activation work

- Define a permanent non-self-referential MKS owner transfer at the exact first
  physical activation boundary; do not borrow a backend from enclosing
  resources.
- Join prepared target facts to qualified encoder, ADC, PWM, current-loop, and
  aggregate-commit owners only after their individual evidence gates pass.
- Establish truthful compare staging and latch observation without treating a
  software write as physical completion.
- Qualify cross-MCPWM epoch alignment, dead-time/operator routing,
  PWM-synchronous current sampling, encoder correlation, WCET/jitter, and every
  safe-state transition before changing any gate.
- Exercise disconnected-load and then powered-load procedures only on a
  physically reviewed, available MKS ESP32 FOC V1.0 board.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee V1.0 was not contacted, reset, flashed, or driven. The
SLogic16U3 remained unused, and no motor, driver, analyzer, or process-power
path was energized.
