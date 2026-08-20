# M9 bounded graph-probe metadata evidence

Date: 2026-08-20

This checkpoint makes each HostExact diagnostic probe's canonical name,
retained-sample ceiling, and event-ordinal decimation stride editable. The
operation preserves stable probe/source/type identity, validates the complete
ALGP V2 candidate transactionally, and participates in the exact ALGW/ALGP pair
history and persistence path.

## Source boundary

- Interface implementation:
  `903eb34ab985e007b732fc13f811e7f71bbb16a0`
  (`feat: edit bounded graph probe metadata`).
- Preceding firmware evidence checkpoint:
  `beb890ee6643c518ad77bd058327efa952aec388`.
- Firmware source, protocol, target images, board packages, and physical-device
  state are unchanged by this checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or external probe lead is involved.
- Workstation Wi-Fi configuration and the Alumina device AP were not touched.
  Browser qualification used `127.0.0.1` only.
- Live CSGRS/Hyper path dependencies, including the concurrently edited
  HyperCurve tree, were compiled read-only. No sibling status, diff, reset,
  pin, format, stage, or edit was performed.

## Transactional core replacement

`GraphProbeDocument::replace_probe_metadata` changes only one retained probe's
name and `GraphProbeCapture`. The stable probe ID, exact output endpoint, and
workspace-resolved value type remain fixed. A changed candidate advances the
sidecar revision once; exact reapplication returns without changing revision or
canonical bytes.

The replacement reconstructs and validates the complete sidecar before
mutation. It therefore retains all existing ALGP V2 invariants:

- probe names are 1–64 ASCII bytes, begin with a letter, contain only letters,
  digits, `_`, `-`, or `.`, and remain unique;
- output endpoints remain unique and valid in the exact bound workspace;
- maximum retained samples and event-ordinal stride are nonzero and no greater
  than their embedded/caller policy (one million each in the interactive
  profile);
- active trigger identity and Boolean-stream type remain valid; and
- the trigger's complete pre/current/post sample count still fits the edited
  source probe's retention ceiling.

Malformed/duplicate names, zero or excessive capture values, a trigger window
that no longer fits, an unknown probe, a foreign workspace, revision overflow,
or any later canonical encoding failure leaves the original document intact.
The existing ALGP V2 wire format and canonical reference identity do not
change; the public 64-byte name constant merely lets the UI share the core
boundary without duplicating it.

## Bounded editor integration

Every visible probe row now exposes:

- a 64-character name field whose accepted canonical language is enforced by
  core replay;
- a nonzero retained-value field bounded by the sidecar policy;
- a nonzero event-stride field bounded by the sidecar policy;
- explicit `apply metadata` and `reset fields` controls; and
- the existing remove and Boolean edge-trigger controls.

Applying a candidate first mutates and encodes a cloned sidecar, then records
the exact prior ALGW/ALGP pair, and only then replaces visible state. A changed
edit marks current-pair persistence dirty. An exact no-op records no history and
causes no persistence write. Undo/redo restores the exact previous/next ALGP
bytes and refreshes fields from the replayed canonical metadata.

Edit fields are transient and never enter ALGP. A trigger-only change, probe
addition/removal elsewhere, or graph workspace rebinding preserves an unrelated
unsaved field only when that probe's canonical metadata is unchanged. A changed
metadata identity resets that row. History navigation, stored-pair restoration,
and explicit ALGW/ALGP import reset fields from admitted canonical state. This
prevents stale UI text from crossing an exact replay boundary without needlessly
discarding unrelated work.

Replacing a probe package now also resets the plot cursor to zero for a disabled,
waiting, or invalid replay trigger; a matched trigger still selects its exact
tick. No stale trigger cursor survives a changed capture stride or window.

## Regression coverage

Core regression proves successful metadata replacement, revision advancement,
unchanged ID/source/type, canonical replay, exact no-op behavior, and closed
failure for duplicate/malformed names, zero sample count, zero stride, and an
active trigger window larger than its revised retention ceiling.

Application regressions prove:

- a changed name/count/stride leaves ALGW unchanged but changes exact ALGP;
- the edit creates one pair-history transition and marks persistence dirty;
- undo/redo restores exact prior/edited sidecar encodings and canonical fields;
- exact reapplication does not change history or persistence state;
- duplicate-name and undersized-trigger candidates preserve sidecar/history;
  and
- an unrelated trigger edit preserves an unsaved probe draft, while history
  navigation resets it from the restored sidecar.

The existing full-frame regression renders all new bounded row controls through
the same native/browser egui path.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt -p alumina-interface-core -- --check
cargo fmt -p alumina-interface -- --check
cargo test --workspace --all-targets -- --format terse
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps \
  --document-private-items --locked
./scripts/audit-source-policy.sh
env NO_COLOR=true trunk build --release --locked
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 275 executable workspace tests pass: 56 application/coordinator, 82
protocol-client, 136 core, and one public exact-control integration test.
Native and WASM warnings-denied Clippy, strict Alumina rustdoc, package-scoped
format checks, the local-source/permissive-license audit, optimized Trunk
assembly, both WASM validators, and gzip/Brotli integrity pass. Dependency
builds emitted only warnings from the live concurrently edited read-only
HyperCurve tree.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The final optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,357,816 | `de5ab8331b022d5fefb403242dc18990fb415491cd18dc31c3f57c6a6bd56d27` |
| `alumina-interface_bg.wasm.gz` | 2,814,150 | `257f74b9e9c8c457116da50bbc22db64bf8499f12640f83ea04de705a9da8eb1` |
| `alumina-interface_bg.wasm.br` | 2,223,388 | `6bcdbc0e825892aa17698956c3b629047433f4c874621d5a7c620db5b23db966` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and dedicated worker loaded from `127.0.0.1:8765`
in a fresh isolated Chromium profile. Runtime inspection found the unchanged
one-value `algwp1:` carrier with a 3,755-byte ALGW segment, a 407-byte ALGP
segment, and 8,332 total text characters. The scrolled Control Graph view
visibly rendered probe rows 2–7 with canonical names, `retain 4096`, `stride 1`,
`apply metadata`, and `reset fields`; Boolean rows also retained rising,
falling, and either-edge buttons. The exact trigger-selected mixed-signal plot
remained directly below those controls.

The accepted 1,440-by-757 capture
`/tmp/alumina-probe-metadata-browser.png` is 218,747 bytes with SHA-256
`41fa420cf4a68a30737576ad76b77f07d5d126122b12d0a70b27e201af3b6b8b`.
The localhost Chromium and HTTP server were stopped afterward. Chromium logged
only software-WebGL screenshot readback stalls and background Google
registration/authentication errors; no Alumina application, device, or WLAN
error appeared.

## Closed claims and licensing

This is HostExact presentation/authoring metadata. It does not allocate device
capture memory, negotiate telemetry bandwidth, arm a hardware trigger, read a
GPIO, deploy a graph, or grant firmware/output/safety authority. The edited
retention ceiling describes bounded host intent only; live capability-bound
capture remains a separate protocol and device qualification problem.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the existing local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
