# Authenticated diagnostic transport V1

`alumina-diagnostics::transport` carries canonical `ALMOVW01` resource
overviews and `ALMDIG01` digital captures through the existing authenticated
native protocol. The transport is evidence-only. It grants no pin ownership,
output lease, command authority, arm transition, or safety reset.

All records use exact little-endian integers, reject nonzero reserved bytes and
unknown flags, and consume the supplied body exactly. Every session repeats the
complete `DiagnosticContext`: stable device ID, nonzero boot ID, complete
capability document length and SHA-256, active configuration SHA-256, and
integer device-clock frequency. Core 0 compares that context and the outer
frame configuration digest with its own authority before admitting a session.
A boot or configuration change atomically drops subscriptions, pending events,
captures, and retained bytes.

## Telemetry lifecycle

| Operation | Body | Result |
| --- | --- | --- |
| `TelemetrySubscribe` (`0x0701`) | variable `ALMTLS01` request | fixed 120-byte `ALMTST01` status |
| `TelemetryUnsubscribe` (`0x0702`) | fixed 56-byte `ALMTLR01` reference | fixed status |
| `TelemetryEvent` (`0x0703`) | variable `ALMTEV01` event | device-originated event, no response |
| `TelemetryStatus` (`0x0704`) | fixed session reference | fixed status |

The subscribe header is 160 bytes followed by strictly increasing four-byte
typed resource IDs. It binds a nonzero subscription ID, full context, minimum
period in device cycles, maximum complete event bytes, and the mandatory V1
`LATEST_ONLY` policy. Its SHA-256 is the immutable subscription identity used
by every later reference, status, and event.

An event has a 112-byte envelope followed by one complete overview. It repeats
the subscription ID and digest, event sequence, cumulative drop count, overview
length, and overview SHA-256. Decoding independently validates the overview,
requires its context and sequence to match, and requires its resources to equal
the subscription exactly.

The service retains at most one unpublished event. Replacing that slot increments
the cumulative drop count before encoding the replacement. Produced events are
accounted exactly as:

```text
next_sequence - 1 = published + dropped + pending(0 or 1)
```

Production before the admitted minimum period is rejected. Sending does not
clear the slot; only acknowledgement of the exact current sequence does. This
makes loss and retry observable without allowing an unbounded queue.

## Digital-capture lifecycle

| Operation | Body | Result |
| --- | --- | --- |
| `WaveformConfigure` (`0x0c01`) | variable `ALMWCF01` request | fixed 144-byte `ALMWST01` status |
| `WaveformArm` (`0x0c02`) | fixed 64-byte `ALMWRF01` reference | fixed status |
| `WaveformChunk` (`0x0c03`) | variable `ALMWCH01` live event | device-originated event, no response |
| `WaveformStop` (`0x0c04`) | fixed session reference | fixed status |
| `WaveformStatus` (`0x0c05`) | fixed session reference | fixed status |
| `WaveformRead` (`0x0c06`) | fixed 112-byte `ALMWRD01` range | one `ALMWCH01` range envelope |

The configure header is 192 bytes followed by strictly increasing channel
resource IDs. It binds a nonzero capture ID, complete context, required edge
timestamp semantics, explicit permission for any software-sampling fallback,
pre/posttrigger intervals, inclusive earliest/latest trigger cycles, fixed
transition capacity, maximum chunk bytes, and exact trigger channel/edge. Its
SHA-256 is the immutable configuration identity.

Core 0 admits a configuration only when the worst-case complete record
(`224 + 16 * channels + 16 * transition_capacity`) fits the composition's fixed
retention buffer. Arm assigns a nonzero monotonic generation. A producer may
publish only a complete independently decoded capture whose context, capture
ID, resource order, requested windows, trigger, capacity, and software-sampling
permission match the configuration. Status exposes lifecycle, arm/trigger
cycles, complete record length/SHA-256, live cursor, drop count, and quality
flags.

Each chunk has a 144-byte envelope binding capture ID, configuration digest,
complete record length/digest, offset, range length, range digest, and canonical
final-range flag. The current 384-byte service response leaves 312 bytes for an
operation body, so native range payloads are at most 168 bytes. Live delivery
advances only after exact acknowledgement or an explicit drop transition;
dropped live ranges remain recoverable with side-effect-free `WaveformRead`.
The client accepts ranges only at its contiguous expected offset and validates
the complete capture and SHA-256 before exposing it.

## Fixed composition budgets

The portable codecs are allocation-free and caller-bounded. The initial native
policy admits at most 1,076 request-body bytes, 229 telemetry selectors, 221
waveform selectors, 65,536 requested transitions, a 4 MiB abstract retained
record ceiling, and 168 bytes per range. A concrete service may be smaller.

The current TinyBee and T-Deck Pro firmware compositions deliberately use zero
request, event, configuration, and record storage with provider policy `NONE`.
They retain the authenticated dispatcher and context reconciliation, but
subscribe and configure return `Unsupported` rather than reserving scarce
internal SRAM for evidence that cannot yet arrive. Installing a physical board
resource sampler or capture owner must simultaneously select explicit nonzero
composition budgets and qualify the linked release image. The deterministic
host fixture uses fixed 176-byte subscription, 432-byte event, 208-byte
configuration, and 2,048-byte record budgets with `SIMULATED` providers and
labels every document/sample/source accordingly.

## Verification boundary

Host tests cover canonical round trips, hostile lengths/flags/order, context
substitution, digest tampering, exact lifecycle accounting, minimum-rate
enforcement, idempotent mutation retry, ambiguous-response reconciliation,
latest-only loss, live-chunk loss, range retry, full-record validation, and a
real localhost HTTP/HMAC/native-frame exchange. TinyBee and T-Deck Pro target
checks prove composition only.

No physical Wi-Fi, AP association, serial link, GPIO, SLogic capture, or board
reset is part of this checkpoint. Physical acquisition and WebSocket event
delivery remain explicit HIL/integration gates.
