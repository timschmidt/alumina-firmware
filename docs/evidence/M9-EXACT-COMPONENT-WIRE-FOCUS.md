# M9 exact component-wire focus evidence

Date: 2026-08-22

Authoritative interface source:
`3313b720ca21fecaf950a4a93105ce10424ce373`
(`feat: highlight exact component wire sources`)

This checkpoint closes exact transient selection and visible highlighting of a
component-local wire reached from final `ALGM` provenance. It extends the
earlier final-to-source navigation boundary without adding a second canonical
selection field, compatibility path, or firmware command.

This is host UI navigation only. It grants no firmware, network, storage, GPIO,
motion, arming, timing, start, or safety authority.

## One revalidated transient authority

The source browser's existing `last_opened` final/origin pair remains the sole
wire-focus authority. Every accepted pair has already been revalidated against
the current fresh flattening, complete occurrence path, exact component digest,
admitted dependency, and retained local wire. `opened_component_wire`
therefore derives an optional local wire ID from that exact transient pair; it
introduces no parallel selection state and encodes no canonical bytes.

Both component destinations consume the same derived identity:

- an authoritative source opens the main control canvas;
- a non-authoritative source opens the selected-definition canvas; and
- both display the exact local wire ID and endpoints in `LIGHT_GREEN`, while
  retaining the target node for the existing node inspector.

The canvas passes that identity into the ordinary wire painter. An exact opened
source has precedence over incident-node highlighting: its complete path and
arrowhead use a 3.6-pixel `LIGHT_GREEN` stroke, selected-node incident wires
remain white at 2.4 pixels, and ordinary typed wires retain their 1.5-pixel
type color.

A forged open retains the last valid revalidated selection. If a later
component edit recursively remaps the component digest and the exact
final/origin pair disappears, existing source-browser reconciliation clears
the wire focus and pending scroll rather than guessing an identity in the
rebuilt definition.

## Native and adversarial coverage

The complete qualification passed:

- 98 application/coordinator tests;
- 82 client tests;
- 183 core tests;
- 1 exact-control integration test; and
- 1 compile-fail rustdoc test.

That is 365 unit, integration, and compile-fail checks. Expanded regressions
prove:

- opening an authoritative component node leaves wire focus absent;
- opening authoritative final wire `w1` derives exact local `w1`, retains
  its target node, and selects the 3.6-pixel `LIGHT_GREEN` stroke even though
  that node would otherwise make the wire white;
- a forged later origin changes only rejection status and retains the last
  valid exact wire focus;
- a private wrapper wire created through ordinary monotonic definition
  authoring is located from exact occurrence `[1]` as local `w2`;
- opening that private wire selects the wrapper definition and target-node
  inspector without changing `ALGS`, the component package, history, or
  persistence; and
- editing that target recursively remaps the wrapper digest and clears the
  now-stale wire focus and one-shot scroll.

## Native, WASM, source, and release qualification

The following checks passed against the live, read-only workspace CSGRS/Hyper
paths:

```text
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --no-deps --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown --no-deps --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --offline
env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run --offline
bash scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
wasm-validate dist/alumina-interface_bg.wasm
gzip/brotli integrity and byte-exact decompression checks
git diff --check
```

The strict `--no-deps` native and WASM interface lints were warning-free.
Hypercurve was being edited concurrently and emitted its current read-only
warnings for one unused diagnostic closure parameter and three unused internal
helper groups. No Hyper/CSGRS repository state was inspected or changed.
`Cargo.lock` remained SHA-256
`c40eedd3b67fa82583151fa5b74fc3721f6278d6592c729d67aa63bfa05605d5`.

The source-policy audit accepted only sibling Alumina/CSGRS/Hyper paths and the
permissive native/WASM license inventories.

| Optimized artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `index.html` | 1,295 | `c0577ebc4c5a579da13ed94d80300f3016c2899b51632bbfb72954cb446b81eb` |
| `alumina-interface.js` | 91,816 | `a37c8e9ad58c8061b25e4085ca56c403db168a9d5186decbcb0e44dccb2269e7` |
| `alumina-interface_bg.wasm` | 6,776,083 | `fd5430e8b8c35bf7e7ffe300ea3314dbdd2ae90249d16c6ef13f65f4ab41e55b` |
| WASM gzip | 2,982,583 | `0406d7f8f89dbb12002b045415288a4a3586571437a3d2c54273eb7852feec24` |
| WASM Brotli | 2,346,121 | `2dd93510fdfcae42bb2cd83715cfe39e4006761811dd6b70736b1f420b7e2e6d` |

Both validators accepted the optimized module. Independent gzip and Brotli
decompression reproduced the uncompressed WASM byte-for-byte.

## Optimized browser focus

The application and proof script addressed only the optimized bundle through
loopback HTTP and an isolated Chromium debugging port. The proof cleared only
disposable origin-local persistence, opened the visible Control graph, and
used the visible flattened-source controls.

It opened final node `n3` at exact authoritative occurrence `[1/1]`, local
`n1`, then opened final wire `w1` at the same occurrence, local `w1` from
`n1.p1` to `n4.p1`. The destination visibly reported:

```text
Exact ALGM source wire w1 selected below · n1.p1 → n4.p1 · target node #4 remains available in the inspector
```

The target `#4 Setpoint 50 Hz to 10 Hz` remained selected in the inspector.
Both opens retained the exact 14,770-byte reference `ALGS` with SHA-256
`d7a5fba83da9f254eb0d50eab301129f933016a400c9d154c5f2d97d8029cf9d`.
A fresh reload retained those exact bytes and reset the transient source status
to its initial prompt.

| Transient browser evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| source browser before node open | 299,128 | `4196c43cfe3c8d8b12d0051b2b20bc8373730234916de87e26081df3096ead21` |
| authoritative node destination | 307,168 | `d42e225a4a6466c97110570278421fecc488a4a60a7c5051a512af7852b13f29` |
| visible exact-node success | 308,220 | `dbcb53f3bd86af36c71bd83801e0d0014cdc614a0655d075191ed8a541345d83` |
| authoritative exact-wire destination | 311,910 | `89d6860d2335fd0d8d9f3bcd8d0ec39206e904dffef8c6f807caf133516076c4` |
| visible exact-wire success | 311,390 | `e8171413add6b474369d964dfff5a47ae0ff73ca5a66b4661e2b7de470193577` |
| source browser after fresh reload | 300,092 | `5423f61bf75ae1ff2b135f194b55497885f6b7d4a465e46067d868942fba50c2` |

The retained parsed result is 1,847 bytes with SHA-256
`af12d28874bad849f93cbb71f05b4b30a643f11b896a2969f5ceefc0434782b4`.
Its retained OCR projection is 4,856 bytes with SHA-256
`c616806855187c72ec721450607273f00aa3d374fd12129fe343e896cbc52fd3`.
Screenshots, bounded locator frames, OCR, and the result remain transient
`/tmp/alumina-wire-focus-proof.lA98AM` evidence and are not repository inputs.
Failed/intermediate captures and the disposable Chromium profile were removed;
Chromium and the loopback server were stopped.

## Hardware, network, licensing, and closed claims

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, read, or
configured. No serial port, GPIO, motor/driver/process power, analyzer, board
AP, or Alumina device-network path was used. Workstation Wi-Fi and
NetworkManager were not changed.

Hypercurve remained a moving read-only dependency. This work did not inspect
its repository state or edit, format, pin, stage, or commit any Hyper/CSGRS
repository. The implementation is independently authored under MIT. The
source-policy audit found no GPL-family dependency, copied external
planner/control implementation, compatibility shim, or retired-repository
path.

Exact transient component-local wire identity, authoritative/private canvas
focus, distinct stroke and arrowhead highlighting, retained target-node
inspection, canonical/history/persistence isolation, forged-origin retention,
and stale-digest reconciliation are now closed. Child rebinding/replacement,
nested binding import/exchange, component rename/version evolution, parameter
promotion/overrides, coordinated descendant/control-authority replacement,
runtime panel injection, collaboration/conflict handling, and crash-durable
journals remain separate work.
