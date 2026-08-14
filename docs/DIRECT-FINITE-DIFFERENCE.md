# Direct finite-difference machine IR

Status: portable schema, independent admission, dense logical execution, and
deterministic cached-partition simulation are implemented. Browser/WASM
lowering, target output-engine composition, WCET qualification, and physical
motion evidence remain open.

## Numerical contract

Machine-block schema V2 execution kind `2` represents a piecewise cubic
position sequence directly in the motor command lattice. Every coefficient is
a signed Q31.32 number of command steps. With `S = 2^32`, one axis record stores

```text
p(k) = p0 + k d1 + C(k, 2) d2 + C(k, 3) d3
```

for integer update indices `0 <= k <= update_count`. A dense executor applies

```text
p  += d1
d1 += d2
d2 += d3
```

once per exact `update_period_ticks`. All additions are checked. The closed form
is used independently during admission, so the live recurrence is not its own
certificate.

The Q31.32 domain covers `[-2^31, 2^31 - 2^-32]` relative command steps. The
absolute integer machine origin remains in `JobDescriptor`; cached coefficients
are relative to that origin. The first record begins at exact Q31.32 zero and
every following record, including across a block boundary, must repeat the
preceding exact terminal `p`. A stream contains only one execution kind; motion
and direct records are never mixed implicitly.

The sole Q31.32-to-step projection is nearest integer with exact ties to even,
implemented symmetrically for negative coordinates. Consequently firmware's
projection error is at most one half step. This does not certify browser
coefficient approximation: the authoritative compiler must separately bind the
exact source, chosen coefficients, pointwise error, machine resolution, and
every remaining physical uncertainty into the job evidence budget.

## Record and timing semantics

One canonical record contains:

| Field | Width |
| --- | ---: |
| update period | `u32` ticks |
| update count | `u32` |
| flags | `u32`, zero |
| reserved | `u32`, zero |
| `p0`, `d1`, `d2`, `d3` for each axis | four little-endian `i64` values |

Its exclusive end tick must equal
`start_tick + update_period_ticks * update_count` exactly. Dense update `k`
occurs at `epoch + start_tick + k * update_period_ticks`, for
`1 <= k <= update_count`. When rounding changes by one step, the rising edge is
scheduled at that update boundary. Direction and first enable changes occur at
the record start; a falling edge occurs exactly `pulse_high_cycles` after its
rise. All boundaries, updates, and falls lie on the configured output quantum.

A final rise must leave enough time for its falling edge before the record end.
The authoritative compiler may therefore append exact update padding; firmware
does not silently extend a record. Direction may change between records only
when prior pulse-low and direction-hold requirements plus the next setup time
are all satisfied. A local asynchronous fault ignores ordinary hold timing,
lowers every active pulse, disables every enabled axis, invalidates the cached
token, and requires the caller to apply that safe logical transaction.

Every update is a real-time deadline even when it creates no output edge. This
keeps the recurrence cost explicit and makes later timer/DMA/WCET qualification
possible. Output-empty frames are reported separately from physical logical
transactions in simulation.

## Bounded admission

Independent block validators require:

- exact kind, axis width, identity, sequence, digest chain, and tick continuity;
- nonzero bounded period/count and exact end-tick redundancy;
- checked Q31.32 position, first-difference, and second-difference state;
- exact Q31.32 phase continuity and bounded rounded displacement;
- no first-difference sign reversal inside one record; and
- an absolute first-difference bound below one whole step per update.

The physical preflight tightens the last bound for each axis. Let `R` be the
larger of pulse-high-plus-low time and the configured maximum-frequency period,
and let `M = ceil(R / update_period)`. It requires

```text
abs(d1(k)) <= floor((S - 1) / M)
```

at every discrete update. The strict `S - 1` numerator proves that, even after
the worst threshold-crossing overshoot, another rounded boundary cannot be
crossed in fewer than `M` updates. Exact discrete first-difference extrema are
found from endpoints and the second-difference sign crossing. First and last
integer crossings use monotonic binary search. Admission therefore costs
`records * axes * log2(update_count)` rather than dense update or step count;
the regression suite admits a zero-edge billion-update record without looping a
billion times.

The complete candidate block is sparse-preflighted on a private copy of current
direction, enable, rise/fall, integer, and Q31.32 state before its first live
record is installed. A rejection leaves live state unchanged and returns the
unique `AdmittedBlock`. Once accepted, the token remains owned until every
dense update and final pulse fall has occurred and terminal tick, integer
position, and Q31.32 position all match independent job admission.

## Compiler obligations and open gates

The browser/WASM compiler remains authoritative for geometry and CAM. Its next
lowering stage must:

1. derive each polynomial time interval from exact Hyperpath/Hypersolve
   schedule evidence rather than sampled display geometry;
2. choose update grids and Q31.32 coefficients with a caller-bounded exact or
   interval-certified approximation proof;
3. split at every required direction change, coefficient-range boundary,
   physical timing boundary, and error-budget boundary;
4. preserve the shared multi-MCU time model and independently replay every
   resulting `ALMBLK02` partition through production firmware validators; and
5. cache only complete content-addressed partitions plus their evidence before
   deterministic schedule commit.

The current implementation owns no ESP peripheral and makes no TinyBee timing,
DMA, memory-bandwidth, safe-output, or energization claim. Direct events must
still be composed into the qualified single-owner GPIO/RMT/I2S output engine,
measured under real Wi-Fi/service load, and passed through the existing board
armability and safety gates.
