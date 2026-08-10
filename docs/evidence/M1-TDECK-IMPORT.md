# M1 T-Deck import evidence

Date: 2026-08-10

## Source identity

- repository: `https://github.com/timschmidt/t-deck-async-drivers-rs`;
- commit: `497f77e012c6493fc4beadea1cd278dd8918c486`;
- commit tree: `2d6ce517623938f8a4c7c1b069e74680d7eb3be6`;
- source worktree: clean at import;
- tracked files: 117 total: 103 within the ten component trees and 14 root
  metadata/reference files.

The complete mapping and every component tree ID are recorded in
[`imports/t-deck-async-drivers-rs.toml`](../../imports/t-deck-async-drivers-rs.toml).
Immediately after copying, recursive comparisons were byte-identical for all ten
component trees and the root metadata. The root comparison excluded only the
source repository's `.git` directory.

## Integration delta

No Rust source, documentation, fixture, datasheet, or driver behavior was
changed. A post-integration recursive comparison reports only eight changed
manifests:

- four driver manifests and both example manifests replace inherited author or
  license fields with the exact Apache-2.0 upstream values;
- `examples/patina/Cargo.toml` retargets five relative dependencies to their new
  `../../drivers/` locations;
- two otherwise unchanged driver manifests remove one trailing blank line at
  EOF to satisfy the repository whitespace gate.

The root workspace registers all ten crates, carries the upstream dependency
constraints, and makes the ESP32-S3 target settings available without changing
the default host target.

## Reproduced checks

Portable toolchain: Rust 1.88.0. ESP toolchain used for this evidence:
`rustc 1.90.0-nightly (abf50ae2e 2025-09-16)`.

```console
cargo metadata --no-deps --offline --format-version 1
cargo generate-lockfile --offline
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings

cargo +esp check --target xtensa-esp32s3-none-elf --locked --lib \
  -p embedded-bus-async -p sx126x_async \
  -p t-deck-pro-battery-async -p t-deck-pro-epd-async \
  -p t-deck-pro-gps-async -p t-deck-pro-keyboard-async \
  -p t-deck-pro-lora-async -p t-deck-pro-touch-async

cargo +esp check --target xtensa-esp32s3-none-elf --locked --bins \
  -p i2c-tester -p patina
```

Results: metadata and locked portable gates passed; all eight driver libraries
and both integration binaries compiled for `xtensa-esp32s3-none-elf`.

## Claim boundary

This evidence promotes the imported source only to compile-checked. It does not
claim T-Deck Pro pin reconciliation, flash success, peripheral smoke tests,
dual-core Alumina runtime integration, or hardware qualification.
