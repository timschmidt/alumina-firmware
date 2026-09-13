# Alumina patch to esp-rtos 0.2.0

This is the published `esp-rtos` 0.2.0 source from the `esp-rs/esp-hal`
project, licensed `MIT OR Apache-2.0` as declared in its `Cargo.toml`.

Alumina carries two instances of one source correction in `src/lib.rs`: stack
bounds use byte counts, while `slice_from_raw_parts_mut::<MaybeUninit<u32>>`
requires an element count. The original 0.2.0 code passed byte counts; this copy
divides each count by four. Current upstream uses the same conversions for both
the linker-owned core-0 stack and `SecondCoreStack::new`.

During physical qualification, the two second-core startup failure branches
use `core::panic!` instead of the defmt panic macro so the target's UART panic
handler retains their diagnostic message. This does not alter either branch's
condition or control flow.

A passive atomic startup-stage byte is also exposed for target panic evidence.
It is written only at existing second-core handshake boundaries, including the
main-task registration phases, and is not read for scheduling decisions.
Alumina reports it over UART if startup panics.

The same target-only diagnostic surface has one safe wrapper around read-only
Xtensa exception-register instructions. It preserves the exception cause,
address, PC, and debug cause that defmt's default panic bridge would otherwise
erase before Alumina's UART panic handler runs. These values are diagnostic
only and do not alter exception or scheduler behavior.

For the core-1 main task only, esp-rtos's duplicate hardware-watchpoint writes
are omitted both during registration and when returning from its idle context.
ESP-HAL 1.0 has already installed that permanent guard in `start_core1_init`;
reprogramming Xtensa breakpoint 0 faulted on the qualified classic ESP32
revision 1.0. The idle context explicitly reuses the same stack and guard.

All dynamically allocated esp-radio tasks, including its timer helper, are
pinned to the service core. This makes Alumina's core ownership invariant
enforceable below the Wi-Fi adapter: core 0 owns radio/service/idle work, while
core 1 contains only its permanently guarded Embassy real-time executor. Core
0 and all of its ordinary RTOS tasks retain the upstream watchpoint path.

The local patch is intentionally narrow so the firmware can retain its
otherwise-qualified esp-hal 1.0 dependency set. Replace this vendor directory
with the compatible upstream release when Alumina performs the esp-rtos 0.3 /
esp-hal 1.1 migration.

Upstream repository: <https://github.com/esp-rs/esp-hal>
