# M5 / I3 exact UI-only CNC geometry import evidence

Date: 2026-08-13

Status: implemented development checkpoint; not a general RS-274 interpreter,
firmware protocol, M5 exit gate, reproducible release pin, or hardware
qualification.

## Result and source identity

`alumina-interface` commit
`a0072f77c78633369b457c86beba5329d5e72dc2` adds a bounded optional source
adapter and connects it to the existing authoritative Machine/CAM transaction.
The coordinated firmware-schema and motion baseline is `aluminafw` commit
`80928dad5fca4234fad9e4c3d912dce14d728502`.

The final native/WASM checks and optimized bundle observed the shared Hypercurve
checkout clean at `9b0ced6c8607364da45ea912e6d0ff2247f09d0c`. That checkout changed several
times while the checkpoint was developed and is expected to keep changing. The
identity is an observed development input, not a pin or request to hold the
working tree. No published CSGRS release may substitute for the current sibling
stack; the interface source-policy audit enforces local CSGRS, Hyper, and
Alumina sources.

## Exact selected source boundary

`import_exact_cnc_geometry`:

- admits at most 1 MiB of ASCII source, 4 KiB per physical line, 65,536 lines,
  262,144 parsed words, 4,096 retained curves, and 128 characters per decimal;
- parses every coordinate directly into `hyperreal::Rational`, including exact
  inch conversion by `127/5 mm`, before constructing geometry;
- accepts explicit `G17`, `G20`/`G21`, `G90`/`G91`, `G90.1`/`G91.1`, an initial
  absolute `G0 X/Y`, connected `G1`/`G2`/`G3` geometry with explicit I/J arc
  centres, optional integral `N` numbers, comments, a paired `%` envelope, and
  terminal `M2`/`M30`;
- constructs native exact Hypercurve `LineSeg2`, `CircularArc2`, and
  `CurvePath2` objects; and
- rejects feed, Z/other axes, spindle, tool, compensation, canned cycles, R
  arcs, full circles, disconnected rapid motion, checksums/macros, malformed or
  ambiguous modal state, and every unknown word or code.

All counters are checked. Temporary line, word, provenance, and curve storage
uses fallible reservation. Hypercurve retains authority for exact primitive and
connected-path validity. Every strict prefix of the representative source fails
without returning partial geometry.

## Canonical separation and transaction

The import report retains raw-byte SHA-256, byte/line/word/position counts,
exact endpoints, and per-curve source line, optional N number, motion, units,
endpoint mode, and I/J mode. These are review facts only. Raw CNC text, modal
state, comments, file extension, and provenance are never firmware input or
canonical job identity.

The built-in direct Hypercurve fixture remains the default. A candidate file or
editor draft can replace it only after this complete chain succeeds:

```text
bounded bytes -> exact selected parser -> native Hypercurve path
  -> current ALMCFG05 machine profile and resolution budget
  -> native-extrema travel proof
  -> Hyperpath/Hypersolve exact-stop jerk schedule
  -> bounded integer step/tick lowering and rounded-travel proof
  -> production StepperExecutor preflight
  -> immutable cache partition and independent event replay
  -> reconstructed ALMEVD01 evidence replay
```

The direct fixture, equivalent CNC text, and a comment-only variant produce the
same exact geometry digest, cache partition, and evidence identity while the
two text sources retain different raw hashes. A source containing `F` and a
syntactically valid 301 mm path both reject without changing the previously
accepted source, schedule, cache, simulation, or evidence.

The browser exposes the selected semantics and bounds, draft-only file/editor
controls, active source type, raw-versus-exact identity, exact endpoints, and a
bounded provenance table. Downloading moves draft bytes only; compiling is the
state-changing operation. Firmware continues to consume only canonical integer
machine IR and immutable cached partitions.

## Verification record

The following passed from `alumina-interface`:

```sh
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo clippy --workspace --target wasm32-unknown-unknown \
  --all-targets --offline -- -D warnings
cargo test --workspace --target wasm32-unknown-unknown --no-run --offline
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --offline
scripts/audit-source-policy.sh
env -u NO_COLOR trunk build --release --locked --offline
wasm-tools validate dist/alumina-interface_bg.wasm
```

Observed results:

- 28 application, 37 protocol-client, and 106 exact-core tests passed, plus the
  cross-crate exact-control integration test and compile-fail value-boundary
  test;
- the exact-core total includes five CNC cases covering native line/arc and
  provenance, exact inch/incremental conversion, rejected process/ambiguity,
  admission limits, and every strict representative prefix;
- all native and WASM test targets linked; strict native/WASM Clippy, strict
  Rustdoc, and the local-source/permissive-license audit passed;
- the optimized WASM validated at 5,345,732 bytes with SHA-256
  `0ffaa504dd5cb0970b12788916a9135517252092ea8fd0474d0fc41243b34c5d`;
  its 2,409,207-byte gzip and 1,931,517-byte Brotli forms both decompress to
  exactly that digest; and
- loopback-only headless Chromium with software WebGL loaded the dedicated
  worker and visibly rendered the default non-armable Machine/CAM workspace,
  source controls, exact path, machine facts, and safety warnings.

The source-policy audit accepted the MIT/Apache-compatible inventory and local
workspace sources; no GPL dependency was introduced.

## Closed claims and remaining gates

- This importer covers one connected two-axis line/explicit-IJ-arc geometry
  subset only. It does not execute process semantics or promise CNC dialect
  compatibility.
- General Bezier/NURBS source import, multiple contours, tool/work transforms,
  process semantics, generated CNC export, nonzero-radius path blending, and
  broader machine kinematics remain future UI work.
- Raw source provenance is retained in UI state but is not yet embedded in
  canonical schedule evidence; exact resulting geometry is bound there.
- No firmware code or protocol shim was added for this adapter. G-code remains
  non-load-bearing and cannot reach firmware.
- No WLAN interface, NetworkManager state, TinyBee AP, USB/serial endpoint,
  physical board, reset, flash, output, motor, or process power was contacted.
  The bare TinyBee remains non-armable and all physical gates remain closed.
