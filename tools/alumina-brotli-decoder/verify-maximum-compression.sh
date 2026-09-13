#!/usr/bin/env bash
set -euo pipefail

tool_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_directory="$(cd -- "$tool_directory/../.." && pwd)"
interface_directory="${ALUMINA_INTERFACE_DIRECTORY:-$repository_directory/../alumina-interface}"
interface_dist="$interface_directory/dist"
generated_directory="$tool_directory/target/generated-assets"
benchmark_directory="$(mktemp -d /tmp/alumina-brotli-window-XXXXXX)"

cleanup() {
    rm -rf -- "$benchmark_directory"
}
trap cleanup EXIT

verify_asset() {
    local label="$1"
    local source="$2"
    local retained="$3"
    local retained_size
    local best_size
    local best_windows=""

    retained_size="$(stat -c %s "$retained")"
    best_size=""
    for window in 0 {10..24}; do
        local candidate="$benchmark_directory/$label-standard-w$window.br"
        local candidate_size
        brotli -q 11 -w "$window" -n -f "$source" -o "$candidate"
        candidate_size="$(stat -c %s "$candidate")"
        printf '%s standard-w%s %s bytes\n' "$label" "$window" "$candidate_size"
        if [[ -z "$best_size" ]] || (( candidate_size < best_size )); then
            best_size="$candidate_size"
            best_windows="$window"
        elif (( candidate_size == best_size )); then
            best_windows="$best_windows,$window"
        fi
    done

    cmp --silent "$retained" "$benchmark_directory/$label-standard-w23.br" || {
        printf '%s retained bytes differ from a fresh q11/w23 encoding\n' "$label" >&2
        return 1
    }
    if (( retained_size != best_size )); then
        printf '%s retained=%s best=%s windows=%s\n' \
            "$label" "$retained_size" "$best_size" "$best_windows" >&2
        return 1
    fi

    for window in {24..30}; do
        local candidate="$benchmark_directory/$label-large-w$window.br"
        local candidate_size
        brotli -q 11 --large_window="$window" -n -f "$source" -o "$candidate"
        candidate_size="$(stat -c %s "$candidate")"
        printf '%s incompatible-large-w%s %s bytes\n' "$label" "$window" "$candidate_size"
        if (( candidate_size < retained_size )); then
            printf '%s incompatible window %s is unexpectedly smaller: %s < %s\n' \
                "$label" "$window" "$candidate_size" "$retained_size" >&2
            return 1
        fi
    done

    printf '%s retained minimum %s bytes; standard winners=%s\n' \
        "$label" "$retained_size" "$best_windows"
}

verify_asset \
    js \
    "$interface_dist/alumina-interface.js" \
    "$generated_directory/alumina-interface.js.br"
verify_asset \
    wasm \
    "$interface_dist/alumina-interface_bg.wasm" \
    "$generated_directory/alumina-interface_bg.wasm.br"
