# M9 capability-derived graph resource-selector evidence

Date: 2026-08-20

This checkpoint supplies the authority-safe counterpart to exact graph literal
editing. One existing physical-input node can now select another resource only
from the complete capability-derived catalog. Device identity, capability
digest, resource class, and typed resource selector never become free-form
text.

## Source boundary

- Interface implementation:
  `c4178fcea892f90371776ce781cf3cd73bbf35c7`
  (`feat: select capability-bound graph resources`).
- Preceding firmware evidence checkpoint:
  `fd33d2852dbc8090286ab32aa848a21cfab25986`.
- Both repositories remained on `agent/zizmor-ci-hardening` throughout the
  checkpoint. `alumina-firmware` is the canonical firmware repository; the
  retired `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and physical-device
  state are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed, or
  contacted. No GPIO, motor, motor power, or analyzer lead was involved.
- Workstation Wi-Fi and the Alumina device AP were not touched. Browser
  qualification used `127.0.0.1` only.
- Current CSGRS/Hyper sibling path dependencies were compiled read-only. No
  sibling status, diff, reset, pin, format, stage, or edit was performed.

## Window-free selection boundary

Each `GraphCapabilityNodeEntry` now retains the exact reviewed node-local
resource-parameter ID alongside its complete target-bound prototype. The
public `entry_index_for_node` lookup recognizes a managed node only when all of
these facts match an offered entry:

- opaque node kind and version;
- concrete Realtime device owner;
- complete input and output port shape;
- resource parameter ID, name, and registered graph type; and
- the complete typed resource-handle value.

The lookup does not use a node label, resource display label, numeric GPIO, or
partial digest. An independently constructed handle that is merely well typed
but absent from the offered catalog does not become selector authority. An
exact value equal to an offered entry is intentionally value-identical; this is
an authoring admission rule, not proof of live-session provenance.

`select_graph_capability_node_resource` performs the replacement transaction.
It requires a catalog, the reviewed deployment/semantic registry, a workspace,
a stable node ID, and a catalog index. It then:

1. resolves both IDs without fallback;
2. proves the current node already matches an entry in the supplied catalog;
3. rejects an exact no-op and any selected entry of another reviewed kind;
4. resolves the exact resource parameter from the derived prototype;
5. rejects the same underlying resource-handle identity on any other node,
   even if a future schema gives that handle another graph type ID;
6. edits a cloned workspace through `set_parameter`;
7. reruns complete draft semantic analysis and canonical ALGW encoding; and
8. replaces the caller's workspace only after every prior step succeeds.

A successful selection advances graph/workspace revisions and canonical
identity. It preserves the node ID, integer placement, next-node cursor, and
next-wire cursor. Unknown IDs, a raw unlisted handle, a foreign-device catalog,
a duplicate resource, a kind change, a semantic-registry mismatch, structural
failure, or encoding failure leaves the prior canonical workspace unchanged.

The catalog still verifies content identity rather than authenticating a
transport session. A production caller must derive it from an authenticated
live capability exchange, retain the active configuration context, and pass
the resulting graph through the existing independent deployment admission.
This selector itself grants no resource lease or execution authority.

## Visible TinyBee workflow

The separate offline TinyBee target-I/O draft now exposes two typed selectors:
one of the four reviewed catalog entries and one existing catalog-managed graph
node. Adding a node selects its stable node ID. Rebinding is enabled only when
the destination is unused and differs from the node's current entry. The node
list reports the resolved board resource, while the UI explicitly states that
no label, device ID, digest, class, or GPIO number is text-editable.

The offline reference remains restricted to stable Boolean reads of GPIO22,
GPIO32, GPIO33, and GPIO35. ADC, UART, timers, shifted output, storage, every
other GPIO, raw pin access, output, motion, and deployment remain closed. Its
conspicuous reference device/configuration values cannot identify the connected
board.

## Reproduced checks

Run from `alumina-interface` at the implementation commit:

```console
cargo fmt --package alumina-interface-core --package alumina-interface -- --check
cargo test --workspace --all-targets --locked
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

All 301 executable workspace tests pass: 68 application/coordinator, 82
protocol-client, 149 core, and one public exact-control integration test. Both
warnings-denied Alumina Clippy commands, complete WASM test-target linking,
strict Alumina rustdoc, package-scoped formatting, the local-source and
permissive-license audit, optimized Trunk assembly, WASM validation, and
gzip/Brotli integrity pass.

Focused core tests prove GPIO22-to-GPIO33 replacement, complete target handle
identity, changed canonical bytes, stable node/placement/cursors, and exact
no-op stability. They also reject a duplicate held by another node, unknown
entry/node IDs, an unlisted but structurally valid GPIO2 handle, a
foreign-device catalog, a cross-kind entry, and an unreviewed semantic registry
without mutation. UI tests exercise the same successful transition and
duplicate rollback, add all four reviewed resources at most once, and reset the
separate draft without reusing identities.

The authoritative HostExact identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The final optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,349,550 | `56d1c25ca057629d028e5e65b432d5ef879ead0538b8b0fec8d6eed64983e3e2` |
| `alumina-interface_bg.wasm.gz` | 2,815,147 | `cb9fabb87b3f6f3ba2b725be20988ecdedcc4107fbce44ef1de6dfb1afb3624e` |
| `alumina-interface_bg.wasm.br` | 2,223,900 | `5094dff342e342a870bdd4fa138a516789f94c03bb8edf3a97253d114b616e99` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The optimized bundle and worker loaded from `127.0.0.1:8765` in a fresh
isolated Chromium profile. Runtime inspection found one canvas and the
unchanged `algwp1:` carrier with a 3,755-byte ALGW segment, 407-byte ALGP
segment, and 8,332 total text characters.

The browser then exercised the actual egui controls. It created a GPIO22 stable
input, opened the derived resource list, selected GPIO33, and invoked `rebind
selected node`. The same stable node `#1` remained selected and visibly changed
to GPIO33; the status reported an exact-catalog GPIO22-to-GPIO33 rebind with
deployment still disabled. Visible target-draft ALGW prefixes changed from the
empty `da739d569f6b877…`, through GPIO22 `049b1990f8053051…`, to GPIO33
`5b6378f777378e9e…`. This target draft is separate from browser pair storage,
whose authoritative HostExact bytes stayed unchanged after every click.

The final 1,440-by-900 capture
`/tmp/alumina-resource-selector-browser.png` is 263,512 bytes with SHA-256
`6bd6282c8bcb1d818b1b935c0ef84ba130627401e3b84f8e934bf7a6db148dda`.
It visibly includes the selected GPIO33 catalog entry, stable node `#1`, the
resolved GPIO33 managed-node label, closed raw-identity wording, and the success
status. Chromium and the localhost HTTP server were stopped afterward.
Software-WebGL readback/performance notices and background Google registration
quota messages were emitted; no Alumina application, device, or WLAN error
appeared.

## Closed claims and licensing

This is bounded host authoring support over a digest-verified offline catalog.
It does not authenticate a live MCU, discover an active configuration, persist
the target draft, lower or upload a graph, acquire a resource, read a pin, arm
an output, or command motion. Firmware safety and deployment admission remain
independent authorities.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the current local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
