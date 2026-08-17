# M5 / I1-I3 exact CAM compiler evidence

Date: 2026-08-11

Status: implemented development checkpoint; not an M5 exit-gate, reproducible
compiler release, browser-workflow qualification, or hardware qualification.

## Scope and source identity

The coordinated `alumina-interface` checkpoint is commit
`ed512177b200c3737857e1b77bbd67f471197539`. It consumes the real
`alumina-machine-ir` crate from this `alumina-firmware` tree at
`9d5fb750a0408c5f9bfb7151d809f43f4e67eecb` and has no copied UI-side machine
schema.

The rendering boundary is Hypergraphics
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`. Its two relevant changes are:

- `d0952a5f2623a4141bdf0dd89114fea93685a1fd`, which adds certified exact
  Hypercurve curve/path line adapters; and
- `31811aeb17bd2dc827db5669558f6251e0c2f2aa`, which adds certified curved-region
  boundaries while retaining authoritative material/hole roles.

The interface selects the current sibling CSGRS tree at
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0` and Hypercurve at
`6cb75a0546e7b8e7b39838b42b2babd5246f6802`. CSGRS's local package version is
only a manifest label: the source audit requires the sibling checkout and
rejects a published-registry substitute. The complete source table is recorded
in the interface's `docs/HYPER-BASELINE.md`.

Hyperphysics still had concurrent tracked modifications and Hyperlimit retained
untracked fuzz artifacts at capture time. Neither was changed for this
checkpoint. Those live inputs make this development evidence, not a clean,
atomic release source set.

## Implemented evidence chain

- `ExactScene` owns an exact line/arc/cubic path and a curved region containing
  an explicit hole. Hypergraphics alone produces their certified display line
  meshes; display chords cannot enter CAM.
- Exact line and circular-arc families promote directly from Hypercurve to
  Hyperpath. An unsupported general cubic returns a typed blocker instead of a
  float or display-chord fallback.
- A separate motion compiler asks Hypercurve for motion-policy subdivision,
  computes chord lengths as exact `Real` values, and certifies nearest machine
  positions and cumulative timer boundaries with Hyperlimit comparisons.
- Output records are the sibling `alumina-machine-ir::ExecutionSegment<2>`
  type. The boundary therefore ends in the canonical firmware schema rather
  than an interface-owned imitation.
- The retained report separates source-curve subdivision, per-axis command
  lattice error, Euclidean command-chord error, timer-boundary error, and
  segment-duration error. It does not mislabel those terms as physical
  following accuracy.

The deterministic line/semicircle/cubic fixture reports:

```text
source_curves=3
source_fragments=4
canonical_segments=197
final_steps=960,0
end_tick=1583188
ideal_chord_path_length_mm_display_f64=15.831876712961
source_chord_error_mm=1/1024
curve_to_canonical_chord_bound_mm_display_f64=0.009815397265
timer_boundary_error_seconds=1/2000000
segment_duration_error_seconds=1/1000000
```

The two fields named `display_f64` are lossy diagnostics only. Their
authoritative values remain exact expressions. At 80 steps/mm on two axes, the
conservative source-curve-to-canonical-command-chord bound is represented
exactly as `1/1024 + sqrt(2)/160 mm`. The 1 MHz fixture proves a half-tick bound
for every cumulative boundary and a one-tick bound for every derived segment
duration.

## Verification record

The following passed in `alumina-interface` against the source identities above:

```console
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz
gzip -t dist/alumina-interface.js.gz
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Observed results:

- 13 exact-core tests, two protocol-client tests, and one compile-fail doc test
  passed; the example target compiled.
- Strict native and WASM Clippy, rustdoc with warnings denied, formatting, and
  the local-source/permissive-license audit passed.
- The release WASM validated, and every recorded gzip/Brotli stream passed its
  integrity check.

Production artifact facts:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 3,791,907 | `a02518db33fc3f777bd26025080e29db3910b9062b91cb0a55b67b70854ae6d7` |
| `alumina-interface_bg.wasm.br` | 1,462,064 | compression integrity verified |
| `alumina-interface_bg.wasm.gz` | 1,776,640 | compression integrity verified |
| `alumina-interface.js` | 75,563 | `142202f87e449d354d954f449eaf62a8ddc5b8100897f41b0ccd303bf4ca695a` |
| `index.html` | 1,290 | `aeb69fcf104256c53ddb649aeb7f8f1b3db12d045c565c562c2e3603d1ceb324` |
| `Cargo.lock` | - | `2b5be2b5c541edb50887bc2da66eead19db2c66cef61e3ae78d61f037c0a355a` |

Hypergraphics separately passed 31 unit tests, one dispatch integration test,
two README tests, benchmark smoke runs, strict all-feature/all-target Clippy,
and rustdoc with warnings denied.

No board was connected, flashed, armed, or energized.

## Open gates

- The compiler currently schedules constant feed over its independently
  certified chord path. It does not claim exact general-Bezier arc-length
  parameterization, path-wide lookahead, acceleration, jerk, or machine
  kinematics.
- Machine capabilities, calibration uncertainty, following/control error,
  discrete step-event timing, qualified timer jitter, and physical mechanics
  remain separate closed inputs to the eventual complete error budget.
- Canonical block partitioning, manifest identity, SD caching, browser Wi-Fi
  upload, multi-MCU partitioning, and deterministic scheduled start are not yet
  connected to this compiler output.
- Browser automation, visual goldens, picking, filled-region triangulation, and
  the complete operator workflow remain open.
- Release qualification requires a clean, coherently reviewed and pinned local
  Hyper source set. It must not be replaced with the old published CSGRS
  release.
