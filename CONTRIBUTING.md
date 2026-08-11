# Contributing to Aluminafw

## Licensing

New contributions are accepted under `MIT OR Apache-2.0`. Imported files retain
their original license, copyright, notices, and provenance. Add or update
`THIRD_PARTY.toml` before copying any external source or substantial asset.

Do not add credentials, private signing keys, Wi-Fi secrets, production device
identities, or unlicensed board photographs. Annotated photographs require a
source, author, license, exact board revision, and content digest.

## Clean-room motion and FOC work

Synthetos/g2 and SimpleFOC are behavioral references, not implementation sources.
The following boundary applies even when an upstream license would permit more:

1. Requirements authors may study published feature descriptions, user manuals,
   protocol-independent behavior, academic/control mathematics, standards, and
   hardware datasheets. They write implementation-neutral requirements and
   observable test vectors with citations.
2. Implementers work from those approved requirements and general mathematical
   sources. They do not translate, transcribe, port, or structurally imitate the
   reference implementation.
3. Every implementation change records the requirements/test-vector identifiers
   it satisfies and lists the sources actually consulted.
4. Review compares behavior, numerical invariants, timing, safety, and traces—not
   source similarity. Any suspiciously source-shaped artifact is rejected and
   independently rewritten.
5. Numerical proposal code never decides canonical geometry or motion without
   exact/certified replay at the boundary specified by the Hyper stack.

The same policy applies to Klipper architectural inspiration: no Klipper protocol
or GPL implementation code is imported.

Do not clone, vendor, or make build/test tooling fetch a GPL-, LGPL-, or
AGPL-family source tree. Such projects may appear only as URLs and license-
identified functional references in the research ledger. Dependency review and
the cargo-deny allowlist remain mandatory even when a transitive package seems
technically useful.

## Safety and scope

- Core 0 owns service work; core 1 owns every hazardous or deterministic resource.
- Single-core application MCUs are rejected.
- Tests use disconnected, current-limited, or harmless loads until the relevant
  board qualification gate is archived.
- Never weaken an interlock, output timeout, digest check, queue bound, or fault
  latch solely to make a test or demo pass.
- Firmware never accepts raw G-code or source geometry.

## Local checks

Run from the repository root:

```text
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee-v1
cargo xtask board check t-deck-pro
```

ESP firmware/driver checks use the `esp` toolchain and an explicit target through
`xtask`; the root default remains the pinned stable host toolchain so portable
logic never requires hardware to test.
