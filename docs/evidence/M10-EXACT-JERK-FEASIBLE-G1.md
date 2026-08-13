# M10 exact jerk-feasible lossless-line G1 motion — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint; positive nonzero-boundary motion
across lossless exact line-to-line G1 continuations, not curvature-bearing
continuity, retained blends, globally time-optimal S-curves, N-axis projection,
or physical motion qualification.

## Result and source identity

Hyperpath commit `513a13fb63c38de6bfcee18577358771001bc16c`
couples exact acceleration lookahead to bounded component-local jerk
feasibility. Alumina Interface commit
`b282018fbc810ba6fef885da9b6f8bbc580fac24` enables positive caller ceilings
only for lossless source-line pairs and lowers a positive G1 fixture through
the production stepper preflight. The coordinated firmware, cache, executor,
and safety baseline before this evidence record is `aluminafw` commit
`4adbad85299491eca7711de254dd12429bfcf00c`.

The final native/WASM gates and optimized browser build completed against
Hypercurve HEAD `d85eec9aa6bcde54ebbfd5ac08a3ac72d2f244e9` while its
`src/bezier_offset.rs` contained concurrent tracked edits. The tracked binary
diff observed after the gates and a final successful interface-core check had
SHA-256
`491ddc3ad6cd04e92a4a22ca4c21b7e1617193522e947b1169d1ecbcb96303ce`.
Hypercurve passed through coherent and temporarily non-compiling intermediate
states during this work. No Alumina or Hyperpath change chased, reset, pinned,
or modified that tree; this is the observed development snapshot which
completed qualification, and continued Hypercurve editing is expected.

No published CSGRS package substituted for the sibling workspace. The source
audit accepted only local Alumina/CSGRS/Hyper packages and the existing
MIT/Apache-compatible dependency inventory. This increment did not inspect,
copy, translate, link, or depend on Synthetos/g2, SimpleFOC, FluidNC, or other
GPL-family source. No GPL-family dependency was introduced.

## Exact component-local refinement

Hyperpath first retains the complete
`PlannedLookaheadFeedSchedule`: effective caller/global/geometric node limits,
the exact squared-speed forward trace, the reverse-pass result, and independent
caller/corner/span replay. It then partitions the selected node vector into
maximal structurally positive components separated by exact zero nodes.

For one component, every touching span is proposed through the exact two-phase
monotonic transition. If any proposal fails only its dynamic replay, every node
in that component is divided by two exactly and the complete component is tried
again. The caller supplies a maximum refinement count; exhaustion returns
`JerkRefinementBudgetExceeded` rather than an uncertified schedule.

This construction has useful conservative invariants:

- no zero node can become positive;
- components on opposite sides of an exact stop are independent;
- every feed is less than or equal to the acceleration-only proposal;
- caller, global, corner, reversal, and acceleration reachability remain valid
  under lowering;
- relative feeds within a component are preserved by uniform scaling; and
- each retained nonzero span owns its fully replayed monotonic transition.

The final `PlannedJerkFeasibleLookaheadSchedule` retains the original
acceleration plan, component ranges and halving counts, final node schedule,
new caller and lookahead replay, and one optional certified transition per
span. `all_satisfied()` requires both planning layers and every transition.

This is an exact bounded search, not a floating cubic/root proposal or a claim
of time optimality. Regression coverage demonstrates a G1 component refined
from feed `8` to feed `1` in three exact halvings, a constant-positive component
requiring none, two stop-separated components requiring different refinement
counts, exact corner-stop preservation, refinement-budget failure, and
generated single-span policies.

## Alumina eligibility and phase policy

Alumina supplies zero entry/exit ceilings and zero retained radius everywhere.
An internal join receives a positive caller ceiling only if:

1. both adjacent metric carriers are lines;
2. both map to source spans with zero source-to-motion approximation error; and
3. Hyperpath's exact tangent classifier independently selects G1 continuity.

The source-span test keeps every certified cubic chord boundary stopped even if
two adjacent chords happen to be collinear. Restricting both carriers to lines
also keeps line/arc and arc/arc G1 joins stopped. This is necessary because G1
tangent continuity alone does not certify continuous normal acceleration or
bounded vector jerk across a curvature discontinuity.

For a selected zero/zero span, Alumina retains its four-phase symmetric
rest-to-rest construction. For a selected nonzero span, it consumes the exact
two-phase transition already retained and replayed by Hyperpath. The public
type is now `CertifiedJerkSchedule2`; no compatibility alias or legacy schedule
entry point was added.

Interface-core regression coverage proves:

- two connected lossless one-millimetre source lines retain positive feed at
  their exact G1 join, use two phases per element, and lower through production
  `StepperExecutor` preflight to the exact terminal coordinate;
- an exact reversal remains a zero-feed join with four phases per element;
- an exactly tangent line-to-arc G1 join remains stopped because curvature
  continuity is not certified; and
- every generated cubic chord boundary remains zero with four-phase
  rest-to-rest replay.

The default Machine/CAM fixture contains no eligible lossless line-to-line G1
join. It therefore continues to show 35 zero nodes, 33 stop joins, 34 spans,
four phases per span, 2,453 samples, 2,452 firmware segments, 246 cache blocks,
32,800 rising edges, and terminal position `[19200, 0]`. Its visible UI now
states the actual eligibility policy and reports zero positive components,
rather than describing positive nodes as globally disabled.

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
CARGO_NET_OFFLINE=true trunk build --release --locked --public-url /
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br

# unchanged aluminafw portable baseline
cargo test --locked --offline
```

Observed results:

- Hyperpath passed 2 unit, 436 integration/property, and 2 README tests;
- Alumina Interface passed 28 application, 37 protocol-client, 115 exact-core,
  1 integration, and 1 compile-fail test;
- native and WASM strict Clippy, strict rustdoc, all WASM test-target links, and
  the local-source/permissive-license audit passed;
- the optimized 5,384,033-byte WASM validated; its 2,424,991-byte gzip and
  1,942,999-byte Brotli forms passed integrity checks, and the uncompressed
  artifact has SHA-256
  `8785de3c63b5f2059fe942e44125a0f3f4bcc79b475dd04a5f48189871d9a798`;
- headless Chromium loaded the app and dedicated worker from `127.0.0.1`, then
  visibly rendered the default exact Machine/CAM fixture, 35 nodes, 33 joins,
  34 spans, zero eligible positive components, and the active lossless-line
  G1-only policy; and
- no WLAN association, serial contact, reset, flash, analyzer capture, GPIO
  operation, or physical board operation occurred.

## Open boundary

This does not permit motion across a retained true corner or merely G1
curvature-bearing join. Still open are native/retained blend construction,
curvature and normal-acceleration continuity, vector-jerk limits, direction and
N-axis constraint projection, time-optimal positive-node selection, more
general internal-peak profiles, hold/resume replanning, and simulator/HIL
correlation. The next planner increment should add an exact retained geometric
carrier whose curvature and axis-projected dynamics can justify positive feed,
not weaken the current source-line eligibility gate.
