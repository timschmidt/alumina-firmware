# M9 offline diagnostic explorer evidence

Date: 2026-08-12 (America/Detroit)

## Claim

This checkpoint establishes a portable canonical boundary and a visible
simulation-only board diagnostic path. It does **not** claim live telemetry,
TinyBee Wi-Fi transport, GPIO acquisition, analyzer correlation, output tests,
or motion qualification.

## Implemented

- New MIT OR Apache-2.0 `no_std` crate `alumina-diagnostics`.
- Allocation-free bounded decoders and caller-buffer encoders for `ALMOVW01`
  resource overviews and `ALMDIG01` digital edge captures.
- Complete device, boot, capability, configuration, and integer clock identity.
- Exact scalar values, sample provenance/quality/age facts, capture source,
  trigger, pre/post window, capacity, stride, overflow/loss/confidence flags,
  and canonical ordering checks.
- Deterministic `alumina-sim` TinyBee fixture bound to the current 8 MiB board
  capability: four values, four digital channels, fourteen transitions,
  GPIO33 rising at +500 cycles, and an explicit unqualified-clock flag.
- Board-name-independent `alumina-interface-core` reconciliation requiring the
  overview, capture, and complete board capability to agree before UI state is
  returned.
- A visible board ledger/status/capture plot cross-linked through exact typed
  resources. Hover derives an integer cycle cursor; clicking a lane selects the
  corresponding board resource.
- Prominent simulator/no-authority and missing-photo/hotspot gates.

## Verification

Firmware workspace:

```text
cargo test --locked --offline
  passed default members, including 5 alumina-diagnostics and 34 alumina-sim tests

cargo clippy -p alumina-diagnostics -p alumina-sim \
  --all-targets --locked --offline -- -D warnings
  passed

cargo +esp check -p alumina-diagnostics \
  --target xtensa-esp32-none-elf --locked --offline
  passed
```

Interface workspace:

```text
cargo test --workspace --locked --offline
  passed: 22 application + 32 client + 93 core + 1 integration tests
  plus the compile-fail documentation boundary

cargo clippy -p alumina-interface-core -p alumina-interface \
  --all-targets --locked --offline --no-deps -- -D warnings
  passed

cargo check --workspace --target wasm32-unknown-unknown --locked --offline
  passed

env -u NO_COLOR trunk build --release --locked --offline
  passed, including wasm-opt and gzip/brotli hooks

wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
  passed
```

Final optimized artifact facts:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface.js` | 91,813 | `4a925e49bb34c8b21b523248353dbe7a6fcec1a2cce8c5ea807c55ab46b107c1` |
| `alumina-interface_bg.wasm` | 5,025,418 | `142268dee7eacc92ab50e245a40a4b6a7f365b9f3bc4c1327e11f0dfe5ea1428` |
| `alumina-interface_bg.wasm.gz` | 2,277,187 | `ac18a7122df31ea0695bb8b27754b5bc8ecd183dfa0d3aa48478151a493adbb1` |
| `alumina-interface_bg.wasm.br` | 1,830,923 | `b9bbd7443d49d6b5307809eae01cb369671fc4a2e12a020b1e5dfc6a075b033e` |
| interface `Cargo.lock` | 97,179 | `c551c62dfc3fd77f57d5846a4380dc4039570ee7a39413436351a3af2440c67b` |
| firmware `Cargo.lock` | 68,857 | `6b4816c44bb7e2bfa3553ae568c681bf3b5d43879615eb8b88c5c0e15a835188` |
| inspected screenshot | 546,226 | `efa8005f8b79753c118712b2c667bc8a2fc462fa358aa71df7861d593c4a273a` |

A headless Chromium software-WebGL run loaded the production bundle from a
temporary `127.0.0.1` server and visibly rendered the exact package identity,
simulation/no-authority warning, GPIO33 overview value/age, four digital lanes,
trigger at +500 cycles, cursor values, `CLOCK UNQUALIFIED`, and the existing
no-photo/no-hotspot gate. The temporary server was stopped after inspection.

No NetworkManager setting changed. No request was sent to the Alumina AP, the
attached TinyBee, a serial port, or the SLogic16U3.

## Open gates

- authenticated bounded overview subscription and waveform configure/arm/chunk
  operation bodies;
- firmware acquisition producers and explicit resource/buffer admission;
- live TinyBee AP/HTTP validation once the workstation has an independent
  Internet path;
- physical GPIO capture and SLogic comparison;
- licensed revision photograph and reviewed hotspot coverage; and
- analog waveform/calibration/uncertainty formats and min/max-envelope plots.
