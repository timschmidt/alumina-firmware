# M8 AS5600 transport and closed target-ownership evidence

Date: 2026-08-11

Status: an independently authored, read-only async AS5600 transport and its
classic-ESP32 target composition compile. Safe boot still selects no encoder
mode, sends no I²C transaction, initializes no ADC/MCPWM peripheral, and exposes
no inverter command. The MKS ESP32 FOC V1.0 package remains non-armable.

## Primary specification boundary

The implementation was written from the ams OSRAM AS5600 datasheet v1-06, not
from a sensor library, vendor motor-control example, SimpleFOC, or GPL-family
source. The relevant public hardware facts are:

- the fixed seven-bit I²C address is `0x36`;
- STATUS is register `0x0B`, with magnet-detected, too-weak, and too-strong
  flags at bits 5, 4, and 3;
- RAW ANGLE is the unscaled, unmodified 12-bit count at `0x0C`/`0x0D`;
- the device supports address-pointer reload and two-byte raw-angle reads;
- the device admits I²C through 1 MHz Fast-mode Plus, while the MKS board
  package retains its conservative 400 kHz connector limit; and
- sensor sampling is specified as a typical 150 microseconds and filter
  settling varies with configuration (typical 0.286–2.2 milliseconds).

The last fact prevents a false control-rate claim: this checkpoint does not
pretend that an AS5600 produces a new measured angle at a 20 kHz current-loop
rate. Sensor-domain scheduling, held/extrapolated electrical angle, latency and
filter modeling, or selection of a faster sensor remain explicit later design
work.

## Read-only portable driver

`drivers/alumina-as5600` is `no_std`, allocation-free, generic over the
`embedded-hal-async` seven-bit I²C trait, and has no dependency beyond that
permissively licensed trait crate. Its API deliberately contains no write,
configuration, zero/range programming, OTP, or burn method.

`read_raw_angle` performs one `write_read`: the single write byte only reloads
the address pointer at `0x0C`, then the read receives both RAW ANGLE bytes. It
rejects rather than masks nonzero bits outside the documented 12-bit field and
returns `RawAngle`, whose constructor enforces the exact `0..4096` count
lattice. It does not convert to floating point, degrees, radians, or electrical
angle.

`read_status` separates the three defined flags from all reserved bits. Reserved
bits are retained for diagnostics but cannot accidentally acquire magnet-policy
meaning. `read_observation` intentionally performs three transactions: STATUS,
RAW ANGLE, STATUS. It reports whether the defined flags remained stable and
whether both endpoints were nominal. The type and documentation say explicitly
that these are bracketing observations, not a simultaneous sample.

The seven unit tests use an independently authored scripted I²C implementation
and verify exact address/register/length behavior, 12-bit endpoints, malformed
high-bit rejection, defined/reserved status separation, stable and changing
status brackets, and preservation of the concrete bus error.

## MKS type-state composition

The core-1 singleton split is unchanged. `establish_safe_outputs` synchronously
converts all six EG2133 phase-control pins, four encoder SDA/SCL lines, and two
encoder auxiliary lines to retained no-pull GPIO inputs. The four current GPIOs
are hardware input-only on classic ESP32 and retain their concrete GPIO types;
this preserves their ADC1 channel identity for a future calibrated transition
without giving them an output capability. The method does not construct I²C,
ADC, or MCPWM. This preserves the mode-selectable connector boundary and avoids
silently selecting AS5600 merely because the firmware booted.

After that transaction, ownership is grouped into three closed states:

- each `ClosedPowerStage<Pwm>` retains one raw MCPWM singleton and three
  high-impedance `Input` owners, with no raw-token extractor and no
  `alumina_foc::PowerStage` implementation;
- `UncalibratedCurrentSense` retains ADC1 and all four concrete input-only GPIO
  owners, with no ADC construction, conversion method, or
  `alumina_foc::CurrentSense` implementation; and
- `DormantEncoderResources` retains both I²C singletons and all connector input
  owners.

An explicit synchronous `activate_as5600_encoders` type-state transition
consumes the dormant state and returns `As5600EncoderResources`. It configures
two independent released open-drain buses at 400 kHz after the phase-safe state
already exists, but performs no I²C transaction. The transition and read-only
methods compile for `xtensa-esp32-none-elf`; no production task calls them yet.
A later stored connector-mode selection must make that transition reachable.

No AS5600 count is silently promoted to a control value. A future sampler must
pair it with a measured observation cycle and a digest-bound
`alumina_foc::RotorCalibration` that explicitly supplies 4096 counts per turn,
direction, pole pairs, reference/electrical zero, count uncertainty, alignment
uncertainty, and a rotation precision policy.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc -p alumina-as5600 --no-deps \
  --locked --offline
cargo +esp clippy -p alumina-as5600 --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-as5600 --target xtensa-esp32s3-none-elf \
  --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

The default portable workspace has 291 passing tests, including all seven new
AS5600 tests and the existing 20 exact FOC tests. Strict host Clippy and
warnings-denied driver rustdoc pass.
Strict no-std Clippy passes for the driver on classic ESP32 and ESP32-S3, and
strict classic-ESP32 firmware Clippy type-checks the dormant/active connector
transition plus the sealed MCPWM/ADC owners. All three release firmware targets
link.

The final MKS ELF has SHA-256:

```text
f5a3c2cbe2e9028e0c7d75c84ea2c6d4cc5d4d39e3e8661c55775dabc08e40c9
```

`xtensa-esp32-elf-size` reports 823,308 bytes of text, 10,704 bytes of data,
and 251,440 bytes of BSS. These are reproducibility observations, not flash-fit,
stack-watermark, latency, or timing qualification.

The full-workspace all-feature cargo-tree output has 848 nonempty
package/license records, no missing license, and no
GPL/AGPL/LGPL/SSPL-family expression. The new crate introduces no external
dependency. A Rust/C/C++/header/Cargo-manifest scan outside documentation and
the explicit reference ledger has no GPL-family match. `cargo-deny` remains
configured in CI but is not installed locally, so no local `cargo deny` result
is claimed.

## Physical and energization boundary

No board was visible, flashed, connected to motor power, or energized. No I²C
transaction ran on physical hardware. Bus pull-ups, address acknowledgement,
raw-count sequence, magnet quality, sensor filtering/latency/repeatability,
mechanical direction, pole pairs, electrical alignment, ADC offset/gain/sample
phase, MCPWM behavior, reset impedance, shutdown latency, WCET, or jitter was
measured.

The six phase owners remain GPIO inputs; no code constructs an MCPWM operator,
attaches a phase pin, starts a timer, constructs ADC1, converts a current input,
implements a power/current trait, or emits nonzero duty. Every firmware motion
operation still returns `MotionUnsupported`, configuration still rejects the
board's merely `Described` shutdown stage, and the capability remains
non-armable.

All new implementation and documentation are repository-owned under
`MIT OR Apache-2.0`. No GPL-family source, vendor example implementation,
Synthetos/g2 implementation, or SimpleFOC implementation was inspected, copied,
or used.
