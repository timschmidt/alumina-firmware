# M9 exact typed case/state evidence

Date: 2026-08-26

Status: schema-generic typed constants and Boolean-selected typed cases are
implemented for Boolean and exact-rational control Streams. The 18-kind editor
palette, native/WASM simulation, canonical replay, maximum-compression browser
bundle, and all five current dual-core board packages qualify. This slice is
`HostExact` only; it adds no deployment opcode or physical-output authority.

## Fixed behavior admission

The old Boolean-specific simulator variants are replaced by two generic fixed
behaviors:

- `TypedConstant` requires zero inputs, one Stream output, one parameter whose
  registered type exactly equals that Stream's sample type, an empty output
  dependency, and no transition or state;
- `TypedCase` requires one Boolean selector and two identical typed value
  Streams, all on the output clock, three distinct required bounded input
  queues, a complete three-input output dependency, and no parameter,
  transition, or hidden state.

Evaluation reconstructs the selected or parameter-owned value through the
authoritative `GraphSchema`. It does not convert an exact value through a float
or dispatch through a separate exact-only evaluator. Both case branches must
be available at every tick even when one is not selected. Wrong clocks/types,
missing queues, aliased input bindings, incomplete dependencies, wrong
parameters, or unexpected state reject during registry admission.

The concrete fixed kinds are `control.bool.constant`, `control.bool.case`,
`control.exact.constant`, and `control.exact.case`, all V1. Boolean and exact
delays continue to share the existing schema-generic read-before-write
`UnitDelay` behavior.

## Lossless reset-dominant register

The new exact regression composes:

```text
loaded  = case(load, current, data)
next    = case(reset, loaded, -5/7)
current = delay(next, initial=7/3)
```

The prior-state trace is exactly
`[7/3, 7/3, 11/5, 11/5, -5/7, -5/7, 17/11]`. A simultaneous load of `13/7`
and reset visibly selects `-5/7`. Reversing every caller sample produces an
identical `GraphSimulation`; canonical `ALGT` encoding and independent replay
reproduce every typed value.

Malformed Boolean and exact constant/case bindings independently fail with the
generic contract diagnosis. The earlier reset-dominant Boolean latch continues
to pass unchanged.

## Registry, trace, and editor identity

The representative 21-node PID/interlock graph is unchanged, so its `ALGR`
identity remains
`96a3348264a9b65d267b45f9a6419a44ee60473fd961abcf4436295e10b3735f`.
The expanded 18-binding registry has `ALSI` identity
`3ccd0aa5dbc2f745b897ca06e9a75717365cd977ab303e3c1ca48bdcf8f7c525`.
Its 8,292-byte representative trace has `ALGT` identity
`a89c8bfa87e7dced328d6cb6583a10a618431775bd3a51d2ab61a56997d4c177`.

The palette is derived from the audited registry and now exposes 18 fixed
kind/version schemas. Exact constant prototypes receive exact zero; Boolean
constant and delay prototypes receive false; cases have no hidden parameter.
UI regressions create all five non-fixture Boolean/exact case-state prototypes,
preserve monotonic node IDs, edit exact `13/7` and Boolean `true`, and prove
malformed literal edits cannot mutate the draft.

## Authoritative sources

The interface working tree is based on
`0a111d2e5243db0e356e9befa1ac28c77785753a`; exact source hashes identify this
uncommitted integrated state:

| Source | SHA-256 |
| --- | --- |
| `crates/alumina-interface-core/src/graph/simulation.rs` | `632e6612ecafa1b7fcd720b4780a5644fee262988a726caf0c3762d3f67f99db` |
| `crates/alumina-interface-core/src/graph/control_fixture.rs` | `513ebe98ed9b5241ba74f6baf6731c5f6a3884138348cdac96286cc1e0ec7ee2` |
| `src/control_graph_ui.rs` | `c31ce92bcfb1ae8f51e551d611601baee3181088c48038d9f20068858680ed54` |
| `docs/TYPED-CASE-STATE-V1.md` | `c157df91dfea93fdd7b3cc526ff8a74751f8d9ae46906846e428f5a915194c41` |

## Maximum-compression WASM qualification

The rebuilt release WASM passed `wasm-tools validate`. Both retained streams
passed `brotli -t`; no gzip compatibility copy exists.

| Payload | Source bytes / SHA-256 | q11/w24 bytes / SHA-256 |
| --- | --- | --- |
| JS | 91,816 / `a49f2e051ad95868c425f5ac2b1ff6ec024428455987ec98db87e861694b1c58` | 10,902 / `9b17a48d4e56e496bbaa01d40db6ba69dbfaa052cb4d734493728bb797fceb1a` |
| WASM | 8,301,998 / `57237a32b79e0df87d241023bc6660b1281cafb59dbd1b8591f82c4fef2c4164` | 2,793,833 / `6ce0383da3cc897b9312906ad736686b8d3f4336ae670a404959d5b97266a05f` |

Reproducible gzip level 9 uses 3,597,034 bytes; retained Brotli uses 2,804,735,
saving 792,299 bytes (22.03%). The exhaustive q11 sweep proves 10,902 bytes is
the JS minimum at standard windows 18–24 and 2,793,833 is the WASM minimum at
standard windows 23–24. Nonstandard large windows grow to 10,903 and
2,793,836 bytes respectively. The 2,755-byte manifest identity is
`5ad1f429476cc14c88a66bf741837f0ef0c560604b0658d723aba9c669e78f35`.

A fresh Chromium 151 profile loaded the checked-in simulator route table. All
strict resource/header/bootstrap/canvas checks passed; WASM bindings installed,
the bootstrap completed in 898.5 ms, and there were no loading failures or
exceptions. The visually inspected 1440×1000 image remained the intentional
offline non-armable TinyBee machine/CAM view.

| Browser artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| strict result JSON | 10,263 | `5bd4b7c0fc41c6ee9166b0f777079703c3516b251045621460e9031f433b47bb` |
| inspected PNG | 229,558 | `be566144c6a9d214e12264173ad329ddc9fa6f3bbba8756853031024a61fccf5` |

The first strict attempt intentionally failed because a stale long-lived
simulator executable served the preceding exact bundle. Rebuilding and
restarting the executable made the same exact checks pass; no expectation was
relaxed.

## Board and flash qualification

Warnings-denied per-chip clippy and optimized release links pass for every
current dual-core package:

| Board | text | data | BSS | ELF SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB | 4,159,760 | 12,672 | 282,240 | `6ebbd3f0522a719054bd43807f46dcfce779f9dc4cfad50421250d94618791a6` |
| TinyBee V1.0, 4 MiB | 4,159,772 | 12,672 | 282,240 | `ae896c8e3638195143334568bea9955664f486799228ef3e567c832328aba34c` |
| MKS ESP32 FOC V1.0 | 4,079,440 | 11,280 | 283,632 | `d04f81c71612482ad88cc14897972c29655a96ba7f5a27285c805fb06daa23fa` |
| T-Deck Pro | 4,010,781 | 12,480 | 3,548,736 | `a4d128b1bda1e4dd1081bfebd23f195c949466201e2545e36ce6edca313e6a43` |
| current T-LoRa Pager stub | 3,961,157 | 11,168 | 3,550,036 | `c677d0de2b133c66b0e32e89b6b0f61c72326093189b2a7832c5b65da9b2dacd` |

For the primary TinyBee, the linked `.bss` remains 146,012 bytes, `.stack`
37,924 bytes, and bootloader-reclaimed `.dram2_uninit` 98,304 bytes. `espflash
save-image` reports 4,172,544 app bytes in the 8,323,072-byte partition and
produces a 4,238,080-byte merged image with SHA-256
`3efef15449a1605aab0e611bd66088c73caadc7652326f7d649e0cc370d37aee`.
The independently linked 4 MiB variant is 4,172,560 bytes and is correctly
rejected 43,792 bytes over its 4,128,768-byte app partition. No interface
feature was removed to make that opportunistic target fit.

No firmware was flashed. The available MKS TinyBee V1.0 remains a bare,
USB-powered board with no motor or motor power; this checkpoint makes no new
physical network, startup, safety, timing, or output claim.

## Reproduced checks and boundary

Interface verification passed 108 application tests, 82 client tests, 198 core
tests, one independent integration replay, and one compile-fail documentation
test. Firmware's default-member suite passed all 583 listed tests. Formatting,
diff checks, valid host warnings-denied clippy, five per-chip warnings-denied
clippy commands, maximum-compression verification, WASM validation, browser
qualification, and all five release links passed.

```console
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
trunk build --release

bash tools/alumina-brotli-decoder/build-assets.sh
bash tools/alumina-brotli-decoder/verify-maximum-compression.sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo xtask build --board mks-tinybee-v1 --profile release
cargo xtask build --board mks-tinybee-v1-4mb --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board t-lora-pager-current --profile release
```

The schema-generic implementation and interface assets are MIT; firmware and
embedding code are `MIT OR Apache-2.0`. No GPL/AGPL/LGPL/SSPL-family source,
dependency, or asset was introduced.
