# ESP32 peripheral and protocol coverage

## What “all” means

There is no finite, stable list of every protocol that can run over an ESP32
UART, SPI bus, radio, or TCP socket. The actionable platform promise is:

1. Every peripheral exposed by the pinned ESP chip/HAL metadata can be represented
   in the board capability schema, including an explicit `unimplemented` or
   `unavailable-on-this-chip` state.
2. Common transport and protocol implementations use stable extension contracts,
   so adding one does not change the board, graph, or control architecture.
3. The connected firmware reports exactly what is compiled, physically routed,
   allocated, and qualified. The interface generates only deployable nodes by
   default.
4. Support claims are per chip, board revision, driver mode, execution domain,
   and qualification level—not “ESP32” in the abstract.

This makes broad support achievable without claiming that an untested peripheral
or arbitrary third-party wire protocol is production-ready.

## Generated coverage ledger

At the pinned toolchain revision, `xtask` should combine:

- `esp-metadata`/`esp-hal` chip and peripheral metadata;
- aluminafw driver registrations and stability annotations;
- board routing, fitted devices, reserved pins, and electrical constraints;
- selected feature flags and memory/runtime requirements;
- registered protocol adapters and graph-node schemas; and
- CI/HIL qualification records.

It emits one canonical ledger used by CI, firmware discovery, documentation, and
`alumina-interface`. Each row has:

```text
chip -> board/revision -> physical peripheral -> driver/mode -> resource ID
     -> owner core/domain -> clock/DMA/interrupt/memory constraints
     -> protocol adapters -> node kinds -> compile/bench/qualified status
```

CI fails when the pinned HAL adds/removes/changes a peripheral without a reviewed
ledger update, when a board claims a resource absent on its chip, or when a graph
node advertises an implementation not present in firmware.

## Hardware peripheral families

The exact set varies substantially across ESP32, S2/S3, C2/C3/C5/C6/C61, H2,
and P4. The first boards use classic ESP32 and ESP32-S3. A board is selectable
only when its application MCU supplies two cores; single-core families remain
visible in generated metadata solely to explain `unsupported-core-count`.

| Family | Resources/modes to represent | Initial implementation intent |
| --- | --- | --- |
| CPU/runtime | core count, clock tree, reset reason, software interrupt, critical sections, low-power/ULP core where fitted | Required for every board; second-core and clock ownership in M2 |
| Time/watchdog | system timer, timer groups, alarms, capture/compare where fitted, RTC time, watchdogs | Monotonic clock, scheduler, deadline and watchdog foundation in M2–M4 |
| GPIO/IO matrix | digital in/out, open drain, pulls, inversion, edge/level interrupt, debounce, RTC GPIO, strapping/input-only/reserved pins | Full board-routed GPIO lifecycle in M2–M4 |
| DMA | chip-specific DMA/PDMA/GDMA channels, descriptors, circular buffers, memory constraints | Internal allocation service; never exposed as an unsafe raw UI handle |
| Waveform/counter | RMT RX/TX, PCNT, LEDC, MCPWM, capture, sigma-delta where fitted | Step, PWM, encoder, pulse measurement and control nodes in M4–M8 |
| Analog | ADC unit/channel, oneshot/continuous modes as available, calibration, DAC, touch, temperature and analog sensor blocks | Board-calibrated ADC first; other functions only on routed/qualified chips |
| I²C | master, async/blocking, addressing/speed, shared-device topology; slave if HAL/chip implementation is selected | Master and T-Deck devices in M1–M4; slave later by board use case |
| SPI | master/slave, full/half duplex, DMA, chip-select policy, shared buses, QSPI/Octal modes where supported | Master/shared devices in M1–M4; slave/high-speed modes later |
| I²S/audio/parallel | standard audio, TDM/PDM where available, parallel/camera modes, DMA, TinyBee static/stream shift output | TinyBee output and T-Deck needs first; audio/parallel modes board driven |
| UART | async RX/TX, hardware flow control, half-duplex/RS-485 direction, break, framing/error/timestamps | Console plus general scheduled serial resource in M4 |
| TWAI/CAN | controller, bit timing, filters, alerts, timestamping, queue policy | Raw CAN frames in M4/M10; higher protocols are adapters |
| USB | Serial/JTAG and USB OTG/device/host capabilities where fitted | Diagnostics first; class support staged per S3/S2/P4 board |
| Ethernet | MAC/RMII and external SPI Ethernet device profiles | After first Wi-Fi release, driven by selected industrial board |
| SD/storage | SPI SD, SDMMC where fitted, flash partitions, immutable assets/config/job caches | TinyBee SPI SD first; resumable idle-only mutation and bounded verified job reads/prefetch during execution |
| LCD/camera/media | LCD/CAM/parallel interfaces, frame buffers, camera and display device profiles | T-Deck EPD via SPI first; camera/LCD boards are independent packages |
| Crypto/security | RNG, AES, SHA, RSA/ECC/HMAC where fitted, eFuse, secure boot, flash encryption, signature verification | Use for platform security; expose safe services, not raw key/eFuse access |
| Radio | Wi-Fi, BLE, ESP-NOW, low-level IEEE 802.15.4 where chip-supported | Wi-Fi first; BLE provisioning/ESP-NOW/802.15.4 staged by board and coexistence limits |
| External/virtual I/O | I²S shift outputs, GPIO expanders, ADC/DAC/relay modules, remote CAN/Modbus I/O | Same typed resource/lease model with explicit timing/electrical limits |

Some current HAL functions are unstable or incomplete, and radio coexistence or
ADC/resource conflicts vary by chip. The ledger records pinned-version maturity
and mutual exclusions. Aluminafw does not hide an unstable dependency behind a
stable-looking UI claim.

## Protocol layers

Separate physical resource, transport/framing, application protocol, and device
driver. For example:

```text
Uart(2) -> RS-485 half duplex -> Modbus RTU client -> industrial temperature node
Twai(0) -> CAN 2.0 frames     -> CANopen adapter   -> servo drive node
Spi(2)  -> device CS/IRQ      -> SX126x driver     -> LoRa packet node
TCP     -> bounded socket     -> Modbus TCP        -> remote I/O node
```

This prevents every device driver from creating its own pin, timeout, buffer,
telemetry, and graph semantics.

### Transport/framing adapters

Planned extension traits/schema cover:

- byte streams and datagrams with fixed buffer pools;
- baud/bit rate, framing, flow control, turnaround, chip-select, address, and bus
  arbitration;
- transaction deadlines, retries, cancellation, priority, and maximum response;
- CRC/checksum and explicit endian/word encoding;
- timestamp/clock domain and real-time versus service ownership;
- error counters, bus recovery, offline state, and health telemetry; and
- resource leases plus safe behavior when a client or protocol task disappears.

Parsers for complex or variable-length protocols run on core 0. Core 1 may own a
timed transport engine and accept/emit only bounded validated transaction frames
when a real-time deadline requires it.

### Initial protocol catalog

| Layer | Protocols/features | Planned order |
| --- | --- | --- |
| Network foundation | IPv4, DHCP client/server as needed, DNS/mDNS, TCP, UDP, time sync | M3 |
| Web/control | HTTP, WebSocket binary telemetry/commands, JSON discovery/config, signed update upload | M3–M4 |
| Serial basics | raw UART, line/packet framing, COBS/SLIP where selected, RS-485 direction | M4 |
| Source geometry import | Selected CNC G-code geometry lifted exactly into Hypercurve/Hyperpath | UI/WASM only in M5; no firmware parser or GRBL/FluidNC semantics |
| Industrial serial | Modbus RTU client/server with bounded maps | M10 or earlier if selected hardware requires it |
| CAN | raw TWAI/CAN frames, filters, diagnostics; CANopen/J1939 only when chosen | M4/M10; ordinary peripheral use, not Alumina multi-MCU transport |
| Messaging | MQTT client and durable/offline policy if required | M10, service core only |
| Radio local | BLE provisioning/GATT, ESP-NOW datagrams | After Wi-Fi baseline and coexistence tests |
| 802.15.4 | raw MAC/PHY first; Thread/Zigbee only with an audited compatible stack | Chip/board driven; dual-core eligibility and radio-coexistence policy apply |
| T-Deck | SX1262/LoRa packets and GPS data via imported drivers | M1–M3 |
| Storage | Raw append-only SD cache region for authoritative content-addressed per-MCU jobs; optional separate exchange filesystem | M3; mutation idle-only, execution uses bounded prefetch |
| Automation gateways | Modbus TCP, OPC UA/MQTT gateways, or custom protocols | Service-domain plugins selected by real deployments |

TLS, mTLS, MQTT, CANopen, Thread, Zigbee, OPC UA, EtherNet/IP, and other substantial
stacks are not one-line “protocol flags.” Each needs dependency/license review,
bounded memory and parser analysis, threat model, interoperability fixtures, and
a board/runtime qualification record. The extension model admits them; the UI
does not advertise them until implemented.

The native Alumina multi-MCU clock/job protocol is Wi-Fi/LAN-only in this plan.
UART, USB, TWAI/CAN, Ethernet, LoRa, ESP-NOW, and industrial protocols may expose
ordinary device or graph resources, but none is an alternate synchronization
transport. Adding one would require a new architecture and qualification
decision rather than a transport enum value.

## Capability and graph-node contract

Every peripheral driver or protocol registers:

- stable kind/schema version and implementation revision;
- supported chips/modes and required board routing/devices;
- parameters with types, units, ranges, defaults, and mutual exclusions;
- resource claims, owner domain/core, interrupts, DMA, buffers, and clocks;
- commands, events, sampled/stream values, timestamps, and quality flags;
- safe state, watchdog, timeout/recovery, and fault mapping;
- maximum rates/lengths/concurrency and measured qualification limits;
- graph nodes and their allowed HostExact/Service/Realtime domains; and
- simulator, compile test, HIL test, and qualification identifiers;
- exact/rational hardware facts plus bounded calibration/timing uncertainty that
  affect authoritative UI precision and path planning; and
- annotated-photo hotspot IDs for every user-visible connector/resource.

`alumina-interface` derives palette entries, configuration forms, terminals,
help, warnings, and plot channel metadata from this record. It does not contain a
second board/protocol capability database.

Representative generated nodes:

- `GPIO Read`, `GPIO Event`, `Scheduled Digital Out`, `Safe PWM Out`;
- `ADC Sample`, `Timed ADC Stream`, `Pulse Count`, `Quadrature Encoder`;
- `UART Transaction`, `RS-485 Transaction`, `I²C Device Transaction`, `SPI Device
  Transaction`, `CAN Send/Receive`;
- `Modbus Read/Write`, `GPS Fix`, `LoRa Packet`, `HTTP Request` (service only);
- `Stepper Axis`, `Servo Axis`, `Heater`, `Spindle/Laser`, `Safety Input`;
- `Timer/Timed Loop`, `Rate Transition`, `Watchdog`, and `Fault Latch`; and
- device-specific nodes generated on top of the generic transport/resource.

## Implementation waves

### Wave A — Foundation and requested hardware

- clock/timers/watchdogs, GPIO/interrupts, DMA allocation;
- I²C/SPI/UART foundations and all imported T-Deck devices;
- TinyBee I²S output engine, limits, ADC inputs, SPI SD;
- Wi-Fi, embassy-net, HTTP/WebSocket, provisioning, time sync;
- immutable job cache, UI/MCU heartbeat clock samples, and per-MCU readiness;
- LEDC/RMT/PCNT needed for scheduled I/O and stepper qualification.

### Wave B — Motion, analog, and industrial basics

- MKS ESP32 FOC V1.0 MCPWM, synchronized ADC1 current channels, dual AS5600
  buses, enables, and safe power profile;
- TWAI/CAN, RS-485 and selected Modbus modes;
- TMC UART/SPI devices, encoder/capture modes, DAC/touch where selected;
- Ethernet or USB for the first board that physically requires it.

### Wave C — Broader chip and protocol coverage

- BLE/ESP-NOW and supported radio coexistence profiles;
- supported IEEE 802.15.4 modes only on an eligible dual-core board;
- camera/LCD/audio/parallel I²S, SDMMC, low-power/ULP workflows;
- additional industrial/messaging protocols selected from deployments.
- current T-LoRa Pager metadata/build stub, followed later by board-driven
  peripheral support.

Within each wave, representability and clear `unsupported` diagnostics land
before deployment nodes; driver compile support lands before bench/qualified
status.

## Acceptance criteria for a coverage claim

A row may be marked `implemented` only when:

- its pinned chip/HAL feature compiles for every claimed mode;
- resource ownership and conflicts are validated from board metadata;
- simulator/loopback behavior and error paths are tested;
- API/config/telemetry schemas and generated graph nodes agree by digest;
- buffer, stack, allocation, rate, and timing limits are documented;
- safe state, watchdog, cancellation, recovery, and fault telemetry are defined;
- the relevant HIL fixture passes on every board marked `bench` or higher; and
- license/security/interoperability evidence exists for external protocol stacks.

The coverage page generated for a release should show `unavailable`,
`unimplemented`, `experimental`, `bench`, `motion-qualified`, or
`production-qualified` rather than a misleading boolean checkbox.
