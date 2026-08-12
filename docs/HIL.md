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

The first reviewed analyzer is the Sipeed SLogic16U3. Its published digital
input range is 0–10 V, its threshold is adjustable from 0–6 V, and its
four-channel streaming mode supports up to 800 MHz. Use exactly four active
channels at 400 MHz for the initial run, a 1.6 V threshold, at least one ground
lead adjacent to the U1 probes, and a direct USB 3 connection. A 200 MHz run is
the validator's lower bound; higher rates do not compensate for long probe
grounds or an unmeasured signal voltage.

Before connecting the analyzer, use the DSO or meter to confirm U1 VCC is
3.0–3.6 V and the EXP1 marker high is within the analyzer's reviewed 0–10 V
range. Do not connect either analyzer VCC pin to the TinyBee. Connect only
analyzer inputs and ground.

Use the first 74HC595, U1, as the preferred physical probe location. The
V1.0_003 schematic identifies:

| SLogic | Signal | Preferred board point | ESP32 route | Role |
| --- | --- | --- | ---: | --- |
| D0 | BCLK | U1 pin 11, `SRCLK` | GPIO25 | shift clock |
| D1 | WS/RCLK | U1 pin 12, `RCLK` | GPIO26 | PCM-short pulse and register latch |
| D2 | DATA | U1 pin 14, `SER` | GPIO27 | serial data into the 74HC595 chain |
| D3 | MARKER | EXP1 pin 4, `LCD_RS_O` | GPIO4 | high only while circular DMA is live; post-stop result code |
| GND | ground | U1 pin 8 or the nearest verified ground pad | — | common reference |

EXP1, EXP2, the LCD/serial-display headers, all motor and process connectors,
and every StepStick socket must be empty. The marker route is package-declared
nonhazardous, begins and ends low, and may be level-shifted by the fitted
74HCT125 path. Its level must therefore be measured before connecting D3. Do
not infer EXP1 pin 1 from the keyed shroud: locate the square pin-1 pad and then
confirm pin 4/LCD_RS against the actual board and annotated photo.

Arm on the first D0/BCLK rising edge with at least 1 ms of pre-trigger history
and at least 400 ms total capture at 400 MHz (160,000,000 samples). If streamed
capture cannot retain that interval without drops, stop: do not lower the rate
below 200 MHz or stitch separate captures into a pass record. Save both the raw
Sigrok session and a VCD export. The available 10 MHz DSO is useful for marker
level and slower electrical-envelope checks but cannot establish every 16 MHz
serial bit.

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

D3 rises immediately before the HAL circular-start call and falls immediately
after the stop call returns. After a 1 ms low gap, it emits a self-delimiting
outcome: a 1 ms high sentinel, 1 ms low, `marker_code` pulses of 100 us high and
100 us low, then a final 1 ms high sentinel and permanent low. Codes 1–7 name
the software loop exit (`1` is complete); bit 3 means stop failed, bit 4 means
the safe rewrite failed, and code 32 means start failed. A pass requires code
1. The marker only brackets software calls; physical WS/BCLK remains the
authority for start and stop timing.

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

Copy
[`tinybee-pcm-short-slogic16u3.toml`](hil/templates/tinybee-pcm-short-slogic16u3.toml)
to `docs/hil/runs/<run-id>/record.toml`, retain the unedited `.sr`, exported
`.vcd`, analysis report, review notes, actual-fixture photo, and annotated copy,
and label the four exported one-bit VCD references as `D0`–`D3` (the analyzer
also accepts their reviewed signal or GPIO names). Generate a new canonical
analysis report directly from the retained VCD:

```console
cargo xtask hil analyze-tinybee-vcd \
  docs/hil/runs/<run-id>/tinybee-pcm-safe.vcd \
  docs/hil/runs/<run-id>/analysis.toml
```

The analyzer streams the edge file with bounded frame state, rejects unknown
selected levels, fractional-picosecond timestamps, missing/duplicate aliases,
malformed marker grammar, and unsafe paths, and refuses to overwrite an
existing report. It reconstructs the static, live, and post-stop 24-bit image
suffixes, exact BCLK count per WS frame, marker result, live tail, BCLK/frame
period extrema, and DATA setup/hold. The report binds those values to the VCD's
SHA-256. Copy its decoded values exactly into the record, add every remaining
asset digest and reviewed fact, then validate the complete record:

```console
cargo xtask hil validate-tinybee-record \
  docs/hil/runs/<run-id>/record.toml
```

The validator independently verifies the VCD and report digests, replays the
canonical report, and requires every copied measurement to match. A `pass`
also requires marker code 1, safe static/live images, at least 50,000 complete
live frames, no invalid or non-64-bit frame, at most one admitted final live
frame, two complete post-stop frames with at least one observed safe latch, and
all named timing intervals. Manual waveform review remains mandatory for pulse
width, duty cycle, electrical integrity, anomalies at exact timestamps, and
facts outside the four-channel decoder.

The validator admits only the primary `mks-tinybee-v1` 8 MiB package. The 4 MiB
variant remains build-supported but requires its own independently identified
physical fixture before it can contribute hardware evidence.

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

## TinyBee capability-bound graph-input timing capture

Fixture ID: `mks-tinybee-graph-input-timing-safe`

Purpose: measure the first capability-bound graph opcode on its real target and
prove the complete physical-to-graph path under production Wi-Fi/web load. The
fixture samples the protected X-endstop/GPIO33 route, applies the exact stored
configuration's active-low pull-up, 2 ms assertion/release debounce, and 10 ms
sampling watchdog, then executes `StableBooleanInput -> BooleanStreamSink` at
1 kHz on core 1. Core 0 runs the production AP, DHCP, network runner, and HTTP
tasks. Storage, motion streaming, and every process-output command path remain
uninitialized.

This is still a disconnected-load fixture. It synchronously installs the full
24-bit disabled/off image `0x001249` before reporting core 1 ready, but it does
not replace or satisfy the independent PCM startup/safe-image qualification.
Motor power, main/process power, all motor and heater/fan connectors, every
StepStick socket, and both display headers must remain disconnected.

Build only:

```console
cargo xtask hil build mks-tinybee-graph-input-timing-safe
```

The command never flashes hardware. After the physical checklist and analyzer
wiring have been independently reviewed, the explicit operator action is:

```console
espflash flash --monitor --chip esp32 \
  target/xtensa-esp32-none-elf/release/alumina-hil-mks-tinybee-graph-input-timing-safe
```

### Connections

The V1.0_003 schematic gives J14/X- pin 1 as ground, pin 2 as +5 V, and pin 3
as the protected X-endstop signal. Never bridge or probe pin 2 as part of this
fixture. With J14 open, first measure pin 3 at 2.7–3.6 V. The assertion action
is an insulated jumper from J14 pin 3 to J14 pin 1 only.

Select the SLogic16U3 four-channel bank, use a 1.6 V threshold, and attach at
least one short ground lead. Only D0–D2 are used: leave D3 physically
unconnected. Do not connect either analyzer VCC pin to the TinyBee.

| SLogic | Signal | Board point | ESP32 route | Meaning |
| --- | --- | --- | ---: | --- |
| D0 | TIMING | EXP1 pin 4, `LCD_RS_O` | GPIO4 | high only around the fixed graph `release` call |
| D1 | INPUT | J14/X- pin 3 | GPIO33 through the protected endstop network | raw active-low assertion |
| D2 | SINK | EXP1 pin 3, `LCD_EN_O` | GPIO21 | graph sink value, changed only after a completed release |
| D3 | UNUSED | no connection | — | selected only because four channels is the analyzer's minimum bank |
| GND | ground | J14/X- pin 1 or another verified ground | — | common reference |

Locate EXP1 pin 1 from the square PCB pad, not from an assumed cable
orientation. EXP1 pin 3 and pin 4 traverse the fitted display buffer and may be
near 5 V. Measure both high levels within 3.0–5.5 V before the recorded run;
the SLogic16U3's reviewed 0–10 V input range is required here. EXP1 and EXP2
must otherwise remain empty. The retained annotated photograph must visibly
label J14 pins 1/2/3, EXP1 pins 3/4, every analyzer lead, the unconnected D3
lead, and every disconnected load group.

### Run and capture

Leave J14 open through reset. The firmware establishes the safe shift image and
a known debounced X input before core 0 is allowed to start Wi-Fi. Classic
ESP32 radio initialization may suspend core 1 beyond the 10 ms operational
watchdog. This is allowed only while the fixture is unarmed and all outputs are
already safe: the firmware records the maximum startup gap, discards the old
input monitor, and requires a fresh complete debounce after network startup.
The 10 ms watchdog is then strict for the remainder of the run. Join
`Alumina-mks-tinybee-v1` with the development-fixture password
`alumina-development`, verify `http://192.168.4.1/api/v1/health`, and keep a
request loop active throughout the analyzer capture. Retain a log that counts
at least 25 successful responses and zero failures; this is the independent
proof that the real AP/web tasks were scheduled on core 0 during the trace.

Arm the analyzer on the D1 falling edge with at least 20 ms pre-trigger history
and at least 200 ms total capture. The preferred initial setting is the
four-channel bank at 400 MHz, 50 ms pre-trigger (20,000,000 samples), and
300 ms total (120,000,000 samples).
Once armed, bridge only J14 pin 3 to pin 1 and hold it through the remainder of
the capture. Contact bounce is accepted as raw evidence, but a pass allows at
most 16 raw transitions and derives the 2 ms debounce interval from the final
falling edge before D2 asserts.

Each D0 pulse conservatively includes both GPIO writes around the graph call.
The graph package declares 50 device cycles per node and 200 cycles of executor
reserve: its complete release budget is therefore 300 us inside the 1 ms
period. D2 must rise only after D1 is low, no earlier than the configured 2 ms
debounce (with a 100 us capture/dispatch allowance), no later than 3.2 ms, and
within 10 us after the correlated D0 falling edge. A missed graph window,
unavailable/stale resource, malformed report, or hardware sampling error
forces both markers low, reapplies the complete safe image, and permanently
stops releases.

Sparse `ALUMINA_HIL_BOOT`, `ALUMINA_HIL_RUNNING`, and
`ALUMINA_HIL_*_FAULT` text is emitted on UART0 for commissioning. These writes
occur only during staged startup or after a terminal failure; no UART write or
formatting occurs in the graph release loop or analyzer capture path.

The graph actor itself runs on core 1 from software interrupt 2 at interrupt
priority 3. Core 0 owns the production radio/network/HTTP tasks. A run record
must retain `ALUMINA_HIL_WIFI_STARTUP`; `watchdog_observed=true` is acceptable
only for the unarmed radio-startup interval described above, never after
`stage=realtime-ready`.

The isolated fixture deliberately observes the configured interlock without
feeding its active reaction into an armed safety machine: no arming or energy
output API exists in the artifact. Production firmware continues to treat an
active interlock as a local fail-closed safety event.

### Analysis and record

Label exported one-bit VCD references `D0`, `D1`, and `D2`. Retain the unedited
analyzer session, VCD, HTTP-load log, actual-fixture photograph, distinct
annotated derivative, and review notes. Generate the immutable analysis report:

```console
cargo xtask hil analyze-tinybee-graph-vcd \
  docs/hil/runs/<run-id>/tinybee-graph-input.vcd \
  docs/hil/runs/<run-id>/analysis.toml
```

The streaming decoder rejects unknown levels, missing or duplicate aliases,
sink assertion without raw input authority, sink changes inside a release,
multiple sink assertions, and non-integer-picosecond time. It reconstructs all
complete release pulses and periods, pre/post horizons, raw bounce count, the
qualifying assertion edge, graph-sink edge, end-to-end debounce/dispatch time,
and sink-update latency after the completed graph call. Its report is bound to
the VCD SHA-256.

Copy
[`tinybee-graph-input-timing-slogic16u3.toml`](hil/templates/tinybee-graph-input-timing-slogic16u3.toml)
to the run directory, copy every generated measurement exactly, complete all
physical/network/instrument identities and asset digests, then validate:

```console
cargo xtask hil validate-tinybee-graph-record \
  docs/hil/runs/<run-id>/record.toml
```

A `pass` requires at least 150 complete releases, at least 20 before the raw
assertion and 20 after the sink assertion, every period within the reviewed
1 ms ±100 us interval, a maximum D0 pulse no greater than the package's 300 us
budget, bounded end-to-end latency, one graph-sink assertion, active Wi-Fi, and
the retained successful HTTP load. The validator independently verifies every
artifact/evidence digest, streams and decodes the retained VCD again, and
requires the VCD, immutable analysis report, and copied record fields to agree
exactly. It cannot replace manual electrical and waveform review and cannot
change the TinyBee package's `Compiles` qualification by itself.

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
