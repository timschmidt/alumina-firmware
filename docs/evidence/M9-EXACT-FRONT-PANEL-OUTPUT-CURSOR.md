# M9 exact front-panel output cursor

Date: 2026-08-26

## Scope and authority boundary

This slice makes component `OutputIndicator` items hierarchy-safe and useful
against the exact host simulation. It changes `alumina-interface`; this
firmware repository records the shared plan, embedded bundle, browser delivery,
and target-size evidence.

The feature is read-only host inspection. It creates no graph, CAM, deployment,
device-command, pin, timer, motion, or safety authority. Firmware continues to
reject `ALFR`, does not receive the cursor, and does not resolve a component
hierarchy. No cursor action can arm a machine or alter a cached command stream.

## Recursive public-output authority

`GraphFrontPanelOutputKey` identifies one public output by its stable
root-to-nested occurrence path and `GraphComponentOutputId`. Resolution:

1. requires the exact `ALGH` digest already reproduced by the supplied
   flattening;
2. resolves the selected occurrence and public output by stable identities;
3. follows public-output placeholder ports recursively through nested
   components;
4. reaches one final ordinary output in the flattened `ALGW`;
5. requires the public and final value types to agree exactly; and
6. admits only a registered Stream, retaining its exact sample type and clock.

Unknown paths or outputs, path-limit violations, non-Stream outputs, type drift,
and inconsistent hierarchy provenance reject without a result.

Sample selection binds the supplied execution to the current `ALFR` hierarchy,
flattened workspace, simulation registry, and inclusive horizon. It reproduces
the supplied-input analysis used by the run, converts every matching local
clock tick to an exact rational root-clock tick, and selects the latest entry at
or before the cursor. Equal-time entries use deterministic trace sequence as
the final order. A negative or post-horizon cursor, foreign graph, registry or
horizon, wrong output clock, or wrong exact sample type rejects; a current
`ALFR` result never silently falls back to an unrelated reference trace.

## Transient exact UI cursor

Every visible `OutputIndicator` on the selected panel shares one root-clock
cursor. Text admission accepts only a canonical integer or reduced
`numerator/denominator`, with no whitespace, leading-zero spelling, mixed
fraction, or implicit floating-point path. The exact value must lie in the
inclusive simulation horizon. The panel exposes apply, start, end, and
restore-accepted actions and labels whether projection comes from the current
`ALFR`/`ALGT` pair or the reference `ALGT`.

Cursor draft, accepted cursor, and status are transient inspection state.
Moving or rejecting the cursor leaves `ALFR`, `ALGT`, `ALGS`, browser
persistence, and unified undo/redo history byte-identical.

The implementation sources have these identities:

| Source | SHA-256 |
| --- | --- |
| `alumina-interface/src/control_graph_ui.rs` | `de43b3d6ce06e24598512dbd2c874686aaf9989ebb7fe28aa7c70a2695169c0f` |
| `alumina-interface-core/src/graph/front_panel_runtime.rs` | `c693fe447ec61a2dfa367713a68914ccb0363870797a4243468dbab7f4f60930` |
| `docs/GRAPH-FRONT-PANEL-RUN-V1.md` | `2c5b22eed6dd3a03f1cf4c58a2a65b263d40971335c0bb988b8829b4b8633546` |

## Exact regressions

The core nested-output regression constructs a wrapper around a source/sink
component. The outer and inner public-output keys independently resolve to the
same final endpoint. At exact root cursor `3/2`, sample-and-hold selects local
tick `1`, root tick `1`, sequence `101`, and exact Boolean `true`; cursor `1/2`
selects the preceding exact `false`. Unknown occurrence `99`, cursors `-1` and
`4` outside horizon `0…3`, and an execution produced for a different horizon
all reject with typed errors.

The UI regression runs one exact `7/2` input control, retains complete `ALGS`
bytes, unified history, canonical `ALFR`, and replayable `ALGT`, then rejects
noncanonical cursor text `02` atomically. It accepts canonical `3/2`, renders
the hierarchy-resolved output with exact local tick/sequence and root-time
provenance, proves every retained authority byte remains unchanged, rejects a
post-horizon cursor without moving the accepted cursor, and renders the panel
through a headless egui frame. The companion timeline regression still retains
its 277-byte `ALFR`
`dae1e6102b248b612669f50239646b03f705f1ed26cfba52375e1f9db3377acd`
and 9,549-byte `ALGT`
`11e049305202f25806b7b75b608b4c217e1eca6be02eb4ebc89cbfb3ce9fa61f`.

## Native and WASM qualification

The complete current-workspace offline test run passed: 107
`alumina-interface`, 82 client, 196 core, one integration, and one compile-fail
doc test. Strict
`cargo clippy --workspace --all-targets --offline -- -D warnings`,
`cargo fmt --all -- --check`, and
`cargo check --workspace --target wasm32-unknown-unknown --offline` also passed.
The optimized locked/offline Trunk build consumed the current sibling
Hyper/CSGRS working trees; no old published `csgrs` release or compatibility
shim was introduced.

## Maximum-Brotli release and browser delivery

The current embedded manifest is 2,755 bytes with SHA-256
`796da75a271bd34f4825f4208b55c2cfa7041017f8de20945773dbcf235f9a7e`.

| Asset | Source bytes | Source SHA-256 | q11/w24 bytes | Wire SHA-256 |
| --- | ---: | --- | ---: | --- |
| JS | 91,816 | `27f4824be89278070f2879147cab71362c994f827e70084da0fba14250d9e6ea` | 10,904 | `f3c06c85169118fa5f61b66b8f61575e3d7f742b0907b8885b6e5b9952f277e9` |
| WASM | 8,294,700 | `d46d9797f94eafebc922e1e68f737381cad2c6cc16e04f26e3b7080863459ab6` | 2,790,968 | `011455c54b0a38a5a1c4dfd0a0cc09a3beaf1674e82d068b32386cdc43f401a5` |

The retained 2,801,872 bytes save 793,257 bytes (22.06%) versus reproducible
gzip level 9 over the same exact sources. There is no gzip copy or negotiation
path. An exhaustive quality-11 sweep proves w24 ties the smallest standard
encoding: JS wins at explicit windows 17 through 24 and WASM at 23 and 24.
Nonstandard windows 25 through 30 are one byte larger for JS and two bytes
larger for WASM.

A freshly rebuilt loopback simulator served that exact table to Chromium 151.
Every strict resource/header, bootstrap-integrity, decoded-source, installed
WASM binding, 1440×1000 canvas, data-URL favicon, no-failure, and no-exception
check passed. Bootstrap completed in 885.5 ms. The 10,275-byte JSON result has
SHA-256
`7334a81246487c201b31ccb0d21320605c4aa08ab37e552b4c439e47eddada5e`;
the visually inspected 229,558-byte PNG has SHA-256
`be566144c6a9d214e12264173ad329ddc9fa6f3bbba8756853031024a61fccf5`.
Browser startup proves exact release delivery and execution. Cursor interaction
is covered by the exact native core/UI and rendered-egui regressions, not
claimed from the generic startup screenshot.

## Target, physical, and license boundary

Both TinyBee variants link the complete interface. The 8 MiB primary ELF is
`fb02ac59325fa39bbfba8af90dcd1bd38cdc0843dd9d9941cdc25cedbe0af347`.
Its app uses 4,169,680 of 8,323,072 bytes (50.10%); the 4,235,216-byte merged
image is
`0e2d434236648b92741c71009ceb9f13dfd5f61b56d830d88a17e414b4492689`.
The opportunistic 4 MiB ELF is
`0ed3f9f5f0e4f7fdd8b498dd209082afda46bb40c02ddcae947278660cf94a83`;
its 4,169,696-byte app exceeds the 4,128,768-byte partition by 40,928 bytes, so
no 4 MiB merged image exists and no interface function was removed to make it
fit.

This slice did not flash or contact the bare USB-powered MKS TinyBee V1.0. No
motor or motor power is connected, and the earlier serial disappearance before
write verification remains an explicit physical qualification boundary. There
is no new boot, AP, GPIO, timing, safety, or motion claim.

The interface is MIT. Firmware, bundle integration, and this evidence are
`MIT OR Apache-2.0`; the dedicated decoder retains its permissive
BSD-3-Clause/MIT notice. No GPL/AGPL/LGPL/SSPL-family source, dependency, or
asset was introduced.
