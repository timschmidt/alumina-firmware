# ADR 0001: Strict dual-core ownership

Status: accepted, 2026-08-10.

Core 0 owns Wi-Fi, HTTP/WebSocket, SD, configuration parsing, telemetry encoding,
T-Deck devices, and idle work. Core 1 owns safety, motion, FOC, deterministic
sampled I/O, and every output capable of hazardous energy. Bounded command,
urgent-safety, and telemetry channels are the only normal cross-core paths.

Boards with fewer than two application cores fail selection. Dual cores do not
remove flash-cache coupling, so armed firmware also prohibits flash mutation and
keeps critical code/data and active buffers in internal memory.
