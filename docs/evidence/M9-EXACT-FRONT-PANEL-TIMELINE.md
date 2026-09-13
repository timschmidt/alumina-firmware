# M9 exact front-panel timeline execution

Date: 2026-08-26

## Scope and authority boundary

This slice makes component `InputControl` items executable in the authoritative
browser/WASM host simulator and adds bounded multi-change timeline authoring.
It changes `alumina-interface`; this firmware repository records the shared
delivery-plan, bundle, and target-size evidence.

Canonical `ALFR` V1 is host-only simulation authority. Firmware does not parse
`ALFR`, receive a panel draft, interpret graph hierarchy bytes, or derive a
device command from this work. The slice allocates no board resource, claims no
WCET, arms no machine, commands no pin, and grants no deployment or safety
authority. CAM, exact motion lowering, cached device streams, and authenticated
start remain separate explicit boundaries.

## Exact authority and timeline model

Each control is keyed by its stable root-to-nested component occurrence path
and `GraphFrontPanelItemId`. Resolution is reproduced from the exact `ALGH`
digest and hierarchy flattening. It follows stable public-input identities
through nested placeholders to one final ordinary `Stream` input and retains
that Stream's exact sample type and clock.

Authority fails closed:

- an authored structural wire owns its target and makes the panel control
  inactive;
- if multiple panel levels reach the same unowned target, the shallowest stable
  occurrence wins and every other control remains visibly superseded;
- non-Stream, missing, foreign, connected, superseded, mistyped, duplicate, or
  oversized schedules reject; and
- every active control must have exactly one schedule before a run exists.

The UI retains one transient schedule draft per stable control key. Its first
row is mandatory local tick `0`. Later ticks use canonical unsigned decimal
`u64` syntax with no sign, whitespace, or leading zero and must be strictly
increasing. Values use the shared bounded schema-directed exact graph-literal
parser; there is no floating-point or implicit unit-conversion path. Visible
actions add a row, remove a noninitial row, reset to one constant row, or run
the complete active-control set. The interactive policy permits 4,096 changes
per control, 65,536 total changes and expanded samples, one million inclusive
root ticks, path depth 64, 4,096 controls, and 16 MiB of canonical `ALFR` bytes.

Successful construction sorts controls canonically, binds the exact `ALGH`,
flattened `ALGW`, simulation registry, root clock, and horizon, then expands
each sample-and-hold schedule at every tick of its own Stream clock. Conversion
uses analyzed exact rational rates. The ordinary deterministic simulator emits
`ALGT` V2 with injected-input entries distinct from caller external-source and
modeled-output entries. Independent `ALFR` replay re-resolves authority,
revalidates values and limits, and requires byte-identical re-encoding;
independent `ALGT` replay regenerates the complete simulation.

Every draft text or row edit immediately discards prior `ALFR`/`ALGT` runtime
evidence. Drafts and successful runs do not enter `ALGS`, browser persistence,
or undo/redo history. The canonical authoring session therefore cannot mistake
an exploratory panel run for saved control authority.

The implementation sources have these identities:

| Source | SHA-256 |
| --- | --- |
| `alumina-interface/src/control_graph_ui.rs` | `8f2b36eb986256080b6fc0490a88a351bbf77f213ff549409ead54ba0f18595b` |
| `alumina-interface-core/src/graph/front_panel_runtime.rs` | `f5832260413e3c9acf2566d7daf562d590de49e63d4a11f82f95861f06fb8993` |
| `docs/GRAPH-FRONT-PANEL-RUN-V1.md` | `d3fd2b1eb961f373a44e87bcb9d0ef513984a2c2929e469f0559bee84ab646f7` |

## Exact regression identity

The end-to-end UI regression constructs an active rational Stream control,
first rejects noncanonical tick text `02`, and proves the complete `ALGS`
encoding and history are unchanged. It then runs local changes `0 -> 0`,
`2 -> 7/2`, and `4 -> -1`, verifies the expanded sample-and-hold values,
independently replays the run, and renders the selected timeline through a
headless egui frame.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| canonical `ALFR` V1 | 277 | `dae1e6102b248b612669f50239646b03f705f1ed26cfba52375e1f9db3377acd` |
| replayable `ALGT` V2 | 9,549 | `11e049305202f25806b7b75b608b4c217e1eca6be02eb4ebc89cbfb3ce9fa61f` |

## Native and WASM qualification

The complete current-workspace test command passed offline:

```console
cargo test --workspace --offline
```

Results were 106 `alumina-interface`, 82 client, 195 core, one integration,
and one compile-fail doc test, all passing. Strict
`cargo clippy --workspace --all-targets --offline -- -D warnings`,
`cargo fmt --all -- --check`, and
`cargo check --workspace --target wasm32-unknown-unknown --offline` also
passed. The optimized Trunk build completed with locked offline dependencies
against the current sibling Hyper/CSGRS source graph. No sibling repository was
pinned, formatted, or modified by this slice.

## Maximum-Brotli release and browser delivery

The final current-source release was captured under manifest identity
`9ec8323f055cf8de30215a7ce9b04ee980e74bd1039d4b6eb57a4fc6be3d584a`.
The exact 91,816-byte JS and 8,285,445-byte WASM sources become 10,911 and
2,787,315 bytes respectively at standard q11/w24. An exhaustive quality-11
window sweep proves these retained streams tie the smallest standard encoding;
nonstandard windows 25 through 30 are larger. Their combined 2,798,226 bytes
save 792,315 bytes (22.07%) versus reproducible gzip level 9. There is no gzip
copy or content-negotiation compatibility path.

A fresh Chromium 151 profile loaded the exact embedded asset table from the
loopback simulator. All resource identities and headers, integrity-pinned
decode evidence, installed WASM bindings, 1440x1000 canvas, local data-URL
favicon, and no-failure/no-exception checks passed. Bootstrap completed in
877.7 ms. The 10,291-byte result JSON has SHA-256
`13cf8fde67cf072dec52336e23c5ed821347014eac32e5c8c90b2f1b01157ffb`;
the 229,558-byte screenshot has SHA-256
`be566144c6a9d214e12264173ad329ddc9fa6f3bbba8756853031024a61fccf5`.
This proves release delivery and application execution. Timeline interaction is
covered by native exact/UI tests, not claimed from that generic browser-startup
session.

## Target and physical boundary

Both TinyBee variants relinked with the complete release. The 8 MiB primary
uses 4,166,032 of 8,323,072 app bytes (50.05%); its 4,231,568-byte merged image
has SHA-256
`a15f168fe6747aa4c3a7c5110f1cde58d35c057f58bf4a9058350cf56c225727`.
The opportunistic 4 MiB ELF links, but `espflash save-image` rejects its
4,166,048-byte app as 37,280 bytes larger than the 4,128,768-byte partition.
No UI function was removed to force that optional variant to fit.

The bare USB-powered MKS TinyBee V1.0 had no motor or motor power connected.
`espflash board-info` reconfirmed ESP32 revision 1.0, dual cores, and 8 MiB
flash. A current-image write reached the exact app-size report, after which
`/dev/ttyUSB0` disappeared before verification; the stalled operation was
stopped. No successful write, boot, AP, physical browser, pin, timing, or motion
claim follows from that attempt.

## License boundary and deferred work

The interface is MIT and the new firmware-side bundle/evidence machinery is
`MIT OR Apache-2.0`. The dedicated decoder uses permissive BSD-3-Clause/MIT
code with its notice retained. No GPL/AGPL/LGPL/SSPL-family source, dependency,
or asset was introduced.

Responsive/grouped panel layout, permissions/signatures, live authenticated
device controls, and promotion of an exact host run through explicit
CAM/deployment authority remain open. Arbitrary-cursor output indicators close
in the subsequent
[`M9-EXACT-FRONT-PANEL-OUTPUT-CURSOR.md`](M9-EXACT-FRONT-PANEL-OUTPUT-CURSOR.md)
slice. Firmware continues to reject `ALFR` as an input format.
