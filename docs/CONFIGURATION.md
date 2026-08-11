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
| 28 | 4 | local watchdog cycles; nonzero for hazardous/timed output |
| 32 | 32 | reserved zero |

Binding-role values are:

| Range | Roles in numeric order |
| --- | --- |
| 1–9 | axis step, direction, enable, minimum limit, maximum limit, encoder A, encoder B, encoder index, motor fault |
| 10–19 | probe, E-stop, safety interlock, digital input, digital output, analog input, PWM output, serial port, timer, counter |
| 20–29 | FOC phase U/V/W, current A/B/C, bus voltage, encoder, enable, fault |
| 30–37 | process output, storage, I2C bus, SPI bus, TWAI bus, capture input, waveform output, fitted device |

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

A stepper axis requires unique step, direction, and enable bindings plus full
steps, microsteps, gearing, travel/revolution, calibration, position range,
velocity, acceleration, and jerk facts. A FOC axis requires unique U/V/W and
enable resources plus pole pairs, current/voltage limits, carrier/dead-time, and
control rate. Position minimum must compare exactly below maximum.

Motion policy requires at least one complete stepper or FOC axis and a local
E-stop or safety-interlock binding. The FOC policy bit is present exactly when a
FOC axis exists. The header's realtime-record count must equal the independently
derived count. These are structural admission rules, not claims that the board
or machine has passed HIL.

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
carries 1–192 bytes, filling at most the runtime's 256-byte command payload.
Actions are begin, data, finish, activate, clear, and abort.

Core 1 accepts only contiguous identity-stable data, feeds the same allocation-
free stream validator, and cannot produce a candidate-valid report until exact
length, SHA-256, board capabilities, records, resources, and cross-record policy
all pass. Activate requires that exact candidate; clear requires the exact active
identity. A rejected candidate never removes an older active identity.

The fixed 128-byte `ALCR` report fills one telemetry payload and carries state,
transaction/candidate identity, consumed bytes, compact validated summary,
fault family, plus the independent active digest/length. This allows a rejected
or receiving replacement candidate to be reported without hiding the older
configuration that remains active. Ordinary report loss is handled by periodic
replay; malformed configuration data never uses the urgent safety channel as an
activation mechanism.

## Current implementation boundary

The portable canonical format, SD publication reader, dual independent
validators, core framing, and lifecycle actor are implemented. Firmware routing,
durable two-phase active selection, boot recovery, safety-state transition, and
job-service identity handoff are the next gate. Until that gate is complete,
both firmware job services retain a zero active identity and no configuration
can enable job preparation.
