# M10 exact machine scheduling and executor preflight — offline evidence

Date: 2026-08-13

Historical scope: this records the initial line/arc and `ALMEVD01` checkpoint.
It is extended, not retroactively rewritten, by
[`M10-CERTIFIED-CUBIC-MOTION.md`](M10-CERTIFIED-CUBIC-MOTION.md).

## Result

The authoritative browser/WASM path now carries exact retained line/arc
geometry through configuration-derived dynamics, exact-stop lookahead,
jerk-limited scheduling, bounded firmware-V1 interpolation, production
electrical preflight, cached partitioning, and deterministic replay.

- Canonical configuration advances to V5 (`ALMCFG05`). Every motion document
  carries one exact instance-zero `TimerTickHertz`; every stepper document also
  carries one exact `StepperOutputQuantumCycles`. Core 1 retains both and
  rejects an executor whose compiled clock or selected backend differs.
- `ConfigurationDocumentView` exposes only completely validated canonical bytes
  and iterates records allocation-free. The interface derives command density,
  uncertainty, usable travel, pulse-rate-limited velocity, acceleration, jerk,
  following error, resource bindings, device time, and output lattice from that
  view rather than a second machine schema.
- `MachineResolutionBudget2` exactly composes source and controller allocations,
  endpoint half-step error, one-step DDA tracking, full-travel calibration,
  following error, and half-tick position error. An insufficient or unresolved
  envelope fails before scheduling.
- Hypercurve lines and circular arcs promote losslessly into Hyperpath. V1 uses
  zero-radius/zero-feed joins as explicit unblended stops. Hyperpath/Hypersolve
  replay the complete lookahead and four symmetric constant-jerk phases for
  each retained element, including curved-path acceleration and jerk limits.
- The current firmware IR remains constant velocity. Each smooth phase is
  subdivided by the certified spatial `A*dt²/8` bound, evaluated against the
  retained source, and rounded only at the configured step and tick lattices.
  Zero-step timed holds remain explicit segments.
- `preflight_stepper_segments` is allocation-free and uses the production
  `StepperExecutor` validation path. It checks pulse and rate limits,
  direction/driver setup and hold, output-grid alignment, continuity, overflow,
  terminal position/tick, emitted step counts, and earliest legal finish.
- Machine-bound packaging requires exact configuration/capability identity,
  then independently replays real chained 512-byte blocks before producing the
  content-addressed SD object and chunk manifest.
- `alumina-sim::replay_cached_stepper_partition` independently decodes and
  first rehashes the complete immutable object, admits every block through
  `RealtimeJob`, executes it through
  `CachedStepperExecutor` at exact deadlines, acknowledges ownership in order,
  and compares terminal position, tick, block digest, step counts, and finish.
- Canonical `ALMEVD01` V1 evidence binds exact-rational source geometry,
  configuration/capability, error allocations, timer/output lattice, executor
  results, and partition object/manifest identities. Replay reconstructs bytes
  rather than trusting a second parser.

The representative real-schema fixture uses a canonical two-stepper TinyBee
configuration, an exact 1 MHz device clock, one-cycle output quantum, and 1,600
nominal steps/mm. It schedules the exact four-millimetre line plus radius-two
semicircle, stops at their unblended join, lowers under a `1/1000 mm`
interpolation bound, ends at `[12800, 0]` steps, packages and simulates the
cached stream, reconstructs identical evidence, rejects a foreign capability,
and rejects transcript tampering.

## Verification commands

```sh
# hyperpath
cargo test --locked --offline

# aluminafw portable and target compositions
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked --offline
cargo +esp check -p alumina-firmware --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline
cargo +esp check -p alumina-firmware --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline
cargo +esp clippy -p alumina-firmware --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release

# alumina-interface native and browser/WASM
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo test --no-run --target wasm32-unknown-unknown \
  -p alumina-interface-core --locked --offline
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
```

Observed on the reviewed source tree:

- all 158 native interface unit/integration tests passed, together with the
  compile-fail Rustdoc test and all package doc tests;
- the complete interface-core unit/integration test binaries compiled for
  `wasm32-unknown-unknown`, including the machine-bound scheduling fixture;
- all 425 Hyperpath unit/integration/README tests passed, including the new
  zero-radius exact-stop and nonzero-feed rejection case;
- the complete portable `aluminafw` default-member suite passed, including all
  23 configuration tests and the production-executor preflight test;
- strict host, WASM, TinyBee, and T-Deck Pro Clippy passed, as did strict
  Rustdoc and workspace formatting;
- TinyBee 8 MiB, TinyBee 4 MiB, T-Deck Pro, and MKS ESP32 FOC V1 release images
  linked. GNU `size` reported respectively `1,037,396/12,200/249,944`,
  `1,037,420/12,200/249,944`, `976,021/12,960/525,408`, and
  `975,344/10,960/251,184` bytes of text/data/BSS;
- the source-policy audit accepted native and WASM dependency inventories,
  rejected GPL-family/missing-license entries, and confirmed every
  Alumina/CSGRS/Hyper package came from the sibling workspace. The lockfile adds
  only local `alumina-config` and `alumina-motion` edges; and
- the optimized browser bundle validated. Its WASM is 5,056,846 bytes with
  SHA-256 `9a18faa8af6cd86a97417f9d29ca310d583b6ba4a6b6a9b4a44837b5b6fcfb5f`;
  Brotli and gzip sizes are 1,838,868 and 2,288,870 bytes and both pass integrity
  checks.

`cargo-deny` is not installed in this offline workstation environment, so its
CI-only policy command is not claimed locally. No registry dependency was added
by this checkpoint, and the existing deny policy remains mandatory.

## Closed claims and remaining gates

- This is a development checkpoint against the current sibling source trees.
  The externally modified Hypercurve tree is authoritative for this build but
  prevents a clean reproducible release pin.
- V1 scheduling supports exactly two Cartesian stepper axes, axis-aligned lines,
  and explicit circular arcs. It stops at every unblended join. General curves,
  nonzero-radius blends, broader kinematics, and direction-aware limit use
  remain fail-closed or future work.
- `ALMEVD01` source serialization currently requires exact-rational line/arc
  parameters. The event-level simulator result is verification evidence, not a
  transcript flag.
- The firmware executes a certified constant-velocity approximation of the
  smooth jerk schedule; it does not contain an onboard jerk planner.
- No serial/USB endpoint, NetworkManager state, WLAN interface, TinyBee AP, or
  physical board was contacted. No board was reset or flashed. Motors, drivers,
  and process power remained disconnected.
- Wi-Fi/AP/HTTP, TinyBee PCM-short timing, actual pulse widths, calibration,
  following behavior, safety response, and motion remain physically
  unqualified. The TinyBee package remains non-armable until its disconnected-
  load SLogic and subsequent HIL gates pass.
