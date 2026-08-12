# M9 TinyBee graph-input timing harness evidence

Date: 2026-08-12

Status: the disconnected-load, Wi-Fi-loaded graph-input timing fixture and its
strict capture/record tooling are implemented at `aluminafw`
`73fa08933b5b75f1a93e6f05aed6d00c5a8825f4`. No serial device was opened, and
the available bare `MKS TinyBee v1.0` was not enumerated, erased, flashed,
reset, or driven for this checkpoint. No motor or motor-power connection was
present. The available SLogic16U3 remained disconnected. Consequently this
record makes no physical timing, electrical, boot, Wi-Fi, or board-
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

Core 1 establishes the complete static safe shift image before readiness and
continues sampling GPIO33 through core-0 network startup, graph priming,
confirmation, the future start epoch, and every release. A missing/stale
sample, watchdog reaction, graph error, wrong execution count, or missing sink
value reapplies the safe image, drives both observation markers low, and stops
all future releases. The fixture never initializes storage, motion/I²S
streaming, arming, or any process-output command API. It intentionally observes
but does not route the active interlock reaction into an armed safety machine,
because the isolated artifact contains neither arming nor energy-output
authority.

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

The initial SLogic16U3 contract uses D0=GPIO4 release timing, D1=GPIO33 raw
input, and D2=GPIO21 graph sink at 400 MHz with a 1.6 V threshold, 50 ms
pre-trigger, and 300 ms total capture. Pass policy requires at least 150
complete 1 kHz releases, 20 releases on each side of the assertion/sink event,
periods within 1 ms ±100 us, release pulses no wider than 300 us, one sink
assertion, 1.9–3.2 ms physical-input-to-sink latency, and at most 10 us from the
correlated completed release to the sink edge. At least 25 health requests must
succeed with zero failures.

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

All 373 default-member tests and 23 focused `xtask` tests pass. Strict host
Clippy and rustdoc pass. Strict target Clippy passes for the new HIL artifact,
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

The next gate is physical operator coordination: inspect and photograph the
actual bare board, verify the disconnected state and pin voltages, connect the
SLogic16U3 exactly as documented, and review the wiring before the explicit
flash. Only the retained capture and a passing human-reviewed run record can
establish physical input/timing evidence. This harness record alone leaves the
board at `Compiles` and cannot close the independent PCM safe-image gate.
