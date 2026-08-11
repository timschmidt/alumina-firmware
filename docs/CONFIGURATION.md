# Canonical machine configuration V1

`ALMCFG01` is the content-addressed machine/resource authority emitted by the
browser/WASM compiler and independently validated on both ESP cores. It is not
JSON, FluidNC configuration, G-code, a Rust memory image, or executable code.
The complete bytes are uploaded as storage object kind `MachineConfiguration`
(`6`) and remain inert until a separate safe, durable activation transaction.

SHA-256 over the complete document is the configuration digest carried by jobs,
commands, status, and both core-local active identities. The document embeds the
exact immutable `ALMCAP01` board digest, so a configuration cannot move silently
between revisions or capability/qualification changes.

## Common rules and header

Integers are little-endian. Reserved bytes are zero. Unknown flags, record
kinds, roles, facts, owners, polarities, or evidence values reject. Records are
fixed-width and strictly ordered by `(kind, instance, selector)`; duplicate keys
are consequently impossible. V1 admits 1–256 records and no trailing data.

The fixed 80-byte header is:

| Offset | Bytes | Meaning |
| ---: | ---: | --- |
| 0 | 8 | ASCII `ALMCFG01` |
| 8 | 2 | exact schema version `1` |
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
| 0 | 2 | kind: resource binding `1`, exact scalar `2` |
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
| 20–29 | FOC phase U/V/W, current A/B/C, bus voltage, encoder, enable, fault |
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
| 19–23 | current-sense ohms, current-sense V/A, safety reaction seconds, process duration seconds, timer tick Hz |

All facts except signed position bounds are positive. The browser can therefore
retain Hyper exact values through CAM and emit a reduced rational only at this
explicit hardware boundary; measured uncertainty remains a separate exact
bound rather than being folded into an approximate nominal.

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
calibration, position range, velocity, acceleration, and jerk facts. In the
current version, a FOC axis requires unique U/V/W and `FocEnable` resources plus
pole pairs, current/voltage limits, carrier/dead-time, and control rate. Position
minimum must compare exactly below maximum.

That dedicated-enable rule is intentionally too strict for the MKS ESP32 FOC
V1.0 schematic, which establishes no independent enable. The board remains
non-armable and rejects FOC configuration. A future configuration version must
replace the mandatory enable with an explicit, board-qualified shutdown
contract: either a dedicated disable/enable path or a measured phase-input safe
state with named reset, latency, and fault behavior. It must not preserve the
old shape through a fake pin, alias, or compatibility shim.

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
commands. Every command has a 64-byte `ALCC` prefix containing version, action,
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

The canonical format, SD publication reader, dual independent validators, core
framing, authenticated firmware routing, boot recovery, safe-state transitions,
executable safety-input profile, job-identity handoff, and raw-media two-phase
selection journal are implemented. The portable monitor consumes that profile
with exact-cycle debounce, polarity, first-stale-cycle watchdogs, arming facts,
and typed transitions; target GPIO sampling remains a later hardware gate.
Activation, abort, and clear replay as complete fail-closed states across every
injected write/sync cut. Both current board packages remain explicitly
non-armable pending physical qualification, so a successfully committed
configuration still cannot make `JobPrepare` executable on either image. No
physical-board lifecycle, runtime stack watermark, or Wi-Fi/SD concurrency claim
is made by this software checkpoint.
