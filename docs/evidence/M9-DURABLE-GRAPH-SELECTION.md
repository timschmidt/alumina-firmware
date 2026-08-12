# M9 durable graph-selection evidence

Date: 2026-08-12

## Scope

This checkpoint makes the selected `ALGRIR01` package a power-cut-safe property
of provisioned cache media rather than a boot-local coordinator choice. A graph
does not gain execution authority merely because its activation intent, core-1
selection, or journal commit exists. Firmware authorizes both permanent actors
only after the complete selection is durable and independently admitted on both
cores. Boot repeats that admission against the already recovered and authorized
machine configuration.

The reviewed implementation commits are:

- `aluminafw` `5fa19688ccff3ab748c6b449f916fa07ab51407e`
  (`Journal durable graph selections`);
- `aluminafw` `722b91d1f348a208280a9d62381a3061bc721ab0`
  (`Recover durable graph selections`);
- `aluminafw` `1616c26a48ce8ca4b366c8188a16904ea5c1143c`
  (`Keep classic ESP32 pre-init calls in range`);
- `aluminafw` `e539f10766c0cb1ea7c90856513aa1acfc87d657`
  (`Replay durable graph lifecycle in simulation`); and
- `alumina-interface` `01396472d87728dbcfc1cbb6aae6efa8211687b7`
  (`Reconcile durable graph activation`).

No connected hardware was read, erased, flashed, reset, or driven. The
available MKS TinyBee V1.0 remained a bare board without motor power or motors.

## Durable identity and mutation lane

Raw cache-media record kinds 8, 9, and 10 are `GraphPrepare`, `GraphCommit`, and
`GraphAbort`. Each carries one canonical 160-byte `ALMGRS01` payload. It binds
the transition action and nonzero transaction to the complete typed
`PublishedObject`, embedded package digest, and audited implementation-registry
digest. Activation additionally reopens the referenced immutable publication
before appending its prepare.

The replayed `GraphJournal` holds exactly one last committed selection and at
most one durable but inert transition. Uploads, configuration transitions, and
graph transitions share one serialized mutation lane. Exact prepare and abort
retries are idempotent. A matching commit is the only record that changes the
active selection; an unmatched prepare never does.

Normal activation follows this authority order:

1. append and synchronize graph prepare;
2. select the independently validated candidate on core 1 without authority;
3. append and synchronize graph commit; and
4. authorize the matching core-1 and service actors.

Normal clear follows `prepare -> core-1 clear -> commit -> service-actor clear`.
This keeps the old committed selector until the realtime actor has removed its
active image. Journal errors retain a distinct canonical `Durability` fault.
`Preparing`, `Committing`, and boot `Recovering` are explicit coordinator phases
and are visible to the browser reconciler rather than being mistaken for
completion.

## Configuration-first boot replay

Graph bootstrap remains closed until the committed machine configuration has a
nonzero active digest and is authorized. It then reads the graph journal,
durably aborts any orphan prepare, and reopens every byte of the last committed
graph publication. The service validator and realtime deployment independently
check the same device, capability, configuration, publication, package, and
implementation identities. Core 1 first selects the recovered package without
authority; authorization follows only after both validators agree with the
already committed selector.

If the committed graph no longer validates, recovery remains fail closed. It
first aborts any staged candidate as necessary, clears the realtime actor, and
then durably clears the invalid selector. Configuration, job, upload, and graph
mutators remain mutually excluded while bootstrap, durable mutation, selection,
execution, or reconciliation is in progress. Commit and authorization failures
retain enough operation identity for an exact authenticated retry.

## Power-cut and composed replay coverage

Raw-media tests inject a cut at every modeled write/synchronization boundary of
activation prepare and commit. Replacement replay exposes only the complete old
or complete new active selection; a prepared replacement remains inert. The
same exhaustive boundary injection covers clear commit and abort. After each
cut, a fresh media instance mounts the surviving bytes and checks active and
pending identities rather than trusting in-memory state.

A portable composed replay uses the real `ProvisionedCache`, graph journal,
`ServiceGraphValidation`, and `RealtimeGraphDeployment` primitives. It proves:

- prepare leaves the candidate inert;
- core-1 selection alone exposes no authorized package bytes;
- commit precedes authorization;
- reboot retains an old complete selection while exposing and aborting an
  orphan replacement;
- recovery transfers the committed publication through both independent
  validators before authorization; and
- clear commits only after core 1 reports an empty graph deployment.

This is deterministic host simulation of the storage and deployment primitives,
not a physical SD-card power-removal run or a boot of the Embassy coordinator.

## Verification

The firmware checkpoint reproduced:

```console
cargo fmt --all -- --check
cargo test --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo clippy -p alumina-runtime --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --offline
cargo xtask check --board mks-tinybee
cargo xtask build --board mks-tinybee --profile release
cargo xtask build --board mks-tinybee-4mb --profile release
cargo xtask build --board mks-esp32-foc-v1 --profile release
cargo xtask build --board t-deck-pro --profile release
git diff --check
```

All 363 default-member portable tests and all default-member doc tests pass.
Strict portable Clippy and rustdoc pass. All four board-qualified release images
link:

| Linked target ELF | Bytes | SHA-256 |
| --- | ---: | --- |
| TinyBee V1.0, 8 MiB primary | 10,395,216 | `515389a159f035a8a731d6c047c029dd907dee4792e7ee3b049e7400161b51f6` |
| TinyBee V1.0, 4 MiB opportunistic | 10,398,060 | `b003ce4c119745d964cccd5ad1842c8e2c33cb22d53808452a8b1029c15990f3` |
| MKS ESP32 FOC V1.0 | 9,869,096 | `c6e5d58403b18f60f977c0d908e30a3d8ff5a422b5f523aeae9dece96055d8d4` |
| T-Deck Pro | 10,267,268 | `8557a9c2d65ad9242e055d409b1986a526c8a11dbe8d782ae1853b5cf4765062` |

These are ELF lengths, not flash payload lengths. The firmware lockfile is
68,504 bytes with SHA-256
`5e8fa5c4549e581ddcdf7e18b8d758bbcd953c55e6f7f33d56c2a9aa4dfaef3b`.

The classic ESP32 build also crossed a version-specific `esp-hal` 1.0 direct
Xtensa-call range boundary after the coordinator grew. The project linker
composition now keeps `__pre_init` and the private HAL `esp32_init` together at
the beginning of `.text`; the verified TinyBee ELF places them at `0x400d2c18`
and `0x400d2c28`, 16 bytes apart. The override is restricted to
`xtensa-esp32-none-elf`; ESP32-S3 continues to use the upstream composition.
This is link evidence, not a boot claim.

The interface checkpoint reproduced:

```console
cargo fmt --all -- --check
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

All 98 native unit tests and all doc tests pass. Native and WASM strict Clippy,
strict rustdoc, the checked-out sibling CSGRS/Hyper source and permissive-license
audit, optimized Trunk build, WASM validation, and compression checks pass.

| Interface artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `alumina-interface_bg.wasm` | 4,260,753 | `b8d5d63db3f90920ee6d6aec1b1156b87d627947cd2e6a199e686666d853ba1c` |
| `alumina-interface_bg.wasm.gz` | 1,980,128 | `99e20ab4dd18cc937ddf3fc8bb534a418d2b75f28290246197e2a4ca6e19332f` |
| `alumina-interface_bg.wasm.br` | 1,615,369 | `4b73455d19b5d9c35595241eeb866600eecc00f0dfb07e0263e8e968bef81f47` |
| `alumina-interface.js` | 89,162 | `12efc3498ecb2c8da6cad130ee7046b02eaf01af9b4e3856119f5286869b874c` |
| `index.html` | 1,295 | `0956ea93c3b8fc122cbaf4be42564f87e26426214b7c73f2a0809a54eb683c21` |

The interface lockfile is 96,554 bytes with SHA-256
`f7b0bf4bb9c54ecc536ad697513bde6a3c0f037058eebc027f9d6a22fd0a79ae`.

## Closed claims and next gate

There is no physical boot/reboot SD recovery result, target graph timing,
measured executor WCET, interrupt/Wi-Fi coexistence measurement, graph-owned
physical side effect, or connected-load test. The existing graph opcodes remain
resource-free Boolean operations. Arena limits and available graph resources
are not yet published as canonical board capabilities, and no GPIO, timer, ADC,
PWM, stepper, FOC, safety-output, storage, or protocol resource is yet selectable
by a graph package. Those capability and safety bindings are the next gate.
