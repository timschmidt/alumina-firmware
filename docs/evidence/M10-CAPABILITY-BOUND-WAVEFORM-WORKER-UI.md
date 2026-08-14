# M10 capability-bound waveform worker and live-MCU UI evidence

Date: 2026-08-14

Status: the production browser control worker can now construct, authenticate,
reconcile, download, and present one capability-selected digital input capture.
The standalone host simulator supplies deterministic acquisition only after the
ordinary diagnostic service has admitted an explicit input-only arm request.
The rendering realm independently binds the complete canonical record back to
the current device, boot, board capability, zero configuration, clock,
capture-attempt identity, and stable Boolean input authority. A loopback-only
Chromium run completed this lifecycle. This is software/browser simulation
evidence; every physical MCU, radio, GPIO, timing, analyzer, motor, output,
machine-arm, and safety claim remains closed.

The coordinated implementation checkpoints are:

- `aluminafw` `c4e8e94cc6989afce73bd59c2322c104ccf0a77b`;
- `alumina-interface` `ddbcce10cd59b89e12b001d6269079b96849b10b`.

## Narrow simulator provider seam

`DiagnosticServiceState::armed_waveform_configuration` exposes the validated
configuration only while the private service owner is in `Armed`. This is a
provider borrow, not a network response or client capability. Existing status,
range, stop, context, identity, and record validation remain in the production
service.

`alumina-sim::diagnostics::simulated_immediate_waveform_capture` accepts that
view and:

- rejects every trigger shape except immediate with zero pretrigger and a
  nonzero posttrigger window;
- chooses a start inside the exact admitted trigger deadline;
- mirrors device, boot, capability, configuration, frequency, capture ID,
  ordered channels, requested duration, and transition capacity;
- generates at most four deterministic level transitions per channel, strictly
  ordered within `[start, end)`;
- marks the record `SIMULATED` and `CLOCK_UNQUALIFIED`; and
- encodes one canonical `ALMDIG01` record that must pass the ordinary service's
  complete retained-record validator.

The authenticated HTTP fixture keeps this provider disabled by default. The
standalone `alumina-sim-http` binary opts in so the production browser worker can
exercise configure, diagnostic arm, completion status, and exact range reads.
A unit test admits a real 2,000-cycle four-channel configuration through the
service, borrows it only after arm, produces 16 transitions, retains the record,
and decodes every mirrored fact.

No ESP board composition attaches this provider. The firmware diagnostic
composition still reports waveform acquisition unsupported until a separately
qualified hardware task owns an admitted peripheral.

## Strict public identity boundary

The client now decodes `/api/v1/identity` into one bounded current schema. It
requires:

- protocol version 1 and no unknown JSON fields;
- a nonempty canonical lowercase board ID of at most 64 bytes;
- one known credential provenance whose derived production eligibility agrees
  with the reported Boolean;
- a nonzero fixed lowercase-hex 16-byte device ID; and
- a nonzero fixed lowercase-hex capability digest with a document length inside
  the interactive capability policy.

The browser fetch accepts firmware's streamed response without requiring a
`Content-Length`, but checks it when present and independently caps the received
body at 512 bytes.

This public endpoint is descriptive and is not trusted as machine authority.
The worker requires its board ID and capability identity to agree with the
authenticated complete capability bytes. The rendering supervisor later
requires its device ID to agree with the canonical capture context, alongside
the authenticated boot and clock facts. A changed stable identity resets clock,
health, capability, and capture state. Failed reconnect discovery retains the
last internally consistent evidence rather than publishing a capability with a
missing identity.

## Worker-owned capture authority

Worker schema v4 adds:

- a redacted public device/board/credential-provenance snapshot;
- a bounded capture command containing only connection ID, one through four
  strictly ordered canonical resource selectors, and exact duration cycles;
- capture phase, attempt ID, contiguous/total bytes, independent failure count,
  and bounded error in each replacement snapshot; and
- one credential-free `WorkerWaveformDocument` event containing complete
  canonical record bytes.

The UI cannot supply diagnostic context, trigger deadlines, arm horizon,
retention policy, chunk size, or capture ID. The worker derives those fields
from its current authority:

- public `DeviceId`, reconciled with signed board facts and later record
  context;
- authenticated `BootId`, latest frequency, and last transmit cycle;
- the complete authenticated board-capability identity and bytes;
- configuration digest zero;
- only resources with exact `StableBooleanInput` graph access;
- immediate trigger, zero pretrigger, and a duration no greater than two
  seconds at the reported frequency;
- 64 retained transitions and 168-byte range reads;
- a 30-second diagnostic arm horizon; and
- a nonzero capture ID composed from worker generation and capture sequence.

The browser adapter drives one operation from the transport-independent
`WaveformCaptureMachine`. HMAC/session construction failure and ambiguous fetch
abandon both corresponding pending states; the next operation follows the
machine's exact status/range reconciliation rules. A successful heartbeat may
drive at most eight waveform operations, preserving independent health and
capability bounds. The worker explicitly requests diagnostic arm after
configuration. That operation does not and cannot create machine arm, resource
lease, pin write, motion schedule, or process-energy authority.

Snapshot validation rejects capture state without a nonzero attempt ID, boot,
strict identity, and complete matching capability. It also rejects impossible
range progress, a complete record with missing bytes, or a complete state with
an outstanding capture failure. Unit tests cover request ordering/bounds,
identity/capability mismatch, phase/progress relationships, strict public
identity, and JSON-round-trip record tampering.

## Rendering-realm admission and UI

The worker decodes and validates the full record before publishing it once per
capture attempt. Schema v4 decodes it again and requires zero configuration.
The rendering supervisor decodes it a third time and requires exact agreement
with:

- current connection generation and capture ID;
- public device ID;
- authenticated boot ID and latest reported clock frequency;
- the admitted board document's capability identity; and
- every channel's presence and `StableBooleanInput` access in that document.

The live-MCU panel offers a repeated `Capture inputs (2 ms)` action over the
first four ordered admitted Boolean inputs. It reports lifecycle/range progress
and isolated failures, labels simulated evidence prominently, draws exact edge
lanes using capability aliases, and shows every channel level at an integer
hover cycle. Screen coordinates are lossy display projections only; retained
cycles never become a command.

A new capture identity removes the prior trace before acquisition. A reboot,
stable-identity change, capability reset/change, worker-generation change, or
disconnect removes both stale capture evidence and any incompatible retained
board document. This closes the same-generation stale-evidence case found
during review.

## Localhost Chromium evidence

The finalized optimized bundle was served from `127.0.0.1:8097`; the
authenticated simulator listened on `127.0.0.1:8098`. Chromium 147 ran with
background networking, component updates, default apps, extensions, and sync
disabled, and host resolution rejected every non-loopback name. The connected
bare MKS TinyBee V1.0 and workstation WLAN were not contacted.

`tests/browser/worker-clock-harness.html?expect=waveform` uses the production
module worker and schema v4. It reaches the capture command only after clock,
health, public identity, complete capability, and one-time capability-document
checks pass. It rejects a malformed/duplicate capture event, mismatched
generation, incomplete range progress, any capture error, or a record length
that differs from the complete snapshot.

The final run reached `passed` with:

- worker generation 1 and capture sequence 1;
- six accepted and zero rejected heartbeat samples, with a qualified exact
  cycle interval;
- available service/real-time health and zero health failures;
- public board ID `mks-tinybee-v1`, device ID `ALUM-SIM:TINYBEE`, and
  development-fallback credential provenance;
- the exact 3,435-byte TinyBee capability with digest
  `0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5`;
- GPIO22, GPIO32, GPIO33, and GPIO35 over a 2,000-cycle window at 1 MHz;
- one 544-byte `ALMDIG01` record with four channels, 16 transitions,
  `SIMULATED`, and `CLOCK_UNQUALIFIED`;
- exact range progress 544/544 bytes; and
- zero consecutive waveform failures and no waveform error.

The harness observes typed worker events rather than application pixels. The
optimized application and trace renderer compiled in the same bundle, but this
run does not claim pixel-level layout or operator-usability review.

## Verification

At the commits above:

- package-scoped `cargo fmt` and repository `git diff --check`: passed without
  formatting sibling Hyper repositories;
- `cargo test --locked --offline` in `aluminafw`: 540 tests passed, including
  57 `alumina-sim` tests;
- firmware default-member warnings-denied Clippy and rustdoc: passed;
- all five board descriptors validated;
- `cargo xtask check --board ...` compiled TinyBee 8 MiB, TinyBee 4 MiB,
  T-Deck Pro, and MKS ESP32 FOC V1 for their declared Xtensa targets without
  flashing;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface`: 212 tests passed, including 57 client tests;
- native and `wasm32-unknown-unknown` workspace warnings-denied Clippy: passed;
- native and WASM warnings-denied workspace rustdoc: passed;
- `scripts/audit-source-policy.sh`: reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- optimized locked/offline Trunk build: passed after expressing the host's
  `NO_COLOR` value in the Boolean form required by Trunk 0.21.14;
- `wasm-tools validate`, gzip integrity, and Brotli integrity: passed; and
- the final loopback Chromium `waveform` expectation: passed.

Tool versions were Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
147.0.7727.137, and Node.js 22.22.2.

The optimized browser artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 5,780,185 | `48b5779c6e294f19a7733f00b8a8e2e15a5da799077830aa90e6189e4e2ec3cb` |
| `alumina-interface.js` | 91,813 | `4199d5580ef6fc2dc1ec601dc176e738a80109062b552c01642f0f7ff764d3bd` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

The optimized WASM compressed to 2,577,372 gzip bytes and 2,050,788 Brotli
bytes.

## License and moving-Hyper boundary

All implementation is repository-owned and covered by the repositories'
existing permissive licenses. No manifest or lockfile changed. No registry
dependency, copied vendor implementation, asset, or GPL/AGPL/LGPL/SSPL-family
source was introduced. The native/WASM inventory reported neither missing
license metadata nor a forbidden license; configured `cargo-deny` CI remains
the release authority, and no local `cargo deny` result is claimed.

Full application/WASM verification compiled the current sibling CSGRS/Hyper
worktrees. Hypercurve remained a user-owned, actively edited dependency boundary
throughout this checkpoint. This work did not edit, format, reset, pin, stage,
commit, or inspect its diffs. Artifact hashes identify the bytes from this
integration run; they do not claim a frozen Hyper source checkpoint.

## Claims deliberately kept closed

This checkpoint does not establish:

- browser-to-ESP Wi-Fi, AP association, CORS, radio, or background-tab
  reliability;
- physical TinyBee identity, input levels, capture timing, interrupt behavior,
  stack use, queue occupancy, or core placement;
- RMT/PCNT/DMA/software-sampling hardware attachment or overload behavior;
- comparison with the available SLogic16U3 or DSO;
- live telemetry or authenticated WebSocket/event delivery;
- diagnostic-output leasing, analog acquisition, or annotated-photo coverage;
- motor, I2S/shift-register, MCPWM, ADC, SD, endstop, E-stop, or safety-chain
  behavior; or
- any machine arm, motion, process-energy, interlock-reset, or safety authority.

The connected bare MKS TinyBee V1.0 remained untouched, with no motor power or
motors connected. Physical Wi-Fi work remains postponed until the workstation
can associate with the board without interrupting the user's internet session.
