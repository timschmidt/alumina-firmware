# M10 runtime-health client evidence

## Claim

The coordinated boundary consists of:

- `aluminafw` implementation
  `12e2e6ff1ac4a541b0db11f63321ea4e02585cf2`, which produces the fixed
  passive health response; and
- `alumina-interface` implementation
  `7e233ff4d8b9610bd43f056a6bf28e8b50aee8dd`, which independently
  decodes and monotonically retains it in the headless client.

This follow-up does not change firmware, motion, safety, or output behavior. It
adds the portable client model and authenticated browser transport seam needed
before queue/stack diagnostics can enter the worker-owned board explorer.

## Client authority and exact views

`RuntimeHealthModel` is scoped to one authenticated boot session. Its request
is exactly bodyless `HealthSnapshot` with `Digest::ZERO`. The accepted
124-byte body must first pass the shared independent `AHLT`/`ASWM` validators.
The client then applies its own session continuity rules:

- service response cycle cannot regress;
- service and real-time stack domains, epoch, allocation, low exclusion, and
  painted extent cannot change;
- sample and completed-sweep counters and sample cycles cannot regress;
- observed minimum headroom cannot increase; and
- temporary absence of the real-time report does not erase the last witness
  against which its next present report is checked.

An exact duplicate is accepted but not reported as progress. A malformed,
substituted, regressed, or non-success response cannot replace the last valid
snapshot. Firmware `Unsupported` is accepted only with an empty body and
clears boot-scoped stack evidence instead of inventing zero measurements. The
caller explicitly resets the model when authentication discovers another boot.

The UI-facing wrappers preserve integer facts:

- command, deterministic-work, and lossy-telemetry depth, capacity, and free
  credits;
- service/real-time allocation, exclusion, monitored, painted, unpainted,
  minimum-headroom, and observed-maximum-used byte counts;
- epoch, sample, sample-age, sample-count, and completed-sweep facts; and
- absent, present-stale, and present-fresh real-time states.

No percentage, floating-point conversion, allocator claim, convergence claim,
safety state, or armability is inferred. `observed_maximum_used_bytes` remains
subject to the firmware checkpoint's partial-epoch and incremental-scan limits.

## Browser transport seam

`AuthenticatedHttpSession` now exposes its public configuration digest so the
browser adapter can reject a nonzero-config session before spending an HMAC
counter, native sequence, or correlation identity. Both window and worker
variants:

1. build the exact zero-config request;
2. reuse the production HMAC/CORS/no-cache/no-redirect control fetch;
3. authenticate and correlate the complete native response before model use;
4. abandon a spent pending session request after ambiguous fetch failure; and
5. retain model evidence on transport or semantic failure.

This is a compiled browser seam, not a browser traffic result. It is not yet
called by `ControlWorkerRuntime`, included in the versioned
`DeviceSessionSnapshot` JSON schema, or drawn in the board explorer. Those
changes need an explicit bounded poll/error history and worker-schema version
change.

## Reproducible verification

Run from `alumina-interface` at the implementation commit:

~~~sh
cargo fmt -p alumina-interface-client -- --check
git diff --check
cargo test -p alumina-interface-client --locked --offline
cargo clippy -p alumina-interface-client --all-targets \
  --locked --offline -- -D warnings
cargo clippy -p alumina-interface-client \
  --target wasm32-unknown-unknown --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc -p alumina-interface-client \
  --no-deps --locked --offline
RUSTDOCFLAGS=-Dwarnings cargo doc -p alumina-interface-client \
  --no-deps --target wasm32-unknown-unknown --locked --offline
~~~

All checks passed. The package now lists exactly 45 native tests: 37 preceding
tests plus eight new health tests covering bodyless native framing, exact
derived facts, duplicates, explicit unsupported state, malformed/non-success
retention, response-cycle regression, continuity across real-time absence,
stale visibility, and boot reset. The WASM target passes warnings-denied Clippy
and rustdoc, so both browser adapter variants are type-checked.

The only lockfile change records `alumina-runtime` in sibling
`alumina-service`'s path-package dependency list after the firmware health
commit. No external package or version was added.

## License and moving-Hyper isolation

The locked/offline client dependency graphs emit:

| Target graph | Package/license records | Missing | GPL/AGPL/LGPL/SSPL family |
| --- | ---: | ---: | ---: |
| native | 126 | 0 | 0 |
| `wasm32-unknown-unknown` | 158 | 0 | 0 |

All added source is independently authored under the interface repository's MIT
license and consumes only the sibling repository's `MIT OR Apache-2.0` health
types. No GPL-family code, dependency, source, or asset was introduced.

`cargo tree -p alumina-interface-client --target all` contains zero CSGRS,
Hypercurve, Hypergraphics, Hyperlimit, Hyperpath, Hyperreal, or Hypersolve
packages. The root CAD/CAM application, Trunk bundle, and moving geometry graph
were deliberately not built. Hypercurve implementation source and concurrent
diffs were not inspected, changed, formatted, staged, reset, pinned, or
incorporated into this claim; only its dirty working-tree status was observed
to preserve that boundary.

## Hardware and open work

No network operation, Wi-Fi association, browser fetch, serial contact, reset,
flash, GPIO transition, or peripheral activation was attempted. The bare MKS
TinyBee V1.0 and SLogic16U3 remained untouched.

The next interface checkpoint must:

- add bounded health polling cadence and error history to the dedicated worker;
- version and validate serialized health fields in the worker/UI contract;
- reset health evidence with authentication boot replacement;
- render queue pressure, both stack epochs/sweeps/headroom, freshness, and
  partial/incremental caveats in the board-debug view; and
- exercise the exact signed response over the isolated browser/AP bench before
  making any live-network or runtime-memory claim.
