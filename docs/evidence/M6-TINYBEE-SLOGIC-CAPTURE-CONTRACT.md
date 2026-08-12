# M6 TinyBee SLogic capture-contract evidence

Date: 2026-08-11

Status: the disconnected-load capture contract, analyzer-only marker, bounded
waveform decoder, and strict evidence record are implemented. The connected
bare `MKS TinyBee v1.0` was not enumerated, flashed, reset, or captured for this
checkpoint, the SLogic16U3 remains disconnected, and no physical waveform or
qualification claim is made. The primary record admits only the separately
identified 8 MiB `mks-tinybee-v1` package; the 4 MiB variant remains a supported
build target but cannot borrow this future fixture's identity.

## Physical and firmware boundary

The isolated release-only HIL binary consumes otherwise dormant core-0 service
tokens to configure GPIO4/LCD_RS safe-low. EXP1 and every display, StepStick,
motor, heater, fan, and process connector must remain empty. The marker rises
immediately before the HAL circular-start call, falls immediately after stop
returns, and only after the two-frame safe rewrite emits a self-delimiting
result: two 1 ms sentinels surrounding 100 us-high/100 us-low count pulses.
Code 1 is the sole successful loop/stop/rewrite result. No log, await, Wi-Fi,
storage, second-core, motion, or process-output operation occurs while the
circular transfer is live.

The reviewed initial probe map comes from the official V1.0_003 schematic:
SLogic D0/D1/D2 observe U1 74HC595 SRCLK/RCLK/SER (GPIO25/26/27), D3 observes
EXP1 pin 4 LCD_RS_O (GPIO4), and ground is U1 pin 8 or a nearer meter-verified
ground. The procedure requires measuring U1 VCC and the potentially
level-shifted marker before connecting inputs, never connects analyzer VCC, and
requires an operator-owned top photograph plus an annotated derivative. A
vendor image, generated lookalike, connector shroud, or textual pin number is
not accepted as physical fixture identity.

## Bounded waveform authority

`cargo xtask hil analyze-tinybee-vcd` accepts only a retained VCD below
`docs/hil/runs`, resolves exactly four reviewed one-bit aliases, and streams it
without retaining the capture. Each phase retains only an exact BCLK count and
the last 24 DATA bits; the marker parser retains a fixed grammar capped at code
32. Unknown selected values, time reversal/overflow, non-integer-picosecond
timestamps, duplicate/missing aliases, malformed sentinels, unsafe paths, and
report overwrite all fail closed.

The canonical report contains the source VCD SHA-256 and reconstructed static,
live, tail, post-stop, image-anomaly, BCLK/frame-period, and DATA setup/hold
facts. `cargo xtask hil validate-tinybee-record` separately hashes every capture,
photo, report, note, and release ELF; validates the disconnected-load fixture,
instrument setup, primary board identity, and timing policy; replays the report;
and requires every transcribed decoded value to match it exactly. Passing the
record still requires manual four-channel waveform review and does not close
reset/brownout, all-bit routing, energized load, production streaming, or
armability gates.

## Reproduction

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test -p xtask
cargo clippy -p xtask --all-targets -- -D warnings
cargo +esp clippy -p alumina-firmware \
  --bin alumina-hil-mks-tinybee-pcm-short-safe \
  --no-default-features --features hil-mks-tinybee-pcm-short-safe \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo xtask hil build mks-tinybee-pcm-short-safe
git diff --check
```

The unit suite covers canonical report replay, VCD digest binding, disconnected
load and channel-map gates, duration/timing/image/marker rejection, safe evidence
paths and digests, standard `$dumpvars` initialization, complete phase/frame
reconstruction, malformed frames, unknown logic levels, and sub-picosecond
rejection. All 19 focused `xtask` tests and all 332 default-member tests pass.
Strict all-target host Clippy, warnings-denied `xtask` rustdoc, strict HIL target
Clippy, and strict production target Clippy for TinyBee 8 MiB, TinyBee 4 MiB,
T-Deck Pro, and MKS ESP32 FOC V1.0 pass. The HIL release artifact has SHA-256
`a6ef783bc9aa1476ed24558762de19b09a1eaccb777d4b8affa0dcc7aff14afe`,
62,936 text bytes, 3,120 data bytes, and 193,488 aggregate BSS bytes, of which
183,932 bytes are the linker-residual `.stack` capacity rather than a measured
runtime watermark.

All four ordinary production releases also link. Their observed
text/data/aggregate-BSS sizes are 917,400/12,040/250,096 bytes for primary
TinyBee, 917,420/12,040/250,096 for TinyBee 4 MiB,
858,825/12,800/525,568 for T-Deck Pro, and 860,152/10,792/251,344 for MKS ESP32
FOC V1.0. No image was run or flashed as part of these checks.

## License and source boundary

All new implementation is independently authored under `MIT OR Apache-2.0` and
adds no package dependency. Official MKS schematic and Sipeed instrument manuals
provide hardware/tool facts only. No vendor firmware implementation, GPL-family
source, decoder, dependency, or asset was fetched, copied, linked, or vendored.
An operator's already-installed capture/export program remains external tooling;
the repository retains only its operator-created data and uses its own Rust VCD
decoder as the evidence authority.

The resolved all-feature offline Cargo inventory contains 851 records (329
sorted unique package/license records), with no missing license and no GPL,
LGPL, AGPL, or SSPL-family license. The Rust/C/C++/JavaScript/TypeScript/CSS/HTML/
SVG source-and-manifest scan is also clear.
