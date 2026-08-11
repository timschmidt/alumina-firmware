# M4 canonical machine-configuration IR evidence

Date: 2026-08-10

Status: portable canonical schema, exact resource/machine validation, real cache
publication streaming, independent core actor, and host tests. This does not
claim firmware routing, durable activation, board armability, or HIL.

## Implemented boundary

`alumina-config` introduces the content-addressed `ALMCFG01` V1 authority:

- 80-byte header with exact board-capability digest, record counts, policy, and
  derived length;
- fixed 64-byte strictly ordered resource bindings and exact-scalar records;
- reduced signed rationals and separate reduced nonnegative uncertainty;
- typed pin, shifted-output, ADC, timer, RMT/timed-output, UART/I2C/SPI/TWAI,
  PCNT, storage, safety, fitted-device, stepper, process, capture, and FOC roles;
- exact mechanics, calibration, range/dynamic, encoder, motor, PWM/current-sense,
  control-rate, reaction, duration, and timer facts needed by browser CAM;
- resource existence/namespace, ownership, duplicate, safe-state, hazardous-use,
  electrical-polarity/mode, bus-rate, timing, watchdog, axis-completeness, exact
  range, safety, and policy validation; and
- canonical 96-byte published-object selection plus 64-byte commit/rollback
  identity requests.

Storage object kind `6` is now `MachineConfiguration`; publication remains inert.
No existing object number changed.

## Independent validation path

`ServiceConfigurationValidation` opens an exact `PublishedObject` through the
real power-loss-safe `ProvisionedCache` reader. It accepts at most one verified
SD chunk per step, validates all bytes on core 0, and splits the same bytes into
contiguous 192-byte maximum intercore data commands. Begin/data/finish/activate/
clear/abort command forms fit exactly within the existing 256-byte command
payload and bind transaction, SHA-256, length, and offset.

`RealtimeConfigurationService` rehashes and reruns the complete validator. It
rejects gaps, identity changes, malformed records, early activation, mutation in
a forbidden state, and stale clear. A replacement candidate can be receiving or
rejected while the prior active identity remains separately reported. The fixed
128-byte report exactly fills, but does not enlarge, the existing telemetry
payload.

The real-media integration test provisions a RAM block device, uploads a
976-byte TinyBee motion configuration in six 173-byte chunks, atomically
publishes it, reopens it with the production reader, transfers it through both
actors, and proves identical final identity/summary. Other tests exercise chunk
splits including one-byte and record-boundary cases, exact framing/reserved bytes, wrong
board/digest, duplicate/hazardous/input-only resources, incomplete axes, invalid
ranges/rationals, corruption, sequence gaps, forbidden mutation, activation,
retained active identity, and clear.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test -p alumina-config --locked
cargo clippy -p alumina-config --all-targets --locked -- -D warnings
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
git diff --check
```

The focused crate has ten passing tests and the complete default workspace has
160. Host and both ESP target strict Clippy gates pass.

`alumina-config` is repository-owned `MIT OR Apache-2.0` code. It adds no new
registry package: SHA-256, Embassy test `block_on`, board facts, capability
encoding, protocol, and storage are already admitted MIT/Apache-compatible
dependencies. No GPL-family implementation code or asset is used.

Deduplicated offline workspace, TinyBee firmware, and T-Deck Pro firmware
package/license inventories contain 308, 226, and 233 records respectively,
with neither a missing-license entry nor a GPL/AGPL/LGPL/SSPL-family match.
Repository implementation/import trees contain no GPL-family license marker or
source header.

## Claim boundary and next gate

No firmware image yet dispatches these commands and no physical board was
connected. The raw-media fail-safe two-phase selector is now implemented and
recorded separately in `M4-DURABLE-CONFIGURATION-SELECTION.md`. The next
checkpoint wires authenticated configuration operations into core 0, receives
periodic core-1 reports, performs boot recovery, updates both job actors only
after exact activation, and keeps both non-armable boards closed pending HIL.
