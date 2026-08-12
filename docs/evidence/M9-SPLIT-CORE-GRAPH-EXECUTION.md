# M9 authenticated split-core graph execution evidence

Date: 2026-08-12

## Scope

This checkpoint advances an authenticated, SD-published `ALGRIR01` package from
dual-core selection into live, repeatable execution by permanent Embassy task
owners. The browser supplies one exact future device-cycle epoch. Core 0 and
core 1 independently admit the same run identity, execute only their fixed
domain schedules, report their next release cycles and completed ticks, and
reconcile an exact stop before the shared bridge can be reused.

The reviewed implementation commits are:

- `aluminafw` `a14786b517fc9b326dc25e5bcb65eb6d8a82c461`
  (`Add reloadable split-core graph actors`);
- `aluminafw` `64739e92c5e52d8a193b37e29ea69267125bc360`
  (`Install graph actors in split-core firmware`);
- `aluminafw` `64fcfacfce947b31418973980309975048bc0345`
  (`Schedule exact split-core graph runs`); and
- `alumina-interface` `1bcdbaebbb7dac9bbc0a945aa54be317fe55670d`
  (`Drive exact graph runs from the browser`).

No connected hardware was read, erased, flashed, reset, or driven. The
available MKS TinyBee V1.0 remained a bare, unpowered-load board throughout.

## Permanent split-core ownership

One statically allocated `ReloadableGraphBridge` is shared only through bounded
critical sections. A `FixedGraphServiceActor` remains owned by the core-0
service task and a `FixedGraphRealtimeActor` remains owned by the core-1
realtime task. Their package, state, local queues, cursors, and execution phase
never move between cores and are not allocated per run.

Each actor independently installs the exact selected 4,096-byte package under
device, capability, configuration, implementation, storage-content, and
package-digest authority. The bridge cannot prime until both sides selected the
same `GraphDeploymentIdentity`. Repeated install, run, stop, rerun, and clear
operations reuse the permanent allocations without lifetime rebinding.

Core 0 runs Service tick zero while priming, before core 1 becomes Running. This
preserves the audited source-first rate-transition contract. Core 1 then
prepares and activates the same `GraphRunIdentity`; core 0 does not report
Running until it observes the matching core-1 report and shared bridge state.

## Exact authenticated run contract

Protocol V1 assigns `GraphStart` `0x0e05` and `GraphStop` `0x0e06`. Both carry
one canonical 136-byte `ALGRPR01` body containing:

- graph-install transaction ID;
- nonzero boot-local run ID;
- exact future device-cycle tick-zero epoch;
- full immutable storage-content digest;
- embedded package digest; and
- graph implementation-registry digest.

The body is accepted only for the currently authorized active package. A start
must be at least 500 ms and at most 60 s in the future, use a strictly increasing
run ID, and arrive while safety/configuration/job state permits execution. An
exact retry is idempotent. The inter-core `ALGX` command repeats every field in
128 initialized bytes; decode/re-encode equality and zero reserved bytes are
required on both sides.

The browser `GraphRunMachine` deliberately separates request acceptance from
execution. `Starting` becomes `AwaitingRunning` after an accepted mutation and
becomes `Running` only when both actors and the bridge report the exact run as
Running. Normal stop uses `Stopping` and `AwaitingStopped`; `Complete` requires
both actors Installed, the bridge Empty, and no future release. Ambiguous HTTP
I/O preserves the exact request for retry. A foreign transaction, package, run,
or epoch cannot advance browser authority.

## Release and fault semantics

Every nonempty domain retains the package-declared executor reserve. Its next
release window is the exact scheduled cycle through checked
`scheduled + reserve`. The service task chooses the earlier of its ordinary
10 ms wake and the next Service release. The realtime task includes the next
Realtime release in its minimum wake calculation. Neither task executes the
other domain.

An on-time dispatch passes the scheduled cycle—not the later observation—to the
fixed executor. A dispatch after the reserve passes the observed cycle and
therefore latches `UnexpectedReleaseCycle`. Missing execution/safety authority
latches `SafetyNotAuthorized` at the due release. Queue, arithmetic, shape, and
initialization failures retain their existing fail-closed behavior.

The first execution fault is shared and immutable until both actors acknowledge
the exact stop. Peer-observed faults and the intermediate state where one actor
has stopped while the other still observes `Stopping` have canonical telemetry
representations. The browser retains the first fault report even after the
firmware latch is cleared for a later run, and automatically makes exact stop
the next mutation.

Core 1 emits a canonical 64-byte `ALXR` execution observation. Core 0 combines
actor phases, bridge phase, run/epoch, next cycles, last ticks, and the shared
fault into a canonical 64-byte `ALXS` suffix. The complete authenticated
`GraphCoordinatorReport` is version 2 and exactly 312 bytes, which fills but
does not exceed the fixed service-response boundary.

## Verification

The firmware checkpoint reproduced:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo xtask build --board t-deck-pro --profile release
git diff --check
```

All 358 default-member portable tests and all default-member doc tests pass.
Strict portable Clippy passes. A raw `cargo test --workspace` is intentionally
not the host gate because it includes ESP-HAL-only members; its upstream target
guard rejected x86_64 before project tests ran. All four board-qualified
release images link:

| Linked target ELF | Bytes | SHA-256 |
| --- | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 10,288,932 | `c2e0c4233c8f84f7fc8d46b233dc74fca867fb967c8b73f25be787c715000057` |
| TinyBee V1.0, 4 MiB opportunistic | 10,290,264 | `706598a52922f6e5ef23e300e6396f2c54ffc211ec121be105db18b59f24dd41` |
| MKS ESP32 FOC V1.0 | 9,780,244 | `abe556af866a14e2152496c2d08037193b90a9e5ca0d817321c9a5b6d193efdc` |
| T-Deck Pro | 10,164,796 | `52000ba42e07a959cd6c4f01b29ab78ef18bfa8e7ef64693b002d866eb1979f7` |

These are ELF lengths, not flash payload lengths. Existing image-layout gates
remain authoritative. The firmware lockfile SHA-256 is
`5e8fa5c4549e581ddcdf7e18b8d758bbcd953c55e6f7f33d56c2a9aa4dfaef3b`.
The permanent actor allocations required reducing the secondary general heap
reservation to 4 KiB; the separate 64 KiB reclaimed radio/runtime heap remains.
No runtime heap low-water claim is made.

The interface checkpoint reproduced:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 98 native unit tests and all doc tests pass. Native and WASM strict Clippy,
strict rustdoc, the checked-out sibling CSGRS/Hyper source and permissive-license
audit, optimized Trunk build, WASM validation, and compression checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,260,756 | `6f621bd9900efd330e82cc11591b0dc1b8ac782a722e220b662d606032229c09` |
| `alumina-interface_bg.wasm.gz` | 1,980,102 | `5af3acc5d957bcc617cb2dafd87c89ec3b513f8ea224e2e9c41eb1bd7be9b42f` |
| `alumina-interface_bg.wasm.br` | 1,615,669 | `d58aee37b3b6e8fae33ea0124a82521cb1e5325da951ec8f895a0045a115a17c` |
| `alumina-interface.js` | 89,162 | `12efc3498ecb2c8da6cad130ee7046b02eaf01af9b4e3856119f5286869b874c` |
| `index.html` | 1,295 | `e5a2dd7e3fd141e63a59b6c0d579a76ee7c53f273ef7c7182dc4c17d769830cd` |

The interface lockfile SHA-256 is
`f7b0bf4bb9c54ecc536ad697513bde6a3c0f037058eebc027f9d6a22fd0a79ae`.

## Closed claims and next gate

This is fixed-opcode functional execution and compile-link evidence. The three
V1 opcodes remain resource-free Boolean constant/latest/sink operations. No
GPIO, timer, ADC, PWM, stepper, FOC, safety-output, storage, or protocol side
effect is graph-addressable. The package-declared reserve is enforced as a
lateness boundary but is not measured target WCET, interrupt latency, or Wi-Fi
coexistence evidence. No target graph run, logic-analyzer capture, heap/stack
watermark, or HIL timing result exists.

Active graph selection is still boot-ephemeral at this checkpoint. Reboot
recovery must gain a power-cut-tested durable selector before a selected graph
can survive reset. Capability-published arena limits and resource-bearing
opcodes must then bind every selector to board ownership and safety policy
before physical graph effects are admitted.
