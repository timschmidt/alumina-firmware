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
| `TelemetryPoll` (`0x0705`) | fixed 72-byte `ALMTPR01` acknowledgement/fetch | retained event or empty success |

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

`TelemetryPoll` carries the immutable subscription ID/digest and the newest
event sequence the caller has completely validated. If that sequence is the
current retained event, the service acknowledges it before returning any newer
retained event. Repeating a poll after an ambiguous response is idempotent, and
an already acknowledged sequence remains valid. Sequence zero makes no
acknowledgement claim; a reconstructed page/worker can therefore reattach to the
same exact subscription and receive an event it has not yet seen. Any other
sequence conflicts instead of guessing caller progress.

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
final-range flag. The current 512-byte service response leaves 440 bytes for an
operation body and admits the 432-byte four-input telemetry event. Waveform
range envelopes deliberately retain their 312-byte bound, so native range
payloads remain at most 168 bytes. Live delivery
advances only after exact acknowledgement or an explicit drop transition;
dropped live ranges remain recoverable with side-effect-free `WaveformRead`.
The client accepts ranges only at its contiguous expected offset and validates
the complete capture and SHA-256 before exposing it.

## Fixed composition budgets

The portable codecs are allocation-free and caller-bounded. The initial native
policy admits at most 1,076 request-body bytes, 229 telemetry selectors, 221
waveform selectors, 65,536 requested transitions, a 4 MiB abstract retained
record ceiling, and 168 bytes per range. A concrete service may be smaller.

Current fixed composition budgets are:

| Composition | Request | Event | Waveform configure | Waveform record | Providers |
| --- | ---: | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 176 | 432 | 0 | 0 | four-input semantic overview |
| TinyBee V1.0, 4 MiB variant | 176 | 432 | 0 | 0 | four-input semantic overview |
| T-Deck Pro | 0 | 0 | 0 | 0 | none |
| MKS ESP32 FOC V1.0 | 0 | 0 | 0 | 0 | none |
| deterministic host fixture | 176 | 432 | 208 | 2,048 | simulated overview and capture |

For TinyBee, the 176-byte request is exactly the 160-byte subscription header
plus four selectors. The 432-byte retained event is exactly the 112-byte event
envelope plus a 320-byte four-sample overview. The provider consumes a distinct
112-byte internal `ALMRTI01` core-1 snapshot inside the existing 128-byte lossy
inter-core payload; it does not reuse or weaken the canonical safety snapshot.
Only configured stable-input semantics are translated. Waveform configure and
record storage remain zero, so every capture operation returns `Unsupported`.

T-Deck Pro and MKS ESP32 FOC retain the authenticated dispatcher and context
reconciliation, but subscribe and configure return `Unsupported` rather than
reserving scarce internal SRAM for evidence that cannot arrive. Adding another
board provider must select explicit board-local budgets and qualify the linked
release image. The host fixture labels every document/sample/source as
simulated.

## Verification boundary

Host tests cover canonical round trips, hostile lengths/flags/order, context
substitution, digest tampering, exact lifecycle accounting, minimum-rate
enforcement, idempotent mutation retry, ambiguous-response reconciliation,
latest-only loss, retained-event replay, zero-claim worker reattachment,
live-chunk loss, range retry, full-record validation, and a real localhost
HTTP/HMAC/native-frame exchange. The production browser worker and rendering
realm also pass fresh and same-boot replacement Chromium telemetry runs over
loopback. The TinyBee resource-overview provider additionally passes exact
core-1 snapshot codec/observer/event tests and linked checks for both flash
variants. T-Deck Pro and MKS ESP32 FOC still compile the zero-storage policy.
All target results prove composition only.

No physical Wi-Fi, AP association, serial link, GPIO, SLogic capture, or board
reset is part of this checkpoint. No TinyBee input value or timing result is
claimed until physical HIL independently reconciles the linked image and input
routes. A future authenticated WebSocket may carry the same canonical event
contract at higher rates; the current authenticated polling path is complete
without it.
