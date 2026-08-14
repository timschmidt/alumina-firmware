# M10 canonical planner/lowering evidence V3 — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint. The authoritative browser compiler
now reconstructs and commits the exact planning and lowering decisions which
produce one canonical cached machine stream. This is audit/replay evidence for
local exact CAM; it is not a firmware compatibility format, shared multi-MCU
retiming, direct jerk-IR execution, hardware timing, or physical motion
qualification.

## Result and source identity

Alumina Interface commit
`062675c66fafd05a9441f1bf5bcab6fc55d3f2fe` removes the V2 evidence type and
implements canonical outer format `ALMEVD03` with two new domain-separated
subtranscripts:

- `ALMPLN01` for exact caller policy, planner state, and certification; and
- `ALMLOW01` for resolution/lowering policy, exact timer selection, canonical
  points/segments, and executor replay.

There is no V2 parser, adapter, fallback, or compatibility alias. All callers
are controlled and update with the new schedule-aware construction/replay API.
The firmware repository parent before this evidence record is
`dc848a1fba13820404551ac3fc0ceeaf5126bb17`; firmware machine IR, stepper
preflight, cache, and simulator code were not weakened or duplicated.

The final coherent gate batch used:

- Hypercurve HEAD `de9628dd962a8dcbbe20a527f743a1d2abcff225` with concurrent
  tracked edits in `src/bezier_offset.rs`, `src/bezier_region.rs`, and
  `src/curve_region_boolean.rs`; binary diff SHA-256
  `14da56a6db9295aba186b9cb585d662241571971f7be08fde90811e5067ba71c`;
- clean Hypersolve HEAD `6ce08b714cdba1e3668e1af6c83f0a249bda9bb5`;
- clean Hyperpath HEAD `d792aa8dc843218b26fc0d1730033e5cd06bdf2f`;
  and
- clean CSGRS HEAD `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`.

Hypercurve is intentionally a moving read-only dependency. Pre/post snapshots
around the final native tests, both strict-Clippy targets, rustdoc, WASM link,
source audit, optimized build, compression checks, and render retained the
same HEAD, file set, and diff digest above. Alumina did not edit, format, reset,
or pin that tree. This is a coherent tested development snapshot, not a release
pin.

No published CSGRS package substituted for the sibling checkout. The native
and WASM dependency inventories contain no GPL-family package. This increment
did not inspect, translate, copy, link, or depend on Synthetos/g2, SimpleFOC,
FluidNC, or other GPL-family implementation source.

## Outer evidence contract

The 476-byte default `ALMEVD03` record binds:

- configuration and capability identities;
- independent exact-source, metric-path, and source-approximation digests;
- planner and lowering subtranscript SHA-256 identities and canonical byte
  lengths;
- immutable partition object and manifest identities and byte length;
- timer rate, output quantum, block/point/segment counts, initial/final steps,
  terminal tick/finish cycle, emitted-step counts; and
- exact total/source/controller/source-to-motion/interpolation allocations.

Import never trusts decoded fields. It first verifies the expected outer
SHA-256, reconstructs all domains from the live schedule, lowered program, and
partition, and then requires byte-for-byte outer equality. Unknown versions,
truncation, reordering, substitution, or a policy change therefore reject
without creating a second parser or source of truth.

## Exact planner subtranscript

`ALMPLN01` binds the decisions which final stream identity alone cannot prove:

- configuration/capability identities, strict predicate mode and refinement
  floor;
- caller source-reduction element/depth limits and maximum jerk-component
  halvings;
- complete exact tangent spans and native-extrema travel envelope;
- scalar feed/acceleration/jerk limits and, when applicable, every affine
  span/axis projection row, exact bottleneck, signed residual, status, and
  proof source;
- caller entry/join/exit ceilings and retained radii;
- acceleration-only effective nodes, forward nodes, reverse result, caller
  ceiling replays, join classifications, and span reachability replays;
- stop-separated positive components, uniform halving counts, final nodes,
  monotonic transition proposals and construction proofs;
- every selected constant-jerk phase and complete element length/continuity/
  feed/acceleration/jerk certification row; and
- exact total retained length and traversal time.

The builder rejects unsatisfied planner reports, inconsistent route/tangent/
phase counts, an unsatisfied affine projection, or disagreement between the
schedule and lowered program.

## Exact lowering subtranscript

`ALMLOW01` binds:

- configuration/capability identities, device timer, and backend output
  quantum;
- the complete exact resolution budget, including endpoint, DDA tracking,
  command-lattice, calibration, following, output-grid, and required totals;
- source reduction, controller interpolation, quantization, and resulting
  curve-to-canonical evidence;
- caller point limit and exact timer-factor lattice/ceiling;
- selected raw and reduced factor, complete candidate replay count,
  factor-one/immediate-predecessor failures, and exact ideal/scheduled/delay/
  extension/grid-padding bounds;
- every point's source/motion/phase/subdivision provenance, exact metric
  coordinate, exact ideal time, canonical step coordinates, and tick;
- every canonical execution segment; and
- the complete allocation-free production executor preflight.

The builder requires point/segment cardinality, zero initial tick, terminal
point/preflight tick, segment count, timer policy/report agreement, exact
schedule/program total-time agreement, and all partition terminal facts.

## Structural exactness and bounds

Planner and lowering `Real` values use Hyperreal's compact exact structural
serde representation. Rational scale, symbolic class, and computable expression
structure are retained; transient approximation caches, inferred atomic facts,
and abort signals are excluded by Hyperreal's serde contract. No primitive
float or display string participates.

Each encoded `Real` is written through a fallible 1 MiB bounded sink. Each
incrementally hashed planner or lowering transcript is capped at 64 MiB. Count,
allocation, serialization, field, or aggregate overflow is a typed failure.
Any change to the structural schema requires a transcript-version change.

The representative fixture visibly reports:

- planner digest prefix `2b36edecab8230bc` and 813,033 canonical bytes;
- lowering digest prefix `0b42642140e23938` and 1,277,845 canonical bytes; and
- a 476-byte `ALMEVD03` outer record.

## Decision-boundary regressions

The exact-core regression rebuilds identical evidence twice and replays it,
then proves all of the following:

- forcing representative symbolic values through 128-bit dyadic refinement
  does not change one evidence byte, because caches are not semantic state;
- changing only the caller's source-reduction element/depth bounds changes the
  planner subtranscript and outer identity while leaving lowering identity
  unchanged;
- changing only the timer-factor lattice changes the lowering subtranscript
  and outer identity while leaving planner identity, points, segments, and
  packaged partition bytes unchanged; and
- corrupting externally supplied outer bytes rejects at SHA-256 verification.

These tests establish the intended boundary: a byte-identical final stream is
not permission to forget which exact policy and certification path selected it.

## Verification record

The following completed offline at the interface revision and coherent sibling
snapshot above:

```sh
# aluminafw portable baseline and coordinated evidence
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
git diff --check

# alumina-interface
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run \
  --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -dc dist/alumina-interface_bg.wasm.gz | sha256sum
brotli -d -c dist/alumina-interface_bg.wasm.br | sha256sum
```

Observed results:

- the complete default-member firmware workspace passed its portable tests and
  strict all-target Clippy; firmware target crates were not host-built or
  physically exercised by this documentation checkpoint;
- 28 application, 37 protocol-client, and 117 exact-core tests passed, plus the
  exact-control integration and compile-fail value-boundary tests;
- native/WASM warnings-denied Clippy, warnings-denied rustdoc, every WASM test
  target link, and the local-source/permissive-license audit passed;
- the optimized 5,447,452-byte WASM validated; its 2,443,730-byte gzip and
  1,955,126-byte Brotli forms both expand to SHA-256
  `58729ee1661c226fc8b15239a72e5a6b128631bbbce5e4f6f3688a1b429034ff`;
- headless Chromium loaded the final application and dedicated worker over
  `127.0.0.1` with software WebGL and visibly rendered the complete exact
  Machine/CAM, lowering, cache replay, and `ALMEVD03` identities; and
- no WLAN association, serial/USB contact, reset, flash, analyzer capture,
  GPIO operation, motor/power operation, or other physical board action
  occurred.

## Claim boundary and next work

`ALMEVD03` is browser/compiler audit evidence. Firmware core 1 still consumes
only independently admitted bounded machine IR and never parses Hyperreal,
planner transcripts, JSON, source geometry, or evidence files. No physical
armability or safety claim follows from this checkpoint.

Shared multi-MCU retiming, one deterministic cached-job kickoff policy, direct
native jerk/finite-difference IR, curvature-aware/nonlinear kinematics, vector
acceleration/jerk, retained blends, time-optimal profiles, hold/resume
replanning, physical simulator/HIL correlation, and hardware timing remain
open. The next aligned software boundary is one shared exact retiming decision
across all participant partitions before each MCU's immutable cache publication.
