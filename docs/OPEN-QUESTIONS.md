# Open decisions and planning questions

The proposed defaults let implementation start without widening scope. Please
answer the blocking questions first; the later questions may remain ADRs with a
default until their milestone.

## Blocking before source import and board scaffolding

### Q1. Project and imported-code license

Which license should govern new `aluminafw` code?

- **Proposed default:** Apache-2.0 for the repository initially, matching all
  inspected T-Deck driver crates. A later MIT/Apache-2.0 dual license can cover
  new code while imported files remain explicitly Apache-2.0.
- Alternative: MIT/Apache-2.0 for new code from day one with per-package/source
  notices for Apache-only imports.

This decision determines repository headers, contribution policy, and whether
any SimpleFOC implementation code can be reused. Synthetos/g2 and Klipper should
remain clean-room behavioral references under either default.

### Q2. Exact T-Deck hardware scope

Does “existing T-Deck support” mean only the **T-Deck Pro represented by the
current async driver repository**, or must the original T-Deck also be a first-
wave target?

- **Proposed default:** T-Deck Pro first; treat original T-Deck as a separate
  board package after its exact revision/peripheral inventory is supplied.

### Q3. First-wave board list and revisions

Which physical boards and revisions are available for HIL?

- **Proposed default:** MKS TinyBee V1.0 and T-Deck Pro only for the first release
  candidate; add at most two FluidNC ecosystem boards in M9.
- Please identify any required early board among MKS DLC32 v2.1, Jackpot, 6 Pack,
  BlackBox X32, FYSETC E4, or another controller.
- Confirm the exact TinyBee revision(s) that must be supported.

### Q4. Policy for single-core ESP32-C3/C6-style boards

Should a single-core device ever be allowed to run motion/FOC and Wi-Fi together?

- **Proposed default:** production `rt-isolated` builds fail for single-core
  targets; an opt-in `cooperative-lab` profile permits nonhazardous I/O and
  explicitly disallows simultaneous Wi-Fi plus armed motion/FOC.
- Alternative: support a reduced-rate single-core real-time mode with documented
  timing limits, which adds substantial qualification work.

### Q5. FOC reference hardware

Which motor, power stage/inverter, rotor sensor, current-sense topology, bus
voltage/current, and target loop rates should define the first servo milestone?

- **Proposed default:** choose one ESP32-S3 dual-core board plus a documented
  3-PWM/6-PWM BLDC power stage, incremental or SPI magnetic encoder, inline or
  low-side current sensing, and a small current-limited motor. Qualify voltage
  mode first, then dq current control.
- If the intended first target is a two-phase closed-loop stepper rather than
  BLDC/PMSM, say so; it needs a different H-bridge and sensing profile.

TinyBee step-stick sockets cannot answer this question because step/direction
drivers do not expose the phase PWM/current-sampling path required by FOC.

### Q6. First-release compatibility contract

How much compatibility is required with the existing `alumina-firmware` and
FluidNC/GRBL ecosystems?

- **Proposed default:** preserve `/device`, `/device/image`, `/time`, `/pins`, and
  a safe subset of `/queue` for one migration window; accept G-code and a defined
  subset of FluidNC YAML as import formats; do not promise FluidNC binary/WebUI
  or full GRBL protocol compatibility.
- Please identify any deployed client, configuration, or machine that cannot
  tolerate that migration policy.

## Needed before motion and exact-machine-IR stabilization

### Q7. Machine-resolution/error policy

What should “machine resolution” mean by default?

- **Proposed default:** per-axis minimum of one commanded full/microstep or one
  encoder count, combined with calibration uncertainty and a user-specified
  process tolerance; target path quantization at no more than half of the
  effective command lattice where attainable. Report geometric, time, and
  following error separately.
- Please provide representative machine scales, axis counts, travel/rate,
  encoder resolution, desired path tolerance, and whether rotary axes are in the
  first planner.

### Q8. Path compilation location and offline operation

May the browser/WASM interface be the authoritative CAM/machine-IR compiler, or
is a native/headless compiler also required for automation and reproducibility?

- **Proposed default:** one portable Rust compiler library used by WASM and a
  native CLI; firmware validates/executes only. This avoids making a browser
  session the sole production path.

### Q9. G-code versus exact native curves

Is legacy G-code a first-class long-term job representation, or an import/export
format around exact CAD/CAM?

- **Proposed default:** import G-code for compatibility, but make canonical
  versioned machine IR the execution/job record. Start with certified line
  segments, then add exact-derived native line/arc/Bezier opcodes when verified.

### Q10. Motion feature priority

Which capabilities are first-release requirements beyond Cartesian XYZ?

- Choices include rotary ABC/UVW, CoreXY, delta/SCARA, dual motors/squaring,
  probing, spindle/laser, heater/extrusion, pressure advance, input shaping,
  encoder correction, and TMC UART/SPI diagnostics.
- **Proposed default:** Cartesian XYZ/E, homing/limits/probe, coordinated
  jerk-limited motion, spindle/laser bounded PWM, and TMC configuration; schedule
  other kinematics/process features after the base trace suite passes.

## Needed before networking and production deployment

### Q11. Network threat model and provisioning

Will devices be limited to a trusted local LAN, exposed through a gateway, or
directly reachable from the Internet?

- **Proposed default:** local-LAN/gateway deployment, unique device credentials,
  authenticated mutating API, same-origin interface, signed OTA, and no claim of
  safe direct Internet exposure. AP mode is for first-use/recovery provisioning,
  not a permanently open control network.
- Identify required enterprise Wi-Fi, Ethernet, BLE provisioning, TLS/mTLS, or
  offline-only modes.

### Q12. Bootloader and signing ownership

Who owns release signing keys and manufacturing provisioning, and is A/B OTA
required on all boards despite TinyBee’s 8 MiB flash?

- **Proposed default:** external/per-device or release-system key injection,
  never repository keys; signed manifests/images; recoverable A/B layout where
  flash permits, otherwise a separately justified recovery scheme.

### Q13. Web bundle update relationship

Must firmware and `alumina-interface` always update together, or may a newer UI
connect to older firmware independently?

- **Proposed default:** independently versioned protocol/schema negotiation;
  each firmware embeds a known-compatible UI fallback, while external/newer UIs
  connect when negotiated capabilities match.

## Needed before broad peripheral and graphical-programming scope

### Q14. Initial peripheral/protocol priorities

Which “all ESP32 peripherals and protocols” families have actual first-wave use
cases and hardware?

- **Proposed default order:** GPIO/interrupts, ADC, LEDC/MCPWM/timers/RMT/PCNT/I²S,
  I²C/SPI/UART, SD, Wi-Fi TCP/UDP/HTTP/WebSocket, TWAI/CAN, RS-485/Modbus, then
  Ethernet/USB/BLE/ESP-NOW and board-specific devices. T-Deck LoRa/GPS/display/
  touch/keyboard/battery remain early because drivers already exist.
- Identify industrial protocols such as Modbus TCP/RTU, CANopen, EtherNet/IP,
  MQTT, OPC UA, or custom framing that should change the ordering.

### Q15. Firmware-deployed graph authority

May user-authored graphs control hazardous outputs, or must they be supervised by
fixed firmware safety logic and an approved deployment signature?

- **Proposed default:** graph IR may command resources only through fixed safety
  policies; real-time opcodes are whitelisted and statically budgeted; hazardous
  deployments require an explicit arm action and may optionally require a signed
  graph/deployment manifest.

### Q16. LabVIEW-style ambition boundary

Is the goal a general embedded dataflow environment, or compatibility with
LabVIEW terminology/files/hardware?

- **Proposed default:** adopt useful typed dataflow concepts—nodes/wires, front
  panels, state/loops, timed domains, channels, probes, subgraphs, and deployment—
  without claiming VI file, NI hardware, or visual compatibility.

### Q17. Interface continuity

Should the current eframe/egui UI and graph editing interactions be preserved
visually, or is a substantial interaction redesign acceptable while retaining
data and capabilities?

- **Proposed default:** retain the Rust/egui/Trunk application and migrate in
  place; change the graph schema and interaction model only with fixtures,
  migrations, and usability tests.

## Hardware and delivery inputs

To turn this architecture into a scheduled implementation backlog, please also
provide:

- hardware actually available for automated HIL and quantity of each fixture;
- representative machines, motors/drivers, sensors, loads, and emergency-stop
  wiring;
- acceptable use of external test equipment (logic analyzer, capture MCU,
  oscilloscope, current probe, dynamometer);
- current deployment/user count and backward-compatibility window;
- contributors/reviewers and whether safety/security review is internal or
  external; and
- the desired first demonstrable workflow (for example: exact curve in the UI to
  a three-axis TinyBee pen plot, or closed-loop servo trajectory plus plots).
