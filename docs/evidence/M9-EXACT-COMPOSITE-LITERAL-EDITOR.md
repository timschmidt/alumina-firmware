# M9 exact composite-literal editor evidence

Date: 2026-08-20

This checkpoint replaces the graph editor's scalar-only parameter text path
with one bounded schema-directed exact literal notation shared by native and
WASM code. Arrays, records, options, results, text, and bytes can now cross the
same transactional parameter boundary as exact numeric and Boolean values
without adding floats, JSON authority, or another persistence format.

## Source boundary

- Interface implementation:
  `b75b67315d868daff668159fa8c018d8b00f1ca9`
  (`feat: edit exact composite graph literals`).
- Preceding firmware evidence checkpoint:
  `be2e151d1f0814975a098ea8ae36cfe75a265e98`.
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

## Exact editor notation

`alumina-interface-core` now publicly exposes
`format_graph_literal_text`, `parse_graph_literal_text`,
`GraphLiteralTextLimits`, and their typed error boundary. The notation is
explicitly an editor representation, not a saved document, transport, or
machine-command format. A successful edit still becomes an exact
`TypedGraphValue`, then passes complete candidate-workspace validation and
canonical `ALGR`/`ALGW` encoding. Existing canonical identities do not depend
on the editor spelling.

The registered type directs every token:

| Type shape | Deterministic editor text |
| --- | --- |
| Boolean | `true` or `false` |
| exact rational | existing exact Hyperreal notation, such as `-7/9` |
| measurement interval | `lower..upper` with two exact rational endpoints |
| signed/unsigned lattice count | exact decimal integer |
| text | quoted UTF-8 with `\"`, `\\`, `\n`, `\r`, `\t`, `\0`, and `\u{...}` escapes |
| bytes | lowercase `hex"00ff"` |
| array | `[value,value]` under the registered homogeneous element type |
| record | `{field:value}` in canonical schema field-ID order |
| option | `none` or `some(value)` |
| result | `ok(value)` or `error(value)` |

Whitespace around structure and uppercase input hexadecimal are accepted, then
formatted back to one deterministic spelling. No numeric token is parsed
through binary floating point. Record order is not silently rearranged at the
mutation boundary: missing, extra, duplicate, or reordered fields reject.

The parser checks source bytes before allocation, follows the schema's acyclic
type tree, applies exact depth/value-node/array/text/blob/rational bounds, and
requires complete input consumption. The formatter validates the complete
typed value first and checks every output append. A successful result is
revalidated by `TypedGraphValue::try_new`; no partial value is returned.

`GraphLiteralTextLimits::interactive()` admits at most 2,097,158 UTF-8 bytes in
one source or formatted result. That is sufficient for the complete one-MiB
graph byte-literal ceiling plus `hex""` delimiters. Scalar UI fields use tighter
type-derived limits. A composite whose textual expansion exceeds the
interactive policy remains present in canonical graph bytes but is read-only
in this text field.

## Identity and runtime authority

Resource and job handles are intentionally not writable as text. Their shape
alone cannot prove that a resource belongs to the authenticated capability
document or that a job belongs to the authenticated cache ledger. Both direct
handle roots and handles nested inside another composite return
`IdentityRequiresSelector`. A later resource/cache selector must supply the
full admitted identity before transactional replacement is allowed.

Runtime `Event` and `Stream` types still have no literal and return
`RuntimeOnly`. The editor cannot synthesize a device event, stream sample,
resource grant, deployment operation, or motion authority.

Both the node inspector and canonical component front panel now use the shared
formatter/parser. Apply first parses to a complete typed value, then clones and
validates a candidate workspace through the existing history/persistence
boundary. Invalid source leaves the graph, ALGW/ALGP pair, history, attached
trace, and draft text authority unchanged. Selector-bound or over-policy values
are visibly read-only.

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

All 296 executable workspace tests pass: 67 application/coordinator, 82
protocol-client, 146 core, and one public exact-control integration test. Both
warnings-denied Alumina Clippy commands, complete WASM test-target linking,
strict Alumina rustdoc, package-scoped formatting, the local-source and
permissive-license audit, optimized Trunk assembly, WASM validation, and
gzip/Brotli integrity pass.

Focused tests cover every scalar/composite spelling, both option and result
branches, escaped control and Unicode scalars, uppercase-to-lowercase hex
normalization, harmless whitespace, full signed/unsigned limits, exact
rational/interval retention, all strict UTF-8 prefixes, malformed and trailing
input, record order, array/schema limits, reversed intervals, source/output
ceilings, direct and nested handle closure, runtime-only closure, and the UI
adapter's deterministic composite spelling.

The reference canonical identities remain unchanged:

| Canonical object | Bytes | SHA-256 |
| --- | ---: | --- |
| reference `ALGW` | 3,755 | `bf5135c39b67c46a3a5908d4d0d8a1d13d065b59231890e8fbdda818f064ae16` |
| seven-series triggered `ALGP` V2 | 407 | `50955da7b4464a02f6e3eace1d51a3e9d08f56cdfa9c661259c33195b15ef223` |

The final optimized application artifacts are:

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 6,338,265 | `93a3dbaba1711e6060a2df27d326a579422e78e06720b8be5746fa47d2dbc0e2` |
| `alumina-interface_bg.wasm.gz` | 2,811,083 | `07a26f3366a57701891e4e23826d9a3beaba01ba844a1649901dd8238cf941fd` |
| `alumina-interface_bg.wasm.br` | 2,220,281 | `367258087cf7027cac3562d7050b32651d538de8f56026e9f93b3a0cde433e7e` |

The unchanged 99,392-byte `Cargo.lock` has SHA-256
`e36aac3c277ef7e0b89a2aa319593deae02073c91d8fcb235affa2499f41029c`.

## Browser runtime evidence

The final optimized bundle and worker loaded from `127.0.0.1:8765` in a fresh
isolated Chromium profile. Runtime inspection found the unchanged `algwp1:`
carrier with a 3,755-byte ALGW segment, a 407-byte ALGP segment, and 8,332 total
text characters. The graph workspace visibly states schema-directed
scalar/composite editing and selector-bound resource/job identities.

The 1,440-by-670 capture `/tmp/alumina-probe-metadata-browser.png` is 225,728
bytes with SHA-256
`10ca2e2d9dc94d80b377d51cd2164f9c8be4497053533c4cb3fc556c259ec5e9`.
The localhost Chromium and HTTP server were stopped afterward. Chromium emitted
one background Google registration quota message; no Alumina application,
device, or WLAN error appeared.

The reference controller currently has exact-rational parameters rather than a
manufactured composite fixture. Public core round-trips, strict-prefix tests,
and the direct UI adapter regression are therefore the composite behavioral
evidence. This checkpoint does not claim a general component-authoring UI or an
identity selector.

## Closed claims and licensing

This is exact bounded HostExact authoring support. It does not change graph
execution, firmware graph IR, protocol bytes, target memory, I/O, safety, cache,
or motion authority. Canonical ALGR/ALGW replay remains the sole saved-document
boundary.

The implementation is independently authored under MIT and adds no dependency.
The source-policy audit accepted the current local Alumina/CSGRS/Hyper stacks
and permissive native/WASM inventory. No GPL-family source, library, generated
asset, or tool output was copied, linked, or vendored.
