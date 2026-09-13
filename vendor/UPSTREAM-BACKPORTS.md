# Upstream ESP runtime backports

The vendored packages in this directory retain their upstream `MIT OR
Apache-2.0` licensing. They exist only where the newest compatible crates.io
release contains a verified hardware-affecting defect.

## `esp-rtos-0.3.0` and `xtensa-lx-rt-0.22.0`

Alumina targets the Espressif Xtensa Rust 1.90 toolchain. `esp-rtos 0.4.0`
requires Rust 1.95, so Alumina carries the relevant corrections from upstream
commit `998e4faeaf0afc92b494ece4edc75e80df5624f2` on top of the compatible
release:

- represent both main-core stack slices in `u32` elements rather than bytes; and
- track the idle context's stack owner for overflow checking; and
- preserve Xtensa user mode while interrupt handlers spill register windows,
  matching the user-mode bit in the task context being restored.

The interrupt-runtime patch is the exact assembly change from that commit,
applied to the otherwise unchanged `xtensa-lx-rt 0.22.0` selected by the Rust
1.90-compatible dependency set.

Upstream pull request: <https://github.com/esp-rs/esp-hal/pull/6027>

Remove this backport once the project can move to the upstream release that
contains these changes.

## Alumina task-affinity policy

The optional `radio-tasks-core0` feature is an Alumina-specific composition
policy rather than an upstream bug fix. It maps every task created through the
ESP-radio compatibility scheduler to `Cpu::ProCpu`, including vendor tasks
which request no affinity. Task priorities remain the values selected by the
upstream radio runtime. This preserves the firmware contract that Wi-Fi,
protocol, storage, and idle work stays on core 0 while core 1 owns real-time
execution without changing the vendor scheduler's priority policy.
