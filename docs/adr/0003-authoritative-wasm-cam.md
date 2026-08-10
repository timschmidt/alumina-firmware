# ADR 0003: Browser/WASM is the authoritative CAM compiler

Status: accepted, 2026-08-10.

CSGRS and Hyper exact values remain in `alumina-interface`. Hypercurve, Hyperpath,
and Hypersolve produce and certify paths/schedules to configurable precision. The
named lossy boundary emits canonical integer/fixed-point per-MCU machine IR.

Firmware validates and executes this IR with bounded arithmetic. It does not parse
CAD, graph documents, raw G-code, or source geometry. GPU/display values cannot
satisfy CAM types. Native builds may provide tests and replay, but do not become a
second shipped authority.
