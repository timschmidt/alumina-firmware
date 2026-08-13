# M10 exact monotonic boundary-feed jerk transitions — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint; exact conservative transitions for
one retained element with at least one positive boundary feed, not enabled
blending, general time-optimal S-curve planning, jerk-aware lookahead, physical
motion qualification, or completion of the M5/M6/M10 exit gates.

## Result and source identity

Hyperpath commit `1e484973e25d899cc72447527fdcfeebf134d7d8`
adds the exact monotonic transition proposer and two independent certification
layers. Alumina Interface commit
`4a84d2542aa77f8f5e4ce40d75e96604effdf62a` selects phases from the exact
lookahead boundary nodes while preserving its reachable all-zero policy. The
coordinated firmware, cache, executor, and safety baseline before this evidence
record is `aluminafw` commit
`ee93126035d408ae9aa4b7c5de299643ccbec474`.

The final native/WASM gates and optimized browser build completed against
Hypercurve HEAD `08fb7fef66720b123d32cf94d3e0528eea1c83fd` while that worktree
contained concurrent tracked edits. Immediately after qualification, the
tracked binary diff over `src/bezier_offset.rs`, `src/bezier_region.rs`,
`src/curve.rs`, and `src/curve_region_boolean.rs` had SHA-256
`3d390387e0c88c85efdad6ce52b8c45fffac1230c33b1c70950d2283ca8542d3`.
That is an observed development state, not a request to stop, reset, or pin
Hypercurve. Its continued editing is expected; later work records the snapshot
it actually tests instead of chasing a moving HEAD as a false prerequisite.

No published CSGRS package substituted for the sibling workspace. The source
audit accepted only local Alumina/CSGRS/Hyper packages and the existing
MIT/Apache-compatible dependency inventory. This increment did not inspect,
copy, translate, link, or depend on Synthetos/g2, SimpleFOC, FluidNC, or other
GPL-family source. The construction follows independently stated kinematic
requirements and public mathematical relations. No GPL-family dependency was
introduced.

## Exact construction

For one retained path element of exact length `L`, exact nonnegative boundary
feeds `v0` and `v1`, and `v0 + v1 > 0`, Hyperpath constructs two equal-time
constant-jerk phases. Define

```text
T  = L / (v0 + v1)
vm = (v0 + v1) / 2
a  = (v1 - v0) / T.
```

The first phase moves from `(v0, 0)` to `(vm, a)` with jerk `a/T`; the second
moves from `(vm, a)` to `(v1, 0)` with jerk `-a/T`. Their exact lengths are

```text
L0 = T * (5*v0 + v1) / 6
L1 = T * (v0 + 5*v1) / 6.
```

Therefore `L0 + L1 = T*(v0 + v1) = L`, both element-boundary accelerations are
zero, and the shared feed is exactly between the two requested feeds. The same
construction covers acceleration, deceleration, and equal positive feed. It
uses `Real` values throughout; no timer rounding, floating root, or numerical
proposal decides the result.

Both-zero input is intentionally rejected by this proposer because it has no
positive average feed and needs an internal peak. Alumina's separately
certified four-phase symmetric rest-to-rest proposer continues to own that
case. Negative feeds, degenerate lengths, unresolved exact comparisons, and
violated feed/acceleration/jerk limits fail with typed errors. A short span with
too much requested feed change is rejected; the construction never stretches
a limit or silently lowers caller-owned boundary feeds.

## Independent replay

`PlannedMonotonicJerkTransition` retains the proposed phases and two distinct
reports:

- construction replay checks the requested start/end feeds, zero endpoint
  accelerations, equal phase durations, exact length sum, and monotonic shared
  feed; and
- the generic multi-phase Hyperpath/Hypersolve replay checks per-phase
  kinematics, phase lengths, inter-phase continuity, element length, and every
  feed, acceleration, and jerk limit.

The public result is usable only when both reports are satisfied. Regression
coverage includes acceleration, deceleration, equal positive feed, both-zero
rejection, negative feed, degenerate geometry, a deliberately infeasible short
span, and generated boundary-feed cases replayed independently.

## Alumina integration and unchanged active policy

`CertifiedExactStopSchedule2` now reads each retained element's actual start
and end node from `PlannedLookaheadFeedSchedule`. A zero/zero pair follows the
existing four-phase rest-to-rest construction. If an internal future policy
supplies at least one positive boundary, the private phase selector delegates
to Hyperpath's two-phase monotonic proposer and requires its full replay.

No current UI, configuration, or job policy can supply a positive node ceiling.
The default fixture therefore still has 35 zero nodes, 33 full-stop joins, 34
metric spans, and four phases per span. Firmware schemas, cached command
streams, board arming, and physical-output authority do not change in this
increment. The browser makes that active policy explicit instead of allowing
the dormant branch to be mistaken for enabled blending.

The primitive does not invent retained blend geometry. A positive corner
radius remains valid only when the metric path already contains that blend. It
also does not make acceleration-only lookahead jerk-feasible: a future coupled
pass must lower proposed positive nodes when span length and jerk limits require
it, then replay the combined result.

## Verification record

The following completed offline:

```sh
# hyperpath
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline

# alumina-interface
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --no-deps --target wasm32-unknown-unknown \
  --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
CARGO_NET_OFFLINE=true trunk build --release --locked --public-url /
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br

# unchanged aluminafw portable baseline
cargo test --locked --offline
```

Observed results:

- Hyperpath passed 2 unit, 430 integration/property, and 2 README tests;
- Alumina Interface passed 28 application, 37 protocol-client, 114 exact-core,
  1 integration, and 1 compile-fail test;
- native and WASM strict Clippy, strict rustdoc, all WASM test-target links, and
  the local-source/permissive-license audit passed;
- the optimized 5,366,430-byte WASM validated; its 2,418,053-byte gzip and
  1,937,935-byte Brotli forms passed integrity checks, and the uncompressed
  artifact has SHA-256
  `28e93b5cbdb83ec712b754c93cb9fc01db08ceb54ac3a0d18ebfdfca4e5d69f8`;
- headless Chromium loaded the app and dedicated worker from `127.0.0.1`, then
  visibly rendered the exact Machine/CAM path, 35 nodes, 33 joins, 34 spans,
  and the active all-zero/four-phase policy; and
- no WLAN association, serial contact, reset, flash, analyzer capture, GPIO
  operation, or physical board operation occurred.

## Open boundary

This is a conservative exact building block, not a complete advanced motion
planner. Still open are retained blend construction, curvature and process
limits, jerk-aware forward/reverse feasibility, general internal-peak and
multi-axis profiles, N-axis projection, hold/resume replanning, timer-lattice
lowering of positive-node schedules, and simulator/HIL correlation. The next
planner increment should couple positive-node feasibility to exact retained
geometry and jerk/length constraints before any policy enables blending.
