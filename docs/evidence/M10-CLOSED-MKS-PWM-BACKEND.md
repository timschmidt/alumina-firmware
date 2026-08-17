# M10 closed MKS aggregate PWM backend evidence

## Claim

Implementation commit
`77595f6fc6745812fe6b2f2abda61a42ccedfe28` implements the first MKS ESP32
FOC V1.0 `PwmCommitBankHardware<2>` target seam at exactly the capability that
can be claimed before powered hardware qualification: complete synchronous
safety only.

The borrow-scoped `ClosedPwmCommitBank` uniquely covers both retained target
stage owners. Complete-image staging and physical-latch observation always
return `Unqualified`; no token, report, or completion can be produced. Its
sole successful operation reasserts both stages safe as one call. Raw stages
reapply all six phase routes as no-pull GPIO inputs. The separately compiled
configured-but-disconnected stage state additionally stops and resets both
MCPWM timer-0 counters.

The permanent core-1 `ServoSetpointOutput` rejection seam now invokes the
selected board's synchronous safe transaction before rejecting either an
attempted setpoint stage or commit read. All board arm and output-qualification
gates remain false. No target `PwmCommitBankTargetOwner`, FOC controller, image
write, operator attachment, timer start, or latch-success path was added.

This is static ownership, compile, and linked-safe-path evidence. It is not an
MCPWM timing, pin-level, gate-driver, shutdown-latency, or motor-control
qualification.

## Closed target ownership

The existing MKS boot partition already gives core 1 unique ownership of:

- both MCPWM peripheral singletons;
- GPIO32/33/25 for motor 0 and GPIO26/27/14 for motor 1;
- the four ADC1 current-sense routes;
- both independent encoder buses and their auxiliary inputs; and
- the otherwise empty target safety-input bank.

`establish_safe_outputs` consumes the six phase-pin singletons into no-pull
`Input` values before the realtime loop begins. Each MCPWM singleton remains
sealed beside its three phase inputs in `ClosedPowerStage`; there is no token
extractor and no output or operator construction method.

The new sealed `ClosedPwmStageSafe` operation is implemented for only two
internal ownership states:

| State | Safe operation | Deliberately absent |
| --- | --- | --- |
| `ClosedPowerStage<Pwm>` | Reapply all three no-pull input configurations | MCPWM initialization, compare write, timer operation, pin attachment |
| `ClosedMcpwmStage<Pwm>` | Reapply three inputs, stop timer 0, set counter 0/increasing | Operator construction, pin attachment, timer start, latch claim |

The public trait is documentation-hidden and sealed, so another module cannot
invent a stage type with a weaker interpretation. The phase fields and
controller remain private.

`ClosedPwmCommitBank` holds exclusive mutable borrows of both stage owners for
the duration of the aggregate action and implements the portable hardware
contract as follows:

| Operation | Result |
| --- | --- |
| `stage_images(&[PwmCompareImage; 2])` | Always `Err(Unqualified)`; writes nothing |
| `take_latch(axis)` | Always `Err(Unqualified)`; manufactures no report |
| `force_safe()` | Applies both sealed stage-safe operations, then `Ok(())` |

The backend is private to the MKS target module. Future activation may pass a
transferred aggregate backend to `PwmCommitBankTargetOwner` only at the exact
first physical boundary. The current borrow-scoped form is intentionally not
stored self-referentially in permanent resources and no owner is constructed
during configuration: doing so would bind a stale boundary before an FOC
candidate exists and would obstruct ordinary clear/reconfigure ownership.

## Permanent rejection seam

All current target compositions share the permanent
`ServoSetpointOutput<JOB_AXES>` implementation used by
`ScheduledServoExecution`. Both methods now call `force_safe_outputs` before
returning `Err(())`:

- `stage_servo_setpoints` cannot retain a setpoint mailbox after an internal
  gate bypass; and
- `take_servo_setpoint_commit` cannot report a commit from an absent backend.

Normal admission rejects earlier because `PACKAGE.armable`,
`SERVO_OUTPUT_IMPLEMENTED`, and `SERVO_OUTPUT_QUALIFIED` are all false for the
MKS package. The added calls are defense in depth for internal misuse, not a
new execution route. On the MKS runtime type, they reach the complete six-input
safe transaction. TinyBee and T-Deck Pro similarly reapply their existing
board-specific safe outputs before rejection.

The public MKS board API continues to expose
`SafeOutputError::MotionUnsupported` for unavailable output operations; the
private backend's `Unqualified` cause cannot be mistaken for a successful stage
or observation.

## Verification

From the `alumina-firmware` repository at the implementation commit:

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
  rg 'force_safe_outputs|PwmCommitBankTargetOwner|ClosedPwmCommitBank|ServoFocBank'
```

The default-member listing remains 523 tests. Relevant host checks include the
MKS board-package assertion that the package validates but cannot arm. All host
tests, formatting, warnings-denied host Clippy, warnings-denied rustdoc, and
warnings-denied Clippy for all four ESP configurations passed.

The exact optimized images are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,976 | 12,432 | 249,712 | `f4a8076e8156cadb92344a9b1ec5ecc5a1af8e561ef9900a04b43a44c83b2143` |
| TinyBee V1.0, 4 MiB variant | 1,111,996 | 12,432 | 249,712 | `df5ecec640489141723e61f0f885807997b2e2b6f0c183d752d3dea73e2e7cb6` |
| T-Deck Pro | 1,046,225 | 13,184 | 525,184 | `426d27dc3b82fd6c55939015cd5682ee3247b33eb5b5ca3494939055f5e1b3ba` |
| MKS ESP32 FOC V1.0 | 1,048,112 | 11,184 | 250,960 | `79da481c3a29bd16f79a0fe67e13585f08db674924b4eab1bd55ea281b123ed6` |

Relative to the preceding owner checkpoint, TinyBee text changes by -8 bytes,
the T-Deck Pro by +68 bytes, and the MKS FOC target by +1,384 bytes. Data and
BSS remain exact and unchanged. Shared-target differences follow from routing
the permanent rejection seam through each board's safe transaction.

`llvm-nm -S -C` reports a linked 175-byte MKS `force_safe_outputs` symbol. It
reports no `PwmCommitBankTargetOwner`, `ClosedPwmCommitBank`, or `ServoFocBank`
symbol. The aggregate safety implementation is inlined; the always-rejecting
stage/latch methods and absent controller path are unreachable and eliminated.
The linked safe symbol is static evidence that real boot/fault safety code is
retained, not evidence that a physical pin reached a safe electrical state.

## License and moving-Hyper boundary

This slice changes no manifest or lockfile and imports no source. The
all-features Cargo-tree inventory remains 879 nonempty license records, zero
missing expressions, and zero GPL/AGPL/LGPL/SSPL-family entries. New code and
documentation are repository-owned `MIT OR Apache-2.0`; no SimpleFOC,
Synthetos/g2core, FluidNC, Klipper, or other external implementation was used.

Hypercurve and the other moving Hyper crates were not read, edited, formatted,
pinned, reset, or built for this target slice. `alumina-interface` was not
changed, and firmware still has no Hyper/CSGRS path dependency.

## Open physical and activation work

- At the future exact first boundary, transfer both stages into a permanent
  non-self-referential aggregate owner together with the prepared FOC bank.
- Add shadow compare ownership and truthful latch/readback only after the HAL
  and hardware semantics are independently established; do not reinterpret a
  software write as physical completion.
- Qualify cross-MCPWM epoch alignment, operator/dead-time configuration,
  PWM-synchronous current sampling, encoder correlation, and WCET/jitter.
- Measure the six-pin safe action through reset, watchdog, brownout, staged
  write failure, observation failure, and logical-publication failure before
  changing any arm or qualification constant.
- Perform disconnected-load and then powered-load work on a physically reviewed
  MKS ESP32 FOC V1.0 board when that hardware is available.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven. The
SLogic16U3 remained unused, and no motor, driver, analyzer, or process-power
path was energized.
