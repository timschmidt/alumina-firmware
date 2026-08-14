# Direct finite-difference machine IR

Status: portable schema, independent admission, dense logical execution,
deterministic cached-partition simulation, and the first browser/WASM lowering
for exact stop-to-stop affine Hyperpath spans are implemented. Direct events
also enter the allocation-free scheduled complete-image owner and independent
PCM-short wire simulation. Curves, positive-feed joins, target peripheral
composition, WCET qualification, and physical motion evidence remain open.

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

A pulse fall may lie after a record or cached-block horizon. The single direct
executor owns that scheduled fall independently of the coefficient record and
combines it with later recurrence/output deadlines in exact cycle order. This
permits a polynomial coefficient change at a physically ordinary point inside
a pulse without inserting a false dwell. A following rise must still satisfy
the complete pulse-low and maximum-frequency interval, and direction may
change only after the prior fall plus direction hold and before the next setup
interval. Normal job finish is rejected until every pending fall has actually
been emitted. A local asynchronous fault ignores ordinary hold timing, lowers
every active pulse, disables every enabled axis, invalidates the cached token,
and requires the caller to apply that safe logical transaction.

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
direction, enable, scheduled rise/fall, integer, and Q31.32 state before its
first live record is installed. A rejection leaves live state unchanged and
returns the unique `AdmittedBlock`. Once accepted, the token remains owned until
every dense update has occurred and terminal tick, integer position, and
Q31.32 position all match independent job admission. Any later pulse fall is a
small fixed executor-owned deadline, so the next contiguous block can be
admitted without losing physical ownership; final disable still waits for that
fall and enable hold.

## Scheduled complete-image ownership

`ScheduledShiftedFiniteDifferenceStepper` composes the dense direct executor
with the same complete-image mapping and generation/staging/physical-commit
separation used by the ordinary scheduled stepper. Recurrence-only updates are
real deadlines but allocate no sparse output slot. Every logical change at one
exact device cycle is folded into one complete image and one commit token.

The newest same-cycle image remains deliberately unstageable until that cycle
is closed. This matters at both record and cached-block boundaries: a final
rise in one record and a direction/enable change in the next can share one
physical latch. On block completion the caller must either admit the immediate
successor before planning again, or irrevocably select the terminal owner tail.
The latter seals the boundary and drains every executor-owned fall. A later
successor is rejected, so physical behavior cannot depend on whether a caller
happened to stage an image early.

Each completed block retains the exact generated-image prefix that existed at
its numerical horizon. Its unique token can return once that prefix and the
terminal cycle are physically observed; a fall generated afterward remains
owned by the direct executor rather than being ambiguously assigned to either
block. Final disable cannot be scheduled until the owner tail is complete and
the enable-hold interval is satisfied. Ring exhaustion preserves the next
recurrence deadline, while staging mismatch, token reorder, early/late commit,
or image failure latches the whole retained window until a complete safe image
is requested.

The host PCM-short integration expands those sparse direct images into dense
frames, independently reconstructs all 64 modeled serial bits, and commits a
motion token only at the matching visible latch. The reproducible boundary is
recorded in
[`evidence/M10-SCHEDULED-DIRECT-PCM.md`](evidence/M10-SCHEDULED-DIRECT-PCM.md).

## Permanent target execution-kind dispatch

The core-1 `MotionService` now derives exactly one
`ScheduledExecutionMode` from the independently validated `JobDescriptor` at
prime time and constructs one fixed-memory `ScheduledShiftedExecution` owner.
The owner never infers a kind from block payloads or converts between record
families. A cross-family block is returned unchanged after rejection, and a
completed block preserves whether ordinary integer or direct Q31.32 terminal
state was checked before its unique token returns to the job actor.

The common planning boundary distinguishes an ordinary sealed block from a
direct continuation-open block. Generic polling repeats the latter without
closing or staging its newest same-cycle image. An admitted successor composes
normally; only the descriptor-declared final boundary invokes the explicit
owner-tail operation. A future tail deadline or full output ring leaves that
terminal decision retained across core-1 polls. Final disable is not planned
until the owner tail is complete, and the final block cannot return until its
generated prefix, tail, and disable are owned by the target timeline.

Host regressions cover both execution modes, delayed direct continuation,
explicit terminal-tail resumption, cross-family rejection, complete-image
horizon accounting, and the fixed target-owner memory bound. The bit-level
PCM-short integration now enters through this descriptor-selected owner. The
target builds prove the permanent actor composes for TinyBee and T-Deck Pro,
but their hardware adapters still reject motion streaming and neither board is
armable. The reproducible boundary is recorded in
[`evidence/M10-TARGET-DIRECT-DISPATCH.md`](evidence/M10-TARGET-DIRECT-DISPATCH.md).

## Compiler obligations and open gates

The browser/WASM compiler remains authoritative for geometry and CAM. The first
implemented lowering accepts exact affine spans that stop at every element,
rebuilds their symmetric jerk phases on the device output grid, reruns
Hyperpath/Hypersolve certification, projects exact Newton forward differences
through certified Hyperreal intervals to Q31.32 with ties to even, adaptively
splits near zero velocity to preserve monotonic fixed-point records, propagates
an exact positional-error bound, packages `ALMBLK02` partitions, and replays
them through the production cached executor. `ALMDFE01`/`ALMDFT01` evidence
binds source/planner identities, every interval and coefficient, propagated
error, electrical preflight, and immutable cache identity.
The reproducible implementation and verification record is
[`evidence/M10-BROWSER-DIRECT-FINITE-DIFFERENCE.md`](evidence/M10-BROWSER-DIRECT-FINITE-DIFFERENCE.md).

The remaining lowering stages must:

1. generalize the exact Hyperpath/Hypersolve polynomial-interval construction
   beyond the implemented stop-to-stop affine case;
2. extend the implemented interval-certified projection to curved spans and
   positive-feed joins without sampling display geometry;
3. split at every required direction change, coefficient-range boundary,
   physical timing boundary, and error-budget boundary;
4. preserve the shared multi-MCU time model and independently replay every
   resulting `ALMBLK02` partition through production firmware validators; and
5. cache only complete content-addressed partitions plus their evidence before
   deterministic schedule commit.

The current target composition stops at the abstract real-time resource
boundary and owns no ESP motion-streaming peripheral. It makes no TinyBee
timing, DMA, memory-bandwidth, safe-output, or energization claim. A target
adapter must still bind the single-owner image/PCM contract to a qualified
GPIO/RMT/I2S peripheral, prove its dense hardware horizon and physical latch
source, measure it under real Wi-Fi/service load, and pass the existing board
armability and safety gates.
