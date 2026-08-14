# Servo finite-difference command streams

Machine-IR V3 execution kind `3` is the bounded command language between the
authoritative browser/WASM CAM compiler and a portable realtime FOC-servo
owner. It is separate from kind `2` direct step crossing: servo records carry
setpoints for a configured cascaded control axis and never project position to
integer step edges.

This checkpoint is a software and numerical contract. It does not qualify a
motor, encoder, power stage, ESP peripheral, machine, or energized output.

## Canonical record

Every kind-3 block is exactly 512 canonical bytes with `ALMBLK03`, execution
kind `3`, one to four axes, a chained predecessor digest, and a SHA-256 digest
over bytes `0..480`. Its payload consists of fixed-width records:

| Bytes | Meaning |
| ---: | --- |
| 4 | nonzero `update_period_ticks` |
| 4 | nonzero `update_count` |
| 4 | flags, zero in V3 |
| 4 | reserved zero |
| 32 per axis | Q31.32 absolute position `p0, d1, d2, d3` as signed `i64` |
| 16 per axis | Q2.30 normalized velocity feed-forward `p0, d1, d2, d3` as signed `i32` |
| 16 per axis | Q2.30 normalized quadrature-current feed-forward `p0, d1, d2, d3` as signed `i32` |

Those three fields are contiguous and repeated axis by axis. The record size is
`16 + 64 * axes`, so one payload holds four records at one
axis, two at two axes, and one at three or four axes. All three signals use the
same Newton-forward recurrence

```text
x(k) = p0 + k*d1 + C(k, 2)*d2 + C(k, 3)*d3
```

with checked integer evaluation. Position is expressed in the configured
servo-axis lattice, not implicitly in motor turns or encoder counts. That scale
is part of the active configuration whose digest binds the stream.

## Half-open ownership

A record beginning at tick `t` emits the states for integer update indices
`0 <= k < update_count` at `t + k * update_period_ticks`. The state at
`k = update_count` is not emitted by that record. It is the exact continuation
state and must be the next record's `p0`, including across a block boundary.

The complete stream begins at the descriptor's absolute Q31.32 positions with
both feed-forward vectors at zero. Its final continuation must also have both
feed-forward vectors exactly zero. After the last record, the realtime runner
emits exactly one terminal at-rest hold at the exclusive end tick. The total
dense update count is strictly below `u32::MAX`, leaving one nonzero contiguous
command identity for that hold.

This convention gives every setpoint one owner. Delayed two-block lookahead
cannot duplicate or omit the state shared by adjacent records.

## Independent admission

Generic machine limits cannot authorize a servo stream. A typed admission path
reconstructs every `ServoSetpointAxisAdmissionProfile` from a complete validated
`ServoFocAxisProfile`, then requires all participating axes to share:

- the same nonzero active-configuration digest;
- the same exact position-loop cadence; and
- a caller-bounded nonzero block, segment, and update-count policy.

The profile derives conservative Q31.32 progress per position update from the
configured velocity ceiling, encoder scale, counts per turn, device clock, and
position-loop period. It derives velocity feed-forward authority from the servo
limit and q-current feed-forward authority from the intersection of the current
circle and velocity-controller output range.

Core-0 and core-1 stream validators independently reject cadence mismatch,
missing or reordered blocks, digest-chain disagreement, recurrence overflow,
noncanonical padding, discontinuity, an unsplit discrete reversal, excessive
position delta/rate, feed-forward range escape, terminal non-rest state, and
dense command-count overflow. `ALMJOBD4` and each `ALMJMF02` participant repeat
the execution kind, cadence, and update-count bounds. The generic open/prepare
path rejects kind `3`; callers must supply the matching typed servo profile.

## Portable realtime owner

`CachedServoSetpointRunner` is fixed-memory and owns at most two admitted block
tokens. It decodes one record at a time, retains exact recurrence and chain
state, and reports one simultaneous setpoint vector at each position-loop
deadline. Every vector is a two-phase operation:

1. `plan` returns an opaque token and the complete immutable axis vector;
2. the sole physical control owner applies that vector at the exact cycle; and
3. `commit` advances recurrence, command, and block ownership atomically only
   when the token and deadline match.

Fallible counter and state transitions are staged before mutation. Rejection,
early/late observation, token mismatch, lookahead loss, and arithmetic failure
cannot partially advance live state. A fault latches first cause and keeps
unacknowledgeable tokens available only for post-safe diagnostics.

The portable FOC-axis simulator replays a two-block stream for 401 current-loop
periods, including a shared block boundary and the terminal hold. This proves
deterministic software ownership against the functional plant model; it does
not establish target WCET, interrupt latency, ADC/PWM synchronization, or
electrical behavior.

## Permanent lifecycle and current target gate

The browser compiler projects exact Hyperreal cubic recurrences into Q31.32 and
Q2.30 only after certified dyadic intervals select one ties-to-even integer. It
retains explicit coefficient/error evidence, forces encoded continuity with
the introduced error accounted for, splits at exact discrete extrema and
ownership/profile limits, reuses the firmware encoder and validators, and
publishes only complete content-addressed partitions. See the coordinated
interface contract in `alumina-interface/docs/EXACT-SERVO-MOTION.md`.

The permanent ESP job actors now retain independently validated compact
configuration profiles and rebuild the exact servo limits on both cores. The
core-1 `MotionService` selects kind `3` without fallback, primes one future
simultaneous batch before the distributed start, accepts one opaque physical
commit at a time, and returns each block only after its continuation or terminal
commit. Commit-token substitution, an acknowledgement observed before its
claimed boundary, a report beyond the board-bounded latency, cancellation, and
safety fault all fail closed. Commit-report latency must be strictly shorter
than the configured position-loop period so the next batch can be staged.

`ServoFocBank` extends the complete-axis boundary across one to four axes. It
requires one configuration identity, exact nested loop grid, common boundary,
and distinct boot-local activation identity per axis. Preparation calculates
every encoder, cascaded-servo, current-control, angle, SVPWM, and compare-image
candidate without advancing a successful axis. Commit derives and validates
every next controller first, then installs the complete controller array. A
late or substituted commit on a later axis therefore leaves every estimator,
controller, sequence, and active-image prefix at the prior boundary and latches
the bank closed.

`ScheduledServoFocHardwareBank` implements the simultaneous setpoint-output
boundary in simulation. It injects every due cached axis setpoint into that
complete bank and publishes one opaque cached commit only after all modeled PWM
commits succeed. The two-axis, two-block test replays 401 current periods, three
simultaneous position updates, both block barriers, and the terminal hold.
Separate regressions reject a late second-axis latch and a missing second-axis
encoder observation without first-axis advance, then prove that an enclosing
safe invalidation clears the staged mailbox before cached-job ownership is
faulted.

Every firmware board currently implements this setpoint-output boundary as a
transactional unavailable result. `SERVO_OUTPUT_IMPLEMENTED`,
`SERVO_OUTPUT_QUALIFIED`, qualified commit-report latency, and qualified prime
lead all remain closed for TinyBee, T-Deck Pro, and MKS ESP32 FOC. The job is
therefore rejected before cache ownership or arming; no target PWM, ADC,
encoder, shutdown, timing, or energization claim follows from this milestone.
The exact commands, artifacts, memory correction, and closed gates are recorded
in `docs/evidence/M10-PERMANENT-SERVO-LIFECYCLE.md`.
