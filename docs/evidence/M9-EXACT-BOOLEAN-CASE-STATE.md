# M9 exact Boolean case and visible state composition

Date: 2026-08-26

Status: implemented and qualified as a bounded `HostExact` graph subset. It has
no firmware opcode, deployment authority, resource claim, or physical-output
authority.

## Result

The fixed simulator now admits three additional audited kind/version bindings:

- `control.bool.constant` V1 emits its Boolean parameter on every declared
  output-clock tick;
- `control.bool.case` V1 selects between explicit false and true same-clock
  Boolean Stream inputs; and
- `control.bool.delay` V1 binds the existing read-before-write `UnitDelay` to a
  Boolean Stream and exact Boolean initial state.

`UnitDelay` admission is now correctly typed rather than rational-only: input
and output must be the identical Stream type, and the initial parameter,
`NodeStateContract` type/clock/ports, and canonical storage bound must all
match. Runtime state was already a complete `TypedGraphValue`; rational delay
semantics are unchanged.

The case has three distinct required bounded Stream inputs, identical Boolean
sample type and clock, complete feedthrough declaration, one matching Boolean
output, no parameter, no hidden state, and no rate transition. Both branch
samples must exist at each tick. Every malformed binding tested—wrong constant
parameter, aliased case ports, and incorrect delay output—fails registry
admission without execution.

## Explicit reset priority

The regression graph composes the state machine visibly:

```text
set_case = case(set, current, set)
next     = case(reset, set_case, false_constant)
current  = delay(next, initial=false)
```

The two external Boolean sources pass through independently audited
latest-at-or-before transitions from 50 Hz to 10 Hz. At the simultaneous
`set=true, reset=true` tick, the outer true branch makes `next=false`; reset is
therefore visibly dominant and fail-safe. The current-state trace is exactly:

```text
false, false, true, true, false, false, true
```

The prior state remains true during the conflict tick because every delay is
read before write, then becomes false on the next tick. A later set arms it
again. Reversing every caller-owned external sample produces the identical
simulation, and canonical `ALGT` replay independently regenerates every entry.

## Editor and identities

The audited palette grows from 13 to 16 fixed kinds. Boolean constant and delay
prototypes use reviewed false-safe defaults; the case prototype has no hidden
parameter. UI regression covers all three prototypes, monotonic transactional
node creation, exact Boolean editing, and rejection of a non-Boolean literal.

The representative PID/interlock `ALGR` remains unchanged at
`96a3348264a9b65d267b45f9a6419a44ee60473fd961abcf4436295e10b3735f`.
The expanded semantic/implementation authority changes `ALSI` V2 to
`5bef8dfb024b548cecd6fb023aba789aeca1a290809fe4bf022559cb6b3d3b16`.
Its unchanged 8,292-byte reference sample sequence is consequently rebound as
`ALGT` SHA-256
`80c98efb242980eab77ae750b7bfa3736cf0bf69496c0053233c17c5ff2da283`.
Canonical front-panel run and injected-trace identities were also renewed by
tests rather than accepted under their prior registry binding.

The exact implementation sources are:

| File | SHA-256 |
| --- | --- |
| `crates/alumina-interface-core/src/graph/simulation.rs` | `5b1b4bf28e8ff86a2f92d73f03b9ba99aff25100820f353b4a0475be8cac0d03` |
| `crates/alumina-interface-core/src/graph/control_fixture.rs` | `22de7b3571fb2af4e9b3d9be42f3be6d0455c69d43e528418c73177e9abe28d2` |
| `src/control_graph_ui.rs` | `94e788faf9d8b51191964de84eebf75b998c0fdc65e6bf2715aaefc85eeae6eb` |
| `docs/BOOLEAN-CASE-STATE-V1.md` | `72827bb3b549dba9bd0bb8b4272d8bd561388cc484a8b75c7b7fc7faeda80d53` |

## Maximum Brotli bundle

The release WASM was rebuilt offline from the current workspace stack, passed
`wasm-tools validate`, and was embedded with standard RFC 7932 Brotli
quality 11/window 24. There is no retained gzip resource or negotiation shim.

| Asset | Source bytes | Source SHA-256 | q11/w24 bytes | Wire SHA-256 |
| --- | ---: | --- | ---: | --- |
| JS | 91,816 | `2fcfa36597fb9199cee0ea4e470ade786198966ef6d8703fead0496346ec5ce2` | 10,901 | `f6d92ce543c522855da1bdaeb6e0cd127e532f6d0931c91bd1670ebed582925f` |
| WASM | 8,299,556 | `ba2162bfdc7010b46492350d7084a92248cdaf9efa5353b7c069187fe93f01f2` | 2,793,314 | `3c137ddb6dedae89c871d9598c0cb37fcf00567d137792e9508484ddc7530b18` |

The retained total is 2,804,215 bytes. Reproducible gzip level 9 would use
3,596,845 bytes, so Brotli saves 792,630 bytes (22.04%). The 2,755-byte exact
bundle manifest is
`84c15ed4218e86319d869d17d01d182693805e889d28f02461f313bd188862b2`.

An exhaustive q11 sweep proved that JS windows 17–24 tie at the 10,901-byte
minimum and WASM windows 23–24 tie at the 2,793,314-byte minimum. The
incompatible large-window extension grows those streams to 10,902 and
2,793,316 bytes. Standard w24 is therefore simultaneously the largest standard
window and a byte-minimum winner for both exact assets.

## Browser and board qualification

A fresh Chromium 151 profile loaded the exact simulator-served bundle at
1440×1000. Every strict resource/header/bootstrap/WASM/canvas/favicon/failure
check passed, bootstrap completed in 733 ms, and Chromium reported no loading
failure or uncaught exception. The final artifacts are:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| result JSON | 10,592 | `d54a5c07532f5dce1ccf99733d7d623dc88464cea88265726057822b7fb16fc7` |
| screenshot PNG | 229,558 | `be566144c6a9d214e12264173ad329ddc9fa6f3bbba8756853031024a61fccf5` |

The screenshot is byte-identical to the previously visually inspected
qualification. The strict harness initially rejected its own stale expected
root identity after regeneration; updating that exact pin, rather than
weakening the check, produced the passing run.

Release links passed for TinyBee 8 MiB, TinyBee 4 MiB, MKS ESP32 FOC V1,
T-Deck Pro, and the current T-LoRa Pager stub. TinyBee measurements are:

| Variant | ELF SHA-256 | App bytes / partition | Merged result |
| --- | --- | ---: | --- |
| 8 MiB primary | `3fae25519a26ac32dfe340e931ee2a6352969a2672fcb07e0bd4cb1b6124470b` | 4,172,016 / 8,323,072 (50.13%) | 4,237,552 bytes, `8b739f4844b2cd6c23286714d9d85018b285b04df180c5c37efd0ac3b445ab2e` |
| 4 MiB opportunistic | `cb85d6eea3459b790e55f798c8420c36a00cbc9c8e9bd08a36f2e36d137ff782` | 4,172,032 / 4,128,768; oversize by 43,264 | correctly rejected |

No board was flashed and no physical I/O was attempted in this slice.

## Verification

```console
# alumina-interface
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm

# alumina-firmware
bash tools/alumina-brotli-decoder/build-assets.sh
bash tools/alumina-brotli-decoder/verify-maximum-compression.sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask build --board mks-tinybee-v1 --profile release
cargo xtask build --board mks-tinybee-v1-4mb --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board t-lora-pager-current --profile release
ALUMINA_DEVICE_ORIGIN=http://127.0.0.1:8098 \
ALUMINA_CDP_ORIGIN=http://127.0.0.1:9231 \
node tests/browser/qualify-embedded-interface.mjs
```

The interface is MIT. Firmware, assets, simulator, and supporting crates are
`MIT OR Apache-2.0`. No GPL-family source, dependency, or asset was introduced.
