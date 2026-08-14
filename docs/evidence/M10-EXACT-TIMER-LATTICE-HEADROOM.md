# M10 exact timer/output-lattice headroom — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint. The browser compiler now selects
the smallest factor on a caller-bounded exact rational lattice whose complete
canonical step stream passes the unchanged production firmware preflight. This
is local single-MCU schedule evidence, not shared multi-MCU retiming, direct
jerk-IR execution, hardware timing, or physical motion qualification.

## Result and source identity

Firmware commit `2251ec79362d20cc7cddd787dce8c09f17634b23`
adds the fail-closed `MotionError` timing-pressure classification used by the
compiler. Alumina Interface commit
`621bd2e8669ac7ffb0ed76c3de1f7118d6bc6d65` implements exact one-sided
output-grid lowering, bounded factor selection, retained replay evidence, and
the visible timer-lattice report. It builds on Hyperpath affine-projection
commit `d792aa8dc843218b26fc0d1730033e5cd06bdf2f`.

The final coherent gate set observed Hypercurve HEAD
`72bc0c7f0514b41144d941f2382dc458ffb828d8` with concurrent tracked edits in
`src/bezier_offset.rs`, `src/bezier_region.rs`, and `src/curve.rs`. The tracked
diff had SHA-256
`a729d67db6f1767d588e822fa23a87127cb7f27a18d3fb0d039b2fbf093b322e`
before and after native tests, strict checks, WASM linking, the production
rebuild, artifact validation, and the loopback render. Hypercurve subsequently
advanced to clean HEAD `f6508292039a0249ee63fbd0d6855ddb9ff9a0d1` after
qualification. That later, untested state was not substituted into this
checkpoint. No Alumina change reset, formatted, pinned, or otherwise modified
the Hypercurve tree; continued editing is expected.

No published CSGRS package substituted for the current sibling workspace. The
source audit accepted only local Alumina/CSGRS/Hyper crates and the existing
MIT/Apache-compatible dependency inventory. This increment did not inspect,
copy, translate, link, or depend on Synthetos/g2, SimpleFOC, FluidNC, or other
GPL-family source. No GPL-family dependency was introduced.

## Firmware-owned timing-pressure boundary

`alumina-motion::MotionError::is_time_dilation_candidate` returns true only for
duration-addressable electrical pressure:

- maximum step rate;
- pulse boundary and pulse-low time;
- direction setup and hold; and
- enable setup and hold.

Configuration, identity, state, segment topology/order, arithmetic, overflow,
output-grid, deadline, and output-invariant failures remain false. This method
does not waive a failure. It only permits the browser to construct another
exact schedule; every candidate still traverses the complete production
`preflight_stepper_segments` path.

## Exact output-grid construction

For retained ideal interval `I_i`, exact dilation factor `n/d`, device-cycle
frequency `F`, and output quantum `q` cycles, the compiler emits

```text
D_i(n) = q ceil(n I_i F / (d q))
```

Therefore

```text
D_i(n) / F >= (n/d) I_i >= I_i
0 <= D_i(n) / F - (n/d) I_i < q/F
```

Each interval is ceiled independently; it cannot become shorter than the
retained ideal interval and every duration is exactly divisible by the backend
quantum. Checked addition constructs cumulative `u64` ticks. Coordinates remain
the separately certified step-lattice coordinates, and scheduled point ticks
are assigned only after selection succeeds, so a failed search cannot expose a
partially retimed program.

The intentional `(n/d)` schedule dilation is not mislabeled as a spatial
approximation. `MachineResolutionBudget2` instead reserves one complete output
quantum at maximum vector velocity, conservatively bounding the strictly
smaller grid-only padding. The retained report separately exposes ideal total
time, canonical total time, maximum cumulative delay, maximum segment
extension, and maximum grid-only padding.

## Bounded smallest-factor proof

The interactive policy uses denominator 4,096 and an inclusive numerator
ceiling of 65,536, permitting exact factors from one through 16. The caller can
supply a tighter nonempty lattice. Selection proceeds as follows:

1. build and completely preflight factor one;
2. return any non-timing failure immediately;
3. if required, completely preflight the caller's maximum factor and fail
   closed if it still rejects;
4. binary-search the exact numerator interval; and
5. rebuild and preflight both the selected candidate and its immediate
   predecessor.

For fixed retained intervals, `D_i(n)` is monotone in `n`. Centered first-edge
offsets and terminal gaps, and the production rate, pulse-low, direction, and
enable timing inequalities, are consequently monotone in this factor. A
structural or arithmetic error is not treated as timing pressure and terminates
the search. If the immediate predecessor unexpectedly passes, selection fails
with `TimerDilationMinimalityUncertified` rather than claiming a smallest
factor.

`TimerLatticeScheduleReport2` retains the caller lattice and ceiling, selected
raw numerator/denominator and reduced rational factor, number of complete
candidate replays, factor-one rejection, predecessor rejection, and all exact
time-extension bounds. Production executor preflight remains authoritative;
there is no floating-point margin and no weakened electrical check.

## Exact electrical-ceiling regression

The two-line Cartesian 3-4-5 G1 fixture now uses the unmodified canonical
Configuration V5 profile. Both axes are asserted to be limited exactly by their
derived electrical step rates. Hyperpath still projects the exact scalar
velocity, acceleration, and jerk factor `5/4` and replays all four span/axis
rows.

At this exact continuous ceiling:

- factor `1` rejects with `PulseBoundary { axis: 1 }`;
- a policy capped at `1/1` fails with `TimerDilationBudgetExceeded`;
- the smallest admitted grid coordinate is exactly `4158/4096`, whose reduced
  rational value is `2079/2048`;
- the immediate predecessor `4157/4096` rejects with `Rate { axis: 1 }`;
- the bounded search performs 20 complete production-preflight replays; and
- the admitted program terminates at exact canonical steps `[9600, 12800]` and
  passes production preflight.

A separate output-quantum-four regression proves every emitted interval is a
multiple of four cycles, every actual interval is at least its retained ideal
interval, the maximum grid-only padding is strictly less than `4/F`, and the
last point tick equals the production preflight terminal tick. Invalid empty
factor lattices and ceilings below factor one reject.

## Browser report and current evidence boundary

The Machine/CAM inspector displays the exact selected factor, factor grid and
ceiling, complete replay count, factor-one/predecessor failures, cumulative
delay, per-segment extension, and output-grid padding. The default curved
fixture visibly reports factor `1`, the `1/4096` through `65536/4096` policy,
one complete replay, and no factor-one or predecessor rejection.

Canonical `ALMEVD02` still binds the resulting stream, terminal timing, source,
metric path, source approximation, machine identities, and error allocations.
It does not serialize the affine-projection rows, lookahead traces,
component-refinement/jerk transcripts, or timer-search decisions. A greenfield
`ALMEVD03` boundary will bind those detailed policy and certification
transcripts; no compatibility shim is required because all callers are under
project control.

## Verification record

The following completed offline:

```sh
# aluminafw timing classification and portable baseline
cargo test -p alumina-motion --locked --offline
cargo test --locked --offline
cargo fmt --all -- --check
git diff --check

# alumina-interface
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked --offline
bash scripts/audit-source-policy.sh
CARGO_NET_OFFLINE=true NO_COLOR=true trunk build --release --locked --public-url /
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Observed results:

- the targeted firmware crate passed 28 tests, including the timing-pressure
  classification regression, and the complete portable firmware workspace
  passed;
- Alumina Interface passed 28 application, 37 protocol-client, 117 exact-core,
  1 integration, and 1 compile-fail test;
- native and WASM strict Clippy, strict rustdoc, every WASM test-target link,
  and the local-source/permissive-license audit passed;
- the optimized 5,403,381-byte WASM validated; its 2,431,866-byte gzip and
  1,949,157-byte Brotli forms passed integrity checks, and the uncompressed
  artifact has SHA-256
  `144aaa48985cfc3f0c870b7592197b6d3012b26c22717481ee29e74137ec07db`;
- headless Chromium loaded the application and dedicated worker from
  `127.0.0.1` with software WebGL. A 60,000 ms virtual-time budget visibly
  reached the complete default Machine/CAM report and exact timer policy; and
- no WLAN association, serial contact, reset, flash, analyzer capture, GPIO
  operation, or physical board operation occurred.

## Open boundary

The subsequent
[`M10-CANONICAL-PLANNER-EVIDENCE-V3.md`](M10-CANONICAL-PLANNER-EVIDENCE-V3.md)
checkpoint closes the canonical planner/timer-policy transcript boundary without
rewriting this historical artifact. One shared exact retiming policy across
every participant partition, direct
native jerk/finite-difference IR, curvature-aware axis projection, nonlinear
kinematics, vector acceleration/jerk, retained blends, time-optimal profiles,
hold/resume replanning, physical simulator/HIL correlation, and hardware timing
qualification remain open. The next aligned software boundary is shared
multi-MCU retiming before immutable per-participant cache publication.
