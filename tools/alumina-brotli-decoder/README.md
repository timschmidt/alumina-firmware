# Alumina browser Brotli decoder

This isolated build tool emits the small safe-Rust WASM decoder used by the
embedded interface bootstrap. It is not linked into ESP32 firmware. The large
interface JS and WASM resources are encoded at maximum Brotli quality with the
smallest RFC 7932 window that attains the exhaustive byte minimum
(`quality = 11`, `lgwin = 23`). Incompatible large-window extensions are
deliberately excluded and are measured by `verify-maximum-compression.sh`.

The wrapper is `MIT OR Apache-2.0`. Its `brotli-decompressor` dependency is
`BSD-3-Clause/MIT`, contains no unsafe code under the selected default feature
set, and is covered by `THIRD_PARTY_LICENSES.md`.

Build and unit test with:

```sh
cargo +stable test --manifest-path tools/alumina-brotli-decoder/Cargo.toml --locked
cargo +stable build --manifest-path tools/alumina-brotli-decoder/Cargo.toml \
  --target wasm32-unknown-unknown --release --locked
```
