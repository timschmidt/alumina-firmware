# M10 runtime-health worker and live-device UI evidence

Date: 2026-08-14

Status: the production browser control worker now owns bounded passive
runtime-health polling beside its causal heartbeat loop. Strict schema-v2
worker snapshots carry independently revalidated queue and executor-stack
facts into the live-device panel. A real Chromium/localhost run covered both a
complete health snapshot and a deliberately lost health response followed by
bounded recovery. This is software/browser simulation evidence only; every
physical MCU, radio, output, motion, and safety claim remains closed.

The coordinated implementation checkpoints are:

- `aluminafw` `db623eb3b6dbd7566e47c7fd7e4f4e14d8203d25`;
- `alumina-interface` `6df1b5bab8a4d652a75f643a4a1e6daf3310a716`.

## Worker contract

`alumina-interface-client::worker` advances its exact JSON envelope to schema
version 2. The new projection carries:

- command, deterministic-work, and lossy-telemetry queue depth and capacity;
- service and real-time executor domain, raw ASWM flags, allocation, low
  exclusion, painted extent, minimum headroom, samples, completed sweeps,
  epoch cycle, and sample cycle;
- the enclosing service response cycle and RT freshness; and
- explicit `unobserved`, `unsupported`, or `available` health state, a separate
  consecutive-failure count, and a separate bounded diagnostic.

The projection is not trusted merely because it crossed the browser worker
boundary. Before inserting a snapshot, the rendering realm reconstructs
`StackWatermarkSnapshot` and `RuntimeHealthSnapshot` values and runs the native
ASWM/AHLT validators again. It rejects zero/overflowed queue capacity,
depth-over-capacity, unknown stack flags, executor-domain substitution,
impossible stack layout/counters/time, a fresh flag without an RT witness,
availability/payload mismatch, oversized diagnostics, and unpaired error
counts. Unknown JSON fields remain rejected by Serde.

The conservative Wi-Fi policy now declares both:

- heartbeat interval: 100 through 60,000 ms, default 1,000 ms;
- runtime-health interval: 1,000 through 60,000 ms, default 1,000 ms.

Health becomes due only after a successful authenticated heartbeat. A manual
clock probe cannot pull the health deadline forward, and reconnect/reset does
not shorten an already scheduled health interval. Only one operation per
device is in flight. A health failure retains the previous valid health model
and its own error without changing clock phase, clock estimate, deadline flags,
or safety authority. Fresh authentication clears health evidence whose boot is
not yet known while retaining a causal health error; a validated boot change
clears both the old evidence and old-boot health errors.

Worker diagnostics are canonicalized and capped at 512 UTF-8 bytes. Worker
events never contain the HMAC secret. The health request remains bodyless and
bound to `Digest::ZERO`.

## Visible diagnostics

The live-device panel now renders integer facts only:

- queue occupied/capacity/free counts;
- monitored allocation, conservative observed maximum use, and minimum
  headroom;
- complete allocation, low exclusion, painted extent, and unpainted reserve;
- sample and completed-sweep counts, epoch/sample cycles, and sample age; and
- present-fresh, present-stale, or absent RT witness state.

No percentage, inferred safe margin, arm state, or output authority is created.
The panel explicitly says that an incremental partial-boot watermark is not a
transient-depth, sizing, or safety proof. This checkpoint compiled that panel
into the optimized application; it did not use pixel or operator review as an
acceptance oracle.

## Production-service simulator seam

The existing MIT-licensed `ClockHttpFixture` now routes `FrameKind::Health`
through `RuntimeHealthService`, the same portable service used by firmware. The
fixture owns a boot-scoped health epoch and monotonic sample sequence, publishes
valid service and RT stack observations, and resets them with authentication on
reboot. Its deterministic facts are:

- command queue `0 / 8`;
- work queue `0 / 8`;
- telemetry queue `1 / 32`;
- both allocations 32,768 bytes with 256 excluded and 28,672 painted;
- service minimum headroom 20,480 bytes;
- RT minimum headroom 24,576 bytes; and
- initialized, partial-boot, current-pointer-bounded flags with a fresh RT
  witness.

The fixture still uses the production route classifier, CORS admission,
HMAC-SHA256 V2 request authorization, replay window, signed response proof,
native frame decoder, and fixed health encoder. One new fixture test verifies
the signed response, exact queues, both executor domains, freshness, stable
epoch, and monotonically increasing samples.

## Browser evidence

The final optimized bundle was served from `127.0.0.1:8097`; the authenticated
simulator listened on `127.0.0.1:8098`. Chromium 147 was started with background
networking, component updates, and sync disabled, plus a resolver policy that
rejected every host except `127.0.0.1`. The TinyBee and workstation WLAN were
not contacted.

The harness used a 100 ms heartbeat interval and 1,000 ms health interval. The
simulator deliberately processed control request 2 and closed the connection
without returning its signed health response. The harness retained an
intermediate schema-v2 snapshot with:

- lifecycle `clock_qualified`;
- six accepted and zero rejected clock samples;
- zero general/clock failures and no general error;
- health availability `unobserved`, no fabricated health payload;
- one health failure and a bounded health-specific fetch error.

No premature health retry occurred. On the next due attempt, the worker retained
the same clock session/model, advanced to seven accepted clock samples, and
accepted health with:

- exact queues `0/8`, `0/8`, and `1/32`;
- service and RT ASWM flags `0x000d`;
- two monotonic samples in each executor, demonstrating that the ambiguous
  first health request was processed once rather than replayed;
- common epoch cycle `24,329,401` and sample cycle `25,518,178`;
- the allocation/headroom facts above and `realtime_stack_fresh = true`;
- zero retained health failures and no health error; and
- a qualified clock interval with 7,113 cycles of reported uncertainty.

The DOM reached `passed` only after it had retained both the isolated failure
snapshot and the recovered exact health snapshot. The harness observes typed
worker events rather than reading application pixels.

## Verification

At the checkpoints above:

- `cargo test --locked --offline` in `aluminafw`: 538 tests passed, including
  55 `alumina-sim` tests;
- `cargo clippy --all-targets --locked --offline -- -D warnings`: passed;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface`: 203 tests passed, including 48 client tests;
- native workspace Clippy with `--no-deps -D warnings`: passed;
- full `wasm32-unknown-unknown` workspace check and warnings-denied Clippy:
  passed;
- warnings-denied workspace rustdoc: passed;
- `scripts/audit-source-policy.sh`: reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- optimized locked/offline Trunk build: passed;
- `wasm-tools validate`: passed;
- gzip and Brotli integrity checks: passed; and
- final Chromium health-loss/recovery harness: passed.

Tool versions were Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
147.0.7727.137, and Node.js 22.22.2.

The optimized browser artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 5,637,635 | `0eea7bf4847bff5dc391f43bcaee1bd9c1c89b5001919ea6dd6baa56f1768276` |
| `alumina-interface.js` | 91,813 | `8f304df5e51f160790e2dce59841d68cf48c7e38d4e144153ddf1de1e4cd9ffe` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

The optimized WASM compressed to 2,524,765 gzip bytes and 2,015,144 Brotli
bytes.

No dependency manifest or lockfile changed. The simulator extension reuses
existing MIT-licensed local Alumina crates, and the accepted source/license
inventory contains no GPL-family addition.

## Moving Hyper boundary

The window-free health/client tests do not build CSGRS or Hyper. Full
application/WASM integration necessarily resolved the current sibling workspace
sources. Hypercurve remained a user-owned moving, dirty development boundary;
`src/bezier_region.rs` and `src/curve.rs` were observed modified at the final
verification boundary, and other observed status changed while this checkpoint
was in progress. No Hyper/CSGRS file was edited, formatted, reset, pinned, or
committed by this work. The artifact hash proves the emitted bytes from that
integration run, not a reproducible frozen Hyper source checkpoint.

## Claims deliberately kept closed

This checkpoint does not establish:

- browser-to-ESP Wi-Fi, AP association, CORS, or radio reliability;
- physical TinyBee stack use, queue occupancy, timing, or core placement;
- background-tab scheduling behavior;
- production stack sizing or worst-case transient stack depth;
- motor, I2S/shift-register, MCPWM, ADC, SD, endstop, E-stop, or safety-chain
  behavior;
- live annotated-board rendering or operator usability; or
- any arm, motion, process-energy, interlock-reset, or safety authority.

The connected bare MKS TinyBee V1.0 remained untouched, with no motor power or
motors connected. Physical health traffic remains pending until workstation
Wi-Fi can be used without interrupting the user's internet session.
