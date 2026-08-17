# M10 exact two-pass lookahead planning — offline evidence

Date: 2026-08-13

Status: implemented development checkpoint; exact acceleration-reachability
planning and independent replay, not retained blend construction, arbitrary
boundary-feed jerk synthesis, physical motion qualification, or completion of
the M5/M6/M10 exit gates.

## Result and source identity

Hyperpath commit `b8b4503d92cabcc5c6917969cf293aeac0035ae4`
adds the exact forward/reverse speed-node proposer. Alumina Interface commit
`b3eb40b122b0d4961f2a6f17f7fb97ae1f69c71e` routes the existing conservative
machine schedule through that proposer. The coordinated firmware, cache,
executor, and safety baseline before this evidence record is `alumina-firmware`
commit `e1fc13f2c925d62203164acaa0a1827908911135`.

The final native/WASM gates and optimized browser build completed against
Hypercurve HEAD `08fb7fef66720b123d32cf94d3e0528eea1c83fd` while that worktree
contained concurrent tracked edits. Immediately after qualification, the
tracked binary diff over `src/bezier_offset.rs`, `src/bezier_region.rs`,
`src/curve.rs`, and `src/curve_region_boolean.rs` had SHA-256
`7795fb1083b66d04053d981f3c1725447a320bc58d79a7f1ecf1880cd0fc93b8`.
That is an observed development state, not a request to stop, reset, or pin
Hypercurve. The earlier certified-cubic artifact retains its separate snapshot
digest in its own evidence record.

No published CSGRS package substituted for the sibling workspace. The source
audit accepted only local Alumina/CSGRS/Hyper packages and the existing
MIT/Apache-compatible dependency inventory. This increment did not inspect,
copy, translate, link, or depend on Synthetos/g2 source. It implements the
required behavior from the public squared-speed path-parameterization relation
already cited in Hyperpath's module documentation. No GPL-family dependency was
introduced.

## Exact proposal contract

`LookaheadFeedPlanningLimits` separates caller policy from the selected speed
nodes:

- maximum entry and exit feed ceilings;
- one caller-owned feed ceiling at every retained join; and
- one retained geometric blend radius at every retained join.

The values are ceilings, not requested speeds. A caller can require a stop at
any node by supplying zero. Before allocation or proposal, Hyperpath rejects an
empty route, mismatched route/tangent/limit shapes, negative ceilings or radii,
nonpositive global feed/acceleration, unsupported retained length, unresolved
exact comparison, and unsupported tangent geometry.

For every join, exact Hyperpath tangent predicates select a geometric limit:

| Exact join class | Geometric node ceiling |
| --- | --- |
| G1 continuous | global maximum feed |
| true corner with retained radius `r` | `sqrt(a_max * r)` |
| reversed tangent | exactly zero |
| endpoint mismatch, degenerate, or undecided | reject |

The effective node limit is the exact minimum of caller, global, and geometric
ceilings. Hyperlimit performs every order decision under the caller-selected
predicate policy; an undecided comparison fails instead of becoming a sampled
or floating choice.

Given exact retained element lengths `L`, the forward pass applies

```text
v[i + 1] = min(limit[i + 1], sqrt(v[i]^2 + 2*a_max*L[i]))
```

and the reverse pass applies the symmetric deceleration bound

```text
v[i] = min(v[i], sqrt(v[i + 1]^2 + 2*a_max*L[i])).
```

Every value remains a Hyperreal `Real`; diagonal chord lengths and resulting
radicals remain symbolic. The returned `PlannedLookaheadFeedSchedule` retains
the effective node limits, the complete forward trace, and the final reverse
schedule so the UI and later evidence formats do not have to infer how a node
was lowered.

## Independent replay

The proposer does not certify itself by construction alone. It builds separate
Hypersolve problems which replay:

- every final node against its caller-owned ceiling;
- every entry, join, and exit node against the global feed ceiling;
- every true corner against `v^2 <= a_max*r`;
- every reversal against `v = 0`; and
- both acceleration and deceleration squared-speed distance inequalities over
  every retained span.

The existing `certify_lookahead_feed_schedule` path performs the geometric,
global, and span replay independently of the two passes. A second per-node
replay covers caller ceilings. Any violated or undecided row rejects the
proposal with a typed internal-certification failure instead of returning a
partially trusted schedule.

Regression coverage includes a three-span route whose long middle segment lets
the forward pass propose `sqrt(202)` at the second join before the reverse pass
lowers it to `sqrt(2)`, exact G1 movement with zero radius, a caller-forced G1
stop, a mandatory reversal stop, invalid shapes and negative limits, and
generated single-span policies whose caller/global/bidirectional constraints
are independently replayed.

## Alumina integration and unchanged motion policy

`CertifiedExactStopSchedule2` now owns the complete planned lookahead result
instead of constructing a `LookaheadFeedSchedule` by hand. Its current policy
supplies zero entry, join, and exit ceilings and zero retained radii. Therefore:

- all 35 fixture nodes remain exactly zero;
- all 33 joins, including every certified cubic-chord boundary, remain full
  stops;
- the existing 34 rest-to-rest, four-phase jerk schedules remain valid;
- the 2,453 samples, 2,452 firmware segments, 246 cache blocks, 32,800 rising
  edges, and terminal position `[19200, 0]` remain unchanged; and
- no firmware schema, protocol, cache, executor, board, or arming path changes.

The browser inspector now reports the exact two-pass node/join/span counts and
requires both caller and reachability replay before presenting the schedule as
certified. The current zero policy deliberately prevents positive lookahead
nodes from reaching the rest-to-rest jerk phase generator. Positive nodes will
remain disabled until Alumina can construct the corresponding retained blend
geometry and certify jerk profiles with arbitrary nonzero boundary feeds.

A positive radius is never permission to round an unblended source corner. It
is a limit supplied only for geometry which already contains that retained
blend. The proposer schedules a given metric path; it does not invent blend
geometry.

## Verification record

The following completed offline:

```sh
# hyperpath
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline

# unchanged alumina-firmware portable baseline
cargo test --locked --offline

# alumina-interface
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown \
  --no-run --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env NO_COLOR=false CARGO_NET_OFFLINE=true \
  trunk build --release --locked --public-url /
wasm-tools validate dist/alumina-interface_bg.wasm
```

Observed results:

- Hyperpath passed 2 unit, 426 integration/property, and 2 README tests;
- the unchanged `alumina-firmware` default-member portable and doc-test suite passed;
- Alumina Interface passed 28 application, 37 protocol-client, 112 exact-core,
  1 cross-crate integration, and 1 compile-fail Rustdoc test;
- strict Hyperpath and interface native/WASM Clippy, strict Rustdoc, formatting,
  complete WASM test-target linking, and the local-source/license audit passed;
- the optimized WASM validated at 5,362,231 bytes with SHA-256
  `026a0323696315b528844b06bc8baafa5fefd0f4c52add4025c784d9a6f7c491`;
  its 2,415,458-byte gzip and 1,936,847-byte Brotli forms passed integrity
  checks; and
- loopback-only Chromium with software WebGL fetched the document, generated
  JavaScript, WASM, worker, and favicon, then visibly rendered the non-armable
  Machine/CAM inspector and certified two-pass status for 35 nodes, 33 joins,
  and 34 spans.

The loopback server was stopped after the run. No WLAN association,
NetworkManager mutation, TinyBee AP request, serial/USB operation, board reset,
flash, output, motor, driver, or process-power action occurred.

## Closed claims and next boundary

- This is exact acceleration-reachability node planning, not a proof of global
  time optimality under jerk, axis-direction, kinematic, process, or following
  constraints.
- The current phase generator is still rest-to-rest at every metric element.
  It does not accept arbitrary nonzero boundary feeds.
- Retained nonzero-radius blend construction, curvature/jerk-aware node limits,
  N-axis constraint projection, feed hold/resume replanning, and fixed boundary
  state across independently compiled windows remain open.
- Firmware still validates and executes bounded canonical integer segments; it
  does not run this exact planner or receive Hyperreal objects.
- The cached stream, safety kernel, and disconnected-load HIL gates remain as
  previously recorded. This checkpoint makes no physical timing, calibration,
  motor, safety-response, or armability claim.

The next safe software boundary is a retained blend object plus an exact jerk
profile accepting nonzero endpoint feeds. Only after those independently replay
can Alumina enable positive caller ceilings and measure the resulting canonical
stream in simulation and disconnected-load HIL.
