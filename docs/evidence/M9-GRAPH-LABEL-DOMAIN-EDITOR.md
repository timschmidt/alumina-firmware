# M9 graph label and execution-domain editor evidence

Date: 2026-08-20

Checkpoint state: source implementation, exact-commit native/WASM
qualification, optimized bundle, and localhost browser interaction complete.

This checkpoint adds the two remaining basic node-metadata operations to the
canonical graph workspace without turning either field into hidden execution
authority. Labels remain bounded canonical human metadata. Execution placement
is selected only from reviewed domain families and concrete device identities
already established by the graph.

## Source boundary

- Interface implementation:
  `f96fadda31b1ac4c1869691eeca342112f328690`
  (`feat: edit graph labels and execution domains`).
- Preceding firmware evidence checkpoint:
  `b09fc8e518e03fb20a1a183b08cac05ddbd424d3`.
- Both repositories remained on `agent/zizmor-ci-hardening` throughout the
  checkpoint. `alumina-firmware` is the canonical firmware repository; the
  retired `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and physical-device
  state are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed,
  or contacted. No GPIO, motor, motor power, analyzer lead, or WLAN was used.
- Current CSGRS/Hyper sibling path dependencies remained read-only. No sibling
  status, diff, reset, pin, format, stage, commit, or edit was performed.

## Canonical structural operations

`GraphWorkspaceDocument::set_node_label` transactionally replaces one exact
UTF-8 label. Complete `GraphDocument` reconstruction enforces the embedded byte
ceiling, rejects empty or control-bearing text, preserves the node kind,
domain, ports, parameters, stable identities, placement, and allocation
cursors, and leaves both graph and workspace revisions unchanged for an exact
no-op. A changed label advances both revisions and changes canonical ALGR/ALGW
identity because it is saved graph metadata. Semantic kind matching never uses
the label.

`GraphWorkspaceDocument::set_node_domain` provides the deliberately narrower
structural primitive. It replaces only one concrete `HostExact`,
`Service(device)`, or `Realtime(device)` placement and rejects a zero device
identity through complete document validation. It does not itself admit a
kind/domain pair, a cross-domain wire, a clock relationship, an implementation,
a target capability, or a deployment.

The UI closes those additional authorities before commit. It derives domain
families from the exact audited `NodeSchema`; derives nonzero device identities
only from existing device-cycle clocks or existing node placements; sorts those
identities canonically; exposes no raw device-ID text field; and reruns complete
draft semantic analysis plus canonical encoding on the cloned candidate. A
HostExact-only graph with no reviewed device owner therefore cannot invent a
Service or Realtime target. Label and domain changes use the existing bounded
canonical ALGW/ALGP pair history, refresh the probe binding, participate in
origin-local pair persistence, and detach reference trace evidence whose ALGR
identity no longer matches.

## Implemented checks

Core regression tests prove:

- Unicode label replacement, changed canonical graph identity, stable
  placement/cursors, exact replay, and no-op revision stability;
- atomic rejection of empty, control-bearing, over-byte-limit, and unknown-node
  label edits;
- structural Service placement with every unrelated workspace fact retained;
- independent audited rejection of that placement for a HostExact-only kind;
  and
- atomic rejection of a zero device identity and an unknown node.

UI regression tests prove:

- label changes update the bound ALGW/ALGP pair, undo and redo together, survive
  exact pair persistence/replay, and do not record exact no-ops;
- invalid label edits do not dirty persistence or alter history;
- the representative inspector offers only `HostExact` and rejects a directly
  attempted foreign Service placement without mutation;
- the offline TinyBee clock context deterministically yields HostExact,
  Service, and Realtime choices for its one known reference device when all
  three reviewed families are requested; and
- a headless egui frame renders the selected-node label/domain controls.

## Reproduced checks

Run from `alumina-interface` at the implementation commit against the then-live
workspace Hyper/CSGRS paths:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps \
  --locked -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --locked
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli --test dist/alumina-interface_bg.wasm.br
git diff --check
```

All 304 executable workspace tests pass: 70 application/coordinator, 82
protocol-client, 151 core, and one public exact-control integration test. Both
warnings-denied Alumina Clippy commands, all six WASM test-target links, strict
Alumina rustdoc, package-scoped formatting, the local-source/permissive-license
audit, optimized Trunk assembly, WASM validation, and gzip/Brotli integrity
pass. The exact implementation commit remained clean after qualification.

The unchanged canonical reference pair is:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,369,964 | `01ec4202f923379d22ce4dab579125302c4b553cd8fa6e6592e218390a714dbf` |
| `alumina-interface_bg.wasm.gz` | 2,823,281 | `76f37215232d84b8c3519430aa36f9238ec1673e14be3a53bbb460c8cdecd1f0` |
| `alumina-interface_bg.wasm.br` | 2,229,808 | `fbcd5cefc53984becf8baab1a7fed1db60616ef7bb0f24e147f61401fc4c0965` |
| `alumina-interface.js` | 91,816 | `6f42722097eee9fcd9f701df8287ad303d8655fc59f21af7e7d8b6f7b732b1be` |
| `index.html` | 1,295 | `718a4a0602c90ecfb2692c8fbff6e1d3a970ee62db3181a4b9366f89ddf5d754` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The optimized bundle and worker loaded from `127.0.0.1:8765` in an isolated
Chromium profile after clearing that profile's application pair. The browser
opened the actual Control graph tab, scrolled to the graph, selected stable node
`#1`, focused the egui label field, selected the prior text, entered
`Browser source α`, and clicked `apply label`.

The visible result retained node `#1`, its ports, kind, and canvas placement;
advanced the draft to revision 2; displayed the exact `17 / 256 UTF-8 bytes`;
and offered only `HostExact` with `1 audited concrete choice(s)`. The adjacent
wording states that device identities come only from graph clocks/placements.
The sidebar retained one 4,162-byte undo snapshot and no redo snapshot. The
component refreshed, while both its indicator and the workspace status visibly
reported that the old exact replay was detached after the graph edit.

Origin-local persistence contained an `algwp1:` pair with these independently
hashed canonical artifacts after the click:

| Edited browser object | Bytes | SHA-256 |
| --- | ---: | --- |
| label-edited `ALGW` | 3,757 | `a4bbb9c2ca70d526252454f0ceb23fcabc67d427c01964141099241f42d084d2` |
| rebound `ALGP` V2 | 407 | `5c245cfc5df101f3d6374563c4341645f7d4a31b778218f86bd38e6c5c9a98f2` |

The complete persisted text was 8,336 characters. The 1,440-by-913 final
capture `/tmp/alumina-label-domain-browser.png` is 293,872 bytes with SHA-256
`a30d38524984f8d14a7e13ca6a4ab0ec4cfb76be8a7fc2233caee6e0b8e7e8e4`.
It visibly contains the selected renamed node, exact byte counter, audited
domain selector, stable placement, pair history, and detached-trace evidence.

Chromium and the localhost server were stopped afterward. Software-WebGL
readback/performance notices and background Google registration failures were
emitted; no Alumina application, device, or WLAN error appeared.

## Closed claims and licensing

This is bounded host authoring support. It does not authenticate or discover a
device, create a device identity, lower or upload a graph, acquire a resource,
run Service/Realtime code, read a pin, arm an output, or command motion. Firmware
safety, deployment admission, and physical ownership remain independent.

The implementation is independently authored under MIT and adds no dependency.
No GPL-family source, library, generated asset, or tool output was copied,
linked, or vendored.
