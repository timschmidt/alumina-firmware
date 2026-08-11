# M7 distributed clocks and cached-job scheduling evidence

Date: 2026-08-10

Status: canonical clock and schedule protocols, causal exact clock estimation,
boot-bound cached schedules, firmware service/real-time authority separation,
and adversarial two-MCU simulation are implemented. This is software evidence,
not a physical timing, atomic-start, armability, or motion-output claim.

This file records the version-1 checkpoint as reproduced on its date. Schedule
wire version 2 and the later `Priming`/`Primed` hardware-horizon boundary are
documented in `M7-PRESTART-HARDWARE-PRIMING.md`; no version-1 compatibility path
is retained.

## Implemented boundary

The portable `alumina-clock` crate defines allocation-free, canonical clock
messages and an exact causal affine estimator:

- each authenticated heartbeat binds the UI probe time, boot identity, device
  receive/transmit cycles, declared counter frequency, queue horizon, minimum
  commit lead, maximum tolerated lateness, deadline-miss count, and bounded
  queue observations;
- the estimator uses the causal inequalities from every accepted exchange and
  bounded oscillator drift. It does not assume equal Wi-Fi delay in the two
  directions or silently replace uncertainty with a midpoint estimate;
- UI time is converted to an interval of possible device cycles. Commit is
  healthy only when the sample count, age, uncertainty, lead, admitted horizon,
  and device-reported deadline state all satisfy the caller's explicit policy;
  and
- 64-bit counters are carried end to end, including deadline-miss counts, so a
  long-lived fault history cannot wrap through a narrow wire field.

`alumina-job` adds a canonical three-stage schedule protocol. `JobPrepare`
validates and retains the cached stream, `JobCommit` installs an exact local
start cycle without authorizing it, and `JobConfirm` authorizes that installed
commit. Every schedule is bound to the current boot ID, configuration and
capability identities, prepared-stream digest, participant-set digest, local
job digest, clock probe, clock uncertainty, finite lease, confirm deadline,
abort guard, and commit ID. A missing confirm expires locally; an exact abort is
accepted only before its guard. A late start, expired lease, malformed state
transition, or execution handoff failure becomes terminal rather than running a
different interpretation.

The ESP firmware wires those contracts across the existing bounded dual-core
boundary:

- core 0 owns authenticated Wi-Fi heartbeat service, stores only a fresh and
  safety-healthy probe for commit admission, and correlates service and
  real-time job reports;
- core 1 samples its own Embassy clock, reports real-time deadline health,
  independently decodes every schedule command, and rechecks zero missed
  deadlines and armed safety before confirm;
- commit requires an attended finite-lease policy, a completely prepared
  cached stream, pre-admission of the first block, and a start within the
  device's reported lead and horizon; and
- neither first board package is armable and the firmware has no arm
  transition. If a start action is nevertheless reached before a motor
  executor exists, it faults the schedule and latches safety fault instead of
  touching an output.

Cached-autonomous policy, lease renewal, browser/WASM clock-worker integration,
and the real-time motor executor remain closed.

## Simulation and failure semantics

The deterministic simulator uses two independent affine device clocks with
different offsets, +40 ppm and -35 ppm frequency errors, and asymmetric Wi-Fi
delays. Exact causal observations map one UI epoch into bounded device-local
start intervals, and both simulated devices start within their declared error
bounds.

Further scenarios deliberately drop install and confirm messages. The
coordinator reconciles installed state before confirmation and aborts reachable
participants within their abort window when confirmation is incomplete. A
device reboot changes its boot identity and rejects stale commit and confirm
messages.

Wi-Fi messaging cannot prove atomic start in the presence of an unreachable
participant after some peers receive confirmation. The protocol exposes that
limit rather than concealing it: the UI must reconcile status and abort
reachable peers before the guard, while a hardwired E-stop/interlock remains
the authority for hazards requiring fail-safe coordinated shutdown.

## Reproduced checks

Run from the repository root:

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board t-deck-pro --profile release
cargo tree --workspace --all-features --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree --locked --offline --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32-none-elf \
  --no-default-features --features board-mks-tinybee --locked --offline \
  --prefix none --format '{p}|{l}'
cargo tree -p alumina-firmware --target xtensa-esp32s3-none-elf \
  --no-default-features --features board-t-deck-pro --locked --offline \
  --prefix none --format '{p}|{l}'
git diff --check
```

The complete default workspace has 180 passing unit tests. Focused suites have
4 clock, 11 job, and 14 simulator tests. Host and both ESP strict Clippy gates
pass, and both optimized images link. `llvm-size` reports:

| Board image | text | data | bss aggregate | linker `.stack` |
| --- | ---: | ---: | ---: | ---: |
| MKS TinyBee V1.x | 823,692 | 11,976 | 250,160 | 45,580 |
| T-Deck Pro | 771,749 | 12,728 | 525,632 | 152,156 |

These are linked capacity observations, not runtime stack watermarks or timing
measurements. The default, all-feature workspace, TinyBee, and T-Deck Pro
deduplicated package/license inventories contain 60, 320, 230, and 237 records,
respectively. None has a missing license or a GPL/AGPL/LGPL/SSPL-family license,
and source/header/manifest scans find no GPL-family implementation material.
`cargo-deny` remains configured in the repository and CI, but it is not
installed locally, so this checkpoint does not claim a local `cargo deny`
result.

## License and claim boundary

New code is repository-owned `MIT OR Apache-2.0`. Permissive MIT, Apache-2.0,
BSD, and ISC dependencies remain acceptable under review. GPL-family code,
assets, source checkouts, and implementation dependencies are excluded. No
Synthetos or SimpleFOC implementation source was used for this milestone.

No board was connected, flashed, or energized. No physical Wi-Fi exchange, SD
cache run, distributed start, E-stop edge, timer jitter, queue latency,
stack-watermark, or output behavior is claimed. Both board packages remain
non-armable and no code path can intentionally begin physical motion. The next
software gate is the independently validated real-time motion executor, with
exact curve-to-machine-resolution lowering, before HIL can qualify any output.
