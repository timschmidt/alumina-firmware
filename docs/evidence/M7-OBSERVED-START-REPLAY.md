# M7 observed-start telemetry and replay evidence

Date: 2026-08-11

Status: firmware, canonical protocol, browser coordinator, and deterministic
two-MCU simulation now preserve and replay the first job-owned output
observation. This is portable software and target-compilation evidence. It is
not a physical latch, radio, GPIO, synchronization-tolerance, motion, or safety
qualification; neither board was connected, flashed, armed, or energized.

The coordinated implementation checkpoints are:

- `aluminafw` commit `93c3d10`;
- `alumina-interface` commit `6e9aeab`; and
- the current sibling CSGRS/Hyper workspace resolved by the interface lockfile
  and source-policy audit. No published legacy CSGRS release is used.

## Canonical observation boundary

Schedule report wire version 3 is a deliberate greenfield replacement. The
96-byte `ALMJSCH3` image retains the existing lifecycle/commit fields and adds
one fixed 32-byte first-output record. Combined `JobStatus` is correspondingly
336-byte `ALMJST02` version 2. No decoder, alias, or compatibility shim accepts
the prior report/status images.

The observation binds:

- an explicit `SimulatedLatch`, `PeripheralLatch`, or `SoftwareBracket`
  authority;
- a nonzero backend-local output correlation token;
- the scheduled cycle, which must equal the installed local start cycle; and
- conservative earliest and latest observed device cycles.

Simulator and peripheral-latch authorities require one exact cycle. Only the
explicitly weaker software bracket may report an interval. An observation
cannot precede the scheduled cycle, exist before the one-shot start transition,
or change after acceptance. The same observation is retry-idempotent; a
different token, source, or interval is a conflict. Schedule completion is
rejected until an observation exists.

If the observation's latest cycle is later than the installed synchronization
tolerance, core 1 retains it and changes the schedule to the distinct
`Faulted/StartObservation` state. Evidence that caused the fault is therefore
not erased by the failure path.

The target motion service recognizes the first complete-image commit separately
from later commits, proves that its scheduled update is the primed epoch, and
forwards its token and observed cycle into the realtime schedule. Core 0 permits
exactly the monotonic `Running`-without-observation to
`Running`-with-observation enrichment. Current TinyBee and T-Deck Pro backends
still reject motion streaming before this path is reachable, so the
`PeripheralLatch` authority makes no present physical claim.

## Browser-time replay

`alumina-clock` now inverts the retained boot-scoped causal affine envelope with
checked integer arithmetic and outward rounding. Given an observed device-cycle
interval, it returns conservative earliest/latest browser monotonic
nanoseconds, an integer midpoint, uncertainty, probe identity, boot identity,
and accepted-sample count. Reversed input, stale/unhealthy/insufficient models,
arithmetic/range failure, or caller-exceeding uncertainty produces no estimate.

The browser participant controller independently rejects a later authenticated
status that erases or replaces an accepted observation. The global coordinator
then requires one exact input and retained observation for every sorted
participant, verifies device, boot, commit, and scheduled-cycle identity, and
maps each interval through that participant's clock model. It preserves the
source authority and returns:

- each participant's cycle and browser-time interval;
- the conservative outer-union edge spread; and
- the maximum displacement of any interval endpoint from the shared UI epoch.

Missing observations, duplicate/missing participants, changed boots, foreign
commits, stale clocks, or overwide mappings fail closed. The interface diagnostic
panel exposes the exact simulated edge spread separately from the conservative
replayed spread and lists each participant's source and reconstructed interval.

## Deterministic and adversarial coverage

The representative two-device M7 simulation starts affine clocks with different
offsets and rate adjustments from one browser epoch. Each authority records a
typed simulated-latch cycle before completion. Browser reconciliation proves
that each known simulated UI edge lies within its reconstructed interval and
bounds both participant spread and shared-epoch error. Repeating the complete
simulation yields the identical report.

Focused tests additionally prove:

- reversed cycle intervals and mappings wider than policy are rejected;
- a schedule cannot complete without its first-output observation;
- an out-of-tolerance observation is retained in its dedicated fault;
- a conflicting second observation is rejected;
- a confirmed participant set with no observations cannot be reconciled; and
- an authenticated later status cannot erase previously accepted evidence.

## Reproduced checks

From `aluminafw`:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
ALUMINA_XTENSA_BIN=/home/tim/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin
env PATH="$ALUMINA_XTENSA_BIN:$PATH" \
  cargo +esp build -p alumina-firmware --bin alumina-firmware --release \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline
env PATH="$ALUMINA_XTENSA_BIN:$PATH" \
  cargo +esp build -p alumina-firmware --bin alumina-firmware --release \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline
git diff --check
```

All 247 portable default-member tests pass. This includes 5 clock tests, 19 job
tests, and 25 simulator tests. Warnings-denied native Clippy and both real
Xtensa board-target Clippy checks pass. Both optimized firmware images link.

| Board release ELF | `.text` | `.data` | `.bss` | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| MKS TinyBee V1.x | 877,600 | 11,992 | 250,144 | `baa2afb6a53ffd8f75a3febf8d014810e5a4785e221029d4ca16d5d58dffaeb2` |
| T-Deck Pro | 817,365 | 12,752 | 525,616 | `7c8aaec943aa45f7fce1cd6b4622dae36d0c5b128947401b85226c80ce6f32df` |

From `alumina-interface`:

```console
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo check --workspace --all-targets --target wasm32-unknown-unknown \
  --locked --offline
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/index.html.gz dist/alumina-interface.js.gz \
  dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
git diff --check
```

All 52 native tests pass: 8 application/coordinator, 24 headless client, and 20
exact compiler/core tests, plus the intentional compile-fail rustdoc test.
Native and WASM warnings-denied Clippy, WASM checking, warnings-denied rustdoc,
the current-sibling source/license audit, optimized Trunk build, WASM validation,
and compressed-artifact integrity all pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,257,375 | `23a7d134967986a478c2ecb7a06d61c694618432f2b36001d15c32dd8f5abc1a` |
| `alumina-interface_bg.wasm.br` | 1,613,251 | `21ae8317764101942ba6a533f888e872483e33f9de7d26af071b024858e0cddb` |
| `alumina-interface_bg.wasm.gz` | 1,978,570 | `8a7631d6447a8ab4a7588ee45286dbeef230ff23052d20a3dd73655fce566e94` |
| `alumina-interface.js` | 89,162 | `882df4e3e704d5c8561ad266ae13596cd04a5ac10a1f457c21ad4ce0d34efeb9` |
| `alumina-worker.js` | 631 | `cfc5a142c87bab91d29697bc9af98308ff67fddf745259291f80ceb11e342a4a` |
| `index.html` | 1,295 | `19e70bf7e79f6e4d1d8aef8e1ff591bacede831a91164b762e4f3022c440e2a7` |
| `Cargo.lock` | — | `d30a66cdb03fddda03f41001f719b9475124d4f15dc6de8461d8ffae3041aff3` |

## License and remaining physical boundary

New code is independently authored under the repository's MIT or
`MIT OR Apache-2.0` terms. The strengthened interface source audit resolves
every Alumina crate/driver and every selected CSGRS/Hyper crate to its current
sibling checkout and accepts the native and WASM license inventories. It rejects
missing metadata and GPL/AGPL/LGPL/SSPL-family licenses. No GPL-family
implementation source, Synthetos implementation source, or SimpleFOC
implementation source was introduced or consulted.

The next exit gate is harmless two-board HIL. It requires qualified
interrupt/peripheral latch ownership, logic-analyzer capture of both first
edges, telemetry agreement with those captures, and nominal plus saturated
Wi-Fi trials. The existing isolated safe-image procedure must be followed with
machine loads disconnected. Until that evidence exists, both first board
packages remain non-armable and the UI must label every current result as
simulation rather than measured hardware synchronization.
