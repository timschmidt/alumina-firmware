# ADR 0002: Board facts are separate from machine configuration

Status: accepted, 2026-08-10.

A compile-time board package owns immutable PCB facts: chip, revision, routed and
virtual resources, buses/devices, electrical limits, safe states, clock/DMA facts,
and annotated-photo assets. Stored runtime configuration maps machine functions,
motors, drivers, mechanics, calibration, uncertainty, process policy, and safety
chains onto those capabilities.

Configuration cannot invent a resource, timing mode, safe state, or qualification.
Both layers have canonical digests bound into jobs and control messages.
