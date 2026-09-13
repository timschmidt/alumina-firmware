# M3 authenticated WLAN provisioning

Date: 2026-08-26

Status: the greenfield AP+STA provisioning path is implemented across portable
protocol, simulator, core-0 firmware, browser worker, and visible UI. A clean
Chromium workflow passes against deterministic simulation. No physical AP+STA,
durable credential, interoperability, or sustained-load claim is made here.

## Canonical boundary

`alumina-net::provisioning` defines one fixed, little-endian native family:

| Operation | Body | Bound |
| --- | --- | ---: |
| `NetworkStatus` | `ALMNST01` status | 80 bytes |
| `NetworkScan` | transaction request | 16 bytes |
| scan response | `ALMNSR01`, strongest-first | 424 bytes / 8 entries |
| `NetworkJoin` | generation, exact BSSID, channel, authentication, credential | 128 bytes |
| `NetworkLeave` / `NetworkRecoverAp` | transaction mutation | 24 bytes |

All reserved bytes, enum values, lengths, transaction IDs, SSID/passphrase
policy, scan order, and optional-address relationships are validated before
state changes. Network requests use the existing boot-nonce, counter,
origin-bound HMAC-SHA-256 admission and signed native responses. A join is bound
to the retained scan generation and exact BSSID; it cannot silently retarget a
same-name WLAN.

The client state machine treats credential-bearing ambiguity specially. It
queries status first and accepts only exact observed completion. A newer
generation or unresolved state becomes an explicit error; the passphrase is
never blindly retransmitted. Worker snapshots and debug formatting contain no
credential field, and the UI erases its owned passphrase after constructing the
one join command.

## Runtime ownership

Firmware retains the Wi-Fi controller, AP and station devices, network stack,
DHCP service, HTTP workers, and provisioning state on service core 0. Core 1
receives no radio handle and continues to own real-time execution. The protected
recovery AP remains expected while the station scans, associates, and obtains
infrastructure IPv4. Scan/join/leave/recover work is bounded and serialized.
Current station credentials are deliberately volatile and the status flags say
so; there is no hidden flash persistence or compatibility store.

The interface places authenticated provisioning before high-volume capability,
board-visual, and telemetry detail in each live-device panel. It reports the
embedded interface commit/bundle identity, radio and station phases, recovery
AP state, selected WLAN, address/gateway, credential durability, exact scan
results, and bounded failure state.

## Deterministic browser proof

The simulator begins disconnected with two canonical observations. `Alumina
Lab` is WPA2 Personal on channel 6 at scan RSSI -36 dBm and exact BSSID
`02:a1:51:00:00:01`; `Open Bench` is open on channel 11 at -61 dBm. Successful
protected association yields `192.168.1.77/24` with gateway `192.168.1.1` while
the recovery AP remains active.

`tests/browser/qualify-network-provisioning.mjs` used only visible UI actions
for connection, scan, credential entry, and join. It captured a credential-free
post-scan image and the final UI before issuing any independent request. It then
made exactly one read-only `NetworkStatus` query, spent the maximum boot-scoped
counter after all UI mutations, and independently verified the response HMAC
over status, media, origin, and exact response bytes. This establishes the final
device state without packet-tapping the dedicated browser worker or mutating
the simulator outside the UI.

Every strict check passed:

- a nonzero scan generation was retained;
- the exact protected BSSID, WPA2 authentication, and channel 6 were selected;
- station state was `Addressed` at `192.168.1.77/24` with the exact gateway;
- the recovery AP flag remained set;
- configured, associated, and IPv4-ready flags were set while durable
  credentials remained clear;
- the native status reported no network failure; and
- both screenshots were nontrivial, with no browser loading failure or uncaught
  exception.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| result JSON | 2,394 | `e044df3a11c520477b094787750b63b8e7842dfaf52f4ea45c444429a55cf845` |
| post-scan PNG | 319,711 | `c90d2f9e2c0e0a00eac2bf95b17850a718ce3df16cf15f206f23d6b488a2fd5e` |
| joined PNG | 319,477 | `fdbb57ca5b75295040afd0d21cf5747d1e624aa6647193e63121ca18643dc7a2` |

The JSON contains no device secret, station credential, request HMAC, or
response HMAC.

## Verification

```console
bash tools/alumina-brotli-decoder/verify-maximum-compression.sh
cargo test -p alumina-net -p alumina-sim -p alumina-web-assets --locked --offline
cargo test -p alumina-interface-client --locked --offline
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --features board-mks-tinybee --target xtensa-esp32-none-elf \
  --locked --offline -- -D warnings

ALUMINA_DEVICE_ORIGIN=http://127.0.0.1:8098 \
ALUMINA_CDP_ORIGIN=http://127.0.0.1:9231 \
node tests/browser/qualify-network-provisioning.mjs
```

The selected network operations also have canonical simulator fault-selector
names (`network-status`, `network-scan`, `network-join`, `network-leave`, and
`network-recover-ap`) for subsequent loss/reconciliation browser tests.

## Remaining boundary

The connected MKS TinyBee V1.0 is USB powered with no motors or motor supply,
but this slice has not yet flashed and exercised the final AP+STA image on that
fixture. Physical scan/association, wrong-password recovery, interface transfer
during AP+STA operation, adapter routing, reconnect behavior, runtime stack
watermarks under radio load, and eventual transactional credential persistence
remain separate evidence gates. Production credentials must be unique and
device provisioned; repository development credentials cannot arm production.
