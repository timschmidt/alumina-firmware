# M10 direct finite-difference machine IR — offline evidence

Date: 2026-08-14

Status: implemented portable development checkpoint. Firmware now admits and
executes cached third-order finite-difference step coordinates through a
separately kind-bound canonical machine format. This is host simulation and
native/WASM linkage evidence. It is not target output timing, WCET, Wi-Fi, SD,
safety-response, motor, or machine-accuracy qualification.

## Result and source identity

Firmware commit `ebe860bcbeae2fb5c9fb6680e43e95073a379356` implements the
schema, independent job admission, sparse electrical proof, allocation-free
dense executor, cached-token ownership, deterministic simulation, protocol
documentation, and adversarial regressions. Alumina Interface commit
`a854946131d1eedcaaa52d234f9e049366c054bb` adopts the new job descriptor
without a compatibility shim; its existing compiler deliberately continues to
emit coordinated kind `1`. Browser lowering to direct kind `2` remains the
next separate checkpoint.

The final interface test and strict native/WASM gate batch was bracketed by an
identical moving Hypercurve source fingerprint:

- HEAD `de9628dd962a8dcbbe20a527f743a1d2abcff225`;
- tracked edits in `src/bezier_offset.rs`, `src/bezier_region.rs`, and
  `src/curve_region_boolean.rs`; and
- binary tracked-diff SHA-256
  `fcd7cfb7a4799700832b553df166151fbb38e45d677a0116fb2193a39aef2950`.

Alumina did not edit, format, reset, or pin Hypercurve. The fingerprint names
the exact read-only development state used by this checkpoint, not a release
pin. The same batch resolved clean Hyperpath HEAD
`d792aa8dc843218b26fc0d1730033e5cd06bdf2f`, clean Hypersolve HEAD
`6ce08b714cdba1e3668e1af6c83f0a249bda9bb5`, clean Hypergraphics HEAD
`31811aeb17bd2dc827db5669558f6251e0c2f2aa`, and clean CSGRS HEAD
`b34a2f47b90e3d329028d6337d19dfbc9629fbb0`. The source-policy audit also
required every other Hyper dependency to resolve from its sibling workspace
checkout. Hyperlimit was tracked-clean at
`b0418bddff50183fa782e5caa6da6974a2b969a1` but retained unrelated untracked
fuzz corpus/artifact paths; Alumina did not inspect, change, or package them.

The native and WASM dependency inventories passed the repository GPL-family
rejection policy. This increment added no dependency and did not copy, inspect,
translate, or link Synthetos/g2, SimpleFOC, FluidNC, or other GPL-family
implementation source.

## Canonical greenfield formats

Machine-block V2 is exactly 512 bytes, begins with `ALMBLK02`, and binds an
`ExecutionKind`. Kind `1` preserves coordinated integer-displacement records.
Kind `2` stores one common update period/count followed by signed Q31.32
Newton-forward `p0`, `d1`, `d2`, and `d3` for each of up to eight axes. Payload
length, zero padding, identities, sequence, stream ticks, previous digest, and
block SHA-256 remain independently checked.

Job descriptor V3 begins with `ALMJOBD3`. It binds the selected execution kind
and a nonzero maximum dense update count only for direct streams. Both service
prefetch and real-time admission instantiate the matching validator; a mixed or
substituted block kind fails before admitted progress exists. Retired
`ALMBLK01` and `ALMJOBD2` bytes are rejected. There is no decoder alias, version
negotiation, or compatibility conversion.

## Exact numerical contract

For update index `k`, each axis represents

```text
p(k) = p0 + k d1 + C(k, 2) d2 + C(k, 3) d3
```

in signed Q31.32 relative command steps. The live recurrence performs three
checked additions per axis and update. Independent admission evaluates the
closed form, carries exact Q31.32 terminal state between records and blocks,
and rejects coefficient or cumulative integer overflow.

Nearest integer with exact ties to even is the sole projection from Q31.32 to
the command-step lattice, including negative values. That projection contributes
at most one half step of numerical error. It does not certify source-to-cubic
approximation; the authoritative browser must bind that separate proof into the
complete machine-resolution budget.

Each record must be monotonic per axis. Firmware checks the complete discrete
first-difference range using endpoints and the exact second-difference sign
crossing, then checks rounded terminal displacement. Direction changes are
therefore explicit record boundaries rather than implicit reversals inside a
polynomial.

## Bounded physical admission

The job actors first apply structural bounds independent of a specific output
backend. Before live execution, the direct stepper derives a tighter per-axis
first-difference ceiling from pulse high/low time and maximum step frequency.
If `R` is the larger required edge spacing and `M` is
`ceil(R / update_period)`, admission requires

```text
abs(d1(k)) <= floor((2^32 - 1) / M)
```

at every discrete update. The strict numerator proves that a worst-case
threshold overshoot cannot cross a second rounded boundary within fewer than
`M` updates. Exact monotonic binary searches locate the first and last integer
crossings. Complete electrical admission is therefore proportional to record
count times axis count times `log2(update_count)`, not to dense frame or emitted
step count. A regression admits a zero-edge billion-update record without
dense iteration.

The sparse state carries stream tick, Q31.32 and integer coordinates,
direction/enable state, prior rise/fall cycles, emitted steps, record count, and
update count across records and blocks. It checks setup, hold, pulse-high,
pulse-low, rate, output-grid, epoch, and terminal-fall constraints before the
first live record of a candidate block is installed.

## Dense execution and ownership

`FiniteDifferenceStepperExecutor` is allocation-free. Every declared update is
a real-time deadline, including an update producing no output edge. It emits
explicit direction, enable, step-rise, step-fall, and normal terminal-disable
logical transactions. Lateness faults before returning the uncommitted update
or event. A caller-requested fault immediately lowers active pulses, disables
enabled axes, and invalidates normal completion.

`CachedFiniteDifferenceExecutor` first sparse-preflights every record on a
private state copy. Rejection returns the unchanged unique `AdmittedBlock` and
does not mutate the live executor. Acceptance retains that token through all
dense updates and the last pulse fall. It releases the token only after exact
agreement with independent job admission on terminal stream tick, rounded
absolute position, and retained Q31.32 position. A faulted token cannot be
acknowledged and can be recovered only after requesting the logical safe
transaction.

`alumina-sim::motion::replay_cached_finite_difference_partition` consumes the
actual immutable partition bytes, decodes and admits them through `RealtimeJob`,
runs every dense executor deadline, acknowledges returned tokens in order, and
checks both terminal lattices plus counts and digest. Its two-record fixture
executes 16 updates, three rising edges per axis, seven logical output
transactions, and the expected terminal block identity.

## Verification record

The following completed offline:

```sh
# aluminafw at ebe860bcbeae2fb5c9fb6680e43e95073a379356
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

# alumina-interface at a854946131d1eedcaaa52d234f9e049366c054bb
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run \
  --locked --offline
scripts/audit-source-policy.sh
git diff --check
```

Observed results:

- the complete 413-test portable firmware default-member suite passed,
  including 17 machine-IR, 20 job, 37 motion, and 38 simulator tests;
- firmware warnings-denied Clippy and rustdoc passed;
- 28 application, 37 client, and 120 exact-core interface tests passed, plus
  the exact-control integration and compile-fail boundary tests;
- native/WASM warnings-denied Clippy, warnings-denied rustdoc, every WASM test
  target link, formatting, diff checks, and source/license policy passed; and
- the Hypercurve HEAD/status/diff fingerprint above was identical immediately
  before and after the final test and strict native/WASM gate batch.

The root firmware workspace contains ESP target-only members, so literal host
`cargo test --workspace` is not the portable gate. The documented default-member
command above tests the host-compatible crates; no target result is implied.

## Claim boundary and next work

This closes the portable firmware format, admission, ownership, logical
execution, and immutable-cache simulation boundary. It does not yet lower exact
Hyperpath schedules into Q31.32 records in the browser. It also does not connect
direct events to TinyBee's single-owner PCM/DMA image path, establish
worst-case execution time under service/Wi-Fi load, validate SD delivery, or
authorize a board to arm.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, or driven.
No WLAN association, serial/USB transaction, analyzer capture, GPIO operation,
motor power, or process power occurred. Physical work remains deferred while
the workstation Wi-Fi is required for this development session.
