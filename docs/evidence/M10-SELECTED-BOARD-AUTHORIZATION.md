# M10 selected-board authorization evidence

## Claim

Implementation commit
`7fd220ad36c9e78ad750787eeac2a23b7e16dd7b` makes the retained
selected-board configuration facts a prerequisite for durable core-1
configuration authority and future arming.

The configuration service now offers a pure authorization preflight. It
borrows the active `RealtimeConfiguration` only when the exact command would
otherwise be admissible: mutation is allowed, the action is `Authorize`, the
transaction/digest/length identity matches, no candidate or receive operation
is present, and the active document has the same identity. The preflight does
not set the authorization bit or change any report field. Invalid or forbidden
commands are not intercepted and continue through the service's canonical
rejection path.

For MKS ESP32 FOC V1.0, the selected-board hook requires a retained
`PreparedTargetConfiguration`, independently replays that complete value from
the preflight document, and compares the two values exactly before
`RealtimeConfigurationService::apply` may authorize. A missing selection,
selection replay failure, or value mismatch returns to the permanent loop
before the service mutation. The loop latches its existing real-time identity
fault and invokes complete board safety. On the first authorization attempt,
the active document remains unauthorized and therefore supplies no job
authority. A repeated authorization attempt adds no new authority; if target
state were lost after prior authorization, the separate readiness check also
prevents arming.

Arm reconciliation also snapshots selected-board readiness. MKS readiness
requires the retained configuration digest and FOC-presence shape to match the
authorized document. TinyBee and T-Deck Pro have no additional target fact
layer, so they use pure acknowledgements; readiness is considered only when
the configuration service already exposes an authorized document.

This checkpoint activates no peripheral and changes no arming constant. Every
current board remains non-armable, and every MKS motion/servo implementation
and qualification gate remains false.

## Pure service preflight

`authorization_preflight_configuration` is deliberately narrower than a
second authorization implementation. It reuses the service's private exact
transaction match and identity match predicates and returns `None` for every
other state. `apply` remains the sole state-transition owner.

The existing end-to-end real-time configuration test now proves that:

- a valid `Authorize` command cannot borrow a candidate before activation;
- the same command borrows the exact active document after activation;
- mutation policy denial returns no preflight document;
- a substituted transaction identity returns no preflight document;
- all preflight attempts leave the canonical service report bit-for-bit
  unchanged; and
- applying the admitted command afterward performs the ordinary authorization
  transition.

This preserves the distinction between two failure classes:

| Input/failure | Service mutation | Result |
| --- | --- | --- |
| malformed, stale, or currently forbidden authorization command | none | canonical configuration rejection report |
| otherwise-admissible authorization with missing/substituted MKS target facts | none | real-time fault plus complete safe-output path |
| admissible authorization with exact retained target facts | `active_authorized = true` | exact active configuration becomes job-visible |

## MKS replay and recurring readiness

The MKS authorization hook first requires the retained prepared value installed
during activation. It then calls the same private pure constructor used for
initial target preparation. That constructor rechecks the exact compiled
capability identity, zero-or-two-axis target shape, both schematic stage and
phase-output bundles, both ADC1 pairs, both MCPWM contracts, common control
grid, cached-servo configuration, and document digest. Derived `Eq` comparison
covers the complete retained value; digest equality alone is not substituted
for this one-time replay.

After authorization, every arm reconciliation computes a fresh local input
snapshot containing:

- configuration authorization;
- selected-board target readiness;
- established safe outputs;
- safety-input readiness; and
- deadline health.

The existing job and motion readiness gates remain separate and are all
required. The fast recurring MKS target check compares the retained digest to
the immutable authorized document and checks whether retained FOC facts agree
with its zero/nonzero FOC shape. It does not repeat the full lowering on every
management tick. Configuration clear removes both service authority and the
retained selected-board facts.

No extra long-lived authorization token or boolean was added. The exact MKS
prepared selection remains 272 bytes and continues to be the sole persistent
target fact value.

## Closed target boundary

The MKS target still declares:

```text
PACKAGE.armable             = false
MOTION_OUTPUT_IMPLEMENTED   = false
MOTION_OUTPUT_QUALIFIED     = false
SERVO_OUTPUT_IMPLEMENTED    = false
SERVO_OUTPUT_QUALIFIED      = false
```

The linked MKS selector remains present as a 667-byte
`StoredFocAxisHardwareSelection::from_configuration` symbol. The complete-bank
replay, authorization validation, readiness check, and service preflight are
inlined into the permanent real-time task. The 95-byte target-selection commit
and 205-byte `force_safe_outputs` symbols remain linked.

No MKS release symbol matches `PwmCommitBankTargetOwner`,
`ClosedPwmCommitBank`, `ServoFocBank`, `activate_closed_mcpwm`,
`activate_unqualified_adc1`, or `activate_as5600_encoders`. The six phase pins
remain no-pull inputs beside sealed MCPWM singletons; no compare write, latch
report, current sample, encoder transaction, timer start, operator attachment,
or power-stage enable path is added.

## Reproducible verification

Run from the repository at the implementation commit:

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
llvm-size -A \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg '^\.(bss|stack|data|noinit|text|rodata)'
llvm-nm -S -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg 'realtime_task::POOL|authorization_preflight_configuration|validate_target_authorization|target_configuration_ready|StoredFocAxisHardwareSelection::from_configuration|commit_target_configuration|PwmCommitBankTargetOwner|ClosedPwmCommitBank|ServoFocBank|activate_closed_mcpwm|activate_unqualified_adc1|activate_as5600_encoders|force_safe_outputs'
```

The default-member listing remains 523 tests. Relevant suites include 83 FOC,
54 simulator, and 27 configuration tests. Formatting, all host tests,
warnings-denied host Clippy, warnings-denied rustdoc, diff checks, and
warnings-denied Clippy for all four ESP configurations passed.

The exact optimized images are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,112,460 | 12,432 | 249,712 | `dfe023cb43c79a6eb053b39e0ddd78b06a30d848e44f810306548e9e13d57b17` |
| TinyBee V1.0, 4 MiB variant | 1,112,480 | 12,432 | 249,712 | `4c1f4993cd271a8cb6298c7172cbcb2daf125c394f09c83aa9cbd1a02bf0d866` |
| T-Deck Pro | 1,046,317 | 13,184 | 525,184 | `728e245e51207a87da3d26f30d0180f081b855e73f622ba12f4ef97224a47813` |
| MKS ESP32 FOC V1.0 | 1,053,984 | 11,184 | 250,960 | `410c0994c08281c15ac805c44f4ee63e1511e9725f016f92e6a77929697e13f1` |

Relative to the preceding target-selection checkpoint, text changes by +484
bytes for each TinyBee variant, +92 bytes for T-Deck Pro, and -280 bytes for
MKS after optimization and inlining. Data and aggregate BSS totals are exact
and unchanged on every board. Hashes renew with source/debug identity.

No new persistent target state is introduced. Current linked static-capacity
observations are:

| Board | live `.bss` | `realtime_task::POOL` | linker-residual `.stack` |
| --- | ---: | ---: | ---: |
| TinyBee 8/4 MiB | 182,068 | 50,672 | 2,108 |
| T-Deck Pro | 176,900 | 48,528 | 110,924 |
| MKS ESP32 FOC V1.0 | 179,596 | 49,744 | 5,828 |

The MKS live `.bss`, real-time pool, and residual stack are unchanged from the
target-selection checkpoint. These are linked capacity observations, not
runtime stack watermarks. In particular, TinyBee's 2,108-byte linker residual
remains a hard optimization and measured-watermark constraint before physical
promotion.

## License and moving-Hyper boundary

This slice changes no Cargo manifest or lockfile and imports no source. The
workspace all-features Cargo-tree inventory contains 879 nonempty
package/license records, zero missing expressions, and zero
GPL/AGPL/LGPL/SSPL-family entries. New source and documentation are
repository-owned `MIT OR Apache-2.0`; no SimpleFOC, Synthetos/g2core, FluidNC,
Klipper, or other external implementation was copied or used.

Hypercurve and the other moving Hyper crates were not read, edited, formatted,
pinned, reset, or built for this firmware slice. `alumina-interface` was not
changed, and firmware still has no Hyper/CSGRS path dependency.

## Open work

- Preserve this pre-authorization replay when a future qualified target owner
  consumes the retained MKS facts; authorization alone must never activate it.
- Give selected-board authorization failures a bounded diagnostic cause if
  doing so can preserve the current fail-before-authorize property.
- Reduce TinyBee static use and measure both core stack high-water marks before
  any production or physical timing claim.
- Continue MKS encoder/ADC/PWM synchronization and aggregate-owner work only
  behind independent electrical, timing, and shutdown evidence gates.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee V1.0 was not contacted, reset, flashed, or driven. The
SLogic16U3 remained unused, and no motor, driver, analyzer, or process-power
path was energized.
