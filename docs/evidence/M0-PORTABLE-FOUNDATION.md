# M0 portable-foundation evidence

Date: 2026-08-10

Commit: `45b7edf` (`Establish portable firmware foundations`)

## Implemented claim

- Rust 1.88 workspace with portable default members;
- bounded native frame identity and validation;
- exact integer machine-job structures and admission checks;
- typed board resources with dual-core and ownership invariants;
- fail-closed safety state transitions and physical-reset requirement;
- first-target board registry and JSON capability output;
- dual-license, third-party, clean-room, ADR, and CI policy.

## Reproduced checks

```console
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask board list
cargo xtask board check mks-tinybee-v1
cargo xtask board check t-deck-pro
cargo xtask capabilities --board t-deck-pro --json
git diff --check
```

Results: 17 unit tests and all documentation tests passed; strict Clippy,
formatting, registry validation, and whitespace checks passed.

The `--workspace` form above describes the workspace before ESP-only imported
members were added. Current portable commands use the default members, as shown
in the repository README and CI workflow.
