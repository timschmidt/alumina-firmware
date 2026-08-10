# Development environment

This workspace builds bare-metal Rust firmware for the ESP32-S3 Xtensa target. The root Cargo configuration selects `xtensa-esp32s3-none-elf` and uses `espflash` to flash and monitor binaries.

## Install the toolchain

Install [Rust through `rustup`](https://rustup.rs/), then install the Espressif toolchain manager and flashing utility:

```sh
cargo install espup espflash
espup install
```

`espup` creates an environment script containing the Xtensa toolchain variables. Load it in each shell used for this workspace:

```sh
source "$HOME/export-esp.sh"
```

You may add that command to your shell profile after confirming the generated path. Verify the installation with:

```sh
rustc --version
espflash --version
rustup target list --installed
```

See the [Rust on ESP Book](https://docs.esp-rs.org/book/) and [`espup` repository](https://github.com/esp-rs/espup) for platform-specific prerequisites and toolchain troubleshooting.

## Build the workspace

From the repository root:

```sh
cargo check --workspace --lib --bins --examples --locked
```

Checking `--all-targets` also requests ordinary Rust test harnesses for `no_std` libraries, which cannot run on the bare-metal Xtensa target. Use the explicit library, binary, and example set above for a complete compile check; run hardware tests separately.

Build a selected firmware image or example in release mode:

```sh
cargo build -p patina --bin patina --release --locked
cargo build -p t-deck-pro-epd-async --example simple_example --release --locked
```

## Flash and monitor

Connect the board over USB, then use the configured Cargo runner:

```sh
cargo flash -p patina --bin patina --release
```

The runner invokes `espflash flash --monitor --chip esp32s3`, so logging continues in the same terminal after flashing. Select another package, binary, or example with the usual Cargo options.

If automatic reset does not enter the ROM bootloader, hold the board's BOOT control while resetting or reconnecting it, then retry. Exact controls can vary by T-Deck Pro hardware revision; consult the [vendor repository](https://github.com/Xinyuan-LilyGO/T-Deck-Pro) and schematic for the board in hand.

## Hardware tests

The `patina` `embedded-test` suite runs on a connected board:

```sh
cargo test -p patina --test hello_test
```

These tests cannot be executed by a host-native Rust test runner. Optional JTAG debugging can be configured with [`probe-rs`](https://probe.rs/), but it is not required for the normal `espflash` workflow.
