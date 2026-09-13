# Embedded-interface browser qualification

`qualify-embedded-interface.mjs` attaches to an already running Chromium CDP
endpoint and navigates a real browser to a simulator or physical Alumina
device. It fails unless the exact bootstrap, decoder, Brotli JS/WASM, and worker
load; the integrity evidence and HTTP identities match; a 1440×1000 canvas
renders; the favicon stays local as a data URL; and Chromium reports no loading
failure or uncaught exception.

One local invocation is:

```console
/usr/bin/chromium-browser --headless=new \
  --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port=9231 \
  --user-data-dir=/tmp/alumina-cdp-profile \
  about:blank

ALUMINA_DEVICE_ORIGIN=http://192.168.4.1 \
ALUMINA_CDP_ORIGIN=http://127.0.0.1:9231 \
node tests/browser/qualify-embedded-interface.mjs
```

The default 150-second readiness deadline covers the measured classic-ESP32 AP
transfer time for the 2,820,967-byte q11/w23 Brotli WASM. Results and a
screenshot are written under `/tmp/alumina-embedded-interface-browser` unless
`ALUMINA_BROWSER_OUTPUT_PREFIX` overrides it. The harness does not associate a
network interface, change routes, flash hardware, or grant control authority.

## Authenticated Wi-Fi provisioning

`qualify-network-provisioning.mjs` starts from a clean application session,
connects to the deterministic simulator through the visible UI, scans, enters
the protected test-network credential, and joins the strongest exact BSSID.
All mutation travels through the browser worker. After capturing the UI it
performs one independently signed, read-only `NetworkStatus` request and
verifies the response HMAC before admitting the resulting state. Neither the
station credential nor authenticated request/response tags are written to the
JSON evidence.

```console
ALUMINA_DEVICE_ORIGIN=http://127.0.0.1:8098 \
ALUMINA_CDP_ORIGIN=http://127.0.0.1:9231 \
node tests/browser/qualify-network-provisioning.mjs
```

The simulator must begin from fresh state because the final evidence query
intentionally spends the maximum boot-scoped authentication counter. The
strict pass requires a retained scan generation, exact WPA2 BSSID/channel,
addressed station lease, preserved recovery AP, explicitly volatile
credentials, nontrivial before/after screenshots, and no browser failure or
exception.
