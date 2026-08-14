# Canonical diagnostic evidence

Alumina separates low-rate status from explicit high-rate acquisition. Both
records are evidence only. Neither record creates a resource allocation,
diagnostic-output lease, graph access, command, arm transition, or physical
safety claim.

## Resource overview (`ALMOVW01`)

The overview is an exact bounded snapshot of explicitly present resource
records. Omitted resources have no implied value. Its fixed header binds stable
device identity, nonzero boot identity, the complete board-capability byte
length and digest, the active configuration digest, the integer `DeviceCycle`
clock frequency, snapshot cycle, sequence, and simulated/physical provenance.

Every strictly ordered typed-resource record carries its capture cycle,
provenance (`Measured`, `Latched`, `Inferred`, `LastCommanded`, or `Simulated`),
semantic quality (`Valid`, `Stale`, `Unavailable`, or `Faulted`), orthogonal
quality flags, and an exact scalar. Exact rational scalars must be reduced and
use a positive denominator. An unavailable value has one canonical zero
representation.

The decoder is `no_std`, allocation-free, bounded by caller-selected byte and
record ceilings, and rejects unknown flags, reserved bytes, duplicate or
unordered resources, future sample cycles, simulated/physical provenance
contradictions, and noncanonical values.

## Digital capture (`ALMDIG01`)

The first acquisition record is an edge-oriented digital capture. Its header
repeats the complete diagnostic identity and binds a nonzero capture ID, exact
retained `[start, end)` cycles, requested pre/posttrigger cycles, trigger
channel/edge/cycle/transition, terminal state, fixed capacity and stride, and
overflow, truncated-window, decimation, clock, discontinuity, and
software-sampling quality flags.

Channels are strictly ordered typed resources with initial level, acquisition
source (`RMT`, `PCNT`, DMA, bounded software sampling, external analyzer, or
simulator), and inversion/debounce annotations. Transitions are strictly
ordered by `(cycle offset, channel)`, must change the prior logical level, and
must identify the declared trigger exactly. The decoder is allocation-free and
applies fixed caller ceilings before a UI may allocate.

Analog sampled-waveform records are deliberately not conflated with this edge
format. They will need explicit integer/rational units, acquisition aperture,
ADC trigger timing, calibration identity, uncertainty, min/max-envelope
decimation, and saturation/rail semantics.

## Current offline fixture

`alumina-sim::diagnostics::tinybee_diagnostic_fixture` creates deterministic
records for the four graph-readable TinyBee inputs (GPIO22, GPIO32, GPIO33, and
GPIO35). It binds the current 8 MiB TinyBee capability identity, uses a 1 MHz
simulated clock, and emits a 320-byte overview with four Boolean records and a
512-byte capture with four channels, fourteen transitions, and a GPIO33 rising
trigger at offset 500 cycles. The capture retains `CLOCK_UNQUALIFIED`.

The fixture opens no serial port, network interface, GPIO, or physical I/O. The
browser independently decodes both byte strings, requires their complete
contexts to match each other and the board explorer, rejects unknown typed
resources, and only then owns bounded vectors for presentation. Its ledger,
selected-resource card, and digital plot all resolve through the same
`ResourceId`. Screen coordinates are lossy projections of retained integer
cycles and never flow back into a command or canonical record.

## Dynamic simulator acquisition

`simulated_immediate_waveform_capture` builds a new deterministic record from
the exact configuration currently retained in the service's `Armed` phase. The
portable service exposes that configuration through a narrow provider-only
borrow; it remains inaccessible to the network caller. The simulator accepts
only immediate, zero-pretrigger requests and mirrors the requested context,
capture ID, ordered channels, duration, capacity, and trigger deadline. It
generates at most four transitions per channel, encodes `SIMULATED` and
`CLOCK_UNQUALIFIED`, and submits the result back through the ordinary service
record validator before it can be downloaded.

The HTTP fixture keeps this provider disabled by default so tests must opt in.
The standalone `alumina-sim-http` binary enables it for production browser
worker integration. This is a host simulation provider, not an ESP peripheral
backend or a physical waveform claim.

## Authenticated transport checkpoint

Canonical `TelemetrySubscribe`/`Status`/`Event` and
`WaveformConfigure`/`Arm`/`Status`/`Read`/`Chunk` bodies now bind these records to
the complete service-owned context and exact request/record digests. A bounded
core-0 owner implements latest-only replacement accounting, idempotent retry,
fixed capture retention, explicit live-chunk acknowledgement/drop, and
side-effect-free range recovery. The typed interface client reconciles ambiguous
mutations and validates the complete capture before exposure; a localhost
HTTP/HMAC/native-frame test exercises the same dispatcher.

See [`DIAGNOSTIC-TRANSPORT.md`](DIAGNOSTIC-TRANSPORT.md). Hardware compositions
currently report both evidence providers as unsupported. The production browser
worker can now complete one-shot capture through the opt-in host simulator;
physical TinyBee sampling/capture, WebSocket event delivery, Wi-Fi/AP transport,
and SLogic comparison remain HIL gates rather than simulator claims.
