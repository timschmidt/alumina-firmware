# M9 exact graph/probe pair persistence evidence

Date: 2026-08-20

This checkpoint makes authored diagnostic probes and replay triggers survive
browser reloads with their exact graph workspace. It replaces the former
ALGW-only application storage value with one bounded versioned ALGW/ALGP pair,
adds independent native/browser `.algp` exchange, and admits every sidecar only
against the exact current workspace identity.

## Source boundary

- Interface implementation:
  `4b96c1242511a4e3ce79a6e6fa23a9af0e646d07`
  (`feat: persist exact graph probe pairs`).
- Preceding firmware evidence checkpoint:
  `83de4b2fa059c167db7cfb6331397351d3706eb3`.
- Firmware source, protocol, target images, board packages, and physical-device
  state are unchanged by this checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or external probe lead is involved.
- Workstation Wi-Fi configuration and the Alumina device AP were not touched.
  Browser qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies, including the concurrently edited
  HyperCurve tree, were compiled read-only. No sibling status, diff, reset,
  pin, format, stage, or edit was performed.

## One fail-closed application pair

The origin-local storage key is now
`alumina.graph-workspace-pair.algwp.v1`. Its application envelope is:

```text
algwp1:<lowercase ALGW hex>:<lowercase ALGP hex>
```

The envelope is an application persistence carrier, not a new firmware or core
canonical format. Each decoded artifact has an independent 2 MiB browser
persistence ceiling. Prefix/version mismatch, a missing separator, odd hex,
uppercase or non-hex text, and either artifact exceeding its ceiling reject
before decoded artifact allocation or replay. Encoder length overflow rejects
before allocating the text carrier. The previous ALGW-only storage key and
`algw1:` carrier are deliberately not compatibility inputs.

The browser writes the entire pair with one local-storage `setItem`; it never
maintains independently replaceable ALGW and ALGP keys. Restore performs all
of the following before mutating the visible draft:

1. bound and canonically replay candidate ALGW bytes;
2. apply audited UI semantic and layout admission to that candidate;
3. canonically replay ALGP bytes against that exact candidate ALGW identity;
4. require the sidecar's output endpoints, value types, trigger, embedded
   limits, and byte-for-byte re-encoding to remain valid; and
5. only then commit both documents and clear ephemeral history/transient state.

A malformed ALGP or a valid ALGP for another workspace therefore rejects the
complete stored pair. The reviewed reference graph and sidecar remain loaded;
there is no partial graph-only restore.

Graph, history, probe, and trigger identity changes all mark the pair dirty.
Installing an identical trigger, clearing an already-disabled trigger, or
importing the already-current canonical ALGP is an exact no-op and causes no
redundant persistence write. If an accepted graph edit removes or retypes an
observed output, the now-incompatible sidecar is visibly and atomically
replaced by an empty canonical ALGP bound to the revised workspace. Unbound
probe intent is never retained behind a new ALGW identity.

## Bounded file exchange

The existing platform byte bridge remains parsing-neutral and now has separate
ALGW and ALGP instances. Native and browser shells expose exact `.algw` and
`.algp` open/save or upload/download controls.

- ALGW file admission retains the 20 MiB interactive workspace ceiling and the
  existing canonical replay plus audited draft-admission path.
- ALGP file admission uses its 2 MiB canonical document ceiling.
- ALGP replay always receives the exact current workspace as an external
  identity authority.
- A corrupt, noncanonical, over-limit, trailing, or foreign-workspace sidecar
  leaves both graph and prior sidecar unchanged.
- A valid changed sidecar can mutate only probe/trigger presentation state; it
  cannot mutate ALGW, grant a resource, or create firmware/device authority.

## Regression coverage

Application regressions prove:

- exact ALGW/ALGP text-envelope and document round trips;
- authored trigger identity surviving reload byte-for-byte;
- lowercase canonical text and independent byte ceilings;
- atomic fallback for a valid workspace paired with a sidecar bound elsewhere;
- canonical `.algp` import and exact no-op re-import;
- wrong-workspace sidecar rejection without graph, sidecar, or history loss;
- persistence dirtiness only when canonical probe identity changes; and
- automatic empty-sidecar rebinding when a graph edit removes a probed node,
  followed by successful pair persistence and restore.

The existing headless frame test also renders both file bridges, while core
ALGP tests continue to cover canonical replay, caller/embedded bounds,
corruption, trailing bytes, workspace substitution, typed output resolution,
transactional mutation, and exact trigger windows.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt -p alumina-interface -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 267 executable workspace tests pass: 53 application/coordinator, 82
protocol-client, 131 core, and one public exact-control integration test.
Native and WASM warnings-denied Clippy, strict Alumina rustdoc, the
local-source/permissive-license audit, optimized Trunk assembly, both WASM
validators, and gzip/Brotli integrity pass. Dependency builds emitted only
warnings from the live concurrently edited read-only HyperCurve tree.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,333,879 | `89e5b2cd48a64104949a0dff0358417ae5e25d0f7e033493f6ee637bd60ee435` |
| `alumina-interface_bg.wasm.gz` | 2,804,530 | `ff3925982e34b74b5b1d68235bef1d7467a5dc23b2611c266edf5081434ec1a9` |
| `alumina-interface_bg.wasm.br` | 2,219,040 | `bed750998e36c7f29dec3b606f4d6f5930b2ffe19d62b1247989ff32d46c1195` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The optimized bundle and dedicated worker loaded from `127.0.0.1:8765` in a
fresh isolated Chromium profile. Runtime inspection found exactly one storage
key, `alumina.graph-workspace-pair.algwp.v1`, containing an 8,332-character
`algwp1:` value with a 3,755-byte ALGW segment and a 407-byte ALGP segment.
After an explicit page reload, the Control Graph view reported
`restored exact canonical ALGW/ALGP pair from application storage` and the
sidebar reported `Browser persistence: exact ALGW/ALGP pair saved`.

The accepted 1,440-by-857 capture also shows independent `ALGW file` and
`ALGP file` download/open controls plus the unchanged 407-byte sidecar and
probe-5 falling trigger. `/tmp/alumina-control-pair-browser.png` is 306,768
bytes with SHA-256
`c031fd213e1a846f94bc465c9b15ecfc0e9672f27be4c5b0c488d35a8279a159`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and background Google
registration errors; no Alumina application, device, or WLAN error appeared.

## Closed claims and licensing

This is origin-local HostExact editor persistence and file exchange. It does
not provide collaborative/conflict-aware storage, crash-durable filesystem
transactions, cloud synchronization, firmware storage, authenticated device
telemetry, live device triggers, device-side graph interpretation, or any
physical output/safety authority. ALGP remains a presentation/replay sidecar.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
