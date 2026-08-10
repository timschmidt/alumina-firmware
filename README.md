# aluminafw

`aluminafw` is the planned Embassy-based firmware platform for Alumina machines,
instruments, controllers, and embedded user interfaces. It will combine the
asynchronous peripheral coverage and application structure of
`t-deck-async-drivers-rs` with the embedded web-interface delivery currently in
`alumina-firmware`, while adding a hard boundary between service work and
real-time control.

This repository currently contains the researched implementation plan, not
firmware source. Driver import and scaffolding deliberately begin after the
licensing, first-board, and single-core policy questions in
[Open questions](docs/OPEN-QUESTIONS.md) are resolved.

## Target outcome

- One Embassy application with one executor per ESP32 core on dual-core boards.
- Core 0 owns Wi-Fi, networking, the web server, UI/background peripherals, and
  idle work. Core 1 owns motion, motor-control loops, safety inputs, and timed
  I/O.
- Fixed-capacity messages are the only normal path between the two execution
  domains; service tasks never directly manipulate a live real-time resource.
- Thin compile-time board packages select the ESP32 chip and physical topology.
  A validated runtime machine configuration maps logical functions to those
  declared capabilities.
- T-Deck Pro and MKS TinyBee are the first two vertical slices. The board model
  is designed to admit selected FluidNC-compatible controllers and future relay,
  industrial-I/O, laboratory, and robotics boards without changing the core.
- The web interface discovers firmware capabilities and controls typed pins,
  serial ports, timers, buses, motion axes, and telemetry using a compact,
  versioned protocol.
- Exact CAD and curve data remain exact through CAM. The explicit firmware
  boundary is a checked conversion to integer motor steps, encoder counts, and
  timer ticks with a machine-resolution error certificate.
- The Alumina graph editor grows from a recursive CAD graph into a typed,
  stateful, timed dataflow environment with deterministic firmware deployment,
  streaming plots, and safety-aware I/O nodes.

## Planning set

- [Delivery plan](docs/PLAN.md) — milestones, dependencies, gates, risks, and
  acceptance criteria.
- [Architecture](docs/ARCHITECTURE.md) — dual-core runtime, resources, protocol,
  motion, FOC, exact toolpaths, safety, and proposed workspace layout.
- [Repository audit](docs/REPOSITORY-AUDIT.md) — what is reusable and what must
  change in each requested local repository.
- [Boards and hardware](docs/BOARD-MATRIX.md) — TinyBee mapping, T-Deck inventory,
  FluidNC compatibility strategy, and the board-package contract.
- [Peripheral and protocol coverage](docs/PERIPHERAL-COVERAGE.md) — chip-aware
  resource taxonomy, implementation stages, generated UI nodes, and honest
  qualification levels for the wider ESP32 family.
- [Interface roadmap](docs/INTERFACE-ROADMAP.md) — current-stack migration,
  Hypergraphics adoption, dataflow programming, and plotting.
- [Verification strategy](docs/VERIFICATION.md) — simulation, property tests,
  timing measurements, HIL, safety, and release evidence.
- [Open questions](docs/OPEN-QUESTIONS.md) — decisions needed before source
  import and hardware implementation.
- [Research sources](docs/SOURCES.md) — local and upstream evidence used for the
  plan, captured on 2026-08-10.

## Recommended first deliverable

The first release candidate is intentionally narrower than the eventual
platform: T-Deck peripheral parity, TinyBee XYZ step/direction plus endstops,
AP/STA Wi-Fi, static UI serving, capability discovery, live telemetry, safe
configuration, and an integer-timed jerk-limited motion queue. FOC, exact native
curve execution, additional FluidNC boards, and firmware-deployed graphical
programs follow behind measured real-time and safety gates.
