# M10 authenticated live telemetry worker and UI evidence

Date: 2026-08-15

Status: the production browser control worker now creates a capability-selected
latest-only telemetry subscription, drives its lifecycle and retry-safe event
poll through the existing origin-bound HMAC/native HTTP route, independently
validates each complete canonical event, and publishes bounded passive input
history to the live-MCU UI. A fresh loopback Chromium run reached events 1 and
2. A replacement worker on the unchanged simulator boot first recovered the
retained exact event 2 and then advanced to event 3. This is authenticated
software/browser simulation evidence only; every physical radio, MCU input,
timing, output, motor, and safety claim remains closed.

The coordinated source checkpoints are:

- `alumina-firmware` `49df459d5a33e13d4f40955e37615b7557c01b22`;
- `alumina-interface` `c94f4c1d8565420fcd081c76ae752a59f0de4209`.

## Canonical retry-safe poll

`TelemetryPoll` is assigned operation `0x0705` in the telemetry frame family.
Its fixed 72-byte `ALMTPR01` body contains:

- the nonzero subscription ID;
- the SHA-256 of the complete canonical `ALMTLS01` request;
- the newest event sequence completely validated by the caller, or zero; and
- reserved zero bytes checked again by canonical round trip.

The fixed service response budget is now 512 bytes. After the 56-byte native
frame and 16-byte message headers, the 440-byte operation-body slot admits the
complete 432-byte four-input `ALMTEV01` event with eight bytes of headroom.
Waveform range envelopes deliberately retain their prior 312-byte/168-payload
bound; enlarging telemetry did not silently alter retained-capture chunking.

The service keeps one latest-only event slot. A poll may acknowledge only:

- the exact currently retained sequence; or
- the already acknowledged sequence, making a lost empty acknowledgement
  response idempotent.

Sequence zero makes no acknowledgement claim. It is valid even after earlier
events were acknowledged, so a newly reconstructed caller can reattach to the
same exact subscription without consuming evidence it never saw. Any other
sequence conflicts. A provider can borrow the admitted request and next
sequence only while the session is active and no event is pending. Unit tests
prove identical lost-event replay, lost-ack replay, zero-claim reattachment,
strict next-event progress, and rejection of an impossible future sequence.

## Browser client and worker authority

`TelemetrySubscriptionMachine` now owns the complete immutable request and
emits `TelemetryPoll` only while active. Its acknowledgement field comes only
from the newest complete event that passed independent envelope, overview,
context, resource-list, digest, sequence, drop, snapshot-cycle, and requested
minimum-period validation. Fetch ambiguity abandons both HMAC and diagnostic
pending state while leaving accepted evidence unchanged; the next poll is
byte-identical.

Worker schema v5 adds:

- telemetry lifecycle and exact subscription ID/digest;
- newest accepted event sequence and cumulative device-side replacement count;
- an independent bounded telemetry failure/error channel; and
- `WorkerTelemetryDocument`, containing the complete canonical subscription
  and event bytes with no credential.

The production worker waits for strict public identity, authenticated boot and
clock evidence, and a complete matching `ALMCAP02` document. It then chooses the
first four sorted resources whose exact graph access is
`StableBooleanInput`, constructs a zero-configuration latest-only subscription
with a ten-hertz requested minimum period, and advances one telemetry operation
after each successful heartbeat. The UI cannot supply a raw pin, diagnostic
context, subscription identity, event capacity, or acknowledgement sequence.

The schema validator decodes the request and event after JSON transfer. The
rendering supervisor decodes them again and binds them to the current connection
generation, public device ID, authenticated boot/frequency, capability identity,
subscription ID/digest, snapshot progress, and each resource's stable Boolean
input authority. It keeps at most 64 exact canonical events and erases stale
history on lifecycle, generation, identity, boot, capability, subscription, or
disconnect changes.

The live panel displays exact subscription reference, event/drop/failure state,
simulated versus device provenance, sample quality, Boolean value, captured
cycle, and age. It projects the retained device-cycle history into sampled
digital logic lanes. Screen coordinates are deliberately lossy; canonical
event bytes and cycles cannot flow back into control, and this path has no
lease, write, arm, motion, process-energy, or safety-reset operation.

## Opt-in simulator provider

`alumina-sim::diagnostics::simulated_resource_overview` mirrors the admitted
context, exact requested resource order, provider sequence, and supplied device
snapshot cycle. It produces deterministic alternating Boolean values and marks
the overview and every sample simulated. The ordinary authenticated fixture
keeps this provider disabled. Only the standalone `alumina-sim-http` binary opts
in, and production service validation still owns subscription admission and
event encoding.

No ESP board composition attaches a telemetry provider. TinyBee and T-Deck Pro
retain zero diagnostic event storage and provider policy `NONE`, so their
current images honestly return `Unsupported`. The larger host response budget
does not create a physical sampler or claim internal-SRAM qualification.

## Fresh and replacement-worker Chromium evidence

The finalized optimized interface bundle was served from `127.0.0.1:8097`; the
authenticated simulator listened on `127.0.0.1:8098`. The harness opened
`tests/browser/worker-clock-harness.html?expect=telemetry` directly, so the first
worker was not preceded by another application session. Chromium 147 used the
production module worker and shared optimized WASM artifact. The connected bare
MKS TinyBee V1.0 was not contacted. No command changed, disconnected, or
reassociated the workstation WLAN; every Alumina application endpoint in these
runs was loopback-only.

The harness rejects any document unless it has:

- worker schema v5 and the current connection/generation;
- exact `ALMTLS01` and `ALMTEV01` magic and declared lengths;
- a 176-byte four-resource subscription and 432-byte complete event;
- matching subscription IDs;
- matching nonzero event and overview sequences;
- exactly four complete samples; and
- monotone event sequence, drop count, and snapshot cycle.

The clean first run reached `passed` with:

- worker generation 1;
- seven accepted and zero rejected authenticated clock samples;
- the exact 3,435-byte TinyBee capability with digest
  `0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5`;
- active subscription ID 1 with digest
  `1f998df25f290f53e8e2652c313410e1164afcb3c127e85fae6bd5283312983b`;
- event 1 at device cycle `33,192,370`;
- event 2 at device cycle `33,603,628`;
- zero dropped events, zero telemetry failures, and no telemetry error; and
- available service/real-time health with a qualified clock interval.

Chromium was then terminated while the simulator process and boot remained
unchanged. A new browser profile started worker generation 1 and reconstructed
the identical subscription. Its first document was retained event 2 at the
exact same cycle `33,603,628`; it then advanced to event 3 at cycle
`79,265,560`, again with zero drops and failures. This directly exercises the
zero-claim reattachment rule rather than resetting firmware state.

The harness inspects typed worker events and replacement snapshots, not pixels.
The live status and sampled-lane renderer compiled in the same bundle, but these
runs do not claim pixel-level layout or operator-usability review.

## Verification

At the source commits above:

- package-scoped `cargo fmt` and repository `git diff --check`: passed without
  formatting sibling Hyper repositories;
- `cargo test --locked --offline` in `alumina-firmware`: 541 tests passed, including 58
  `alumina-sim` tests;
- firmware warnings-denied Clippy and rustdoc with dependencies excluded from
  the warning policy: passed;
- all five board descriptors validated;
- `cargo xtask check --board ...` compiled TinyBee 8 MiB, TinyBee 4 MiB,
  T-Deck Pro, and MKS ESP32 FOC V1 for their declared Xtensa targets without
  flashing;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface`: 214 tests passed, including 59 client tests;
- native and `wasm32-unknown-unknown` workspace warnings-denied Clippy and
  rustdoc with dependency linting excluded: passed;
- `scripts/audit-source-policy.sh`: reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- optimized locked/offline Trunk build: passed;
- `wasm-tools validate`, gzip integrity, and Brotli integrity: passed; and
- both the clean and same-boot replacement Chromium telemetry expectations:
  passed.

Tool versions were Rust 1.88.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
147.0.7727.137, and Node.js 22.22.2.

The optimized browser artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 5,836,226 | `3873abaa919f58911220ebc0fe2b375fee885715fda2373dc2ebab37ffd92f77` |
| `alumina-interface_bg.wasm.gz` | 2,599,191 | `97f8947ca24534a84724365b38d57ca67d6e17f24c12fc530efbaa73fd019c02` |
| `alumina-interface_bg.wasm.br` | 2,066,348 | `dad9f9dda8c62b3352b5ab54691336e50e0c67499e618d4da4716f61eae8896a` |
| `alumina-interface.js` | 91,813 | `b163d9ad481fb494a805abf3bb3830b9307a95b160e7444ca61feebc4d2e7689` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

Independent gzip and Brotli decompression both reproduced the uncompressed
WASM SHA-256 above.

## License and moving-Hyper boundary

All implementation is repository-owned under the repositories' existing
permissive licenses. No manifest or lockfile changed. No registry dependency,
copied vendor implementation, asset, or GPL/AGPL/LGPL/SSPL-family source was
introduced. The native/WASM inventory reported neither missing license metadata
nor a forbidden license; configured `cargo-deny` CI remains the release
authority, and no local `cargo deny` result is claimed.

Full interface/WASM verification compiled the current sibling CSGRS/Hyper
worktrees. Hypercurve remained a user-owned, actively edited dependency boundary
throughout. This checkpoint did not edit, format, reset, pin, stage, commit, or
inspect its diffs. One bundle attempt intersected a transient sibling edit;
without touching that repository, a later coherent dependency state built and
passed. Artifact hashes identify this integration run's bytes, not a frozen
Hyper source checkpoint.

## Claims deliberately kept closed

This checkpoint does not establish:

- browser-to-ESP Wi-Fi, AP association, physical CORS/radio behavior, or
  background-tab reliability;
- physical TinyBee identity, GPIO levels, sample timing, interrupt behavior,
  stack use, queue occupancy, or core placement;
- RMT/PCNT/DMA/software-sampling attachment or overload behavior;
- comparison with the available SLogic16U3, DSO, multimeter, or webcam;
- physical telemetry/capture providers or authenticated WebSocket performance;
- annotated-board photography or visual hotspot placement;
- diagnostic-output leasing or analog acquisition;
- motor, I2S/shift-register, MCPWM, ADC, SD, endstop, E-stop, or safety-chain
  behavior; or
- any machine arm, motion, process-energy, interlock-reset, or safety authority.

The connected bare MKS TinyBee V1.0 remained untouched, with no motor power or
motors connected. Physical Wi-Fi work remains postponed until the workstation
can associate with the board without interrupting the user's internet session.
