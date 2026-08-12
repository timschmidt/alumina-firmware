# M9 capability-bound graph-input evidence

Date: 2026-08-12

## Scope

This checkpoint advances deployed graph IR to V2 and admits its first physical
operation: a realtime read of one known, fresh, debounced safety-input semantic
state. It deliberately adds no raw GPIO read, output, motion, PWM, ADC, storage,
network, or arbitrary-code authority.

The reviewed implementation commits are:

- `aluminafw` `772cea7` (`Publish exact graph executor capabilities`);
- `aluminafw` `e64671f6fd692d8905c9540c2764d83905149d2c`
  (`Admit capability-bound graph safety inputs`); and
- `alumina-interface` `9088e9ceb5d7dae428412dc129c530b67d213cb0`
  (`Lower capability-bound graph inputs`).

No connected hardware was read, erased, flashed, reset, or driven. The
available MKS TinyBee V1.0 remained a bare board without motor power or motors.

## Exact capability authority

Canonical capability documents are V2 (`ALMCAP02`) and include an
`ALMGRC02` graph-executor section. That section publishes the exact deployed IR
version and package size, record and per-channel queue ceilings, five split
arena reservations, implemented opcode descriptors, and typed
resource/class/access tuples. The TinyBee 8 MiB primary and 4 MiB opportunistic
variants publish the same executor shape and four read-only resources: GPIO33,
GPIO32, GPIO22, and GPIO35. Each is class 1 with only
`StableBooleanInput` access. T-Deck Pro and MKS ESP32 FOC implement the opcode
but publish no graph-addressable physical resource.

The canonical V2 capability identities are:

| Board package | Document bytes | SHA-256 |
| --- | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 3,435 | `0e82513896e52e0a58fb92de9130c446d590bf649fbc22742209b2d04c8cb0a5` |
| TinyBee V1.0, 4 MiB opportunistic | 3,448 | `ba06ffad44125a4cf5b72ba1a14296a0fb3d91f4364af050616bdf0cebb0ed04` |
| T-Deck Pro | 2,725 | `6c37b509080f40a0ea54e86b9f9aadfed4d284c97494f7e3275af6b3905a8061` |
| MKS ESP32 FOC V1.0 | 2,928 | `627b2c018f44013dec83f1ff118f4158b4de31bb2db6d022c2979a8fd053107f` |

The interface has no production default for graph hardware limits.
`GraphDeploymentLimits::from_capability_document` independently hashes and
decodes the complete document, copies the exact executor limits and palettes,
and requires its identity to match the selected target. `ALDI` V2 binds that
complete identity, all split limits, every opcode/resource entry, the audited
semantic registry, and fixed implementations. A typed resource handle must also
match the target MCU, board-package digest, resource class, and canonical
selector. A general board resource that is absent from the graph palette does
not gain graph authority.

## V2 package and fail-closed execution

Deployed graph packages now use `ALGRIR02`; install, selection, and run bodies
use `ALGRPQ02`, `ALGRPS02`, and `ALGRPR02`. The V2 node parameter for
`StableBooleanInput` contains the shared four-byte typed resource selector in
its low 32 bits and reserved-zero high bits. Allocation-free package decode
rejects noncanonical selectors before lifecycle admission.

Both firmware cores independently admit every node against the selected board
image's static opcode and resource palettes. The live realtime task supplies
resource values only through `SafetyInputMonitor::stable_active_by_resource`
after normal physical sampling and safety reconciliation. An unknown resource,
pre-debounce state, future-dated observation, or first cycle beyond the sample
watchdog returns no value. The executor latches first-cause
`ResourceUnavailable` and stops rather than substituting a false/clear input.
The operation emits one canonical timestamped Boolean Stream item and has no
side effect.

Host cross-repository tests construct the complete TinyBee 8 MiB capability
document through its byte-range API, lower a typed GPIO33 handle, and execute
the emitted package through the firmware's permanent actor types. GPIO34 is
present in the board inventory but absent from the graph palette and is
rejected. A mismatched target capability digest is also rejected. Independent
runtime tests reject a missing opcode and a different resource selector, then
prove successful value delivery and the unavailable-sample terminal fault.

## Verification

The firmware checkpoint reproduced:

```console
cargo fmt --all
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
cargo run -p xtask --locked --offline -- build --board mks-tinybee --profile release
cargo run -p xtask --locked --offline -- build --board mks-tinybee-4mb --profile release
cargo run -p xtask --locked --offline -- build --board mks-esp32-foc-v1 --profile release
cargo run -p xtask --locked --offline -- build --board t-deck-pro --profile release
git diff --check
```

All 369 default-member portable tests and all default-member doc tests pass.
Strict portable Clippy and rustdoc pass. All four board-qualified release images
link:

| Linked target ELF | Bytes | SHA-256 |
| --- | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 10,472,924 | `9ec3291e9ca74d12696fdce6495e076e35190bf94b9459191ad078a3daa6c7a8` |
| TinyBee V1.0, 4 MiB opportunistic | 10,472,128 | `2369724ac78e50361206be28447758370eee0bbdef43a9807ef97ea1a6b3b8b1` |
| MKS ESP32 FOC V1.0 | 9,943,132 | `7c233526d43c6905b55a2b94958510881ca1c989d9be50e0cee74e409a76a5fc` |
| T-Deck Pro | 10,340,852 | `4ebdc08bb24895f1dd9244898d252ee630655d121591fd27d5920d05090fe9db` |

These are ELF lengths, not flash payload lengths. The firmware lockfile is
68,586 bytes with SHA-256
`ea3d7f4d4ee9a2e42fd0e2ec19941e812ad8d9a1259d607caa8ffdad8c5260f4`.

The interface checkpoint reproduced:

```console
cargo fmt --all
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 100 native unit tests and all doc tests pass. Native and WASM strict Clippy,
strict rustdoc, the checked-out sibling CSGRS/Hyper source and permissive-license
audit, optimized Trunk build, WASM validation, and compression checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,260,705 | `aaac9e3876155edbc161d9f3f71bcd1c08f1fb5996d928a451c6fdcd0e0cebf7` |
| `alumina-interface_bg.wasm.gz` | 1,980,128 | `a6a4f3dab43ebb1d4fc856d252a5823b62918ad2a2cf6af7dce2039cd6235664` |
| `alumina-interface_bg.wasm.br` | 1,616,161 | `ebf3031224b643bf7a3259521f5478140d5a44e3ff52c798b999036934b52257` |
| `alumina-interface.js` | 89,162 | `f8b05022a8ad3f4b507aa1d6c1b738ac1606779374cc73bb8c2dbe4f612e5116` |
| `index.html` | 1,295 | `93e098bef45ac353ed3c0d10bc4f82a77cea2adcbb13171ac2ed2dac385468e9` |

The interface lockfile is 96,792 bytes with SHA-256
`789484967e2659c753722fab8ab5c21b6f2765195d95b17e7c7fa1056846989b`.

## Closed claims and next gate

There is no physical TinyBee input observation, board boot result, target graph
timing, measured executor WCET, Wi-Fi/interrupt coexistence measurement, or
graph-owned side effect. No graph operation can arm, clear a safety fault, drive
an output, or bypass the fixed safety state machine. The next gate is
disconnected TinyBee timing/input HIL with explicit wiring instructions and a
logic-analyzer capture, followed by safety-bounded output resource design only
after that input path is physically qualified.
