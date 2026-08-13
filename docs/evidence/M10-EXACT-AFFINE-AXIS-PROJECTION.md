# M10 exact affine axis motion projection — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint; exact velocity, acceleration, and
jerk projection for arbitrary dense axes on affine path spans, integrated for
lossless Cartesian line motion. This is not nonlinear-kinematics, curved-path,
timer-lattice, or physical motion qualification.

## Result and source identity

Hyperpath commit `d792aa8dc843218b26fc0d1730033e5cd06bdf2f`
adds the exact affine-axis projection and independent Hypersolve replay.
Alumina Interface commit `5ed94f5d5f65087c05afa018be349d5506f98d80`
derives exact Cartesian line directions from retained Hyperpath geometry,
projects canonical Configuration V5 limits, displays the retained report, and
lowers a diagonal positive-G1 regression through the production stepper
preflight. The coordinated firmware, cache, executor, and safety baseline
before this evidence record is `aluminafw` commit
`1a112288eb82e9fb65660a774e78f7a158e7207b`.

The final Hyperpath and Interface gates completed against Hypercurve HEAD
`d85eec9aa6bcde54ebbfd5ac08a3ac72d2f244e9` while its
`src/bezier_offset.rs` contained concurrent tracked edits. The final coherent
tracked diff observed after those gates and a successful interface-core check
had SHA-256
`c1583f1c371c28ab32f30435f4bdcd07d74e42bcd2c0d6f60bb3c1f991e4b08e`.
No Alumina or Hyperpath change chased, reset, formatted, pinned, or modified
that tree. This record identifies the tested development snapshot; continued
Hypercurve editing is expected.

No published CSGRS package substituted for the sibling workspace. The source
audit accepted only local Alumina/CSGRS/Hyper packages and the existing
MIT/Apache-compatible dependency inventory. This increment did not inspect,
copy, translate, link, or depend on Synthetos/g2, SimpleFOC, FluidNC, or other
GPL-family source. No GPL-family dependency was introduced.

## Exact affine projection

For an affine path span, axis coordinate `q_i` and scalar path coordinate `s`
obey

```text
q_i(s) = q_i(0) + d_i s
c_i = |d_i|
```

The caller supplies exact nonnegative `c_i` and per-axis limits `V_i`, `A_i`,
and `J_i`. Hyperpath certifies the scalar limits against every dense axis:

```text
c_i v <= V_i
c_i a <= A_i
c_i j <= J_i
```

Every positive `c_i` contributes the exact candidates `V_i/c_i`, `A_i/c_i`,
and `J_i/c_i`. A zero derivative contributes no restriction. Hyperpath chooses
the exact route-wide minimum independently for velocity, acceleration, and
jerk, retaining the first bottleneck deterministically on an exact tie. It
retains one row for every span/axis pair and uses independent Hypersolve
problems to replay all three inequalities plus exact equality at each selected
bottleneck.

The API accepts any nonempty dense axis count. It rejects empty input, shape
mismatch, negative purported absolute derivatives, a span stationary in every
axis, unresolved exact comparisons, an unsafe proposal, or a failed replay.
This formula is deliberately restricted to affine spans. Curved carriers and
nonlinear kinematics require the relevant higher derivative terms and cannot
reuse this certificate.

## Browser compiler integration

For an all-line two-dimensional metric route, Alumina Interface derives each
exact unit-direction row directly from the retained line:

```text
(|dx| / sqrt(dx^2 + dy^2), |dy| / sqrt(dx^2 + dy^2))
```

It derives exact per-axis velocity, acceleration, and jerk limits from the
active canonical Configuration V5, invokes Hyperpath, retains the complete
`PlannedAxisProjectedMotionLimits` in `ScalarMotionLimits2`, and uses the
selected scalar limits for subsequent lookahead and interpolation. Because the
line direction is constant and unit length, the selected scalar acceleration
is also the complete spatial acceleration bound for that span.

If any route carrier is a curve or arc, the interface retains no affine
projection. It instead keeps the prior conservative direction-independent
per-axis minimum together with the existing centripetal, mixed-jerk, and
curvature-jerk policies. The UI states which policy is active and, for an
all-line route, can show every span/axis derivative, three limits, and replay
result. This keeps a line-only proof structurally unavailable to curvature.

The exact integration regression uses two connected 3-4-5 direction lines.
Both spans project to `(3/5, 4/5)`. With equal per-axis configured limits, the
Y axis is the deterministic bottleneck and velocity, acceleration, and jerk
all receive the exact scalar factor `5/4`. Four span/axis rows and all retained
bottlenecks replay successfully. The G1 boundary remains moving, the jerk
schedule certifies, production V1 lowering and `StepperExecutor` preflight
pass, and the exact terminal step coordinate is `[9600, 12800]`.

## Timer/output-quantum boundary retained closed

An earlier form of the diagonal fixture used the default velocity profile. Its
continuous projected axis speed landed exactly on the step-derived electrical
pulse ceiling. Rounding onto the firmware timer/event lattice then caused the
production preflight to reject `PulseBoundary { axis: 1 }`. The projection was
not weakened and preflight was not bypassed. The retained passing fixture uses
an explicit exact 1 mm/s configured velocity with zero uncertainty, below the
electrical ceiling.

This exposes a real next boundary: continuous velocity feasibility does not
prove that its supremum is attainable after discrete event-time rounding,
pulse width, direction setup/hold, and shared-output constraints. The planner
must derive exact attainable bounds from the selected output lattice or reject
the requested policy; it must not insert an unexplained floating-point safety
factor. Until then, production preflight remains the fail-closed authority.

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
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked --offline
bash scripts/audit-source-policy.sh
CARGO_NET_OFFLINE=true NO_COLOR=true trunk build --release --locked --public-url /
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br

# unchanged aluminafw portable baseline plus this evidence record
cargo test --locked --offline
cargo fmt --all -- --check
git diff --check
```

Observed results:

- Hyperpath passed 2 unit, 439 integration/property, and 2 README tests;
- Alumina Interface passed 28 application, 37 protocol-client, 116 exact-core,
  1 integration, and 1 compile-fail test;
- native and WASM strict Clippy, strict rustdoc, all WASM test-target links, and
  the local-source/permissive-license audit passed;
- the optimized 5,394,585-byte WASM validated; its 2,430,158-byte gzip and
  1,947,317-byte Brotli forms passed integrity checks, and the uncompressed
  artifact has SHA-256
  `cf10d64c2aee735b5c1070a847c28409dded95f613d4a0248657188fa638c91d`;
- headless Chromium loaded the app and worker from `127.0.0.1` under software
  WebGL, then visibly rendered the default Machine/CAM fixture and its explicit
  conservative curved-route fallback; and
- no WLAN association, serial contact, reset, flash, analyzer capture, GPIO
  operation, or physical board operation occurred.

## Open boundary

The immediate next increment is exact timer/output-quantum-aware headroom which
can prove an attainable scheduled rate before V1 lowering. Curvature-aware
axis projection, nonlinear kinematics, vector acceleration and jerk across
curved joins, retained blends, time-optimal profiles, hold/resume replanning,
and physical simulator/HIL correlation also remain open. `ALMEVD02` binds the
canonical output plus source, metric, and approximation identities; a future
greenfield evidence version must additionally bind detailed planner-policy and
certification transcripts rather than implying that the current envelope does.
