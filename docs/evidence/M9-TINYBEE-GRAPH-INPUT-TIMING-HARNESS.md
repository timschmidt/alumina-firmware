# M9 TinyBee graph-input timing harness evidence

Date: 2026-08-12

Status: the disconnected-load, Wi-Fi-loaded graph-input timing fixture and its
strict capture/record tooling were first implemented at `alumina-firmware`
`73fa08933b5b75f1a93e6f05aed6d00c5a8825f4`. Subsequent prequalification on the
available bare `MKS TinyBee v1.0` reached `3ee2369` and established the boot and
executor facts recorded below. The board had no StepSticks, motors, motor
power, or process loads. The SLogic16U3 leads were attached according to the
fixture map, with analyzer VCC and D3 disconnected, but no permissively
licensed capture stack was available and no capture was taken. No retained
HTTP-load log, fixture photographs, raw serial log, or complete run record
exists. Consequently this document records commissioning observations only;
it makes no electrical, input-path, web-load, timing-policy, or board-
qualification claim.

## Shared production boundary

The fixture uses the exact production `FixedGraphServiceActor`,
`FixedGraphRealtimeActor`, cross-core bridge, static arena sizes, opcode
palette, and board resource palette. Those declarations now have one shared
`graph_platform` owner used by both the ordinary image and isolated HIL binary;
the test cannot silently substitute smaller arenas or broader authority.

The fixture constructs and independently validates one canonical stored
configuration binding the protected X-endstop/GPIO33 resource as a required,
active-low realtime safety interlock with pull-up, 2 ms active/inactive
debounce, and a 10 ms sample watchdog. It independently admits an `ALGRIR02`
package containing `StableBooleanInput(GPIO33) -> BooleanStreamSink` on both
actors. The realtime schedule is 1 kHz with 50 device cycles of declared WCET
per node and 200 cycles of executor reserve. The package and configuration are
bound to the actual ESP base MAC and the primary TinyBee capability identity.

Core 1 establishes the complete static safe shift image and a debounced GPIO33
state before allowing core-0 network initialization. Classic ESP32 radio
initialization can then suspend the other core for longer than the configured
10 ms operational input watchdog. During this explicitly unarmed boundary the
fixture retains the largest observed sampling gap, notes whether the watchdog
was crossed, and keeps every output at the complete safe image. It discards
that monitor after radio initialization, constructs a fresh monitor, and
requires a new complete debounce before reporting realtime readiness. The
strict 10 ms watchdog applies without exception after that readiness boundary,
through graph priming, confirmation, the future start epoch, and every release.
A missing/stale operational sample, watchdog reaction, graph error, wrong
execution count, or missing sink value reapplies the safe image, drives both
observation markers low, and stops all future releases. The fixture never
initializes storage, motion/I²S streaming, arming, or any process-output command
API. It intentionally observes but does not route the active interlock reaction
into an armed safety machine, because the isolated artifact contains neither
arming nor energy-output authority.

The realtime actor runs from a core-1 interrupt executor on software interrupt
2 at ESP interrupt priority 3. The RTOS scheduler retains software interrupts
0 and 1. Core 0 continues to own radio, network, DHCP, HTTP, and commissioning
UART work; graph releases perform no allocation, formatting, socket access, or
service-core locking.

## Disconnected-board prequalification observations

The flashed `3ee2369` image identified an original dual-core ESP32 revision 1,
240 MHz CPU, 40 MHz crystal, 8 MiB flash, base MAC
`c4:de:e2:f8:c4:ac`, and AP BSSID `c6:de:e2:f8:c4:ac`. UART commissioning
reached these stages in order:

```text
ALUMINA_HIL_BOOT stage=entry
ALUMINA_HIL_BOOT stage=graph-installed
ALUMINA_HIL_BOOT stage=boot-safe-ready
ALUMINA_HIL_BOOT stage=network-starting
ALUMINA_HIL_BOOT stage=network-started
ALUMINA_HIL_BOOT stage=realtime-ready
ALUMINA_HIL_WIFI_STARTUP max_sample_gap=54017 watchdog_observed=true
ALUMINA_HIL_RUNNING ssid=Alumina-mks-tinybee-v1 address=192.168.4.1
```

`TICK_HZ` is 1 MHz in this artifact, so the retained startup gap was 54.017 ms.
An earlier ordinary core-1 executor run failed closed after 2,327 1 kHz
releases with 517 us maximum dispatch lateness, beyond the unchanged 200 us
reserve. The priority-3 interrupt-executor image then ran for more than 180,000
releases without a terminal UART fault while the AP was idle. AP association
was observed, but the workstation network path was needed for the development
session before an HTTP load log could be retained; DHCP/addressing and HTTP
interoperability therefore remain unverified here. Absence of a terminal fault
is not a positive timing measurement and cannot substitute for the required
logic-analyzer trace.

Core 0 runs the production radio/AP, DHCP, network runner, and HTTP tasks. The
future physical run must maintain requests to the static
`/api/v1/health` route while the graph executes. GPIO4/EXP1 pin 4 marks only the
fixed graph release call, and GPIO21/EXP1 pin 3 changes only after its completed
report to mirror the Boolean sink. These otherwise dormant display routes are
used only by this named binary and both begin low.

## Physical and capture contract

The reviewed V1.0_003 schematic identifies J14/X- pin 1 as ground, pin 2 as
+5 V, and pin 3 as the protected GPIO33 signal. The procedure forbids probing
or bridging pin 2. It requires an open-signal measurement of 2.7–3.6 V, an
insulated pin-3-to-pin-1 assertion jumper, measured marker highs, analyzer
inputs and ground only, and no analyzer VCC connection. Every motor, process,
StepStick, and display load must remain disconnected. An operator-owned actual
fixture photograph plus a distinct annotated derivative must visibly identify
the pins, leads, and disconnected load groups.

The initial SLogic16U3 contract selects its minimum four-channel bank and uses
D0=GPIO4 release timing, D1=GPIO33 raw input, and D2=GPIO21 graph sink at
400 MHz with a 1.6 V threshold, 50 ms pre-trigger, and 300 ms total capture. D3
must remain physically unconnected and is ignored by the decoder. Pass policy
requires at least 150 complete 1 kHz releases, 20 releases on each side of the
assertion/sink event, periods within 1 ms ±100 us, release pulses no wider than
300 us, one sink assertion, 1.9–3.2 ms physical-input-to-sink latency, and at
most 10 us from the correlated completed release to the sink edge. At least 25
health requests must succeed with zero failures.

`cargo xtask hil analyze-tinybee-graph-vcd` is a streaming three-signal VCD
decoder. It rejects unknown selected levels, fractional-picosecond timestamps,
time reversal, missing or duplicate aliases, invalid initial/final state, sink
assertion before the raw active-low input, a sink edge inside a release,
multiple/deasserted sink edges, and report overwrite. The immutable report
binds all derived counts and intervals to the VCD SHA-256.

`cargo xtask hil validate-tinybee-graph-record` admits only the primary 8 MiB
`mks-tinybee-v1` package and exact named artifact. It verifies physical,
network, analyzer, photo-license, capture, and policy fields; hashes every
artifact; streams and decodes the retained VCD again; and requires that result,
the immutable report, and every copied record measurement agree exactly. The
4 MiB package remains build-supported but cannot borrow this fixture identity.

## Reproduction

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
cargo +esp clippy -p alumina-firmware \
  --bin alumina-hil-mks-tinybee-graph-input-timing-safe \
  --no-default-features --features hil-mks-tinybee-graph-input-timing-safe \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware \
  --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware \
  --bin alumina-hil-mks-tinybee-pcm-short-safe \
  --no-default-features --features hil-mks-tinybee-pcm-short-safe \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp check -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee-4mb \
  --target xtensa-esp32-none-elf --locked --offline
cargo +esp check -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline
cargo +esp check -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline
cargo xtask hil build mks-tinybee-graph-input-timing-safe
cargo xtask build --board mks-tinybee-v1 --profile release
git diff --check
```

At the original construction checkpoint, all 373 default-member tests and 23
focused `xtask` tests passed. Strict host Clippy and rustdoc passed. Strict
target Clippy passed for the new HIL artifact,
the ordinary primary TinyBee image, and the pre-existing PCM safe fixture. All
four supported board selections pass target check. The ordinary primary
TinyBee and new HIL release images link.

| Linked target ELF | ELF bytes | text | data | aggregate BSS | SHA-256 |
| --- | ---: | ---: | ---: | ---: | --- |
| graph-input timing HIL | 8,122,736 | 699,240 | 10,272 | 251,872 | `7756e81e12b3c076c834a031667e7557d087969edf3c45a44e5cb3e9eb4d4e8b` |
| ordinary TinyBee 8 MiB image | 10,473,528 | 1,023,904 | 12,200 | 249,936 | `f33b168123d1f84f42710ddc236507ffa18db319ad8bcb52da93459c922ded43` |

ELF length is not flash-payload length, and aggregate BSS is not a measured
runtime watermark. The unchanged 68,586-byte lockfile has SHA-256
`ea3d7f4d4ee9a2e42fd0e2ec19941e812ad8d9a1259d607caa8ffdad8c5260f4`.

## License and next gate

All implementation and documentation in this checkpoint are independently
authored under the repository's `MIT OR Apache-2.0` terms. No dependency was
added, and no GPL-family code, decoder, source, or asset was fetched, copied,
linked, or vendored. The MKS schematic supplied hardware facts only.

The next gate is a retained, permissively produced SLogic16U3 capture plus the
simultaneous HTTP-load log. Before that run, retain actual and annotated fixture
photographs, recheck the disconnected state and pin voltages, and independently
review the already prepared wiring. The HTTP client must use a network path
that does not disconnect the active development session. Only the retained
capture and a passing human-reviewed run record can establish physical
input/timing evidence. These prequalification observations leave the board at
`Compiles` and cannot close the independent PCM safe-image gate.
