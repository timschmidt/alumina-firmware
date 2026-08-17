# M10 canonical dual-axis MKS servo configuration evidence

## Claim

Implementation commit
`a9a201faf3c08818d424e542248f425da9eee9f7` constructs and independently
replays one canonical Configuration V6 document for both MKS ESP32 FOC V1.0
motor stages. The same private `RealtimeConfiguration` supplies both cached
servo admission facts and two complete simulator FOC-axis controllers.

The target composition adds a closed dual-stage selection which accepts only
the compiled MKS capability, logical axes 0 and 1, the two fixed schematic
stage/ADC/encoder routes, and one common exact servo/PWM/current-sampling grid.
It exposes configuration facts but no pin transition, compare write, gate
enable, `PowerStage`, or output-authority transition.

This is a canonical configuration, lowering, and deterministic simulator
checkpoint. It is not physical qualification of either power stage, MCPWM,
ADC, encoder, shutdown path, motor, or powered board.

## Exact document

The canonical fixture is 5,072 bytes and has this immutable identity:

`c2b780f774cccb37875bf9778819316e3d7d2e682f14aa74749b808478cd2b4b`

Its independently derived summary is:

- schema version 6;
- 78 records and 78 realtime-relevant records;
- 15 retained resource claims, including the two shutdown-stage claims;
- two FOC axes and zero stepper axes;
- `MOTION | FIELD_ORIENTED_CONTROL`; and
- one required emergency-stop input, declared once at GPIO15.

The two complete axis groups are distinct:

| Logical axis | Power stage | Phase outputs | Current inputs | Encoder |
| --- | --- | --- | --- | --- |
| 0 | `POWER_STAGE_0` | MCPWM engine 0, channels 0/1/2 | ADC1 channels 3/0 | `ENCODER_0` |
| 1 | `POWER_STAGE_1` | MCPWM engine 1, channels 0/1/2 | ADC1 channels 7/6 | `ENCODER_1` |

Both axes retain the same exact control lattice: an 80 MHz device clock,
20 kHz PWM/current rate, 4,000-device-cycle PWM period, velocity divider 20,
position divider 10, and therefore an 800,000-cycle cached-servo update period.
Both lower to an 80 MHz center-aligned counter, peak 2,000, minimum active
width 8, 160 MHz peripheral source, peripheral prescaler 1, timer prescaler 0,
and the same compare-quantization contract. The single global timer-tick scalar
is not duplicated under axis 1.

The configuration regression freezes the complete document digest and proves
both lowerings retain that digest, the same servo grid, identical PWM compare
contracts, and identical current-acquisition synchronization. Slot 2 rejects
as `IncompleteAxis`.

## Identity/profile ownership

`ConfigurationStreamValidator::finish_configuration` now returns the sole
executable `RealtimeConfiguration` directly after the complete stream hash and
semantic replay succeed. Its identity and fixed-capacity realtime profile
remain private fields, so an external host or simulator cannot construct the
container or accidentally pair an identity from one byte stream with the
profile from another.

The existing tuple-returning validator remains available to the internal
core-1 configuration service. The new API is the safe host/simulator path and
does not bypass any syntax, capability, resource, electrical, exact-fact,
completeness, or SHA-256 validation.

## Negative topology proofs

Mutations of the otherwise valid document fail before an executable
configuration is returned:

- mapping axis 1 phase U onto engine 0/channel 0 returns the exact
  `DuplicateResource(TimedOutput { engine: 0, channel: 0 })` cause;
- reusing `POWER_STAGE_0` for axis 1 returns
  `DuplicateResource(Device(POWER_STAGE_0))` before a shutdown topology could
  become ambiguous; and
- deleting the axis 1 encoder binding returns `IncompleteAxis`.

The target-only `StoredFocHardwareBankSelection` applies additional compiled
hardware closure. It rejects a foreign capability digest, any axis count other
than the complete two-axis cached-servo family, logical instances other than
`[0, 1]`, reversed or substituted physical stages, wrong phase or ADC routes,
non-AB current reconstruction, a non-4095 ADC range, unequal servo grids,
unequal PWM compare contracts, unequal current-sampling synchronization, or a
digest disagreement between hardware and cached-servo facts.

## Same-document simulator seam

The simulator independently constructs and streams the same 5,072 bytes in
173-byte chunks against a test-local MKS capability package. It asserts the
same frozen digest before calling `finish_configuration`.

From that one returned value it then:

1. derives `CachedServoConfiguration<2>` and its exact 800,000-cycle command
   cadence;
2. lowers slots 0 and 1 through
   `ConfiguredServoFocHardwareBank<2>::from_configuration`;
3. activates them under distinct boot-local identities at device cycle 80,000;
4. supplies both axes with digest-bound setpoints, encoder observations, raw
   current counts, and rotor counts; and
5. observes one complete simultaneous transaction from cycle 80,000 to the
   common timer-zero boundary at cycle 84,000.

Both controllers advance to period sequence 1 only after both candidate
transitions and both modeled commits succeed. Existing regressions separately
prove that a late axis-1 timer zero or missing axis-1 encoder observation leaves
axis 0 unadvanced.

The simulator power-stage qualification is deliberately synthetic. Its local
copy changes the two power-stage descriptors from `Described` to `Qualified`
and recomputes the capability digest solely so the complete semantic pipeline
can be tested. The compiled board package is unchanged and still describes
both power stages as unqualified.

## Closed target boundary

All MKS output gates remain exactly closed:

- `MOTION_OUTPUT_IMPLEMENTED = false`;
- `MOTION_OUTPUT_QUALIFIED = false`;
- `SERVO_OUTPUT_IMPLEMENTED = false`;
- `SERVO_OUTPUT_QUALIFIED = false`;
- maximum commit lateness remains zero; and
- minimum servo prime lead remains the impossible `u64::MAX` value.

The aggregate target selection returns only two per-axis route selections, an
`Adc1AcquisitionConfiguration`, two stopped/disconnected
`ClosedMcpwmConfiguration` values, and the cached-servo configuration. It does
not consume the closed GPIO inputs or MCPWM singletons and cannot energize a
stage.

The optimized MKS symbol scan contains the already used
`CachedServoConfiguration::from_configuration` path but no
`StoredFocHardwareBankSelection`, `StoredFocAxisHardwareSelection`, or
`ServoFocBank` symbol. Link-time elimination of the new closed selector is
expected while the target output gates remain false; this is closed-gate
evidence, not target execution evidence.

## Reproduction

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
llvm-nm -C \
  target/xtensa-esp32-none-elf/release/alumina-firmware-mks-esp32-foc-v1 | \
  rg 'StoredFocHardwareBankSelection|StoredFocAxisHardwareSelection|ServoFocBank|CachedServoConfiguration.*from_configuration'
```

The default-member listing contains 509 tests. The relevant crate totals are
27 configuration, 70 FOC, and 53 simulator tests. Formatting, diff checks,
warnings-denied host Clippy, warnings-denied rustdoc, and warnings-denied
Clippy for all four ESP packages passed.

All four optimized images linked with section totals unchanged from the prior
multi-axis bank checkpoint:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,111,984 | 12,432 | 249,712 | `e01c4a70028361369d55ff7a5c9c0af83274a18120ad4a3670339afd6144080a` |
| TinyBee V1.0, 4 MiB variant | 1,112,004 | 12,432 | 249,712 | `579161f6455b402a79caa245c125b436bd1276ef8206429bf39b0310143fe027` |
| T-Deck Pro | 1,046,157 | 13,184 | 525,184 | `0702c02f3757f348e5607dd8d6992dabc77acf5e5f066cb7a12be5a98f8f17dd` |
| MKS ESP32 FOC V1.0 | 1,046,728 | 11,184 | 250,960 | `febe30b357c54e262c3997582c375b77901fba7987c1b72162530ebebd368333` |

The hashes renew because the exact source/debug identity changed; unchanged
text/data/BSS totals and absent selector symbols demonstrate that no target
output path was added to the optimized images.

## License and moving-Hyper boundary

The only dependency-graph change is a simulator dev-dependency on the existing
local `board-mks-esp32-foc-v1` workspace package. No registry or Git package was
added. Changed Rust, manifest, and lockfile lines have no GPL, AGPL, LGPL, SSPL,
SimpleFOC, Synthetos/g2core, FluidNC, or Klipper implementation reference. New
code remains repository-owned `MIT OR Apache-2.0`.

`cargo-deny` is configured in CI but is not installed locally, so no local
`cargo deny` result is claimed. Its permissive allowlist and GPL-family
exclusion remain mandatory.

This slice did not read, edit, format, pin, reset, or build Hypercurve and did
not change `alumina-interface`. Firmware still has no Hyper/CSGRS path
dependency. Continuing sibling Hypercurve edits therefore do not affect this
closed firmware/configuration checkpoint.

## Open physical work

- Replace the test-local qualification with independently reviewed physical
  evidence for both real power stages and the all-stage high-impedance shutdown
  transaction, including reset and watchdog behavior.
- Establish each axis's homing and multi-turn encoder seed authority.
- Implement the sole core-1 owner which prepares both MCPWM images, aligns both
  timer-zero epochs, correlates all ADC/encoder observations, and reports a
  commit only after the whole hardware set is physically observed.
- Measure ADC timing, encoder availability, cross-MCPWM skew, shutdown latency,
  current-loop WCET/jitter, stack high-water, static placement, and commit
  reporting under concurrent Wi-Fi/web load.
- Keep every MKS output implementation/qualification gate closed until those
  reviews and measurements pass.

No network interface was changed, no Wi-Fi association was attempted, and the
connected bare TinyBee was not contacted, reset, flashed, or driven. No motor,
driver, or process-power path was energized.
