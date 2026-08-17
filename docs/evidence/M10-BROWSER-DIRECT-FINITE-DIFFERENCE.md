# M10 browser direct finite-difference lowering — offline evidence

Date: 2026-08-14

Status: implemented development checkpoint. The authoritative browser/WASM
compiler can now turn an exact stop-to-stop affine Hyperpath schedule into
certified Q31.32 third-order finite-difference records, immutable `ALMBLK02`
cache partitions, production firmware replay, and independently reconstructible
`ALMDFE01` evidence. Firmware can carry a physical pulse fall across contiguous
direct records and cached blocks without inserting a false dwell. This is
portable native/WASM, immutable-cache simulation, and localhost-browser
evidence. It is not target output timing, WCET, Wi-Fi, SD, safety-response,
motor, or machine-accuracy qualification.

## Result and source identity

Firmware commit `da60adc9a07eab678aed0f0b93cd57c1051ab650` implements the
cross-record pulse-ownership contract, cached-block continuation, terminal
drain, simulator replay, documentation, and adversarial regressions. Alumina
Interface commit `330e3ef40426a07962c8b768bbf5ad1911eb27cd` implements exact
browser lowering, direct partition construction, streamed evidence, visible
artifact exchange/cache replay, and the line-only headless UI regression. No
compatibility decoder, fallback conversion, G-code firmware path, or published
CSGRS substitution was added.

Hypercurve and Hyperphysics were being edited independently while this work was
verified. Short live-worktree runs were useful development checks, but no run
that crossed a source change is treated as final evidence. The final gate batch
used an isolated, read-only copy of every local path dependency and its own
Cargo target directory. `cargo metadata` resolved all local manifests inside
that frozen root and none from the moving workspace. Alumina did not edit,
format, reset, or pin any Hyper or CSGRS repository.

The frozen Hyper/CSGRS graph was:

| Repository | Git state represented by the frozen source | Frozen regular-file tree SHA-256 |
| --- | --- | --- |
| `hyperreal` | clean `f09c147b0352884f8efe88e875c37d8f0f439ba5` | `bb51cd0dad995e4b923e193943723f23a14dee682f1f046b281382a1d5e5fece` |
| `hyperlimit` | tracked-clean `b0418bddff50183fa782e5caa6da6974a2b969a1`; unrelated untracked fuzz artifacts/corpus retained but not built | `89724de7363b09170dc830c832799eb2b0724e1f8d6f0dcc81b1fc78bcd56f8a` |
| `hyperlattice` | clean `a475bb752c1e0fb0cfdb80f4db74a56caa6962c0` | `540243c6cf99b7c461c6bab9dd034e1839a6bf05f72df437af40291c0c8f1e1b` |
| `hypertri` | clean `86189ff6e87f056a3686d81b57952d799945663a` | `ded4770158b9af29c81c6cb2e2eee8ef1623cf12b4396025786c228c5d046e51` |
| `hypermesh` | clean `088c4a4bd32bf8bfea37032432d84e19104f1ab0` | `d4838030fd2a8b23f83c254b4002f048643e85dea5c9ac455d41f05ac028ff50` |
| `hypercurve` | `3ef8689ff2c33ad9fd0c9eb7fbdf9fa015fc395c` plus tracked `src/bezier_offset.rs` diff SHA-256 `05b5114b6e23c7fd68308724d6a91a1e77c0fd6225179ca6fa0511eb1994f121` | `be38a1b1d654519ba9544e9a4a926bd360a179b7716fd5d98f78cd87326f1b7d` |
| `hyperpath` | clean `d792aa8dc843218b26fc0d1730033e5cd06bdf2f` | `b2d9a56587e0e93ee736a4284a4ddc79e3db2a84dbebb0bd0449edca3793bc9d` |
| `hypersolve` | clean `6ce08b714cdba1e3668e1af6c83f0a249bda9bb5` | `8ff6d3a661db424a636257955f2b35b8e25fa9181161c91b677406bab380dd71` |
| `hypergraphics` | clean `31811aeb17bd2dc827db5669558f6251e0c2f2aa` | `ff6e93acdc3f7bffc1637f878ca1f3a439409a036810bcef4dbd14c8691daf14` |
| `hyperphysics` | `a8002f286914356d3ebc5f491695f39f6f1c029e` plus tracked edits in `src/contact.rs`, `src/gjk.rs`, `src/lib.rs`, `src/mass.rs`, `src/property.rs`, `src/shape.rs`, `tests/contact.rs`, `tests/property.rs`, and `tests/shape_queries.rs`; binary diff SHA-256 `99766a9ad8ccb54b8eac523fcc904db4d2df3aa5eeb4c10f5bcb781d57ad9667` | `ef0be0a88cfbca0e6de254becf3d0fb5e8eaa003695020c219315b36eb9d1092` |
| `csgrs` | clean `b34a2f47b90e3d329028d6337d19dfbc9629fbb0` | `1875d18ec944f988b523f8670f8d64706bf14ee9c98439647273f594b3b51bcf` |

Each tree digest hashes the sorted `sha256sum` records for every regular file
in the isolated repository after excluding `.git`, `target`, and `dist`. The
fingerprints identify the exact development input actually tested; they are not
release pins and do not impede continued sibling editing.

The native and WASM source-policy inventories passed the MIT/Apache-compatible
policy. This increment added no dependency and did not copy, translate, or link
Synthetos/g2, SimpleFOC, FluidNC, Klipper, or other GPL-family implementation
source.

## Exact browser lowering

The first direct compiler deliberately accepts only exact affine metric spans
whose certified schedule is at rest at every element boundary. Curved route
elements and positive-feed joins return typed errors; they never silently fall
back to display chords or the coordinated V1 stream.

For every accepted span the compiler:

1. reconstructs its exact symmetric four-phase jerk profile;
2. rounds each phase duration upward to the configured integer output quantum;
3. reruns Hyperpath/Hypersolve velocity, acceleration, jerk, and route
   certification on the grid-retimed profile;
4. composes the exact affine axis map with each cubic-in-time phase position;
5. derives exact Newton forward differences at the output-grid interval; and
6. projects each coefficient to signed Q31.32 only after Hyperreal returns a
   closed certified dyadic interval whose two endpoints choose the same exact
   ties-to-even integer.

No renderer value or sampled display geometry enters that derivation. The
interactive bounded policy permits at most 65,536 records, 256 updates and
10,000 rounded steps per record, requests 128 coefficient bits, and receives a
caller-owned positional-error allocation.

If one coefficient projection has exact error `E1`, `E2`, or `E3`, a record of
`N` updates carries its incoming position error `B_in` to the conservative
terminal bound

```text
B_end = B_in + N E1 + C(N, 2) E2 + C(N, 3) E3.
```

Per-axis step bounds are converted through the exact machine scale into a
conservative L1 millimetre bound and must fit both the compiler policy and the
controller allocation in the machine-resolution certificate. Near zero
velocity, Q31.32 projection can make a long record's discrete derivative
reverse even when the exact cubic is monotonic. A bounded binary refinement
finds the largest structurally valid prefix, then derives fresh exact
coefficients at the next phase offset. The same refinement handles the
per-record rounded-step bound. Record count, update count, arithmetic range,
continuity, direction, electrical timing, and final coordinates are all
replayed before a program exists.

The representative exact 1 mm line ends at `[1600, 0]` command steps, retains
four grid-certified jerk phases, includes adaptively shortened records, and
passes the production finite-difference electrical preflight. An invalid
policy, a one-record allocation, and a `10^-12` mm coefficient-error allocation
each fail transactionally with their distinct typed result.

## Immutable direct cache and evidence

`package_canonical_direct_program` emits only finite-difference kind `2`
`ALMBLK02` records. It queries the firmware schema's two-axis capacity of four
records per 512-byte block, runs an independent stream validator while packing,
and binds the real execution kind plus maximum dense updates into `ALMJOBD3`.
The resulting partition retains terminal integer coordinates, terminal Q31.32
coordinates, dense update count, block chain, object identity, manifest, and
storage chunks. The production cached simulator decodes immutable bytes,
admits each unique token through `RealtimeJob`, executes every dense update,
and checks all retained terminal facts.

`ALMDFE01` is a fixed 344-byte outer record over a streamed, bounded
`ALMDFT01` transcript. Independent reconstruction commits:

- exact source, certified metric path, source-approximation, and
  Hyperpath/Hypersolve planner identities;
- every grid-retimed phase and its nonnegative exact padding;
- each ideal Real coefficient, certified interval endpoints, selected Q31.32
  integer, local projection error, incoming error, and propagated terminal
  error;
- every canonical finite-difference record and production electrical preflight;
  and
- the immutable partition object, manifest, block chain, final coordinates,
  and dense update count.

Rebuilding the same inputs is byte-for-byte deterministic. Outer corruption
with the original digest fails as a digest mismatch; corruption accompanied by
its newly computed outer digest still fails exact reconstruction. Partition
corruption likewise fails before cached execution.

The Machine/CAM UI exposes `ALMDFE01` import/export and reports direct records,
updates, block count, terminal integer/Q31.32 coordinates, coefficient-error
allocation, and evidence/transcript identities. A transactionally imported
line-only CNC source reaches that direct-ready state in the headless UI test.
The built-in line/arc/cubic fixture instead visibly reports its typed curved
element blocker while retaining the separate V1 preview; no firmware
compatibility fallback is created.

## Pulse ownership across records and blocks

A valid coefficient change can occur while the most recent step pulse remains
high. Requiring every finite-difference record to end pulse-low would make an
ordinary jerk-phase boundary unrepresentable or force an artificial dwell.

`FiniteDifferenceStepperExecutor` now owns one optional pending fall per axis,
independently of the active coefficient record. The fall participates in the
next-deadline ordering and may occur after the current record or cached block
horizon. A following record can load while that fall is pending, but complete
preflight still proves pulse-low time, maximum rate, direction hold/setup, and
enable timing across the boundary. Scheduled last-rise/last-fall state remains
identical to sparse admission. Normal finish is illegal until every fall has
actually been emitted and enable hold has elapsed; fault still immediately
requests a complete safe transaction.

The cached owner preserves this state while token ownership moves from one
validated block to the next. The simulator drains a terminal cross-block fall
before normal finish. Dedicated regressions cover a contiguous record
boundary, a cached block boundary, and immutable-partition terminal drain.

## Verification record

The following completed offline for the committed Alumina code and isolated
source graph above:

```sh
# alumina-firmware code commit da60adc9a07eab678aed0f0b93cd57c1051ab650
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked --offline
git diff --check

# isolated alumina-interface commit 330e3ef40426a07962c8b768bbf5ad1911eb27cd
cargo fmt --package alumina-interface --package alumina-interface-client \
  --package alumina-interface-core -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown \
  --no-run --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Observed results:

- all 416 firmware default-member tests passed, including 17 machine-IR, 20
  job, 39 motion, and 39 simulator tests;
- firmware formatting, warnings-denied Clippy, warnings-denied rustdoc, and
  diff checks passed;
- 29 application, 37 protocol-client, and 121 exact-core tests passed, plus the
  exact-control integration and compile-fail value-boundary tests;
- isolated native/WASM warnings-denied Clippy, warnings-denied rustdoc, every
  WASM test-target link, formatting, source/license policy, and source-path
  isolation checks passed;
- the optimized WASM is 5,562,639 bytes with SHA-256
  `720c1df73da919a456d06f7d54ca44c34c91163cebe6c68c686b1bedd1780c42`;
  its 2,493,555-byte gzip
  (`0fc669dcc00c48c59294ad5951b4b82ea07395c152ce3a9d248ef51a1ff28e1b`)
  and 1,992,099-byte Brotli
  (`c184b8942ee6be487684b2b7bb392fb0b88a0b1051d1436b15414cd341cd1d5a`)
  forms passed format integrity checks; and
- headless Chromium loaded the document, generated JavaScript, WASM, worker,
  and favicon from `127.0.0.1:8098` with ANGLE software rendering. It visibly
  rendered the exact CAM inspector, Hypercurve path, Hyperpath/Hypersolve
  certificate, worker-ready state, offline/non-armable warning, and the typed
  curved-fixture direct blocker. The 325,041-byte evidence screenshot has
  SHA-256
  `28fa94a7fe4eb7379fa810e46ef72d8c028199bbc2df74fa62e76176313917fe`.
  The loopback server was stopped after the run.

The root firmware workspace contains ESP target-only members, so the documented
default-member test command is the portable host gate. No ESP target result is
implied by that command.

## Claim boundary and next work

This closes the first exact browser-to-direct-IR, immutable local cache,
portable executor, and replayable evidence boundary. It does not yet support
curved spans, positive-feed joins, or direct-kind shared multi-MCU retiming and
deterministic kickoff. Those extensions must preserve exact polynomial
construction and explicit error allocation; display sampling remains
inadmissible.

The direct logical event stream is not yet composed into TinyBee's qualified
single-owner PCM/DMA output path. No target WCET, DMA lead, Wi-Fi/service-load,
SD delivery, oscillator, E-stop/interlock response, motor, or machine-accuracy
claim is made. The connected bare MKS TinyBee V1.0 was not contacted, reset,
flashed, or driven. No WLAN association, USB/serial transaction, analyzer
capture, GPIO operation, motor power, or process power occurred. The SLogic16U3
was not used.
