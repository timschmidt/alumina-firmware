# M10 authenticated capability worker and live-MCU UI evidence

Date: 2026-08-14

Status: the production browser control worker can now acquire the selected
MCU's complete immutable board-capability document through authenticated,
bounded native ranges. It keeps capability loss separate from clock and health
state, validates the complete canonical document before a one-time schema-v3
transfer, and the rendering realm validates it again before building the
board-name-independent explorer used by the live-MCU panel. A real
Chromium/localhost run covered both retained capability authority during an
independent health loss and an ambiguous first capability range followed by
exact retry and recovery. This is software/browser simulation evidence only;
every physical MCU, radio, annotated-photo, output, motion, and safety claim
remains closed.

The coordinated implementation checkpoints are:

- `aluminafw` `0f727a2b88b45d32448429cc730209e09bb7e5d8`;
- `alumina-interface` `8282e457797f2a8b6f54cb248159a99f5cdd033b`.

## One production capability dispatcher

`alumina-service::capability::CapabilityDocumentService` now owns the portable,
stateless `FrameKind::Capabilities` dispatcher. The firmware adapter supplies
its compile-time selected `BoardPackage`; `ClockHttpFixture` supplies the real
TinyBee package. The simulator therefore no longer duplicates or approximates
the production range semantics.

The dispatcher:

- accepts only canonical native `CapabilitiesGet` with configuration digest
  zero;
- decodes the fixed request, including its nonzero at-most-240-byte bound;
- accepts a zero digest only for initial discovery and otherwise requires the
  compiled declared digest;
- calls the same verified canonical document reader used by firmware;
- emits exact identity, offset, byte count, completion, and range bytes; and
- maps range, missing-digest, length/digest, and board failures to explicit
  fail-closed statuses.

The response body has a compile-time proof that its fixed prefix plus maximum
range fits the universal service response. The simulator's public identity
fixture now reports the same TinyBee board ID, document length, and capability
digest as its authenticated dispatcher. A signed HTTP fixture test decodes two
successive ranges from this shared service and proves stable identity and
contiguous offsets.

## Retry-safe browser acquisition

`alumina-interface-client::capability::CapabilityDownloadMachine` owns one
bounded acquisition independently from browser transport. Its exact phases are
`Discovering`, `Downloading`, and `Complete`. It:

- starts with expected digest zero and offset zero;
- freezes the first accepted complete identity;
- allocates only after checking the declared length against
  `BoardCapabilityLimits::interactive()`;
- accepts only the pending offset and at most the requested range length;
- advances only a contiguous prefix;
- requires stable identity on every later range;
- requires completion to coincide exactly with the declared byte length; and
- independently runs `decode_board_capability` and verifies the complete
  SHA-256 identity before exposing bytes or a borrowed capability view.

An ambiguous fetch abandons the pending transport request and capability range
without advancing the prefix. The HMAC counter is still spent, but the next
capability body contains the identical digest, offset, and maximum length. Unit
tests cover complete real TinyBee reassembly, byte-identical ambiguous retry,
identity substitution, declared-length rejection before allocation, and strict
success/failure body rules.

Both window and worker WASM adapters require the same zero-configuration
authenticated session used by clock and passive health. Request construction,
fetch/authentication, and capability semantic errors stay typed and separate.

## Worker schema v3 and isolation

Schema v3 adds to each replacement snapshot:

- exact capability phase;
- contiguous received byte count;
- optional complete length and 32-byte digest after discovery; and
- a separate consecutive-failure count and bounded error.

Snapshot validation rejects impossible phase/identity/length relationships,
zero identity, documents outside browser policy, unpaired errors, and a
`Complete` state that still carries a failure. Capability progress never
contains the document itself.

After a successful heartbeat, the worker may issue at most four 240-byte
capability reads. Health retains its independent one-through-60-second cadence.
A capability failure does not change clock phase, estimate, history, health
availability, or retained health evidence. Session reopening follows only the
typed authentication/HTTP failures that invalidate session confidence. A
validated boot change clears the old capability acquisition and one-time event
state.

At completion the worker constructs one `WorkerCapabilityDocument` for that
connection generation. Construction decodes and hashes the full bytes. The
event contains no credential and is emitted once per generation. After JSON
transfer, the rendering realm runs the strict envelope validator, decodes and
hashes the document again, rejects stale generations, and calls
`build_board_explorer_snapshot`. Disconnect removes both session and explorer.
A unit test mutates one JSON-round-tripped document byte and proves rejection.

The current JSON byte-array transfer is one-time and bounded by the 4 MiB
interactive document policy. This checkpoint does not claim acceptable
worst-case latency or memory expansion at that maximum; a transferable binary
representation remains a permissible later hardening step without changing the
canonical document or protocol.

## Visible immutable board facts

The live panel is now titled `Live MCUs` and distinguishes capability discovery,
download progress, and completion. Once rendering-realm admission succeeds it
shows exact explorer facts:

- board ID, revision, chip, qualification, and application-core count;
- service/real-time core assignment and flash/internal-SRAM/PSRAM bytes;
- total/service/real-time/hazardous/graph-addressable resource counts;
- alias count;
- licensed visual and reviewed hotspot counts;
- HIL requirement count; and
- the immutable package armable claim.

The panel explicitly says these facts grant no resource lease, output command,
arm transition, or physical-safety claim. The current TinyBee document contains
3,435 bytes with digest
`0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5`.
It describes 62 resources and 51 aliases, is non-armable, and publishes no
licensed visual; therefore the live explorer cannot invent a board photograph
or hotspot overlay.

The optimized application contains this panel and passed WASM compile/lint.
The browser harness below observes typed production-worker events rather than
application pixels, so this checkpoint does not claim a Chromium pixel or
operator-usability review of the panel itself.

## Localhost browser evidence

The finalized optimized bundle was served from `127.0.0.1:8097`; the
authenticated simulator listened on `127.0.0.1:8098`. Chromium 147 ran with
background networking, component updates, extensions, and sync disabled, plus
a resolver policy rejecting every host except `127.0.0.1`. The connected bare
TinyBee and workstation WLAN were not contacted.

The harness uses a 100 ms heartbeat and 1,000 ms health interval. It rejects a
wrong schema, malformed capability magic/length/digest, a duplicate capability
document, or any qualified snapshot that lacks matching complete capability
identity. Qualification requires at least five accepted heartbeats, ensuring
one additional worker completion after the document event so a repeated event
would be observed.

### Lost health response with retained capability

The simulator processed control request 2, then closed the connection without
returning the signed health response. The harness retained an intermediate
schema-v3 snapshot with:

- clock phase `clock_qualified`, six accepted samples, zero clock failures, and
  no clock error;
- health `unobserved`, one isolated health failure, and no fabricated payload;
- capability `complete`, all 3,435 bytes, the expected digest, zero capability
  failures, and its one matching document event.

The next due health attempt recovered at seven accepted clock samples. It
reported exact queues `0/8`, `0/8`, and `1/32`; service and RT stack flags
`0x000d`; two samples in each executor; service and RT minimum headroom 20,480
and 24,576 bytes; a fresh RT witness; and zero retained health failure. The
capability identity remained unchanged.

### Lost first capability range

In an independent process/browser profile, the simulator processed control
request 3—the first capability range—then closed the connection without its
signed response. The intermediate snapshot had:

- one accepted clock sample, phase `sampling`, zero clock failures, and no
  clock error;
- available exact health with zero health failures;
- capability `discovering`, zero received bytes, no accepted identity, one
  isolated capability failure, and a bounded capability fetch error.

The worker retried without advancing capability state. The harness reached
`passed` at five accepted clock samples with available health, all 3,435
capability bytes, the expected digest, zero capability failures, and exactly one
matching complete document event. The downloader's byte-identical retry unit
test supplies the direct request-body proof; the browser run supplies the real
Fetch/HMAC/CORS loss-and-recovery proof.

## Verification

At the commits above:

- `cargo fmt --all -- --check` and `git diff --check`: passed in both
  repositories;
- `cargo test --locked --offline` in `aluminafw`: 539 tests passed, including
  56 `alumina-sim` tests;
- firmware default-member warnings-denied Clippy and rustdoc: passed;
- all four board descriptors validated and `cargo xtask check --board ...`
  compiled TinyBee 8 MiB, TinyBee 4 MiB, T-Deck Pro, and MKS ESP32 FOC V1 for
  their declared Xtensa targets without flashing;
- `cargo test --workspace --all-targets --locked --offline` in
  `alumina-interface`: 209 tests passed, including 54 client tests;
- native and `wasm32-unknown-unknown` workspace warnings-denied Clippy: passed;
- direct WASM client/application checks and warnings-denied workspace rustdoc:
  passed;
- `scripts/audit-source-policy.sh`: reported `source policy: local
  Alumina/CSGRS/Hyper stacks; native and WASM license inventories accepted`;
- optimized locked/offline Trunk build: passed;
- `wasm-tools validate`, gzip integrity, and Brotli integrity: passed; and
- final Chromium health-loss and capability-range-loss expectations: passed.

The intentionally correct portable firmware test command uses workspace
default members. A diagnostic `cargo test --workspace --all-targets` invocation
also tries to compile ESP-HAL firmware bins for the Linux host and was rejected
by ESP-HAL as an unsupported target; the explicit per-board Xtensa checks above
are the relevant embedded build evidence.

Tool versions were Rust 1.97.0, Trunk 0.21.14, wasm-tools 1.235.0, Chromium
147.0.7727.137, and Node.js 22.22.2.

The optimized browser artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 5,680,843 | `d4fcbb514bae0fa9243318f791acdd1167b31f0458d39116d3e0a1c8d59def80` |
| `alumina-interface.js` | 91,813 | `1d395d7ccce5387c615625977b1abe995de103c7c12dd3eb9cba3f6d62e59d5d` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |

The optimized WASM compressed to 2,540,313 gzip bytes and 2,025,372 Brotli
bytes.

## License and moving-Hyper boundary

All new implementation is repository-owned and covered by each repository's
existing permissive license. Manifest/lock changes add only local
`alumina-board`, `alumina-capability`, and TinyBee test-package edges; no new
registry dependency, copied vendor implementation, asset, or GPL-family source
was introduced. The native/WASM inventory has neither missing license metadata
nor a GPL/AGPL/LGPL/SSPL-family entry. Configured `cargo-deny` CI remains the
release authority; no local `cargo deny` result is claimed.

Full application/WASM verification necessarily compiled the current sibling
CSGRS/Hyper worktrees. Hypercurve remained a user-owned moving development
boundary throughout this checkpoint. No Hyper/CSGRS file was edited, formatted,
reset, pinned, staged, or committed by this work. Artifact hashes identify the
bytes emitted by this integration run, not a reproducible frozen Hyper source
checkpoint.

## Claims deliberately kept closed

This checkpoint does not establish:

- browser-to-ESP Wi-Fi, AP association, CORS, radio, or background-tab
  reliability;
- complete public device/security/update/machine-membership discovery;
- physical TinyBee stack use, queue occupancy, timing, or core placement;
- live telemetry subscription, waveform capture, WebSocket delivery, or a
  diagnostic lease;
- a licensed/annotated TinyBee photograph or operator usability;
- acceptable worst-case schema-v3 JSON expansion for a 4 MiB document;
- motor, I2S/shift-register, MCPWM, ADC, SD, endstop, E-stop, or safety-chain
  behavior; or
- any arm, motion, process-energy, interlock-reset, or safety authority.

The connected bare MKS TinyBee V1.0 remained untouched, with no motor power or
motors connected. Physical Wi-Fi work remains postponed until the workstation
can associate with the board without interrupting the user's internet session.
