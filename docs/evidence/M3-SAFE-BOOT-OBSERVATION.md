# M3 safe boot and safety-observation evidence

Date: 2026-08-10

Status: deterministic host-tested and ESP release-linked board composition.
No board was connected or flashed. This does not claim electrical polarity,
reset behavior, shift timing/waveforms, output loads, watchdog recovery, or
storage formatting passed HIL. Both board packages remain `Compiles` and
`armable: false`.

## Implemented claim

- `alumina-shift-register` is independently authored allocation-free `no_std`
  code under `MIT OR Apache-2.0`. It models only serial data, shift clock, and a
  rising storage-register latch. It imports no implementation or asset from
  FluidNC, Synthetos, SimpleFOC, or another firmware project and adds no external
  package dependency beyond the already locked permissive `embedded-hal`.
- A static transaction is rejected before pin activity unless its width is
  1–32 bits, its mask defines every physical output exactly, its image contains
  no out-of-mask bit, and each requested timing is between 1 ns and 1 ms. A
  successful transaction drives latch/clock/data inactive, shifts every bit,
  emits one bounded latch pulse, returns all three lines low, retains their
  ownership, and records the image only after every pin operation completed.
- TinyBee's vendor schematic shows three cascaded 74HC595-compatible devices.
  U1 receives `I2S_DATA`; U1 cascades to U2 and U2 to U3. Q128 is U1 QA and Q151
  is U3 QH, so a complete logical image enters bit 23 first and bit 0 last
  (`MostSignificantFirst`). The formerly missing logical bit 15 is schematic
  net Q143 / `LCD_MOSI`, not an absent output. Board metadata now defines all
  24 bits with mask `0x00ff_ffff`; bit 15 is non-hazardous and safe low.
- TinyBee core 1 first configures GPIO2 as an input/high-impedance driver. It
  then configures word-select/latch GPIO26 low before clock GPIO25 and data
  GPIO27, shifts the complete `0x00001249` safe image MSB-first, and uses a
  blocking `embedded-hal` delay to request at least 100 ns for data setup,
  clock high/low, latch setup, and latch high. Bits 0, 3, 6, 9, and 12 are high
  for the five described active-high StepStick disable routes; step, direction,
  heater, fan, beeper, LCD, and expansion bits are low.
- The static transaction deliberately leaves I2S0 and its DMA channel
  unconfigured but owned by the resulting real-time resource object. This is a
  boot-safe static mode, not the later motion-streaming backend. That backend
  must perform a separately tested no-glitch handoff and must continuously
  preserve complete-image composition.
- T-Deck Pro core 1 explicitly converts its only currently described hazardous
  route, vibration GPIO2, into a retained input/high-impedance driver. Its
  transistor polarity and reset behavior remain an explicit HIL requirement.
- Each board has a 16-byte semantic `SafetyContractId` encoding its relevant
  pin mode/width/order/image policy. Changing the transaction requires changing
  the identifier. This is an in-memory semantic binding, not a cryptographic
  digest or a replacement for board qualification.
- `alumina-safety` now has an exact 40-byte `ALMS` V1 snapshot encoding. It binds
  state, retained fault, known flags, wrapping transition generation, safe
  output contract, and maximum real-time lateness. Unknown states/faults/flags,
  nonzero reserved bytes, zero contracts, impossible state/fault/output facts,
  old or duplicate frame sequences, missing/old transition generations,
  future timestamps, stale timestamps, and contract mismatches are rejected.
- A rejected observation immediately revokes the prior one. An admitted
  observation becomes fail-closed `Boot` with the real-time cache treated busy
  once it is more than 500 ms old. Core 1 publishes immediately after safe
  establishment and every 100 ms thereafter. The ordinary telemetry queue may
  lose samples; it cannot make a stale state durable.
- Firmware launches core 1 before initializing the radio. Core 0 waits for a
  fresh, exact-contract `SafetyState::Safe` snapshot; only then does it
  initialize the Wi-Fi AP/web tasks. A malformed bootstrap publication sends an
  independent emergency-stop request. Safe-output establishment failure
  publishes both the independent fault mailbox and a contract-bound `Fault`
  snapshot and never enables Wi-Fi.
- `StorageServiceState` no longer exposes setters that can indefinitely assert
  a state or job flag. Every status and mutation computes its context at the
  request's current device cycle. Only fresh `Safe` or `Configured` snapshots
  with no real-time job ownership permit mutation. Snapshot rejection,
  expiration, or an independent core-1 fault immediately returns storage to
  `ForbiddenState`.

## Deterministic tests

The safety suite has ten tests, including golden snapshot round-trip, reserved
and length rejection, state/fault/output invariants, freshness at the exact
boundary, fail-closed expiration, wrong-contract/malformed/old-frame revocation,
transition-generation enforcement, and sequence wrap. The service suite has 12
tests, including expiry at the exact configured age boundary, active-job exclusion, and
revocation on malformed snapshot or fault notification. The shift driver has
four tests covering exact ordered edges and delays, rejection without pin
activity for incomplete images and invalid timing, and width-32 arithmetic. A
TinyBee metadata test requires all 24 physical bits including Q143.

The default workspace has 127 passing unit tests.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask check --board mks-tinybee
cargo xtask check --board t-deck-pro
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo tree --locked --offline --prefix none --format '{p}|{l}'
git diff --check
```

Host tests and strict Clippy, both ESP checks and strict Clippy gates, and both
optimized release links pass. Final release section totals are:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 654,264 | 11,768 | 250,368 | 63,476 |
| T-Deck Pro | 604,201 | 12,512 | 525,856 | 170,028 |

Relative to explicit cache provisioning, text grows by 6,460 bytes on TinyBee
and 5,536 bytes on T-Deck Pro; data grows by 64 bytes on each. Aggregate BSS
falls by 64 bytes on each and linker-reserved stack falls by 200/176 bytes due
to changed optimized async-future layout. These are link facts, not runtime
stack-watermark or deadline measurements.

The new implementation crate and all changed workspace packages are
`MIT OR Apache-2.0`. Offline default, TinyBee, and T-Deck dependency graphs have
neither blank-license nor GPL/AGPL/LGPL/SSPL matches. No GPL-family code,
dependency, or asset was added.

## Claim boundary and next evidence

The static GPIO shift waveform is inferred from the vendor schematic and
compile-checked HAL behavior. It has not been observed with a logic analyzer.
The 100 ns requested delays, MSB-first cascade mapping, every Q128–Q151 output,
StepStick disable polarity, heater/fan inactivity, GPIO2 impedance, boot-ROM
interval before application control, and fault/reset behavior must be measured
with disconnected loads before bench promotion. T-Deck vibration polarity must
likewise be measured.

No hardware proof means this change does not make either board armable and does
not authorize connecting motion or process-power loads. Explicit provisioning
is now reachable in the compiled state machine, but its first hardware use must
be on sacrificial media with the HIL sequence from `VERIFICATION.md`.

The next real-time slice should add and simulate immutable cache-prefetch and
command admission while retaining this freshness gate, then implement the
TinyBee I2S/DMA complete-image streaming backend with a no-glitch transition
from the static bootstrap. Logic-analyzer capture under saturated Wi-Fi/SD load
is required before motion qualification.
