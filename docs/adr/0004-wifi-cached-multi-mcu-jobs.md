# ADR 0004: Wi-Fi, per-MCU caches, and scheduled starts

Status: accepted, 2026-08-10.

Wi-Fi/LAN is the only first-generation Alumina coordination transport. The UI
maintains a boot-scoped affine mapping to each MCU cycle counter. Every participant
stores and validates its own immutable SD partition before prepare/commit installs
a sufficiently future local start cycle.

Wi-Fi is not an atomic safety bus. Cached autonomous jobs require complete local
bounds and interlocks; machines needing coordinated emergency removal of energy
use an appropriate physical safety chain.
