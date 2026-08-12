# M9 exact graph-rate evidence

Date: 2026-08-11

## Scope and source

This checkpoint gives audited cross-clock Stream dependencies one exact,
bounded rate-transition contract. It is host-side admission and reporting. At
this source point it does not evaluate a node, schedule firmware work, define a
bridge, or grant Service/Realtime execution authority.

The implementation is `alumina-interface` commit
`a1478c9c03abd32ab05f383a5bbf8f9d0268bb3f`, above bounded-channel commit
`e8938ff`. It resolves CSGRS, every Hyper crate, and the Alumina protocol and
storage crates from the sibling workspace. No published legacy CSGRS package
is selected.

## Exact clock authority

Analysis resolves every document clock into a `GraphClockRate` containing its
clock identity, independent root, and exact `hyperreal::Rational` ticks per
second. A HostMonotonic or DeviceCycle clock is an independent tick-zero root;
a reduced Derived clock shares its source root. Recursive, missing, zero-rate,
or nonreduced clock definitions already fail document construction.

Equal frequency is not phase authority. A transition passes only when both
Stream clocks resolve to the same independent root. Two separately declared
1,000 Hz roots therefore reject even though their numerical rates match.

## First transition contract

`NodeRateTransitionContract` is audited schema authority selected by exact node
kind/version. The first supported policy is
`LatestAtOrBeforeSourceFirst`, and it is deliberately narrow:

- input and output are both Stream types with the identical registered sample
  type;
- their clocks differ but share one tick-zero root;
- the input is required and uses a bounded Stream queue;
- at every target tick, all source samples due at or before that exact instant
  are consumed and the newest is emitted; and
- at every coincident source/target tick, including tick zero, the source is
  processed first. There is no implicit initial sample.

An implicit cross-clock feedthrough, gratuitous same-clock transition, optional
transition input, Event/Stream family change, sample-type change, duplicate
transition identity, or transition not matching complete output feedthrough
rejects during registry admission.

## Rational schedule and memory proof

The reduced source/target frequency ratio directly defines the smallest
repeating schedule. The representative exact 1,000 Hz to 600 Hz transition is
reported as five source ticks to three target ticks. Its minimum input capacity
is `ceil(5 / 3) = 2`; a one-item queue fails before an analysis report exists.

Latest-at-or-before retains one complete sample in addition to the transport
queue. The Boolean fixture has a five-byte canonical typed-sample ceiling, so
the transition report retains exactly five bytes. Reports are canonical by
node/output/input and retain:

- source, target, and common-root clock identities;
- both exact rational frequencies;
- reduced source and target pattern ticks;
- required queue items;
- transition policy; and
- retained sample bytes.

The interactive policy limits transition count to 8,192, either pattern
dimension to 1,000,000 ticks, and aggregate held-sample state to 64 MiB. Checked
arithmetic rejects count, pattern, queue, per-input channel, or held-state
overflow.

## Reproduced checks

At the implementation commit, the full native suite passed 80 tests: 8
application/coordinator, 24 client, and 48 core, plus the intentional
compile-fail rustdoc test. Native and WASM warnings-denied project Clippy,
warnings-denied project rustdoc, the sibling-source/permissive-license audit,
optimized Trunk build, WASM validation, and gzip/Brotli integrity passed.

The sibling Hypercurve tree was changing concurrently during that first
optimized build, so its emitted bundle hashes are intentionally not promoted as
an immutable checkpoint. Descendant interface commit `82ca0e1` retains this
rate implementation and requalifies it with a stable before/after Hypercurve
and Hyperphysics tracked-diff identity; that complete artifact observation is
recorded in the deterministic-simulation evidence.

Focused tests prove the 5:3 report and two-item queue, and reject independent
roots, implicit transitions, insufficient queues, excessive pattern size,
transition-count overflow, and aggregate held-state overflow.

## Closed claims and licensing

This source point defines no runtime evaluator, queue implementation, missed-
deadline behavior, fixed firmware representation, WCET, resource claim,
Service/Realtime opcode, bridge, or deployment package. No arbitrary graph
document reaches firmware.

No target firmware source or binary changed. The connected bare 8 MiB MKS
TinyBee V1.0 was not read, reset, flashed, or otherwise touched; no motor or
motor power was connected. The SLogic16U3 remained disconnected, and no MKS
ESP32 FOC hardware was available. No physical I/O, timing, motion,
synchronization, FOC, or safety claim is made.

The implementation is independently authored under MIT and adds no dependency.
The native/WASM inventory and current-sibling inverse-path audit admit no
GPL-family or missing-license dependency. No GPL-family source or asset was
copied or consulted.
