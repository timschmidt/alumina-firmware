# M7 prestart hardware-priming evidence

Date: 2026-08-11

Status: canonical schedule version 2, fail-closed prestart hardware ownership,
scheduled firmware composition, and a compile-only TinyBee PCM-short HAL surface
are implemented. This is software and link evidence. It is not an original-
ESP32 I²S phase, DMA-refill, physical-latch, static/stream handoff, safe-stop,
cross-block-horizon, synchronization, armability, or machine qualification
claim.

The later [cross-block prefill checkpoint](M6-CROSS-BLOCK-PREFILL.md) closes the
bounded software-horizon item without changing any target or physical claim
above.

## Schedule authority

The version-2 schedule family deliberately changes all three canonical domains:
`ALMJCOM2`, `ALMJREF2`, `ALMJSCH2`, and the prepared-token domain. There is no
version-1 decoder, translation, compatibility shim, or negotiated downgrade.
The fixed body sizes and committed report union do not change.

Core 1 now owns the exact lifecycle:

```text
Prepared -> Installed -> Confirmed -> Priming -> Primed -> Running
                                                    |          |
                                                    v          v
                                                  Fault   Complete/Fault
```

`Aborted` and unconfirmed `Expired` remain terminal side paths. Install rejects
a schedule unless `start - abort_guard` meets the board's nonzero prime-lead
policy. At the guard, `Confirmed` emits exactly one `PrimeHardware` action and
becomes irrevocable over Wi-Fi. The hardware owner must acknowledge before the
local start cycle. Reaching start in `Confirmed` or `Priming`, or acknowledging
at or after start, latches `MissedStart`; it never turns a late software poll
into a different physical start.

The deterministic two-MCU simulator now exercises the guard, prime, explicit
acknowledgement, and clock-derived start on both distinct affine clocks. Unit
tests additionally prove insufficient prime lead rejects without installing,
early acknowledgement is illegal, acknowledgement is idempotent only after
success, late acknowledgement faults, unprimed start faults, and no start fact
is emitted on those paths.

This protocol does not claim atomic distributed start. A participant can fail
locally after the abort guard while an already-primed peer proceeds. A machine
whose hazard analysis requires coordinated start or stop needs an appropriately
rated hardwired interlock; a last-moment Wi-Fi packet is not a substitute.

## Firmware ownership

The realtime firmware now instantiates
`ScheduledShiftedStepper<AXES, OUTPUTS>` rather than the synchronous
single-pending-image coordinator. The prime action transfers the one admitted
block and descriptor-bound machine-lattice origin into that sole owner, binds
the exact scheduled epoch, asks the selected board for writable future capacity,
generates and stages bounded sparse complete images, then requires a separate
target seal covering every changed and unchanged frame through the prime
horizon. A full sparse ring or a first block ending before that boundary fails
instead of overstating runway. `Primed` is reported only after the seal returns
at least the requested exact cycle. Start accepts only the same primed epoch
after the schedule has independently entered `Running`. Physical commit tokens
retire in strict order; block acknowledgement, output-free terminal dwell,
normal disable, and schedule completion remain behind target-reported latch
observations.

The subsequent [circular-DMA checkpoint](M6-CIRCULAR-DMA-HORIZON.md) makes the
portable target boundary more concrete: safe-prefilled frame slots cross an
exact release/preview/push/accept ownership path, while latch observation remains
independent. It also preplans terminal disable before final-block release. That
checkpoint does not change this document's closed hardware claims.

Safety remains authoritative throughout. A prime error latches a driver fault.
Limit/E-stop/interlock/deadline faults first request the board-safe transaction,
then invalidate all planned tokens and admitted ownership. Because a future
qualified backend must also reclaim an already-primed immutable timeline safely,
that transition remains a physical HIL exit gate.

Neither board can enter this path. Both publish
`MOTION_MINIMUM_PRIME_LEAD_CYCLES = u64::MAX`, so the service and realtime
admission checks reject every representable schedule. Both also retain
`MOTION_OUTPUT_QUALIFIED = false` and non-armable packages. TinyBee's one-cycle
output quantum and 20,000-cycle prime horizon are structural placeholders, not
measured hardware properties; T-Deck Pro exposes no motion backend.

## Compile-only TinyBee HAL composition

An unreachable TinyBee-only module consumes the established static-safe owner
and proves the pinned permissive `esp-hal` API can compose original-ESP32 I²S0,
`DMA_I2S0`, GPIO25 BCLK, GPIO26 WS, and GPIO27 data as a blocking PCM-short
transmitter. It configures 32-bit mono slots at a model-only 250 kHz frame rate
and prefills 256 internal-SRAM words with the complete safe image before a
circular transfer can be borrowed. The follow-on compile surface divides that
buffer into one four-byte descriptor per complete modeled frame and exposes
whole-frame availability plus exact single-frame push operations. A separate
compile-only operation writes two safe samples after a stopped circular guard
so the modeled one-frame pipeline has a following latch boundary.

No boot, runtime, fault, or arm path can construct this owner. The selected rate,
channel duplication, byte order, first-frame behavior, peripheral stop phase,
descriptor refill, commit interrupt, and safe static-to-stream transfer are not
accepted hardware facts until captured on the available logic analyzer with all
motor and process loads disconnected.

## Reproduced checks

Run from the repository root:

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

The default workspace passes 227 unit tests; focused `alumina-job`,
`alumina-motion`, and `alumina-sim` suites pass 14, 24, and 20 tests. Strict host
and both-target Clippy gates pass, and both optimized images link. `llvm-size`
reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 858,904 | 11,976 | 250,160 | 32,092 |
| T-Deck Pro | 800,141 | 12,728 | 525,632 | 139,060 |

These are linked-capacity observations, not runtime stack watermarks or timing
evidence.

## Licensing and source boundary

All implementation and documentation in this checkpoint is independently
authored under `MIT OR Apache-2.0`. The HAL and every resolved implementation
dependency are permissively licensed. Sorted deduplicated default, all-feature,
TinyBee, and T-Deck Pro inventories contain 65, 323, 234, and 241 nonempty
package/license records. None has a missing license or a
GPL/AGPL/LGPL/SSPL-family license. The implementation-source/header/manifest
scan is clear.

No GPL-family repository was cloned, vendored, fetched by a build/test tool, or
used as implementation input. Synthetos/g2 remains only a license-identified
behavioral-reference URL in the clean-room ledger and was not consulted for this
slice. No FluidNC, Klipper, or other GPL-family source was inspected. The local
environment does not have `cargo-deny` installed; CI remains configured to run
its license, bans, and source checks, so this checkpoint makes no local
`cargo deny` claim.

Functional hardware facts were taken from Espressif's public programming guide
and technical reference manual, the public `esp-hal` documentation, and the
locally cached source of the already locked permissive `esp-hal` 1.0.0
dependency. Those sources and the cross-chip-diagram limitation are recorded in
`docs/SOURCES.md`.
