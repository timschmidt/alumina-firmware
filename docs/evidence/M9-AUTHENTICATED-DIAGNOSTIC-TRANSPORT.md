# M9 authenticated diagnostic transport — offline evidence

Date: 2026-08-13

## Result

The canonical offline `ALMOVW01`/`ALMDIG01` records now cross an authenticated,
retry-safe transport boundary without weakening their exact identity or loss
semantics.

- `alumina-diagnostics::transport` defines allocation-free canonical bodies for
  telemetry subscription/reference/status/event and waveform
  configure/reference/status/read/chunk operations.
- Every session binds full device, boot, capability, configuration, and clock
  context. SHA-256 binds the complete canonical request, overview, retained
  record, and each range.
- `alumina-service::diagnostics` owns one fixed latest-only subscription and one
  fixed capture on core 0. It implements exact event accounting, minimum-period
  admission, idempotent mutation, explicit chunk acknowledgement/drop, and
  retained-record range recovery.
- Service context is authoritative. A changed boot/configuration clears all
  prior state, foreign request contexts conflict, and an outer frame with the
  wrong configuration digest cannot enter the operation dispatcher.
- Capture configuration is rejected unless its worst-case complete canonical
  record fits the concrete fixed buffer.
- TinyBee and T-Deck Pro compile the dispatcher with providers disabled. They
  return `Unsupported` until real sampling/acquisition owners exist. The host
  simulator explicitly opts into simulated providers.
- `alumina-interface-client` provides transport-independent subscription and
  capture state machines. Ambiguous mutations reconcile by status; ambiguous
  reads repeat the exact range; the complete record is independently decoded
  and hashed before exposure.

The native response body budget is 312 bytes. The 144-byte chunk prefix leaves
an exact 168-byte range payload. The initial target compositions retain the
dispatcher but reserve zero provider storage because no physical acquisition
owner is qualified. The deterministic host composition supplies explicit fixed
buffers; these are implementation bounds, not claims about qualified physical
throughput.

## Exercised paths

Automated host tests cover:

- strict round trips, reserved/flag/length/order limits, 257-selector bombs,
  identity substitution, request/record/chunk tampering, and status accounting;
- latest-only replacement and exact drop counts;
- too-early overview rejection;
- duplicate subscribe/configure/arm/stop behavior and conflicts;
- deliberately lost mutation and range responses followed by status/range
  reconciliation;
- deliberately dropped live capture chunks with authoritative range recovery;
- complete byte-for-byte reconstruction of the 512-byte TinyBee simulated
  capture;
- the production-format HMAC request/response proof and native dispatcher both
  in-process and over a real `127.0.0.1` TCP/HTTP socket; and
- ESP target compilation for TinyBee and T-Deck Pro.

The localhost test opens only an ephemeral loopback listener. It does not touch
NetworkManager, a WLAN interface, serial/USB, the connected TinyBee, or the
SLogic analyzer.

## Verification commands

```sh
# alumina-firmware (portable default-members; target-only ESP crates are explicit)
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked --offline
cargo +esp check -p alumina-firmware --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline
cargo +esp check -p alumina-firmware --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline

# alumina-interface
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
```

Observed on the final source tree:

- formatting passed in both repositories;
- the complete portable `alumina-firmware` default-member test suite passed, including
  10 diagnostic-codec, 14 service, and 37 simulator tests;
- all 153 native interface unit/integration tests passed, as did the compile-fail
  Rustdoc test and all package doc tests;
- strict Clippy passed for both portable workspaces, the TinyBee target, and the
  T-Deck Pro target;
- strict Rustdoc passed for both portable workspaces and the complete interface
  workspace compiled for `wasm32-unknown-unknown`;
- board validation passed for TinyBee 8 MiB, TinyBee 4 MiB, T-Deck Pro, MKS
  ESP32 FOC V1, and the current T-LoRa Pager stub, including the three CI JSON
  capability assertions; and
- release links passed for TinyBee 8 MiB, TinyBee 4 MiB, and T-Deck Pro. GNU
  `size` reported respectively `1,036,700/12,200/249,944`,
  `1,036,724/12,200/249,944`, and `974,405/12,960/525,408` bytes of
  text/data/BSS.

The first TinyBee release-link attempt also served its intended purpose: it
rejected a composition that reserved provider buffers despite having no
provider. The final composition uses zero provider storage, and all three final
release links pass. This is an SRAM composition result, not runtime low-water
evidence.

`cargo-deny` is not installed in this offline workstation environment, so its
CI-only policy command was not claimed locally. The lockfile delta adds no new
registry package, only existing `sha2` and local workspace edges; offline Cargo
metadata contains no GPL-family-only license declaration. The repository's
deny policy continues to allow only the listed permissive licenses and remains
a required CI gate.

## Closed claims and remaining gates

This checkpoint does not claim physical GPIO values, RMT/PCNT/DMA timing,
analog acquisition, Wi-Fi/AP behavior, WebSocket event delivery, background-
browser behavior, SLogic agreement, or safe motor/process control. No provider
has output authority. Physical TinyBee acquisition must be implemented against
the reviewed resource owner and qualified with the available SLogic16U3 before
the hardware provider policy can change from `NONE`.
