# Hardware-in-the-loop fixtures

Hardware qualification is evidence-driven. A linked image, a successful flash,
or an apparently correct waveform does not change a board package's
qualification by itself. Every run must identify the exact source revision,
artifact digest, board revision, fixture wiring, instruments, capture settings,
supply/load state, raw capture files, and reviewer disposition.

## TinyBee PCM-short safe-image capture

Fixture ID: `mks-tinybee-pcm-short-safe`

Purpose: capture the original ESP32 I²S0 PCM-short startup, steady-state
circular-DMA refill, stop, and safe rewrite using only the complete documented
TinyBee disabled/off image. It does not run Wi-Fi, storage, motion, the second
core, or any process-output command path.

This fixture is pre-qualification code. Because first-frame/FIFO/WS phase is the
fact under test, the software cannot prove that an unknown startup latch is
impossible. Physically disconnect motor power, heater/fan/process power, and all
other hazardous loads before flashing it. Logic power and the USB programming
connection may remain. Verify the disconnected state with a meter; do not infer
it from software or a UI.

Build only:

```console
cargo xtask hil list
cargo xtask hil build mks-tinybee-pcm-short-safe
```

The command always uses the release profile and prints the artifact path. It
never invokes a runner or flashes a device. After completing the physical
checklist, flashing remains an explicit operator action:

```console
espflash flash --monitor --chip esp32 \
  target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-pcm-short-safe
```

### Connections and expected phases

Connect the logic analyzer to TinyBee ground and:

| Signal | ESP32 pin | Role |
| --- | ---: | --- |
| BCLK | GPIO25 | shift clock |
| WS/RCLK | GPIO26 | PCM-short pulse and register latch |
| DATA | GPIO27 | serial data into the 74HC595 chain |

Use a sample rate adequate for a nominal 16 MHz BCLK; the available 10 MHz DSO
is useful for slower envelope/electrical checks but cannot establish every
serial bit. Record probe loading and logic thresholds.

The artifact emits these RTT markers, with no RTT operation or await point while
the circular transfer is live:

1. `HIL_STATIC_SAFE`: the blocking bootstrap transaction has installed image
   `0x001249`, then BCLK and WS remain low for 50 ms;
2. a 256-frame safe-prefilled circular transfer starts and about 50,000 exact
   four-byte safe refills are attempted, nominally 200 ms at
   250 kHz, with a two-second fail-closed timeout; and
3. `HIL_PCM_STOPPED`: after DMA stop and two further safe samples, the artifact
   reports the hypothesized model epoch, device-cycle bracket around the HAL
   start call, refill outcome, stop bracket, and safe-rewrite result, then parks
   without starting other services.

The model epoch is a hypothesis, not a physical timestamp. Correlate it with the
captured first WS edge; do not promote it merely because the refill loop
completed.

### Required review

Archive and review at least these facts:

- static image before peripheral attachment and the exact static-to-I²S handoff;
- first BCLK, DATA, and WS levels and the first complete image latched;
- one-BCLK WS pulse width, 64 BCLKs per frame, nominal rates, duty cycles, and
  DATA setup/hold at the receiving chain;
- reconstruction of the full 24-bit MSB-first suffix as `0x001249` at every
  observed latch, including descriptor wrap;
- absence or exact characterization of gaps, duplicated/partial frames, DMA
  `Late`, descriptor errors, and refill jitter;
- stop latency/phase, the two-sample safe rewrite, and final retained image; and
- reset/brownout behavior in a separate safe fixture before any load is attached.

`HIL_RESULT capture complete` means only that the software refill loop, DMA stop,
and safe rewrite returned success. It is not a waveform verdict. Attach the raw
capture and review record before changing `MOTION_OUTPUT_QUALIFIED`, package
armability, timing limits, or any machine configuration.

This safe-only pattern may contribute evidence to
`safe.i2s-reset-fault-watchdog`; it cannot close that broader reset/fault/watchdog
requirement by itself. Because it intentionally sends only one image, it also
cannot close `routing.i2s-all-bits`. A later disconnected-load fixture must
exercise every shifted bit with bounded dwell before that requirement can pass.

### Run record

Record:

- Alumina commit and ELF SHA-256;
- TinyBee PCB revision, ESP32 module marking, and board serial/fixture ID;
- supply voltages/current limit and a photograph showing disconnected loads;
- analyzer/firmware version, channel mapping, sample rate, threshold, and probe;
- RTT log and raw unedited capture SHA-256;
- decoded frame/edge statistics and any anomaly timestamps; and
- reviewer, date, pass/fail/inconclusive disposition, and follow-up issue.

Photographs and capture assets must be repository-owned or permissively
licensed with explicit provenance. GPL-family source, decoders, code, and assets
are not accepted into the implementation or evidence bundle.

## M7 two-board start record

The cross-device start gate uses a strict repository-owned run record in
addition to raw captures. Start from
[`docs/hil/templates/m7-two-board-start.toml`](hil/templates/m7-two-board-start.toml),
copy it below `docs/hil/runs/<run-id>/record.toml`, replace every placeholder,
and validate it from the repository root:

```console
cargo xtask hil validate-record docs/hil/runs/<run-id>/record.toml
```

The validator rejects unknown, duplicate, missing, zero-identity, and unsafe
path fields. It streams and verifies SHA-256 for both release ELFs, both
annotated fixture photos, the browser/device clock log, the raw analyzer
capture, and the reviewer notes. Evidence assets must stay below
`docs/hil/runs`; build artifacts must stay below `target`. Photos must be
operator-owned `CC0-1.0` or attributed `CC-BY-4.0` assets. Vendor/wiki images
are not substitutes for the actual fixture.

Each participant must identify its exact revision, fixture serial, boot ID,
reviewed physical output route, scheduled cycle, and observed cycle interval.
The shared section binds the UI epoch, pre-commit admitted spread, post-run
reconciled spread, physically captured spread, analyzer identity/firmware/sample
rate/threshold, Wi-Fi load condition, and source commits. A `pass` disposition
is impossible when the physical spread exceeds the pre-commit certificate or
when the reconstructed interval excludes the capture. Failed and inconclusive
runs are still retained rather than rewritten as passes.

This schema prepares the evidence boundary; it does not supply a waveform. At
this checkpoint no two-board HIL image has been flashed and no T-Deck output
route has been approved. The T-Deck's real-time GPIO2 vibration route is a
candidate only after its polarity, fixture access, and bounded harmless pulse
are physically reviewed. Do not infer permission to drive it from its presence
in board metadata or this template.
