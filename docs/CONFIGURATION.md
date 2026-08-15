# Canonical machine configuration V6

`ALMCFG06` is the content-addressed machine/resource authority emitted by the
browser/WASM compiler and independently validated on both ESP cores. It is not
JSON, FluidNC configuration, G-code, a Rust memory image, or executable code.
The complete bytes are uploaded as storage object kind `MachineConfiguration`
(`6`) and remain inert until a separate safe, durable activation transaction.

SHA-256 over the complete document is the configuration digest carried by jobs,
commands, status, and both core-local active identities. The document embeds the
exact immutable `ALMCAP04` board digest, so a configuration cannot move silently
between revisions or capability/qualification changes.

## Common rules and header

Integers are little-endian. Reserved bytes are zero. Unknown flags, record
kinds, roles, facts, owners, polarities, or evidence values reject. Records are
fixed-width and strictly ordered by `(kind, instance, selector)`; duplicate keys
are consequently impossible. V6 admits 1–256 records and no trailing data. V1
through V5 are not accepted; firmware and UI are updated together without a
compatibility decoder.

The fixed 80-byte header is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMCFG06` |
| 8 | 2 | exact schema version `6` |
| 10 | 2 | header bytes, exactly `80` |
| 12 | 4 | total bytes, exactly `80 + record_count × 64` |
| 16 | 32 | required canonical board-capability SHA-256 |
| 48 | 2 | total record count, `1..=256` |
| 50 | 2 | derived core-1-relevant record count |
| 52 | 4 | configuration policy flags |
| 56 | 24 | reserved zero |

Policy bits are motion (`1`), cached-autonomous permission (`2`), field-oriented
control (`4`), and laboratory/control resources (`8`). Cached-autonomous and FOC
both imply motion. These bits express candidate intent; later qualification,
interlock, job, lease, and safety gates still decide whether anything can run.

## Fixed records

Every record is exactly 64 bytes. Its common prefix is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 2 | kind: binding `1`, scalar `2`, FOC shutdown `3`, runtime `4`, controller `5`, rotor `6`, current channel `7`, PWM/ADC timing `8`, ADC frontend `9`, PWM hardware `10`, servo `11`, encoder scale `12`, encoder policy `13` |
| 2 | 2 | record bytes, exactly `64` |
| 4 | 2 | logical instance; axis index for axis/motor facts |
| 6 | 2 | kind-specific role or scalar-fact selector |

### Resource binding

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | canonical typed resource ID from `CAPABILITIES.md` |
| 12 | 1 | owner: service `1`, realtime `2` |
| 13 | 1 | polarity: not-applicable `0`, active-high `1`, active-low `2` |
| 14 | 2 | pull-up, pull-down, open-drain, required-interlock flags |
| 16 | 4 | minimum active/debounce-high cycles |
| 20 | 4 | minimum inactive/debounce-low cycles |
| 24 | 4 | requested maximum event/sample/carrier/bus Hz |
| 28 | 4 | local watchdog/sample-gap cycles; nonzero for hazardous/timed output and every safety input |
| 32 | 32 | reserved zero |

Binding-role values are:

| Range | Roles in numeric order |
| --- | --- |
| 1–9 | axis step, direction, enable, minimum limit, maximum limit, encoder A, encoder B, encoder index, motor fault |
| 10–19 | probe, E-stop, safety interlock, digital input, digital output, analog input, PWM output, serial port, timer, counter |
| 20–27 | FOC phase U/V/W, current A/B/C, bus voltage, encoder |
| 28 | removed V1 `FocEnable` selector; always invalid |
| 29 | FOC fault |
| 30–38 | process output, storage, I2C bus, SPI bus, TWAI bus, capture input, waveform output, fitted device, axis disable |

The validator resolves the resource only in the advertised typed namespace. It
requires exact owner agreement, rejects duplicate physical claims, admits only
role-compatible resource kinds, and applies input-only, output-only, no-PWM,
active-high, and active-low board constraints. An I2S-engine constraint also
governs every shifted bit from that engine. Bus frequency may not exceed the
board route. A generic output may not acquire a resource marked hazardous;
hazardous resources require an explicit motion/FOC/process role and watchdog.

Digital inputs use explicit polarity and may use one pull direction. Sampled
encoder/capture and analog/current-sense inputs require a nonzero bounded sample
or event rate. Analog/controller resources use no synthetic pin polarity. Timed
step/PWM/waveform outputs require nonzero active/inactive timing, frequency, and
watchdog bounds.

Limits, probes, motor/FOC faults, E-stops, and safety interlocks additionally
require a finite nonzero sample-gap watchdog. Their active/inactive timing fields
are exact debounce intervals and may be zero for an immediate transition.
E-stop and safety-interlock records must carry `required-interlock`; that flag is
rejected on ordinary bindings. Pull selection, polarity, debounce bounds,
watchdog, physical resource, role, and logical instance are retained in the
core-1-only executable profile. Up to 32 such inputs have stable slots in one
per-MCU configuration. The conservative first-release arm gate requires every
fault-class input, including limits and motor/FOC faults, to be clear; a probe is
the sole role not implicitly arm-blocking unless its record explicitly sets the
required flag. Later homing/probing modes must replace that fallback with an
equally bounded operation-specific policy.

The first target sampler runs at a nominal 1 ms core-1 cadence, so every active
input's maximum sample gap must be at least that nominal period; actual lateness
beyond the configured bound still faults. TinyBee currently realizes GPIO 33,
32, and 22 with floating/pull-up/pull-down modes and GPIO35 as floating-only.
All routes are validated before any new pull mode is applied. T-Deck Pro exposes
no machine safety-input route, so a nonempty safety profile fails closed at the
target boundary. These are compiled routing facts, not electrical or latency
qualification.

### Exact scalar

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 8 | signed nominal numerator |
| 16 | 8 | unsigned nominal denominator |
| 24 | 8 | unsigned absolute-uncertainty numerator |
| 32 | 8 | unsigned absolute-uncertainty denominator |
| 40 | 1 | evidence: declared `1`, measured `2`, qualified `3` |
| 41 | 23 | reserved zero |

Both rationals have positive nonzero denominators, are reduced by GCD, and use
only `0/1` for zero. Uncertainty cannot be negative. Integer facts require
denominator one and zero uncertainty. Physical dimensions are fixed by the fact
selector rather than stored as free-form strings:

| Value | Exact fact |
| ---: | --- |
| 1–5 | axis full steps/turn, microsteps, motor turns/output turn, metres/output turn, calibration scale |
| 6–11 | minimum/maximum position metres, velocity m/s, acceleration m/s², jerk m/s³, following error metres |
| 12–18 | encoder counts/turn, pole pairs, current A, voltage V, PWM Hz, dead time seconds, control Hz |
| 19–24 | current-sense ohms, current-sense V/A, safety reaction seconds, process duration seconds, timer tick Hz, stepper output quantum cycles |

All facts except signed position bounds are positive. The browser can therefore
retain Hyper exact values through CAM and emit a reduced rational only at this
explicit hardware boundary; measured uncertainty remains a separate exact
bound rather than being folded into an approximate nominal.

`AxisCalibrationScale` is a dimensionless multiplicative correction to command
density. For a step/direction axis, commanded steps per metre are exactly
`full_steps_per_turn × microsteps × motor_turns_per_output_turn ×
calibration_scale / travel_metres_per_output_turn`. Its absolute uncertainty is
propagated independently through that expression. It does not rescale the
configured position, velocity, acceleration, jerk, or following-error facts.

`ConfigurationDocumentView` exposes these records allocation-free only after
the complete bytes have passed the same canonical, board-capability, semantic,
binding-budget, and SHA-256 validation used by the streaming firmware path.

Every motion document contains exactly one `TimerTickHertz` fact at logical
instance zero. It is the integer `DeviceCycle` frequency shared by the compiled
stream, clock fitting, firmware scheduler, and simulator; it is not inferred
from a nominal CPU clock. A document with stepper axes also contains exactly
one positive integer `StepperOutputQuantumCycles` fact at instance zero. That
fact is the smallest output interval addressable by the selected backend. Core
1 retains both values and compares them with the compiled Embassy timer and
board backend before it constructs an executor. Canonical record uniqueness
and the instance-zero rule prevent competing time bases or output lattices.

For each stepper axis, the effective frequency ceiling is the lesser of the
binding's `maximum_frequency_hz` and `TimerTickHertz / (minimum_active_cycles +
minimum_inactive_cycles)`. A compiler must additionally replay every emitted
segment on `StepperOutputQuantumCycles`; the aggregate ceiling alone cannot
prove pulse, setup, hold, or direction-transition timing.

### FOC shutdown contract

Kind `3` is a mandatory, axis-local shutdown record for every FOC axis:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | fitted power-stage `Device` resource |
| 12 | 4 | dedicated control resource, or four zero bytes when absent |
| 16 | 4 | inclusive maximum transition-to-off device cycles |
| 20 | 1 | control polarity, or not-applicable for phase high impedance |
| 21 | 1 | evidence, exactly qualified `3` |
| 22 | 42 | reserved zero |

The common selector is the strategy: dedicated enable `1`, dedicated disable
`2`, or phase high impedance `3`. Dedicated enable means the control's inactive
level is safe; dedicated disable means its active level is safe. The validator
checks that polarity against the control resource's board safe value. Phase
high impedance permits no control resource or polarity and requires the power
stage plus all three bound phase resources to advertise `HighImpedance`.

The stage must be a realtime-owned hazardous fitted device at
`SupportLevel::Qualified`. Its topology must contain all three phase resources
and both selected ADC current resources and, for a dedicated strategy, the
control resource. The contract claims the stage/control exclusively, applies
board electrical constraints, requires a nonzero cycle bound, and is always
independently streamed to core 1. A UI claim of qualified evidence cannot
promote a `Described`, `Compiles`, or `Bench` stage. Qualification is immutable
board-package evidence.

### FOC runtime and controller records

Kind `4` has selector zero and stores the normalized fixed-rate controller
snapshot:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 2 | nonzero pole-pair count |
| 10 | 2 | reserved zero |
| 12 | 4 | PWM carrier Hz |
| 16 | 4 | current-loop Hz |
| 20 | 2 | nonzero velocity-loop divider |
| 22 | 2 | nonzero position-loop divider |
| 24 | 4 | normalized maximum phase current, Q2.30 and exactly one |
| 28 | 4 | normalized maximum phase voltage, Q2.30 and exactly one |
| 32 | 32 | reserved zero |

The PWM rate must be an integer multiple of the current-loop rate. The physical
current and voltage facts define what normalized one means; accepting a second
arbitrary scale in the real-time record would create two authorities.

Kind `5` stores one fixed-period Q2.30 PI controller. Selector `1` is direct
current and `2` is quadrature current. Offsets 8, 12, 16, 20, 24, and 28 are,
respectively, proportional gain, integral gain per update, integral minimum,
integral maximum, output minimum, and output maximum; bytes 32–63 are zero.
Gains are nonnegative and both bound pairs are ordered. Cross-record lowering
also proves that every corner of the direct/quadrature output rectangle fits in
the normalized voltage circle.

### FOC rotor record

Kind `6` stores exact absolute-count calibration. Its selector is increasing
count direction `1` or decreasing direction `2`:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | counts per mechanical turn, at least two |
| 12 | 4 | count at the calibration reference, inside the modulus |
| 16 | 4 | wrapping `u32` binary electrical phase at the reference |
| 20 | 4 | maximum symmetric alignment error in binary-phase points |
| 24 | 4 | reduced count-error numerator |
| 28 | 4 | reduced nonzero count-error denominator |
| 32 | 4 | maximum sine/cosine interval width in Q2.30 ULPs |
| 36 | 4 | maximum squared-norm error in Q2.30 ULPs |
| 40 | 1 | measured `2` or qualified `3` evidence |
| 41 | 23 | reserved zero |

The count error is nonnegative, reduced, and represents zero only as `0/1`.
Declared-only rotor calibration rejects. Validation constructs the complete
digest-bound rotor mapping and evaluates a reference observation through the
certified outward rotation implementation under the stated precision policy.

### FOC current-channel records

Kind `7` stores channel zero (selector `1`) or channel one (selector `2`) of the
selected two-shunt pair:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 2 | maximum ADC code |
| 10 | 2 | inclusive valid-code minimum |
| 12 | 2 | inclusive valid-code maximum |
| 14 | 2 | selected zero-current code |
| 16 | 1 | increasing `1` or decreasing `2` code polarity |
| 17 | 1 | measured `2` or qualified `3` evidence |
| 18 | 2 | reserved zero |
| 20 | 4 | lower normalized-current-per-count endpoint, Q2.30 |
| 24 | 4 | upper normalized-current-per-count endpoint, Q2.30 |
| 28 | 4 | maximum additive normalized error, Q2.30 |
| 32 | 4 | maximum result interval width in Q2.30 ULPs |
| 36 | 28 | reserved zero |

Both rails are excluded, the valid window must bracket the zero code, gain is
positive, and additive uncertainty cannot omit half-count quantization. The
complete affine endpoints must remain inside normalized current limits.

### FOC PWM/ADC timing record

Kind `8` selects the physical two-shunt phase order: AB `1`, BC `2`, or CA `3`.
Channel zero is the first named phase and channel one is the second:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | device-cycle Hz |
| 12 | 4 | integer PWM-period cycles |
| 16 | 4 | nominal acquisition offset from period start |
| 20 | 4 | maximum trigger jitter cycles |
| 24 | 4 | maximum acquisition-aperture cycles |
| 28 | 4 | maximum interchannel-skew cycles |
| 32 | 4 | maximum conversion cycles after the last sample |
| 36 | 4 | minimum switching-edge guard cycles |
| 40 | 4 | maximum normalized current slew per device cycle, Q2.30 |
| 44 | 4 | maximum admitted interchannel-skew error, Q2.30 |
| 48 | 4 | normalized maximum phase current, Q2.30 and exactly one |
| 52 | 4 | maximum reconstructed phase-interval width in Q2.30 ULPs |
| 56 | 4 | PWM dead-time cycles |
| 60 | 1 | evidence, exactly qualified `3` |
| 61 | 3 | reserved zero |

The period and all timing bounds must fit one exact device-cycle lattice. The
dead time is nonzero, less than half a period, no larger than the switching
guard, and exactly equal to the axis `PwmDeadTimeSeconds` rational when divided
by device-cycle Hz. The device-cycle rate must equal PWM Hz times period cycles.
Two channel records, this timing record, and the selected two ADC bindings lower
as one validated current-calibration object; none is independently executable.

### FOC ADC-frontend records

Kind `9` stores channel zero (selector `1`) or channel one (selector `2`) of the
selected two-shunt pair:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 1 | programmed attenuation: 0 dB `1`, 2.5 dB `2`, 6 dB `3`, 11 dB `4` |
| 9 | 1 | evidence, exactly qualified `3` |
| 10 | 54 | reserved zero |

These are discrete classic-ESP32 ADC settings, not exact voltage-range claims.
The corresponding kind-`7` measured interval remains the sole mapping from ADC
code to normalized current. Both channel selections are mandatory and retained
with their matching calibrations; neither may come from a target default.

### FOC PWM-hardware record

Kind `10` has selector zero and stores the complete integer MCPWM clock and
compare-lattice selection:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | exact peripheral source clock Hz before division |
| 12 | 4 | exact center-aligned counter clock Hz after both dividers |
| 16 | 2 | integer up/down timer peak ticks |
| 18 | 2 | minimum active and inactive ticks from either rail |
| 20 | 4 | maximum admitted complete Q2.30 compare error in ULPs |
| 24 | 1 | raw zero-based peripheral prescaler |
| 25 | 1 | raw zero-based timer prescaler |
| 26 | 1 | evidence, exactly qualified `3` |
| 27 | 37 | reserved zero |

The source clock must equal `counter_clock × (peripheral_prescaler + 1) ×
(timer_prescaler + 1)` exactly. The counter clock must independently equal
`PWM Hz × 2 × timer_peak`; both representations must agree with the kind-`8`
device-cycle period. Timer peak is at least three, minimum pulse ticks are
nonzero and leave a nonempty interior compare domain, and the compare-error
budget is nonzero. Cross-domain integer products prove that the minimum pulse
duration is not shorter than the configured dead time. All arithmetic is
checked before a digest-bound
`PwmCompareContract` can exist.

### FOC cascaded-servo record

Kind `11` has selector zero and fills the complete payload with the portable
position/velocity cascade selected for the axis:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | position-error to normalized-velocity proportional gain, Q2.30 |
| 12 | 4 | velocity PI proportional gain, Q2.30 |
| 16 | 4 | velocity PI integral gain per velocity update, Q2.30 |
| 20 | 4 | velocity PI integral minimum, Q2.30 |
| 24 | 4 | velocity PI integral maximum, Q2.30 |
| 28 | 4 | velocity PI output minimum, Q2.30 |
| 32 | 4 | velocity PI output maximum, Q2.30 |
| 36 | 4 | symmetric maximum normalized velocity, Q2.30 |
| 40 | 4 | symmetric maximum normalized current-vector magnitude, Q2.30 |
| 44 | 4 | fixed normalized direct-current target, Q2.30 |
| 48 | 8 | maximum following error in unsigned Q31.32 position bits |
| 56 | 8 | maximum velocity-sample age in device cycles |

The position gain is nonnegative. Velocity and current limits are positive and
at most normalized one; the following limit is nonzero. The existing PI and
current-circle validators remain authoritative. Lowering also requires the
sample-age bound to fit one exact velocity period and to be no shorter than the
encoder's maximum observation latency.

### FOC encoder-scale record

Kind `12` has selector zero and stores the exact mechanical mapping used by the
portable absolute-count estimator:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 8 | signed Q31.32 position at the configured raw reference and zero turn |
| 16 | 8 | Q31.32 position bits per positive motor turn, numerator |
| 24 | 8 | Q31.32 position bits per positive motor turn, denominator |
| 32 | 8 | raw counts/second at normalized velocity one, numerator |
| 40 | 8 | raw counts/second at normalized velocity one, denominator |
| 48 | 1 | measured `2` or qualified `3` evidence |
| 49 | 15 | reserved zero |

Both ratios are positive and reduced to lowest terms; zero numerators or
denominators and declared-only evidence reject. The record does not invent a
multi-turn branch. Homing or retained-position policy must still supply the
boot-local signed turn seed before an estimator can emit physical position.

### FOC encoder-policy record

Kind `13` has selector zero and stores the complete timing, ambiguity, and
precision policy for the estimator:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 8 | 4 | exact device-cycle counter frequency |
| 12 | 8 | exact physical sample period in device cycles |
| 20 | 8 | maximum sample-to-availability latency in device cycles |
| 28 | 4 | maximum normalized velocity used for wrap selection, Q2.30 |
| 32 | 4 | maximum normalized velocity admitted to the servo, Q2.30 |
| 36 | 4 | symmetric velocity-estimation error, Q2.30 |
| 40 | 8 | maximum admitted Q31.32 position-interval width in ULPs |
| 48 | 4 | maximum admitted Q2.30 velocity-interval width in ULPs |
| 52 | 1 | evidence, exactly qualified `3` |
| 53 | 11 | reserved zero |

The clock, period, velocity bounds, estimator error, and width policies are
nonzero. Latency cannot exceed one period; admitted velocity cannot exceed the
trackable velocity; trackable velocity cannot exceed normalized one; and the
estimator error cannot exceed admitted velocity. The record's device clock must
equal the PWM/ADC synchronization clock, and its sample period must equal the
exact velocity-loop period derived from the kind-`4` divider grid.

## Cross-record admission

A stepper axis requires unique step and direction bindings plus exactly one
driver-control binding: `AxisEnable` means its active level enables the driver,
while `AxisDisable` means its active level disables the driver. Keeping those
roles distinct avoids interpreting an active-high StepStick disable line as an
enable with inverted intent. Step active/inactive timing means minimum pulse
high/low time; direction and driver-control active/inactive timing means setup
before the next rising step edge and hold after the preceding falling edge.
Each value is expressed in the declared device-cycle domain and must be nonzero.
One per-MCU step stream is limited to eight dense logical axes, matching the
canonical machine-IR mask and record width; a higher step-axis instance rejects
during configuration validation rather than disappearing from execution state.

The axis also requires full steps, microsteps, gearing, travel/revolution,
calibration, position range, velocity, acceleration, and jerk facts. A FOC axis
requires unique U/V/W resources, one supported absolute-encoder device or PCNT
resource, exactly the two ADC resources selected by its phase-pair record, one
qualified shutdown contract, and the same gearing, travel, calibration,
position, velocity, acceleration, jerk, and following-error facts used by the
authoritative browser machine model. It additionally requires pole-pair,
encoder-count, current/voltage-limit, carrier/dead-time/control-rate, shunt, and
current-gain facts.

Every FOC axis has exactly one runtime record, direct and quadrature
current-controller records, one rotor record, two current-channel records, one
PWM/ADC timing record, two ADC-frontend records, one PWM-hardware record, one
cascaded-servo record, one encoder-scale record, and one encoder-policy record.
Runtime pole pairs, encoder modulus, PWM/current-loop rates, and dead-time ratio
must equal their scalar authorities exactly. Phase and current binding rates
must cover the PWM and current loops, while encoder rate times the integer
velocity-loop divider must cover the current-loop rate. Stepper and FOC bindings
cannot describe the same logical axis. At most four complete FOC profiles are
retained in compact logical-instance order on core 1. The allocation-free
profile is compile-time bounded to 4,608 bytes; the V6 four-axis layout is 4,256
bytes on the verified host target. Position minimum must compare exactly below
maximum.

Nominal mechanical scalars select the exact runtime lattices; uncertainty is
retained separately for conservative safety bounds. Checked rational
cross-products require:

```text
position_bits_per_turn
    = 2^32 * travel_metres_per_output_turn
      / (motor_turns_per_output_turn * calibration_scale)

counts_per_second_at_velocity_one
    = velocity_limit_metres_per_second
      * encoder_counts_per_turn
      * motor_turns_per_output_turn
      * calibration_scale
      / travel_metres_per_output_turn
```

The stored Q31.32 following limit must be the exact floor of the conservative
lower endpoint of `AxisFollowingErrorMetres × 2^32`; the admitted position
observation width cannot exceed it. Encoder admitted velocity must equal the
servo velocity limit. The configured normalized estimator error must be at
least the following outward-rounded acceleration term:

```text
ceil_Q2.30(
    acceleration_limit_upper
    * sample_period_cycles
    / (2 * device_cycle_hz * velocity_limit_lower)
)
```

This is the minimum secant-to-newest-sample enclosure. Qualification must add
any timestamp, aperture, transport, or model error not already represented by
the raw-count uncertainty; firmware never infers those missing physical facts.

Optional FOC bus-voltage and fault bindings are retained rather than accepted
and discarded. A fault binding also appears in the canonical safety-input
profile and participates in the conservative local arm gate.

After the whole document passes length, canonical encoding, board capability,
semantic validation, and SHA-256 verification, core 1 may lower a retained FOC
slot. The combined `RealtimeConfiguration` cannot be assembled outside
`alumina-config`; this prevents callers from pairing a profile with a different
identity. Lowering injects that validated digest into `FocParameterSnapshot`,
`RotorCalibration`, `TwoShuntCurrentCalibration`, `PwmCompareContract`,
`ServoCascadeConfig`, and `ServoEncoderProfile`; it also constructs the exact
`ServoLoopGrid`. It revalidates every derived object, retains the three raw V6
outer-loop records for independent replay, and retains both exact ADC
attenuation selections and both raw MCPWM prescalers. Substituting either raw or
derived servo/encoder state is rejected. Lowering does not initialize ADC,
attach an MCPWM operator to a pin, establish the multi-turn seed, or create a
power stage.

After that complete replay, lowering can expose a `ServoFocAxisProfile` that
joins the same digest-bound inner and outer objects for the portable
complete-axis owner. This does not manufacture boot-local facts: the target
must still select the grid epoch, nonzero activation identity and turn seed,
stage the returned neutral image, and acknowledge its exact timer-zero commit
before the owner can become active.

MKS ESP32 FOC V1.0 target lowering additionally requires the configuration's
capability digest to equal the compiled package, matches U/V/W and both ADC
bindings to one exact schematic motor, requires the AB two-shunt pair and the
fixed 12-bit ADC1 range, and carries the stored source clock/prescalers into the
stopped MCPWM owner. Arbitrary callers can no longer construct that owner's
configuration. The board structurally selects phase-high-impedance shutdown,
but its current `Described` power-stage evidence deliberately rejects the
configuration before this target selection is reachable.
Only the later board package produced from measured both-off/reset/fault timing
may mark the stage `Qualified`; no fake pin, implicit alias, or compatibility
shim can bypass that gate.

Motion policy requires at least one complete stepper or FOC axis and a local,
arm-required E-stop or safety-interlock binding. The FOC policy bit is present
exactly when a FOC axis exists. The header's realtime-record count must equal the
independently derived count. These are structural admission rules, not claims
that the board or machine has passed HIL.

## Storage selection requests

`ConfigurationValidate` refers only to an already verified immutable storage
publication. Its 96-byte `ALMCFQ01` body contains version/reserved bytes, a
nonzero boot-local transaction ID, configuration SHA-256, exact `u32` byte
length, and canonical chunk-manifest SHA-256. Object kind and SHA-256 algorithms
are implicit and fixed; opaque/job objects cannot be reinterpreted.

`ConfigurationCommit` and `ConfigurationRollback` use a 64-byte `ALMCFS01`
selection containing version/reserved bytes, transaction ID, configuration
digest, and length. The outer native-frame configuration digest must bind the
same candidate/active identity. Validation, commit, and rollback are distinct;
validation alone never changes outputs or active configuration.

## Independent core transfer

Core 0 opens the exact typed publication, verifies each storage chunk, hashes
and semantically validates the complete document, and emits ordered core
commands. Every command has a 64-byte `ALCC` prefix containing core-wire version
`2`, action,
transaction ID, digest, total bytes, exact offset, and data length. `Data`
carries 1–192 bytes within the runtime's 336-byte command payload; the larger
boundary also carries a boot-bound cached-job prepare without changing this
configuration format.
Actions are begin, data, finish, activate, clear, abort, and authorize. Activate
installs the independently validated identity on core 1 but deliberately marks
it unauthorized. Authorize is a separate exact-identity command sent only after
core 0 has durably committed the matching selector. Other real-time actors may
consume only the authorized identity.

Core 1 accepts only contiguous identity-stable data, feeds the same allocation-
free stream validator, and cannot produce a candidate-valid report until exact
length, SHA-256, board capabilities, records, resources, and cross-record policy
all pass. Activate requires that exact candidate; clear requires the exact active
identity. A rejected candidate never removes an older active identity.

The fixed 128-byte `ALCR` report fills one telemetry payload and carries state,
transaction/candidate identity, consumed bytes, compact validated summary,
fault family, the independent active digest/length, and a one-bit durable-
authorization state. This allows a rejected or receiving replacement candidate
to be reported without hiding the older configuration that remains active.
Ordinary report loss is handled by periodic replay, including persistent
`Cleared` reporting until the next operation; malformed configuration data never
uses the urgent safety channel as an activation mechanism.

## Firmware transaction and status

All configuration requests use the authenticated canonical native-control
route. `ConfigurationGet`, successful requests, and lifecycle errors return the
same fixed 264-byte `ALMCST01` body. It joins the core-0 phase/fault/progress,
operation and committed identities, compact core-0 summary, durable-prepared and
job-authorization flags, and the complete latest `ALCR` report. Reserved bytes,
unknown flags, impossible identity shapes, and an `Active` phase without exact
durable authorization reject canonically.

Activation is ordered as follows:

1. core 0 opens the exact typed publication and independently streams, hashes,
   and validates it;
2. core 1 receives the same bytes, independently validates them, and reports the
   exact candidate;
3. an authenticated commit request appends and syncs a durable prepare record;
4. core 1 activates that candidate but revokes its job identity;
5. core 0 observes the exact unauthorized active report and durably commits the
   selector; and
6. core 0 sends `Authorize`, after which both job actors receive the exact digest.

Rollback of an uncommitted candidate sends `Abort` and durably removes any
matching prepared transition without changing the old active selector. Rollback
of the exact active publication means clear: core 0 first prepares the clear,
core 1 clears and enters `Safe`, and only then does core 0 commit the empty
selector. Clear is idempotent when core 1 is already empty, which permits safe
recovery from a committed selector whose stored bytes cannot pass boot
validation; a different nonempty core-1 active identity still rejects.
Configuration and external storage mutations are serialized, active
selection prevents destructive cache reprovisioning, and all lifecycle writes
require a fresh safe/configured-and-idle observation.

At boot, core 0 replays the selector journal, durably aborts an orphaned prepare,
then reopens and streams any committed publication through both validators.
Core 1 activates it unauthorized and receives authorization only after the
replayed durable identity and both validation results agree. A failure after
core-1 activation but before media commit therefore cannot admit a job; a
failure after media commit but before authorization recovers from the committed
selector and likewise remains closed until revalidation finishes.

## Current implementation boundary

The canonical V6 format, SD publication reader, dual independent validators,
core framing, authenticated firmware routing, boot recovery, safe-state
transitions, executable safety/stepper/FOC profiles, digest-bound inner-current,
servo-grid, cascade, and encoder-profile lowering, job-identity handoff, and
raw-media two-phase selection journal are implemented. There is deliberately no
V5 decoder, adapter, or negotiation path. The portable monitor consumes its
safety profile with exact-cycle debounce, polarity, first-stale-cycle
watchdogs, arming facts, and typed transitions; TinyBee target GPIO sampling is
present but remains physically unqualified.
Activation, abort, and clear replay as complete fail-closed states across every
injected write/sync cut. Both current board packages remain explicitly
non-armable pending physical qualification, so a successfully committed
configuration still cannot make `JobPrepare` executable on any image. No
physical-board lifecycle, multi-turn seed/homing owner, encoder transport task,
closed cascaded-current target task, physical runtime stack-watermark result,
or Wi-Fi/SD concurrency claim is made by this software checkpoint.
