# M9 board-capability explorer evidence

Date: 2026-08-12

## Scope

This offline checkpoint exposes the complete descriptive board ledger to the
browser without confusing description with operation authority. The firmware
decoder is commit
`9975035e05e54b4f9f13bedfc01356f17bef63ef` (`Add bounded board capability
explorer decoding`). The independently owned interface model, visible TinyBee
explorer and documentation are commit
`445abc98d86c145cba0e8058649898d8198967c1` (`Add capability-derived TinyBee
board explorer`).

No device was contacted, reset, erased, flashed, sampled or driven. Workstation
Wi-Fi and NetworkManager were not changed. The connected bare MKS TinyBee V1.0,
its recovery AP, and the disconnected SLogic16U3 remained untouched.

## Complete bounded document view

`alumina-capability::decode_board_capability` is a no-std, allocation-free view
over one complete `ALMCAP02` V2 byte string. Before returning borrowed values it
checks the exact header/length, SHA-256 identity, bounded nonempty UTF-8 strings,
canonical enums/Booleans/options/routes/reserved bytes, graph section, resource
IDs and references, owners, safe images, every intervening section, visuals,
hotspots, normalized points, and exact document end. Duplicate resources,
aliases, interrupts, safe images, visuals and per-visual hotspot IDs fail
closed. Graph-addressable resources must exist in and be Realtime-owned by the
broader descriptive inventory.

The caller selects independent maximum document/string/section/visual/hotspot/
polygon bounds. The interactive policy is 4 MiB, 64 KiB, 4,096, 32, 4,096 and
4,096 respectively. Zero-allocation iterators expose resources, aliases,
visuals, hotspots and points. Round-trip tests cover the primary and 4 MiB
TinyBee packages, T-Deck Pro, and MKS ESP32 FOC V1. A synthetic licensed visual
proves exact metadata and polygon replay; document/string/record/point bounds,
bad Booleans, trailing bytes and expected-identity substitution fail.

The calculated digest is content identity, not authentication. A live caller
must compare it with its authenticated session's exact capability identity.
This checkpoint uses only the locally compiled, digest-verified reference
package.

## Board-name-independent owned model

`BoardExplorerSnapshot` fallibly copies the bounded borrowed view into
window-free owned UI state. Every vector and string allocation is explicitly
reserved and allocation failure is typed. Each resource retains:

- its typed ID, Service/Realtime owner, reset/fault safe value and hazardous
  marker;
- all aliases targeting that exact typed ID; and
- only the separate graph class/access/support records explicitly published by
  the image.

The primary TinyBee document remains 3,435 bytes with SHA-256
`0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5`.
The derived snapshot proves these exact counts:

| Fact | Count |
| --- | ---: |
| descriptive resources | 62 |
| Service / Realtime resources | 21 / 41 |
| hazardous-output resources | 21 |
| graph-readable resources | 4 |
| aliases | 51 |
| buses / devices | 3 / 1 |
| flash regions / clocks | 0 / 2 |
| electrical constraints / interrupts | 9 / 4 |
| safe images / HIL requirements | 1 / 8 |
| visuals | 0 |

GPIO22, GPIO32, GPIO33 and GPIO35 alone carry the current stable Boolean graph
read. Search and explicit filters distinguish all described, graph-readable,
graph-closed, hazardous, Service-owned and Realtime-owned rows. A selected row
shows its exact selector, aliases, owner, safe value, hazard state and graph
operation records. For example, `axis.x.step` resolves to a hazardous I2S
shifted output yet remains graph-closed. The existing target-I/O draft remains a
separate four-entry authoring proof.

## Honest physical-view gate

The TinyBee package publishes zero visuals. The UI therefore draws no board
silhouette, connector placement, photo or hotspot. A prominent physical-view
panel states that no licensed revision photograph or reviewed polygons exist.
The existing `visual.top-hotspots` HIL requirement remains open.

A future overlay requires an operator-owned exact-revision photograph, raster
digest/dimensions/MIME type, SPDX license, attribution, and reconciled normalized
typed-resource polygons. The browser must additionally fetch and verify the
exact raster before drawing it. Generated or vendor lookalike imagery is not
accepted as fixture evidence.

## Verification

Firmware/default-member and decoder checks reproduced offline:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --no-deps
cargo +esp check -p alumina-capability \
  --target xtensa-esp32-none-elf --locked --offline
cargo xtask board check mks-tinybee-v1
cargo xtask board check mks-tinybee-v1-4mb
cargo xtask board check t-deck-pro
cargo xtask board check mks-esp32-foc-v1
cargo xtask board check t-lora-pager-current
```

The seven capability tests and every portable default-member test/doc test
pass. Strict Clippy, warnings-denied docs, the direct classic-ESP32 no-std
decoder build, and all five board-registry checks pass. A literal host
`cargo test --workspace` is intentionally not a valid firmware matrix because
it selects ESP-HAL firmware/examples for x86_64; ESP-HAL correctly rejected that
wrong target. No hardware command was substituted for that target error.

The interface checkpoint reproduced offline:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- \
  -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 146 executable tests pass: 22 application/coordinator, 32 client, 91 core,
and one cross-crate exact-control integration test, plus the intentional
compile-fail rustdoc test. Native/WASM warnings-denied Clippy for repository
targets, WASM check, strict docs, current-sibling/permissive-license audit,
optimized build, both WASM validators and compressed-asset checks pass. The
concurrently dirty sibling Hypercurve emitted its existing unused-variable
warning while dependency code compiled; no sibling file was changed for this
checkpoint.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface.js` | 91,813 | `a4b19fbf45c6588075ec16535607c1d0ec55b8dc017b08dd2d9b26abe15c3158` |
| `alumina-interface_bg.wasm` | 4,987,492 | `dfa06bb5d203277f17df70698786908ae003541a3e2b5191d3b747f5e1ff407f` |
| `alumina-interface_bg.wasm.gz` | 2,260,578 | `e9785ab8f7a4e3740eb46a7cc1cdeb6b16e2af1265a84aecde00316e8d2614bc` |
| `alumina-interface_bg.wasm.br` | 1,818,028 | `70c507872292b6c9beded650860304e9f2193a806e30677a8a70a937c3f97f51` |

The 96,869-byte interface lockfile has SHA-256
`ea4e2984188dd4a4746c65d8dd7e870116da53532f0f4d4f84d426c1aa7f68a9`.
The 68,629-byte firmware lockfile has SHA-256
`ed207d2a5f391c1563a0ee51145d589f6a2d793c355de37041c8b793202b9fc6`.

The committed optimized bundle was served temporarily on `127.0.0.1` and
rendered at 1,440 by 1,600 pixels with headless Chromium software WebGL. Visual
inspection confirmed the exact board/revision/digest/memory/core facts, 62/21/
41/21/4/51 summary, resource/alias ledger, GPIO33 selection, green graph-read
record, hazardous shifted-output labels, conspicuous missing-photo gate, and
separate four-resource target draft. The 392,117-byte transient screenshot has
SHA-256
`f58a988ac71bb5820cb118263f1d2b19d67f9afcbfb060172567ffc81b67cc56`.
The loopback server was stopped immediately after capture.

## Closed claims and licensing

This checkpoint provides no live capability discovery, board-photo
correspondence, telemetry, capture, trigger, diagnostic output test,
configuration mutation, deployment, arming, motion or motor authority. It makes
no new bench or timing claim.

The implementation is independently authored under MIT / Apache-2.0-compatible
repository terms. The policy audit accepted the existing permissive
Alumina/CSGRS/Hyper/native/WASM inventory. No GPL-family source, library, visual
asset or generated output was copied, linked or vendored.
