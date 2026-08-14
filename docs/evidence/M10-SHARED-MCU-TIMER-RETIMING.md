# M10 shared-MCU exact timer retiming — offline evidence

Date: 2026-08-14

Status: implemented development checkpoint. The authoritative browser compiler
now selects and proves one jointly feasible exact timer factor for a complete
same-grid MCU set before creating any immutable participant cache object. This
is offline native/WASM and localhost-browser evidence; it is not physical clock,
radio, SD, output-timing, safety, or synchronized-motion qualification.

## Result and source identity

Alumina Interface commit
`9cbc1dd3e3df92c02360b29b935c126d1cf1267a` implements the shared compiler,
canonical evidence, partition replay, adversarial regressions, and visible
Machine/CAM fixture. The firmware repository parent before this record is
`63089edce07fb01113d08236948bd8cd9e8c3733`; this increment changes firmware
documentation only. It does not add a Hyperreal parser or another planner to
firmware.

The final native/WASM gate batch observed:

- Hypercurve HEAD `de9628dd962a8dcbbe20a527f743a1d2abcff225` with live tracked
  edits in `src/bezier_offset.rs`, `src/bezier_region.rs`, and
  `src/curve_region_boolean.rs`; binary diff SHA-256
  `89986d4728aec32730b051cf5af8a7213ee0de8e4654570dd7685e072bc876b6`;
- clean Hypersolve HEAD `6ce08b714cdba1e3668e1af6c83f0a249bda9bb5`;
- clean Hyperpath HEAD `d792aa8dc843218b26fc0d1730033e5cd06bdf2f`;
  and
- clean CSGRS HEAD `b34a2f47b90e3d329028d6337d19dfbc9629fbb0`.

The Hypercurve snapshot was recorded immediately after the optimized build and
localhost render and remained byte-identical across the subsequent final native
tests, native/WASM strict Clippy, rustdoc, and all WASM test-target links.
Hypercurve is an intentionally moving read-only dependency: Alumina did not
edit, format, reset, or pin it. This identifies a coherent tested development
state, not a release pin. No published CSGRS package substituted for the
current sibling source.

The native and WASM dependency inventories contain no GPL-family package. This
increment did not inspect, translate, copy, link, or depend on Synthetos/g2,
SimpleFOC, FluidNC, or other GPL-family implementation source.

## Joint exact search

`select_shared_timer_lattice_schedule` accepts stable device identities and one
already-certified local schedule/profile per participant. It canonicalizes the
set by `DeviceId` and rejects empty sets, zero or duplicate identities,
configuration/capability disagreement, program/profile disagreement, mixed
timer frequencies, mixed output quanta, different point counts, or any
different exact cumulative ideal event time.

V1 deliberately requires that common event grid. It applies one numerator on a
caller-bounded rational factor lattice to every participant and invokes the
unchanged production stepper preflight for every participant at every candidate.
Only typed duration-pressure failures may request dilation; a structural,
identity, arithmetic, position, or non-timing electrical failure aborts the
whole compile. The retained search report contains every candidate numerator
and every participant's accepted preflight or typed rejection.

The selected numerator must pass every participant. The immediate predecessor
is replayed separately and must fail at least one participant, proving joint
minimality on the chosen lattice. It is valid for another participant to accept
factor one and that predecessor; shared minimality is not falsely reported as
local minimality for every MCU.

## Selected-stream and partition replay

No immutable cache object exists during candidate search. Once a common factor
is selected, each local stream retains its exact ideal and scheduled totals,
cumulative delay, maximum segment extension, output-grid padding, points,
segments, production preflight, and unit/predecessor outcomes.

`package_shared_retimed_scheduled_program` then requires, before block
construction:

- matching device/configuration/capability/stream and partition-policy facts;
- matching timer and output quantum;
- zero initial tick and exact point/segment cardinality;
- every selected tick boundary and step delta to equal the retained exact point
  carrier and canonical segment; and
- preflight event count, terminal position, and terminal tick agreement.

The resulting partition still passes the existing canonical block, manifest,
terminal-progress, and firmware-executor replay. Shared retiming does not create
a privileged cache format or bypass firmware admission.

## Canonical evidence and global job

The compiler builds a fixed 104-byte `ALMSYN01` outer record over an
incrementally hashed, bounded `ALMSRT01` transcript. Independent replay checks
the exact binary-search order and binds:

- common factor policy, timer/output facts, exact totals, terminal tick, complete
  candidate rounds, and all participant outcomes;
- each participant's exact source, metric path, source approximation,
  `ALMPLN01`, and `ALMLOW01` identities and lengths;
- unit, selected, and predecessor outcomes plus every selected point, execution
  segment, and production preflight; and
- stable MCU/stream identity and independently reconstructed partition object,
  manifest, block, position, and terminal facts.

`compile_shared_scheduled_global_job` derives the global timebase, duration, and
synchronization digest rather than trusting caller-supplied values. The caller
template must provide zero placeholders for those fields. The `ALMSYN01` digest
becomes the global `synchronization_digest` and every participant's
`error_evidence_digest` before existing canonical `ALMJMF01` construction.
Reordering the discovery input produces identical artifacts; corruption or any
stale timing/identity field produces no job.

Firmware never parses `ALMSYN01`, `ALMSRT01`, `ALMPLN01`, `ALMLOW01`, source
geometry, or Hyperreal values. Core 1 continues to accept only independently
validated fixed machine IR and the existing canonical global-manifest fields.

## Decision-boundary regressions

The exact-core suite proves both a symmetric and an adversarial participant set.

For two copies of the strict electrical fixture, canonical search selects
`4158/4096`. Factor one rejects with `PulseBoundary` on axis 1, the immediate
predecessor `4157/4096` rejects with `Rate` on axis 1, and the complete search
retains 20 rounds and 40 participant production replays. Empty, duplicate,
zero-identity, identity/profile, timer, output-quantum, event-count/grid, and
factor-budget failures are typed and fail before publication.

For the strict-versus-relaxed fixture, both participants have the same exact
planner dynamics and ideal event grid. Only electrical pulse timing differs:
the strict participant requires 48-cycle high and low widths, while the relaxed
participant requires one cycle. The exact joint minimum is `4151/4096`; the
relaxed participant accepts factor one and predecessor `4150/4096`, while the
strict participant rejects both and therefore alone supplies the bottleneck.
The result retains 20 rounds, 40 production replays, and common terminal tick
2,495,232. Both selected partitions, `ALMSYN01`, transcript replay, corruption
rejection, and the final global job are reconstructed in both discovery orders.

## Browser fixture and artifact

The offline Machine/CAM workspace constructs a real two-participant shared
scheduled job rather than displaying hand-filled status. Its current exact
fixture reports:

- two synchronized participant partitions;
- selected factor `4096/4096`, one complete candidate round, and two participant
  production replays;
- terminal tick 9,639,280;
- `ALMSYN01` digest prefix `a3ca265bddc70b4e`, 104 outer bytes, and
  `ALMSRT01` digest prefix `45c204b6d8451997` over 218,287 transcript bytes;
- global-manifest digest prefix `6259438a6017b6a2` over 1,312 bytes; and
- 125,952 partition bytes for each MCU.

The optimized WASM is 5,487,021 bytes with SHA-256
`ae1527409cce9b21b6c8fea0e5c3d81b5d91598d2cf77782e913e2eaf4c2ad11`.
Its 2,459,299-byte gzip and 1,966,684-byte Brotli forms pass format integrity
checks and expand byte-for-byte to that artifact. Headless Chromium loaded the
document, generated JavaScript, WASM, worker, and favicon over `127.0.0.1` with
ANGLE software rendering and visibly rendered the exact geometry, two-MCU
summary, shared factor, evidence identity, and cache facts. The loopback server
was stopped after the run.

## Verification record

The following completed offline at the interface revision and live sibling
snapshot above:

```sh
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --no-deps --locked --offline -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown \
  --no-deps --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
cargo test --workspace --target wasm32-unknown-unknown --no-run \
  --locked --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
gzip -t dist/alumina-interface_bg.wasm.gz
brotli -t dist/alumina-interface_bg.wasm.br
```

Observed results:

- 28 application, 37 protocol-client, and 120 exact-core tests passed, plus the
  exact-control integration and compile-fail value-boundary tests;
- native/WASM warnings-denied Clippy, warnings-denied rustdoc, every WASM test
  target link, source-policy/license audit, optimized build, WebAssembly
  validation, and byte-identical compression round trips passed; and
- no WLAN association, serial/USB contact, reset, flash, analyzer capture, GPIO
  operation, motor/process-power operation, or other physical board action
  occurred.

The coordinated firmware documentation checkpoint also passed its portable
format, tests, strict-Clippy, and diff checks. `cargo-deny` is not installed
locally, so no local firmware `cargo deny` result is claimed. Firmware target
crates were not host-built or physically exercised by this record.

## Claim boundary and next work

This closes the first exact same-grid shared-factor compiler/evidence boundary.
It does not qualify browser-to-MCU Wi-Fi delivery, physical oscillator models,
simultaneous kickoff, SD replay, I2S/GPIO timing, missed deadlines, E-stop or
interlock behavior, motors, or machine accuracy.

Mixed timer frequencies, output quanta, or event grids still fail closed. A
future model must introduce explicit common synchronization events and exact
bounded idle insertion rather than weakening equality into rounded elapsed
seconds. Physical qualification remains deferred while workstation Wi-Fi is
needed for the development session; the connected bare TinyBee V1.0 was not
contacted.
