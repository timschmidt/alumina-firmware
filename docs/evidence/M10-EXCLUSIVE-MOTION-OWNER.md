# M10 exclusive motion-owner evidence

## Claim

Implementation commit
`dc4acf469c36b4ac751f8249e5e080a75fc6c065` makes the existing
stepper-versus-servo target policy structural and reclaims the inactive
executor's permanent core-1 storage.

The firmware motion composition already admitted exactly one executable
family. A stepper profile rejects any FOC axis, the servo branch rejects any
stepper axis, a job descriptor binds exactly one execution kind, and every
cross-family operation fails closed. Nevertheless, `MotionService` previously
retained complete stepper and servo actors simultaneously beside an active
family tag.

`MotionOwner` is now one allocation-free inline enum:

```text
Empty | Stepper(StepperMotionService) | Servo(ScheduledServoExecution)
```

The selected actor retains its complete axis, block, lookahead, exact
recurrence, output-horizon, staged-commit, and terminal ownership bounds. No
canonical configuration or job format changes, and this does not prohibit a
future deliberately designed mixed-family target composition; such a target
would need a different owner with explicit simultaneous hardware and safety
semantics rather than relying on the old unusable duplicate allocation.

## Transactional and fail-closed behavior

Reconfiguration remains transactional. While idle, firmware constructs and
fully validates a replacement `MotionOwner` locally. Only success replaces the
prior owner; a mixed family, malformed stepper profile, incomplete servo
profile, or target-policy failure leaves the prior configuration intact.
Configuration still rejects while a job is active.

Every lifecycle method now matches the retained variant:

- arm readiness requires the descriptor family, configuration digest, board
  armability, implementation gate, and qualification gate to agree;
- prime requires the descriptor-selected variant and still applies the same
  target implementation/qualification checks before hardware staging;
- start, block admission, poll, normal finish, next deadline, and started state
  reach only the successfully primed owner;
- a stepper request against a servo configuration remains `Unsupported`, while
  an unconfigured request remains a configuration failure;
- fault invalidates only the actor that can own tokens and clears the common
  active lifecycle after the caller has synchronously applied board safety;
  and
- clear resets the selected actor, removes the enum payload, and clears active
  state only after outputs are safe.

The underlying `StepperMotionService`, `ScheduledServoExecution`, exact
stepper/finite-difference runners, cached-servo runner, block ownership, and
hardware traits are unchanged. A target-compiled constant assertion prevents
`MotionService` from silently regressing to the sum of both actor payloads.

All current packages remain non-armable. Their motion and servo implementation
and physical-qualification constants are unchanged, so this storage change
cannot open an output path.

## Exact compiler layouts

`-Zprint-type-sizes` reports the following target-specific layouts. The
variation is solely the board's compile-time `JOB_AXES` width.

| Board composition | Axes | Stepper actor | Servo actor | old `MotionService` | `MotionOwner` | new `MotionService` | Reclaimed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| TinyBee 8/4 MiB | 3 | 6,992 | 3,288 | 10,288 | 6,992 | 7,000 | 3,288 |
| MKS ESP32 FOC V1.0 | 2 | 6,464 | 2,952 | 9,424 | 6,464 | 6,472 | 2,952 |
| T-Deck Pro | 1 | 5,952 | 2,616 | 8,576 | 5,952 | 5,960 | 2,616 |

On each target, the enum occupies exactly the larger stepper payload and the
eight remaining bytes hold the common active lifecycle plus alignment. The
complete servo actor remains available whenever a FOC configuration selects
it; only the impossible simultaneous allocation is removed.

For three-axis TinyBee, the core-1 async future falls from 46,288 to 43,000
bytes and its Embassy task pool falls from 46,328 to 43,040. The corresponding
task-pool reductions are 2,952 bytes on MKS ESP32 FOC and 2,616 bytes on T-Deck
Pro, exactly matching their omitted servo payloads.

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

With the relevant espup Xtensa GCC `bin` directory on `PATH`, repeat the
compiler-layout inspection for each board feature/target pair with:

```sh
cargo +esp rustc -p alumina-firmware --bin alumina-firmware --release \
  --no-default-features --features <board-feature> \
  --target <board-target> --locked --offline -- \
  -Zprint-type-sizes --emit=metadata
```

The default-member listing remains 524 tests, including 83 FOC, 54 simulator,
and 28 configuration tests. Formatting, all host tests, warnings-denied host
Clippy, warnings-denied rustdoc, diff checks, and warnings-denied Clippy for all
four ESP configurations passed.

The exact optimized images are:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,114,956 | 12,432 | 249,712 | `df3663fb256aa9fc9db0812a5ac5da7840a3c2450ad2db5920a6a437791ef17c` |
| TinyBee V1.0, 4 MiB variant | 1,114,976 | 12,432 | 249,712 | `757c008d45d4071f2002cd4651d094f4f2404092e175c1754724b5f12b5eaf20` |
| T-Deck Pro | 1,045,701 | 13,184 | 525,184 | `1185e78ff72e40a126236542759d71b0cb235ec13fe478d0419dd08a916e826b` |
| MKS ESP32 FOC V1.0 | 1,054,144 | 11,184 | 250,960 | `a381772acf9264560e1ca68974f89fd548a3640020c6c3811b3fd3c12d78f8fb` |

Relative to the preceding real-time configuration-storage checkpoint, text
changes by -356 bytes for each TinyBee variant, -1,332 bytes for T-Deck Pro,
and +352 bytes for MKS ESP32 FOC after target-specific optimization and
inlining. Data and aggregate BSS remain exact and unchanged. Hashes renew with
source/debug identity.

The section and symbol inspection gives:

| Board | live `.bss` | `realtime_task::POOL` | linker-residual `.stack` | live `.bss` change |
| --- | ---: | ---: | ---: | ---: |
| TinyBee 8/4 MiB | 174,436 | 43,040 | 9,740 | -3,288 |
| T-Deck Pro | 169,940 | 41,568 | 117,884 | -2,616 |
| MKS ESP32 FOC V1.0 | 172,300 | 42,448 | 13,124 | -2,952 |

Aggregate BSS reported by `llvm-size` is unchanged because these linker scripts
include the residual `.stack` section in that aggregate. Each reduction in
permanent live `.bss` and task-pool storage is matched exactly by increased
linker residual. These are static-capacity observations, not runtime stack
watermarks. TinyBee's 9,740-byte residual remains subject to measured high-water
qualification before physical promotion.

## License, hardware, network, and moving-Hyper boundary

This slice changes no Cargo manifest or lockfile and imports no source. The
workspace all-features Cargo-tree inventory remains 879 nonempty
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
  graph, stepper, and servo loads before treating linker residual as runtime
  headroom.
- If a future machine truly combines stepper and servo axes, design an explicit
  simultaneous mixed-family owner with bounded arbitration, synchronized
  epochs, and complete safety semantics instead of weakening this selection.
- Keep every output/peripheral gate closed until its separate hardware
  qualification evidence exists.
