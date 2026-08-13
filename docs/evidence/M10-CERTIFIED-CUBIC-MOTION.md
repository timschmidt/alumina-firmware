# M10 certified cubic motion and evidence V2 — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint; not a reproducible release pin,
physical motion qualification, nonzero-feed curve interpolation, or completion
of the M5/M10 exit gates.

## Result and source identity

`alumina-interface` commit
`8bff44aee840741a0c00b068534a051d2c34c7cd` extends the authoritative
browser/WASM Machine/CAM path from lossless lines/arcs to retained polynomial
cubic Beziers. The coordinated firmware-schema, executor, cache, and safety
baseline before this evidence record is `aluminafw` commit
`9454cd917138955340c8746f53ac71a137032cb5`.

Hyperpath commit `c65e0136514637305e99e3eadd5432ef5e234e68` supplies exact
Euclidean feed length for non-axis-aligned line carriers while preserving the
strict axis-only ordering APIs. The optimized bundle and its immediately
captured qualification snapshot observed Hypercurve at HEAD
`08fb7fef66720b123d32cf94d3e0528eea1c83fd` with concurrent tracked edits in
`src/bezier_offset.rs`, `src/bezier_region.rs`, and `src/curve.rs`; the tracked
binary-diff SHA-256 was
`cd562aeada7607c31b290db51bc81025fd056cbe56d75bad443283c7328941d8`.
After that artifact was frozen, the same live HEAD/files were observed at diff
SHA-256 `021dcdce11891e74b967547b2ec668cd04d783ac9ffc4eaa1766062631b27635`
while concurrent Hypercurve editing continued; that later state is not claimed
as the optimized artifact input. These are observed development facts, not a
request to hold or reset that worktree and not a release pin.

No published CSGRS release substitutes for the current sibling workspace. The
source-policy audit continued to accept only the local Alumina/CSGRS/Hyper
stack and MIT/Apache-compatible dependency inventory; no GPL-family dependency
was introduced.

## Exact source-to-motion boundary

`CertifiedMetricPath2` keeps the native `CurvePath2` source distinct from the
line/arc path passed to Hyperpath. Every source span records its source index
and family, contiguous motion range, exact rational certified error bound, and
deepest subdivision used.

- Lines and explicit circular arcs are copied losslessly and report zero
  source-to-motion error.
- A polynomial `CubicBezier2` requires a positive caller-owned source-curve
  allocation.
- The endpoint chord is degree-elevated to cubic degree. Subtracting its
  controls from the source controls produces the exact cubic for
  `source(t) - chord(t)`.
- If both interior difference-control squared norms are no greater than the
  exact allocation squared, convexity and the triangle inequality certify the
  Euclidean pointwise bound for every shared local parameter.
- Otherwise the compiler uses Hypercurve's exact de Casteljau half-split and
  repeats the proof. Generated points remain exact `Real` values and become
  native `LineSeg2` objects; no Hypergraphics/display/GPU chord participates.

This motion predicate is deliberately stronger than perpendicular distance to
the chord's supporting line. A regression constructs a collinear cubic which
travels below its start and beyond its end; it proves that the reverse and
overshoot excursions cannot collapse into one misleading endpoint chord.

`MetricPathApproximationLimits2::INTERACTIVE` admits at most 16,384 complete
motion elements and depth 20. Effective depth is further capped so the
worst-case leaf count fits the remaining element budget. Checked counters,
fallible reservations, and all-or-nothing construction prevent partial or
unbounded output. Typed failures distinguish zero/negative allocation,
unsupported families, unresolved exact ordering, deterministic depth
exhaustion, element-budget exhaustion, integer overflow, and allocation
failure.

The recorded error is a conservative certified bound, not a claim that the
least or maximum-attained deviation was computed.

## Conservative scheduling and firmware boundary

Hyperpath/Hypersolve schedule the certified metric path, while the complete
native source path still supplies exact travel extrema. Every metric join,
including every generated cubic-chord boundary, is an exact zero-feed node.
Each metric element receives four independently replayed symmetric
constant-jerk phases. This intentionally slow first policy avoids carrying an
instantaneous chord-direction change at nonzero velocity.

Lowering evaluates the certified metric line/arc path, retains both source and
motion element provenance at each sample, and adds the certified
source-to-motion bound to controller interpolation, endpoint half-step, and
within-segment DDA tracking bounds. It then uses the existing canonical
constant-velocity firmware V1 segments, production `StepperExecutor` preflight,
immutable SD partitioner, and event-level cached executor replay. Firmware does
not receive a cubic evaluator or run exact CAD/CAM.

The default TinyBee fixture uses:

- three exact source curves: a line, radius-two semicircle, and cubic arch;
- a `1/100 mm` source-to-motion allocation and `1/1000 mm` selected controller
  interpolation bound inside the existing `1/10 mm` complete machine budget;
- 34 certified metric elements, 33 exact-stop joins, and four jerk phases per
  metric element;
- 2,453 exact metric samples and 2,452 canonical firmware segments;
- 246 cached blocks, 32,800 replayed rising step edges, and terminal position
  `[19200, 0]`; and
- exact source/motion provenance throughout the visible schedule inspector.

## Canonical evidence V2

Canonical schedule evidence advances from `ALMEVD01` to `ALMEVD02` and
domain-separates three independently hashed reconstruction transcripts:

1. `ALMSRC02`: exact-rational source lines, arcs, and all four cubic controls;
2. `ALMMTR01`: the exact line/arc metric path actually presented to Hyperpath;
3. `ALMAPX01`: source families, contiguous motion ranges, exact certified
   bounds, subdivision depths, counts, and source-to-motion provenance.

The outer transcript binds all three digests to configuration/capability,
partition object/manifest, timer/output lattice, executor terminal facts,
machine allocations, certified source-to-motion bound, and selected controller
interpolation. Replay rebuilds every transcript from the retained program and
partition and requires byte equality. Raw CNC text and comments remain separate
provenance and cannot become firmware authority.

## Visible browser result

The offline Machine/CAM inspector now:

- labels retained exact source and certified motion as distinct authorities;
- colors line, arc, and cubic source provenance separately;
- displays every source-to-motion span, family, motion range, element count,
  bound, and subdivision depth;
- identifies both source and motion element for a selected exact metric sample;
- reports the source-to-motion component in the lowering/error evidence; and
- exposes separate evidence, source, metric, and approximation digest prefixes.

The default workspace remains visibly non-armable, creates no device session,
and treats the plot as a one-way diagnostic projection only.

## Verification record

The following completed from `alumina-interface` after the final failure-contract
review:

```sh
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --offline -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
```

Observed results:

- 28 application, 37 protocol-client, and 112 exact-core tests passed, plus the
  cross-crate exact-control integration test and compile-fail value-boundary
  test;
- pathological cubic reduction, source/motion provenance, exact stops,
  deterministic cache packaging, and byte-identical `ALMEVD02` reconstruction
  passed in the exact core;
- all native tests passed, every workspace test target linked for
  `wasm32-unknown-unknown`, and strict native/WASM Clippy and strict Rustdoc
  passed;
- the local-source/permissive-license audit passed;
- optimized WASM validated at 5,356,204 bytes with SHA-256
  `a2ed14cab2473b175722a670970c3af4975f40842c9a08b6c04090364c710a7f`;
  its 2,413,446-byte gzip and 1,934,812-byte Brotli forms passed integrity
  checks and both decompressed to that exact digest; and
- loopback-only headless Chromium with DNS blocked outside `127.0.0.1` loaded
  the optimized app and worker and visibly rendered the three source curves,
  34-element certified motion path, non-armable state, and source/motion
  diagnostics.

The optimized dependency build observed one dead-code warning in a concurrently
edited private Hypercurve helper. Alumina neither changed nor suppressed that
external warning; strict Alumina Clippy and Rustdoc remained clean.

## Closed claims and remaining gates

- Polynomial cubic source reduction is implemented; quadratic/rational
  Beziers, PH/native curved feed, splines, NURBS, nonzero-radius blends, broader
  kinematics, and direction-aware limits remain fail-closed or future work.
- A full stop at every cubic chord is safe and certifiable but not performant.
  Native/tighter curve carriers and curvature-certified nonzero-feed motion are
  still required.
- The pointwise certificate is a positional bound. It is not tool/process
  clearance, collision, material-removal, or physical following evidence.
- `ALMEVD02` exact primitive serialization still requires rational parameters.
- The firmware still executes a certified constant-velocity approximation of
  the browser's jerk schedule; it does not contain an onboard jerk planner.
- This checkpoint contacted no WLAN interface, NetworkManager state, TinyBee
  AP, serial/USB endpoint, or physical board. No reset, flash, output, motor, or
  process power occurred. The connected bare TinyBee remains non-armable, and
  all SLogic, Wi-Fi/AP/HTTP, timing, calibration, safety, and motion HIL claims
  remain closed.
