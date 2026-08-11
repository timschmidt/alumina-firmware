# Boards and hardware strategy

Research snapshot: 2026-08-11.

## Support levels

“Supported” must be qualified. Every board entry carries one of these levels:

| Level | Meaning |
| --- | --- |
| `described` | Metadata/schema validates; no firmware build claim |
| `compiles` | Firmware and boot image compile in CI |
| `bench` | Boot, safe states, buses, and named I/O pass a hardware smoke suite |
| `motion-qualified` | Timed I/O and safety pass the board’s published trace/load envelope |
| `production-qualified` | Power-loss, update, sustained load, fault, and release evidence pass |

The interface displays this level and does not imply that the existence of an
upstream FluidNC profile makes a board electrically or operationally qualified.

## First vertical slices

| Board | MCU/memory | Why first | Primary slice | Important limitation |
| --- | --- | --- | --- | --- |
| MKS TinyBee V1.x | ESP32-WROOM-32U, 8 MiB flash, 520 KiB SRAM, dual core | Requested target with official hardware and FluidNC configuration | I²S shift-register step/heater/fan output, limits, SD, Wi-Fi, XYZ coordinated motion | Tight internal RAM; virtual outputs are not GPIO/PWM; no native FOC power stage/current sensing |
| LILYGO T-Deck Pro | ESP32-S3, 16 MiB flash, 8 MiB PSRAM, dual core | Existing Embassy drivers and integrated UI/radio hardware | Service-core peripheral parity, web UI, async bus ownership, telemetry | Shared buses and EPD latency require bounded/coalesced service tasks; PSRAM is not real-time memory |
| MKS ESP32 FOC V1.0 | ESP32-WROOM-32D, 4 MiB standard module flash, 520 KiB on-chip SRAM, dual core; received assembly still to confirm | Named first servo target with official schematic and manual | Clean-room dual 3-PWM FOC, dual magnetic sensors, inline current sensing | No fitted cache medium or independent inverter enable is established; phase pins must float for the EG2133 both-off input state, and all shutdown behavior requires bench qualification |

Recommended order:

1. Compile and run T-Deck Pro service tasks under the new core-0 executor while a
   synthetic real-time task measures core-1 deadlines.
2. Bring TinyBee up in safe state, validate physical inputs, then exercise one
   I²S output bit without motors/heaters connected.
3. Add one axis, logic-analyzer timing tests, three-axis coordination, limits,
   SD-cached exact jobs, and the pen/air-cut workflow.
4. Reconcile MKS ESP32 FOC V1.0, start at low voltage/current with one unloaded
   motor, and progress from PWM/sensing validation to qualified closed loops.

## MKS TinyBee model

The vendor describes TinyBee as an ESP32 3D-printer controller with 12–24 V
input, five stepper axes/six motor connectors (dual Z), three thermistor inputs,
heater/fan outputs, endstops, SD, and Wi-Fi. Its high-numbered `IO128`–`IO149`
labels represent bits in an I²S-driven output chain.

### Critical physical resources

| Function | Alumina typed resource |
| --- | --- |
| I²S output bit clock | `Gpio(25)` owned by `I2sOutEngine(0)` |
| I²S output data | `Gpio(27)` owned by `I2sOutEngine(0)` |
| I²S output word select | `Gpio(26)` owned by `I2sOutEngine(0)` |
| SPI MISO/MOSI/SCK | `Gpio(19)`, `Gpio(23)`, `Gpio(18)` as one `SpiBus` |
| SD chip select | `Gpio(5)` as an SPI device claim |
| X/Y/Z negative limits | `Gpio(33)`, `Gpio(32)`, `Gpio(22)` |
| Thermistor inputs noted by official profile | `Gpio(36)`, `Gpio(34)`, `Gpio(39)` as ADC-capable inputs after board/schematic validation |
| USB bridge / auxiliary serial | `Uart(0)` on `Gpio(1/3)` and `Uart(2)` on multiplexed `Gpio(17/16)` |
| Servo/material input | real-time `Gpio(2)` hazardous output and `Gpio(35)` input |

The SPI SD route now has a clean-room async transport and board composition at
`SupportLevel::Compiles`. Core 0 identifies capacity, reads only fixed locator
blocks 2046–2047, and mounts a locator-selected region after identity/log replay.
Core 1 now drives the complete schematic-derived 24-bit static safe image and
retains GPIO2 in high impedance before core 0 initializes Wi-Fi. A fixed safety
snapshot expires after 500 ms, so authenticated provisioning is admitted only
while core 1 continues reporting the matching contract in `Safe` or
`Configured`. This is compile-linked behavior, not a bench verification or
armability claim; there is still no implicit format. GPIO34's TH2/SD-detect
multiplex remains unresolved for HIL and is not crossed from the real-time
domain.

### I²S output mapping

The official FluidNC TinyBee profile and vendor numbering agree on this logical
mapping. `IO128 + n` is accepted as a configuration alias only; internally the
resource is `I2sOut(0, n)`.

| Bits | Vendor aliases | Function |
| ---: | --- | --- |
| 0, 1, 2 | IO128, IO129, IO130 | X disable, step, direction |
| 3, 4, 5 | IO131, IO132, IO133 | Y disable, step, direction |
| 6, 7, 8 | IO134, IO135, IO136 | Z disable, step, direction |
| 9, 10, 11 | IO137, IO138, IO139 | E0 disable, step, direction |
| 12, 13, 14 | IO140, IO141, IO142 | E1 disable, step, direction |
| 15 | IO143 | LCD MOSI (non-hazardous, still part of every complete image) |
| 16, 17, 18 | IO144, IO145, IO146 | heated bed, heater 0, heater 1 |
| 19, 20 | IO147, IO148 | fan 1, fan 2 |
| 21 | IO149 | beeper |
| 22, 23 | board expansion aliases | J1/extra shifted outputs |

This mapping has consequences:

- A motor step event is a timed update to a serialized shift-register image, not
  an ordinary `OutputPin::set_high` call.
- All bits share one stream/update engine, so independent “pin owners” must be
  composed into a single deterministic output image.
- FluidNC’s `I2S_STATIC` and `I2S_STREAM` concepts should map to separate backend
  capabilities. PWM is not promised merely because a bit can toggle.
- Heaters/fans require measured effective update/PWM limits and a fail-safe image
  on underrun/reset. Motion and thermal control must arbitrate the shared image
  in the real-time domain.
- The old `alumina-firmware` PCF8575 interpretation must not be carried forward.

### TinyBee bring-up gates

1. Verify board revision, schematic, strapping pins, power rails, active levels,
   driver enables, and output-chain reset behavior.
2. Boot with a known all-safe I²S image before Wi-Fi or configuration parsing.
3. Confirm every shifted bit with a logic analyzer and disconnected loads.
4. Confirm the complete duplicated-sample PCM-short model, first-frame phase,
   startup clocks, static-to-stream handoff, and DMA starvation behavior.
5. Measure sustained I²S/DMA behavior while Wi-Fi and asset serving are saturated.
6. Verify hard-limit latency and controlled-stop/emergency-disable paths.
7. Verify heater/fan watchdog and thermal-fault shutdown with representative
   loads before exposing those capabilities in the UI.

## T-Deck Pro model

### Imported devices

| Device | Bus/domain | Existing driver |
| --- | --- | --- |
| BQ25896 charger | service-core I²C | `t-deck-pro-battery-async` |
| TCA8418 keyboard | service-core I²C + interrupt | `t-deck-pro-keyboard-async` |
| CST328 touch | service-core I²C + interrupt/reset | `t-deck-pro-touch-async` |
| MIA-M10Q GPS | service-core serial/control as wired | `t-deck-pro-gps-async` |
| UC8253/GDEQ031T10 EPD | service-core shared SPI + control pins | `t-deck-pro-epd-async` |
| SX1262 LoRa | service-core shared SPI + DIO/busy/reset | `t-deck-pro-lora-async` + `sx126x-async-rs` |

The existing application uses a shared I²C bus on GPIO14/GPIO13 and a shared SPI
topology using GPIO36/GPIO33/GPIO47. The current package reconciles every
imported device's chip select, interrupt, reset, power-control, and shared
peripheral. In particular, GPIO45 is CST328 touch reset; the V1.x EPD reset route
is unconnected. This corrects the imported EPD and Patina examples, which
continue to compile for ESP32-S3.

Drivers not present in the requested source repository were not invented during
the import. The package describes the common fitted routes below without making
a driver or bench claim; exact fixture options remain an identity HIL gate.

| Described function | Current typed route | Evidence boundary |
| --- | --- | --- |
| BQ27220 fuel gauge | shared I²C, address `0x55` | metadata only |
| ambient-light sensor | shared I²C, address `0x23`, interrupt GPIO16 | metadata only |
| IMU | shared I²C, address `0x28`, interrupt GPIO21, 1.8 V enable GPIO38 | metadata only |
| microphone | service I²S0/DMA1, data GPIO17, clock GPIO18 | metadata only |
| vibration motor | real-time hazardous GPIO2 | safe state and polarity require HIL |

The SD route now composes and identifies at `SupportLevel::Compiles`, while
holding EPD and LoRa inactive on the shared SPI bus. It discovers/mounts the same
explicit card-resident locator schema and remains unqualified; current boot
safety prevents destructive provisioning on-device. Audio, IMU/light, optional
modem, vibration, and other fitted/optional functions become supported only
through separate reviewed drivers and the revision-specific peripheral suite.

## MKS ESP32 FOC V1.0 model

The selected vendor branch describes a dual-motor integrated board based on an
ESP32-WROOM-32D. The official module datasheet establishes 4 MiB as the normal
flash capacity for that module, while custom capacities and the received
assembly remain physical checks. Only the V1.0 schematic/manual and the module
and EG2133 datasheets were used for this target; vendor example and third-party
FOC source were not inspected or used.

Revision-specific facts reconciled from the V1.0 schematic:

| Function | Motor 0 | Motor 1 |
| --- | --- | --- |
| 3-PWM phase outputs | GPIO32, GPIO33, GPIO25 | GPIO26, GPIO27, GPIO14 |
| Independent inverter enable | none established; GPIO22 is marked unconnected | none established; GPIO12 is marked unconnected |
| AS5600 I²C SDA/SCL | GPIO19 / GPIO18 | GPIO23 / GPIO5 |
| Encoder index/auxiliary input | GPIO15 | GPIO13 |
| Inline current ADC inputs | GPIO39 / GPIO36 | GPIO35 / GPIO34 |

Each phase signal is tied to a paired EG2133 active-high HIN and active-low
LIN-bar input. The component truth table therefore makes a driven low select
the low-side MOSFET and a driven high select the high-side MOSFET. High
impedance is the documented both-off candidate because the driver internally
biases HIN low and LIN-bar high. The firmware must never call a driven-low
phase state “disabled.” Board-level power-on, reset, watchdog, and high-Z
behavior remain physical qualification gates.

The schematic/manual establish no fitted SD card or other mutable cache medium.
The board package therefore reports storage as unavailable instead of
advertising cached-job execution. An external storage design can be added later
as an explicit board variant; it is not inferred from unused GPIOs.

The four current pins are ADC1-class on classic ESP32, which is favorable for
simultaneous Wi-Fi, but that observation does not prove PWM-synchronized sample
quality. Before any torque mode is advertised, reconcile the exact board
revision/module, MOSFET/gate-driver topology, shunts/amplifiers, polarity,
current range, bus/temperature sensing, shutdown/fault path, PWM frequency/dead
time, ADC attenuation/calibration, connector pinout, power input, and cooling.
Vendor current and voltage figures remain unqualified claims until measured.

The board capability record must expose distinct physical power stages,
current-sense channels, sensor buses, timer/ADC relationships, and safe shutdown;
it must not expose six unrelated generic PWM pins. Runtime configuration adds
motor pole pairs, phase order, resistance/inductance/flux or KV facts, sensor
direction/resolution, current/voltage/speed limits, control rates/tuning, and
calibration uncertainty.

The typed package and firmware feature now compile for
`xtensa-esp32-none-elf`. Core 1 takes sole ownership of MCPWM0/1, ADC1, both I²C
buses, and all motor routes, and synchronously makes all six phase pins no-pull
inputs before its first await. GPIO2, which participates in the USB
auto-programming/strap circuit, belongs to core 0 rather than being exposed as
an auxiliary realtime input. Core 0 also owns Wi-Fi and service UART resources
and exposes a permanently faulted non-fitted cache backend. Each encoder
connector is now a distinct compile-supported, externally populated AS5600
endpoint at address `0x36`; that is driver/route evidence, not a presence or
measurement claim. Configuration V3 can retain a complete rotor/current/timing
profile only behind the still-unqualified stage gate. This is a
`compiles`, non-armable safe composition: reset behavior, inverter both-off
behavior, MCPWM/ADC operation, sensor traffic, current measurement, safety
inputs, and energization remain unverified.

## T-LoRa Pager late target

The current LILYGO T-LoRa Pager uses ESP32-S3 with 16 MiB QSPI flash and 8 MiB
QSPI PSRAM and satisfies the dual-core policy. The official inventory includes a
480×222 SPI display, SD, MIA-M10Q GNSS, SX1262-family LoRa options, NFC, motion
sensor, RTC, charger/gauge, haptics, audio, keyboard, rotary input, and an I/O
expander. M10 first adds a compile-only board/resource/photo stub from current
LilyGoLib hardware material. Full driver work waits until TinyBee, T-Deck Pro,
and the first FOC slice are stable. No Pager feature may delay the first workflow.

## Existing Alumina board seeds

`alumina-firmware` contains metadata/modules for these boards. They are useful
inputs but not yet authoritative board packages:

| Seed | Initial action |
| --- | --- |
| `esp32drive` | Reconcile source pin constants with schematic/PCB revision; classify dual-core and power-stage capabilities |
| `esp32cam` | Identify exact module/camera revision and reserved PSRAM/camera/flash pins before exposing general I/O |
| `xprov5` | Reconcile the local ESP Rust board documents; reject any single-core variant before board scheduling |
| `mks_tinybee` | Replace constants with the typed I²S/TinyBee profile above |

No existing board module is copied blindly into the new board registry.

## FluidNC-compatible hardware strategy

FluidNC’s current repositories demonstrate a useful split: firmware provides
hardware abstractions and machines are described in external YAML. Current
official/contributed configuration collections include, among others, MKS
TinyBee, MKS DLC32, BlackBox X32, 6 Pack variants, Jackpot CNC Controller,
FYSETC E4, and TMC2130/TMC2209 examples.

For this greenfield plan, “FluidNC compatible hardware” means only that
`aluminafw` has an independently reviewed board package for the same physical PCB
and revision. FluidNC configuration files are valuable evidence for pin maps,
aliases, bus engines, and hardware use, but are not an accepted or converted
Alumina format. No binary, WebUI, protocol, YAML, or GRBL compatibility is
promised.

### Later physical-board candidates

TinyBee is committed. These remain later physical-board candidates after the
first targets, MKS FOC, and T-LoRa Pager stub:

| Candidate | Why useful | Due diligence before scheduling |
| --- | --- | --- |
| MKS DLC32 v2.1 | Common ESP32 CNC/laser board and vendor overlap | Exact revision, I²S/pin mapping, spindle/laser safety, display support |
| Jackpot CNC Controller | Current FluidNC ecosystem target | Schematic/license, module revision, expansion strategy, HIL hardware |
| 6 Pack / 6 Pack External | Modular controller/expander model | Baseboard/module matrix, bus/timing limits, safety outputs |
| BlackBox X32 | Integrated CNC product profile | Hardware availability, published electrical detail, redistribution/support expectations |
| FYSETC E4 | Multi-axis printer-style ESP32 controller | Revision-specific pin/driver mapping and voltage/load validation |

Schedule a candidate only when its exact dual-core revision, documentation,
hardware fixture, and motivating machine are available. A long compile-only
profile list would reduce safety and maintainability.

## Board package contract

Conceptual metadata shape:

```toml
schema = 1
id = "mks-tinybee-v1"
chip = "esp32"
cores = 2
flash_bytes = 8388608
qualification = "described"

[safe_state]
motion_enable = false
heater_enable = false

[[engines]]
id = "i2so0"
kind = "i2s-static-or-stream"
bck = "gpio.25"
data = "gpio.27"
ws = "gpio.26"
owner = "realtime"

[[aliases]]
name = "IO129"
resource = "i2so0.1"
capabilities = ["step"]
```

The actual schema must also express:

- revision/provenance and schematic hash;
- pin direction, input-only/strapping/reserved status, pulls, active levels,
  voltage domain, isolation, and current/load warnings;
- bus/device topology and exclusive/shared ownership;
- engine frequency/resolution/DMA/interrupt/core limits;
- safe boot, unconfigured, fault, watchdog, and power-down values;
- resource aliases and common FluidNC names;
- licensed photo asset digests plus normalized connector, device, pin, and net
  hotspots used by the live diagnostic UI;
- driver microstep/current modes, step timing, PWM/ADC/sensor relationships,
  clock resolution, and measured/qualified dynamic limits needed by UI CAM;
- flash/partition and asset budgets;
- supported security/update features;
- CI target and required HIL suite; and
- a stable exported capability digest.

Metadata cannot instantiate `esp-hal` peripherals or prove Rust ownership. The
corresponding small Rust module consumes the chip peripherals, constructs typed
service/real-time bundles, and statically verifies what the schema cannot.

## Adding future relay and industrial-I/O boards

A new board adds a package and tests, not branches in the application. Industrial
profiles must additionally model:

- relay normally-open/normally-closed behavior and maximum switching rate;
- boot glitches, external pull networks, and watchdog/failsafe hardware;
- isolation boundaries, logic/field voltage, source/sink/current characteristics;
- analog range, scaling, calibration, uncertainty, and overrange behavior;
- RS-485 termination/direction timing, Modbus/CAN timing, and galvanic isolation;
- safety classification and whether firmware control is advisory or safety
  related; and
- safe behavior on brownout, reset, loss of network, queue underrun, and update.

The capability API exposes these constraints so the interface can prevent, for
example, wiring a high-rate PWM node to a mechanical relay or treating a shifted
TinyBee output as an ADC pin.
