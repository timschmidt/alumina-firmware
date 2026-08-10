# M3 Wi-Fi and web-service foundation evidence

Date: 2026-08-10

Status: release-linked foundation only. This evidence does not close M3
authentication, AP+STA, scan/join, WebSocket, embedded-asset, physical-client,
flood/load, storage-route, update, or hardware-isolation gates.

## Implemented claim

- `alumina-net` is a portable `no_std` policy crate. It validates SSID,
  passphrase, channel, client count, fixed HTTP resources, exact greenfield
  routes, credential provenance, and deterministic AP-preserving supervision
  transitions. Six host tests cover its initial invariants and recovery paths.
- Both board composition roots transfer their unique Wi-Fi peripheral token
  exactly once to core 0. The firmware initializes `esp-radio` and starts the
  protected AP before it starts the core-1 executor. Network runner, DHCP, HTTP,
  controller, station device, socket storage, and supervision state remain
  service-core resources.
- The device AP uses WPA2-Personal, channel 6, at most four clients, and a static
  `192.168.4.1/24` stack. The fixed DHCP server offers only
  `192.168.4.100`–`192.168.4.103` and advertises the local captive URL.
- The first HTTP service admits two connections with 1,024 header bytes, 12
  headers, 2,048 RX and TX bytes per connection, 2 s I/O, 3 s request, and 5 s
  keep-alive limits. It serves only the bootstrap, identity, health, and network
  status GET routes, with exact matching, no-store/nosniff headers, and a
  restrictive bootstrap-page CSP.
- The repository fallback password is accepted only as
  `development-fallback`; identity reports it as non-production-armable without
  returning credential material. `ALUMINA_AP_PASSWORD` may supply a secret build
  input and is tracked by the build script, but build-provisioned credentials
  also remain non-production-armable. Only future transactional unique
  device-stored credentials can satisfy that gate.
- The firmware supplies the scheduler and the 64 KiB reclaimed plus 36 KiB
  ordinary heaps required by the radio runtime. `linkall.x` is emitted by the
  firmware build script, matching the ESP-HAL/T-Deck binary convention and
  preventing target sections from being linked at placeholder addresses.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run -q -p xtask -- check --board mks-tinybee
cargo run -q -p xtask -- check --board t-deck-pro
cargo run -q -p xtask -- build --board mks-tinybee-v1 --profile release
cargo run -q -p xtask -- build --board t-deck-pro --profile release
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked -- -D warnings
git diff --check
```

Results: all 68 portable unit and documentation tests pass with strict host
Clippy. Both board packages pass board-aware checks, strict ESP Clippy, and
optimized release linking. `llvm-size` reports:

| Board image | text | data | bss aggregate |
| --- | ---: | ---: | ---: |
| MKS TinyBee V1.x | 543,544 | 9,992 | 252,144 |
| T-Deck Pro | 494,933 | 10,808 | 527,556 |

The aggregate BSS column includes linker-reserved stack and reclaimed-memory
sections and is not equivalent to application static usage. Section-level
inspection shows the intentional 65,536-byte `.dram2_uninit` reclaimed heap in
both images. These measurements are capacity baselines, not steady-state heap,
stack-watermark, throughput, latency, or RF evidence.

## Claim boundary and required HIL

No serial device was present during this evidence run, so neither image was
flashed. No phone/browser received a lease, loaded a page, saturated the radio,
or exercised reset/recovery. The DHCP implementation documents a possible
limitation for clients which request unicast replies before their address/MAC is
known to `embassy-net`; common broadcast clients are expected to work, but that
expectation is not evidence.

AP+STA coexistence, scan/join/leave, stored credentials, authenticated mutation,
origin/session policy, rate limiting, WebSocket framing/backpressure, embedded
UI assets, storage upload/download, and network-to-command admission are absent.
The service task reports a bootstrap state only. Before promotion, bench tests
must measure association/recovery, leases across representative clients, heap
and stack watermarks, malformed/slow/flood traffic, core affinity, watchdogs,
service saturation while real-time deadlines run, and fail-safe behavior during
radio and core-0 faults.
