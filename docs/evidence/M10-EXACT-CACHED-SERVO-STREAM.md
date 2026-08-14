# M10 exact cached servo command stream — offline evidence

Date: 2026-08-14

Status: implemented portable firmware/browser checkpoint. Firmware commit
`c5262db87c45c73e43864ff6d3dbe6988c862090` defines and replays the bounded
servo command family. Alumina Interface commit
`1e839357511db80f6d926e954fb78d326fef21b3` projects exact browser-side
recurrences into that family and packages content-addressed cached jobs.

This closes an exact source-to-cache-to-portable-servo-owner seam. It is not an
ESP motion-task, target peripheral, WCET, Wi-Fi, SD-media, motor, power-stage,
or machine qualification claim.

## Greenfield protocol boundary

Machine-IR V3 uses canonical 512-byte `ALMBLK03` blocks and assigns execution
kind `3` to FOC-servo setpoints. Each one-to-four-axis record contains:

- one exact nonzero device-tick cadence and update count;
- cubic Newton-forward Q31.32 absolute configured-axis position;
- cubic Newton-forward Q2.30 normalized velocity feed-forward; and
- cubic Newton-forward Q2.30 normalized quadrature-current feed-forward.

Records are half-open: a record emits indices `0 <= k < update_count`, while
its state at `k = update_count` is the exact initial state of its successor.
That ownership rule applies across block boundaries. A complete stream starts
and ends with both feed-forward vectors exactly zero and reserves one additional
nonzero command identity for a separately committed terminal at-rest hold.
Dense stream totals must therefore remain strictly below `u32::MAX`.

`ALMJOBD4` is a 320-byte self-hashed descriptor. It repeats execution kind,
maximum dense updates, exact dense cadence, horizons, identities, and absolute
Q31.32 initial positions. `ALMJMF02` keeps the 320-byte header and 496-byte
participant records while adding execution kind and both dense-grid fields to
every participant. Ordinary coordinated motion requires those fields to be
zero; both recurrence families require them nonzero. The predecessor machine
V2, descriptor V3, and manifest V1 wire identities are rejected rather than
translated. No compatibility shim exists.

## Independent firmware admission

Servo streams cannot enter the generic cached-job path. Typed `open_servo` and
`prepare_servo` paths require a caller to supply limits independently derived
from complete validated `ServoFocAxisProfile` values. Every axis must share the
same active-configuration digest and exact position-loop cadence.

The derived profile binds:

- conservative Q31.32 progress per position update from velocity, encoder
  scale/counts, clock, and nested-loop timing;
- symmetric normalized velocity authority;
- q-current feed-forward authority inside both the configured current circle
  and velocity-controller output range; and
- nonzero block, record, update-count, and cadence horizons.

Core-0 and core-1 validators independently retain full recurrence state and
reject identity, order, chain, cadence, continuity, padding, checked-arithmetic,
discrete-reversal, position-delta/rate, feed-forward, terminal-rest, and dense
command-count violations. A direct-step or ordinary block cannot substitute
for a kind-3 block.

## Allocation-free realtime ownership

`CachedServoSetpointRunner` owns at most two independently admitted block
tokens. It decodes one fixed record at a time and produces one simultaneous
axis vector at every exact position-loop boundary. Each vector uses a two-phase
prepare/commit transaction with an opaque one-shot token. All fallible counter,
lookahead, deadline, and candidate-state work is staged before live mutation.

Wrong tokens, early/late observations, lookahead loss, arithmetic failure, and
fault-latched reuse cannot partially advance recurrence, command, or block
ownership. A completed token returns only after its entire half-open prefix has
committed. The final token remains owned through the sole terminal hold.

`alumina-sim::foc_hardware` replays a two-block cached stream through a complete
configured servo/FOC axis for 401 current-loop periods. It crosses the block
boundary, commits every position setpoint transactionally, and reaches the
terminal hold. Adversarial runner tests cover a wrong physical deadline and
prove that the unique block is recoverable only as faulted ownership.

## Authoritative browser/WASM compiler

`ExactServoAxisRecurrence` retains exact Hyperreal Newton coefficients for
position, velocity feed-forward, and q-current feed-forward. The browser asks
for certified dyadic coefficient intervals at caller-bounded precision. Both
ends must select one ties-to-even Q31.32 or Q2.30 integer; an unresolved
interval or exhausted precision bound is a typed failure.

The evidence retains the exact source value, closed scaled interval, chosen
integer, fractional width, conservative unscaled error, and whether forced
encoded continuity selected an initial coefficient. Every later source span
starts from the prior encoded terminal state, and the exact difference from its
source initial value is included in the error budget.

The compiler examines discrete integer recurrence transitions rather than
display samples or floating derivatives. It splits at every signal extremum,
record/block horizon, position delta/rate bound, and feed-forward bound. Checked
recurrence shifts add no new approximation. Every record is replayed through
the production firmware validator before packaging.

`CanonicalServoMachinePartition<AXES>` supports one through four axes. It uses
the firmware capacity query, encoder, decoder, and complete stream validator;
checks terminal tick, all Q31.32/Q2.30 states, dense update total, and chain
digest; then constructs content-addressed storage chunks and the exact V4
descriptor. A two-block browser fixture proves that packaging and typed
descriptor replay are deterministic.

## Moving-Hyper isolation

Hypercurve was actively edited during this work and was never treated as a
stable live build input. The interface, firmware, CSGRS, and required Hyper
sibling repositories were copied into
`/tmp/alumina-interface-snapshot.v7iSdl`. The converged Hypercurve copy had the
same recorded content fingerprint before and after snapshot construction:

```text
3748aea9d177e5adbe4d9f4e5d66e4c1f4e7e74a14c9e62be8f45f3fdcd631a5
```

After the firmware implementation was sealed, only `aluminafw` and
`alumina-interface` were resynchronized into that layout, excluding their
build targets. The frozen Hyper sources were not recopied. All final interface
commands below therefore used the coordinated commits against one unchanged
dependency snapshot. Alumina did not edit, format, reset, pin, or build the
live Hypercurve worktree as a side effect.

The frozen Hypercurve dependency emitted seven pre-existing unused-variable
warnings. The warnings-denied clippy command still passed for all interface
workspace targets; dependency lint capping kept those sibling warnings visible
without misreporting them as Alumina warnings.

## Reproduced verification

Firmware commands completed at `c5262db87c45c73e43864ff6d3dbe6988c862090`:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo test --locked -- --list
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
git diff --check

cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-tinybee-4mb \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-t-deck-pro \
  --target xtensa-esp32s3-none-elf --locked --offline -- -D warnings
cargo +esp clippy -p alumina-firmware --bin alumina-firmware \
  --no-default-features --features board-mks-esp32-foc-v1 \
  --target xtensa-esp32-none-elf --locked --offline -- -D warnings

cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board t-deck-pro --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
```

The default-member suite passed all 499 tests, including 23 machine-IR, 20
cached-job, 54 motion, 68 FOC, 49 simulator, and 25 configuration tests.
Formatting, diff checks, warnings-denied host clippy, warnings-denied rustdoc,
and warnings-denied clippy for all four ESP packages passed.

All four optimized, board-qualified images linked at the exact firmware commit:

| Board image | text | data | BSS | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 1,104,328 | 12,400 | 249,744 | `a169d1742ebff6628cdc3b47b07c8f75f033d48e9a357e8302f5bcc8397bd14f` |
| TinyBee V1.0, 4 MiB variant | 1,104,348 | 12,400 | 249,744 | `59cd825733d0f88cf59cf2d0ac7e371afaa8e4e7c4631182ccbdc29dcf67e92e` |
| T-Deck Pro | 1,039,113 | 13,152 | 525,216 | `7e3e42303f50de7e8d1b7916af4742d12340cdcbf50e7f890d2de4de1ccc3997` |
| MKS ESP32 FOC V1.0 | 1,038,084 | 11,152 | 250,992 | `ce0d19d920e4dd90152123e8f7ba3f76dd7d5e79164167e17d0ab9c57519b18a` |

A demangled `llvm-nm` scan of all four ELFs returned no match for
`CachedServoSetpointRunner`, `ServoFiniteDifferenceStreamValidator`,
`ServoFiniteDifferenceSegment`, or `cached_servo_admission_profile`. This
confirms the documented target-dispatch gate; absence is not execution or
safety evidence.

The frozen interface snapshot, with source identical to
`1e839357511db80f6d926e954fb78d326fef21b3`, completed:

```sh
cargo test --workspace --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets \
  --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo check --workspace --target wasm32-unknown-unknown \
  --locked --offline
RUSTDOCFLAGS=-Dwarnings CARGO_INCREMENTAL=0 \
  cargo doc --workspace --no-deps --locked --offline
CARGO_INCREMENTAL=0 cargo run --example exact_checkpoint --locked --offline
```

The source-identical live interface checkout separately passed
`cargo fmt --all -- --check` and `git diff --check` immediately before commit.

The native suite passed 30 application, 37 client, 124 exact-core, and one
integration test, plus the compile-fail doctest. Strict clippy, strict rustdoc,
and the complete WASM workspace check passed. The canonical example reproduced
18 blocks/9,216 bytes and these updated identities:

```text
partition_sha256=ccd0a47153dae2c1f7dff1697f7b2d8d1a229535c02881476251911ab8ab3f98
chunk_manifest_sha256=8f2b4611c37aa7bd738ccef3de0d0bfb8829d5a418fe28bd77776a90c4035821
global_job_sha256=2626778741a7046fd1957371132ed44c54790ce5b7f7b4c145930af4f93ddd9c
participant_set_sha256=d26ddc63881977582a1a3138c676be9e60a95db24fd923480a0706021977217a
```

## Licensing and closed claims

The only dependency change is `alumina-motion` adding the existing workspace
crate `alumina-foc`; the interface lock update reflects that same transitive
workspace edge. No external package was introduced. The workspace license is
`MIT OR Apache-2.0`, and the new code was independently authored from the
documented Alumina numerical/control contracts. No GPL-family implementation
source was copied, translated, linked, or used as a code dependency.

An offline all-feature metadata inventory was attempted but could not complete
because the unrelated optional locked package `atomic-polyfill 1.0.3` was not
present in the local Cargo cache. This evidence therefore does not falsely
claim a renewed complete dependency inventory; the existing deny policy remains
the authoritative full-tree CI gate. The commit-local lock diff introduces no
external license surface.

The permanent ESP `MotionService` still selects only coordinated-step and
direct-step recurrence owners. Kind `3` remains unreachable until it is joined
to configuration-derived complete-axis actors, sole target PWM/ADC/encoder
ownership, qualified shutdown, and measured realtime budgets. General
Hyperpath/machine-kinematic production CAM must still construct the exact
source servo recurrences; this checkpoint starts from those exact recurrences
and proves their projection and cache boundary.

The connected bare MKS TinyBee V1.0 was not contacted, reset, flashed, or
driven. No USB/serial transaction, WLAN association, GPIO operation, analyzer
capture, motor power, process power, or MKS ESP32 FOC hardware participated.
