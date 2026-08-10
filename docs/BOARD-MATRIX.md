# Boards and hardware strategy

Research snapshot: 2026-08-10.

## Support levels

“Supported” must be qualified. Every board entry carries one of these levels:

| Level | Meaning |
| --- | --- |
| `described` | Metadata/schema validates; no firmware build claim |
| `compiles` | Firmware and boot image compile in CI |
| `bench` | Boot, safe states, buses, and named I/O pass a hardware smoke suite |
| `motion-qualified` | Timed I/O and safety pass the board’s published trace/load envelope |
| `production-qualified` | Power-loss, update, sustained load, fault, and release evidence pass |

The interface displays this level and does not imply that a converted FluidNC
file makes a board electrically or operationally qualified.

## First vertical slices

| Board | MCU/memory | Why first | Primary slice | Important limitation |
| --- | --- | --- | --- | --- |
| MKS TinyBee V1.x | ESP32-WROOM-32U, 8 MiB flash, 520 KiB SRAM, dual core | Requested target with official hardware and FluidNC configuration | I²S shift-register step/heater/fan output, limits, SD, Wi-Fi, XYZ coordinated motion | Tight internal RAM; virtual outputs are not GPIO/PWM; no native FOC power stage/current sensing |
| LILYGO T-Deck Pro | ESP32-S3, 16 MiB flash, 8 MiB PSRAM, dual core | Existing Embassy drivers and integrated UI/radio hardware | Service-core peripheral parity, web UI, async bus ownership, telemetry | Shared buses and EPD latency require bounded/coalesced service tasks; PSRAM is not real-time memory |

Recommended order:

1. Compile and run T-Deck service tasks under the new core-0 executor while a
   synthetic real-time task measures core-1 deadlines.
2. Bring TinyBee up in safe state, validate physical inputs, then exercise one
   I²S output bit without motors/heaters connected.
3. Add one axis, logic-analyzer timing tests, three-axis coordination, limits,
   and finally hazardous loads behind explicit arming and hardware interlocks.

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
4. Measure sustained I²S/DMA behavior while Wi-Fi and asset serving are saturated.
5. Verify hard-limit latency and controlled-stop/emergency-disable paths.
6. Verify heater/fan watchdog and thermal-fault shutdown with representative
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
topology using GPIO36/GPIO33/GPIO47. The final board package must reconcile every
chip select, interrupt, reset, power-control, and shared peripheral with the
board documentation already stored in the source repository.

Drivers not present in the requested source repository are not invented during
the import. Audio, storage, IMU/light sensors, modem, vibration, and other fitted
or optional T-Deck functions should be inventoried against the exact hardware
revision and added as separate, tested drivers.

## Existing Alumina board seeds

`alumina-firmware` contains metadata/modules for these boards. They are useful
inputs but not yet authoritative board packages:

| Seed | Initial action |
| --- | --- |
| `esp32drive` | Reconcile source pin constants with schematic/PCB revision; classify dual-core and power-stage capabilities |
| `esp32cam` | Identify exact module/camera revision and reserved PSRAM/camera/flash pins before exposing general I/O |
| `xprov5` | Reconcile the local ESP Rust board documents and decide whether its single-core variants are lab-only |
| `mks_tinybee` | Replace constants with the typed I²S/TinyBee profile above |

No existing board module is copied blindly into the new board registry.

## FluidNC-compatible hardware strategy

FluidNC’s current repositories demonstrate a useful split: firmware provides
hardware abstractions and machines are described in external YAML. Current
official/contributed configuration collections include, among others, MKS
TinyBee, MKS DLC32, BlackBox X32, 6 Pack variants, Jackpot CNC Controller,
FYSETC E4, and TMC2130/TMC2209 examples.

“FluidNC compatible” should mean one of two precise things:

1. **Board-compatible:** aluminafw has a reviewed board package for the same PCB
   and revision, including every resource and safe state it claims.
2. **Configuration-import compatible:** an `xtask` converter accepts a documented
   subset of FluidNC YAML (`gpio.N`, `i2so.N`, selected axes/motors/buses) and
   emits Alumina configuration plus explicit unsupported-field diagnostics.

It should not mean binary compatibility with FluidNC, automatic coverage of all
community configurations, or an exact clone of its web UI/protocol.

### Proposed first-wave candidates

TinyBee is committed by the request. The following remain candidates until the
user chooses hardware and revisions:

| Candidate | Why useful | Due diligence before scheduling |
| --- | --- | --- |
| MKS DLC32 v2.1 | Common ESP32 CNC/laser board and vendor overlap | Exact revision, I²S/pin mapping, spindle/laser safety, display support |
| Jackpot CNC Controller | Current FluidNC ecosystem target | Schematic/license, module revision, expansion strategy, HIL hardware |
| 6 Pack / 6 Pack External | Modular controller/expander model | Baseboard/module matrix, bus/timing limits, safety outputs |
| BlackBox X32 | Integrated CNC product profile | Hardware availability, published electrical detail, redistribution/support expectations |
| FYSETC E4 | Multi-axis printer-style ESP32 controller | Revision-specific pin/driver mapping and voltage/load validation |

Pick at most two beyond TinyBee/T-Deck for the first production milestone. The
board contract should be proven with diversity, but a long untested profile list
would reduce safety and maintainability.

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
