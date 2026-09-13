#!/usr/bin/env bash
set -euo pipefail

tool_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_directory="$(cd -- "$tool_directory/../.." && pwd)"
interface_directory="${ALUMINA_INTERFACE_DIRECTORY:-$repository_directory/../alumina-interface}"
interface_dist="$interface_directory/dist"
generated_directory="$tool_directory/target/generated-assets"
interface_commit="$(git -C "$interface_directory" rev-parse HEAD)"

mkdir -p "$generated_directory"
cargo +stable build \
  --manifest-path "$tool_directory/Cargo.toml" \
  --target wasm32-unknown-unknown \
  --release \
  --locked \
  --offline
wasm-bindgen \
  --target web \
  --no-typescript \
  --out-name alumina-brotli-decoder \
  --out-dir "$generated_directory" \
  "$tool_directory/target/wasm32-unknown-unknown/release/alumina_brotli_decoder.wasm"
wasm-opt \
  -Oz \
  --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  "$generated_directory/alumina-brotli-decoder_bg.wasm" \
  -o "$generated_directory/alumina-brotli-decoder_bg.optimized.wasm"
mv "$generated_directory/alumina-brotli-decoder_bg.optimized.wasm" \
  "$generated_directory/alumina-brotli-decoder_bg.wasm"

brotli -q 11 -w 23 -n -f \
  "$interface_dist/alumina-interface.js" \
  -o "$generated_directory/alumina-interface.js.br"
brotli -q 11 -w 23 -n -f \
  "$interface_dist/alumina-interface_bg.wasm" \
  -o "$generated_directory/alumina-interface_bg.wasm.br"
brotli -t "$generated_directory/alumina-interface.js.br"
brotli -t "$generated_directory/alumina-interface_bg.wasm.br"

node "$tool_directory/assemble-assets.mjs" \
  "$repository_directory" \
  "$interface_dist" \
  "$generated_directory" \
  "$interface_commit"
