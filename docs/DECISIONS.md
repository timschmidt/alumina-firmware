# Resolved planning decisions

Decision snapshot: 2026-08-10. These choices supersede the earlier open-question
set. No unanswered product question currently blocks scaffolding or source
import.

## Product and compatibility

| Topic | Decision |
| --- | --- |
| Product status | Greenfield. There are no deployed units and all callers are controlled. |
| Existing Alumina projects | `alumina-firmware` and `alumina-interface` are behavioral examples only. Preserve useful functionality, not source structure, routes, schemas, UI interactions, or data formats. |
| Compatibility machinery | None. No legacy endpoints, protocol adapters, G-code execution route, schema negotiation, migration window, or FluidNC/GRBL compatibility shim. A mismatched UI/firmware schema is rejected and updated as a unit. |
| Firmware scope | Drivers, network/web service, resource protocol, safety, clocks, SD cache, command queues, real-time interpolation, step generation, FOC, telemetry, and thin glue. |
| CAM authority | Browser/WASM is authoritative. Firmware validates and executes already compiled integer machine work. |

## Licensing and clean-room policy

- New Alumina code and planning material target `MIT OR Apache-2.0`.
- A dependency licensed under either MIT or Apache-2.0 alone is acceptable when
  its maintenance, security, footprint, and target support are suitable.
- Every copied T-Deck driver remains Apache-2.0 with its copyright history,
  notice, source revision, and modification record intact.
- Synthetos/g2 and SimpleFOC are functional and architectural references. The
  motion planner and FOC stack are clean-room implementations shaped by exact
  Hyper arithmetic, ESP hardware, published mathematics, hardware datasheets,
  independently written requirements, and black-box tests. No g2 source is
  copied; no SimpleFOC implementation source is required or copied even though
  SimpleFOC is MIT-licensed.
- `THIRD_PARTY.toml`, SPDX headers, dependency-license CI, and an implementation
  provenance log are required before imported or reference-derived code merges.

## Boards and execution domains

| Topic | Decision |
| --- | --- |
| First boards | MKS TinyBee V1.x and LILYGO T-Deck Pro, with physical hardware available for both. |
| First FOC board | MKS ESP32 FOC V1.0 from the named vendor branch; hardware will be added to the bench. |
| T-LoRa Pager | Stub the current ESP32-S3 product late, then add support after the first targets and FOC slice. |
| Single-core chips | Rejected for now. Do not create degraded, cooperative, or Wi-Fi-disabled motion profiles. |
| Core 0 | Wi-Fi, AP/STA management, HTTP/WebSocket, SD service, telemetry encoding, T-Deck devices, and idle/background work. |
| Core 1 | Safety, step/servo motion, FOC, sampled control, limits, synchronized timers, and deterministic I/O. |

Future dual-core ESP32 boards may include relay controllers, industrial I/O,
laboratory equipment, and additional FluidNC-associated hardware. A board package
must describe physical facts and safe states; runtime stored configuration
describes the attached machine.

## Geometry, precision, and motion

- Exact CSGRS/Hyper geometry, transforms, curves, and path facts are retained in
  the UI. Hyperpath is the primary exact path/feed-schedule substrate and
  Hypersolve supplies exact/certified constraint solving where its retained
  problem structure applies.
- Numerical methods may propose a candidate; exact or interval-certified replay
  decides whether it is accepted. The requested precision and any undecided
  predicates are visible job diagnostics.
- Machine resolution is computed from commanded step or PWM/count lattices,
  encoder resolution, timer resolution, calibration uncertainty, process
  tolerance, and qualified driver limits. The target geometric quantization is
  at most half an effective command-lattice unit where attainable; geometric,
  timing, calibration, following, and control errors remain separate.
- Board capabilities and stored machine configuration report enough exact
  rationals, bounded approximations, uncertainty, rates, and driver/motor facts
  for the UI to choose required precision and compile a safe path.
- Initial stepper motion is Cartesian XYZ/E with homing, hard/soft limits,
  probing, forward/reverse lookahead, jerk-limited scheduling, bounded
  spindle/laser PWM, and TMC configuration/telemetry. Additional kinematics and
  process models follow measured base behavior.
- Legacy CNC G-code is a UI-only geometry import/export concern and is not
  load-bearing. Its decimal coordinates are lifted exactly and converted into
  Hypercurve/Hyperpath primitives before planning. Firmware never accepts it.

## Network, storage, and multi-MCU operation

- Every device boots into a local AP by default, serves its matching UI, and
  offers Wi-Fi scanning and association. Devices are intended for a local LAN or
  VPN behind a firewall, not direct Internet exposure.
- Wi-Fi/LAN is the sole planned inter-MCU transport. USB serial, TWAI/CAN, and
  other links remain ordinary peripheral features but are out of scope for
  distributed Alumina coordination.
- The UI estimates an affine mapping between its monotonic time and every MCU's
  unwrapped cycle counter from timestamped heartbeat exchanges, including drift
  and uncertainty.
- Each MCU receives, stores on SD, hashes, validates, and reports readiness for
  its own immutable partition of a global job. The UI performs a prepare/commit
  transaction and supplies a future local start cycle to every MCU. Starts are
  accepted only when clock uncertainty and lead time meet the job's declared
  synchronization tolerance.
- Protocol V1 canonical 256-bit identities use SHA-256. The algorithm is fixed by
  the exact schema version rather than negotiated per connection; browser
  WebCrypto interoperability and ESP acceleration/software availability are
  preferred over introducing a custom browser hash implementation.
- MCU uploads use fixed-size sequential content-addressed chunks with only the
  final chunk shorter. Resume begins at the first missing durable index. This
  bounds firmware RAM and journal state; out-of-order upload complexity is not
  carried into the first implementation.
- A cached job may continue semi-autonomously after Wi-Fi loss. Its outputs,
  duration, safety conditions, local interlocks, and fault behavior are therefore
  complete before start. Distributed emergency stopping depends on a physically
  appropriate safety chain, not Wi-Fi atomicity.
- Firmware and embedded UI update together. Signed bundles, recoverable updates,
  external signing-key ownership, and per-device credentials are accepted
  defaults. No compatibility negotiation layer is built.
- The first network implementation uses the mutually compatible no-std
  `edge-dhcp` 0.6, `edge-http` 0.6, `edge-nal` 0.5, and
  `edge-nal-embassy` 0.6 line over `embassy-net` 0.7. The edge crates provide
  bounded protocol/socket machinery; Alumina owns route policy, authentication,
  authorization, rate limits, job admission, and recovery semantics.
- Initial firmware reserves exactly two HTTP connections, 1,024 header bytes and
  12 headers per connection, 2,048 TCP bytes per direction per connection, one
  1,500-byte DHCP RX/TX pair, and four DHCP leases. These are reviewed starting
  limits, not performance claims, and may change only with renewed release/HIL
  evidence.
- A compile-time development AP password exists solely to make first hardware
  bring-up possible. Its provenance is disclosed without returning the secret,
  and it is structurally non-production-armable. A build-provisioned password is
  also non-production-armable; production requires transactional, unique,
  device-stored credentials plus authenticated mutation routes.

## Interface and graphical programming

- Substantial interface redesign is authorized. Current CSGRS/Hyper versions and
  Hypergraphics replace old geometry types and the hand-built renderer.
- The graph evolves toward a general LabVIEW-style, typed, timed, stateful
  graphical control environment without LabVIEW file, UI, or hardware
  compatibility claims.
- Firmware-deployed graph operations are whitelisted, fixed-memory, statically
  budgeted, and always subordinate to fixed safety policy.
- The primary hardware diagnostic surface uses licensed, revision-specific
  annotated photographs. Connectors, pins, devices, virtual outputs, and buses
  map to stable resource IDs. Live overlays show current value, direction,
  ownership, safety state, faults, qualification, and links to waveform capture.
- Scope/logic capture uses bounded device-side acquisition with timestamps,
  triggers, pre/post buffers, rates, and quality flags. A lower-rate all-resource
  overview is not represented as high-bandwidth raw capture.

## Review and available evidence

The first reviewer is an original RepRap developer. Available hardware and test
equipment include TinyBee, T-Deck Pro, USB debug access, a logic analyzer, a
webcam, a 10 MHz DSO, and a multimeter; MKS ESP32 FOC V1.0 is to be ordered.
Representative systems are RepRaps, AvidCNC-class routers, laser/plasma cutters,
and laboratory equipment from hobby through entry-level professional use.
Endstops, emergency stops, and basic safety interlocks are first-class inputs.

## Chosen first workflow

1. Create an exact 2D contour containing a line, arc, and Bezier in the UI.
2. Fetch a simulated then physical TinyBee capability/machine configuration.
3. Use Hyperpath scheduling and exact/certified constraints to produce a
   reproducible XYZ lattice/tick stream with an error report.
4. Replay it in simulation, including queue starvation, limit, and Wi-Fi-loss
   cases.
5. Upload the immutable stream and manifest to TinyBee SD, validate, arm, and
   execute a pen or air-cut trace.
6. Overlay the live board state on an annotated TinyBee photo and correlate UI
   waveforms, firmware telemetry, and logic-analyzer step/direction captures.

The second workflow partitions a synchronized trace between two simulated MCUs,
then between available dual-core boards using harmless GPIO/capture signals. It
qualifies clock mapping and cached prepare/commit start before a physical machine
depends on distributed motion.
