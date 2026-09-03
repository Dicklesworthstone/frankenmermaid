#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
manifest="$script_dir/Renderer/RendererManifest.json"

digest() {
    shasum -a 256 "$1" | awk '{print $1}'
}

expected_wrapper=$(sed -n 's/.*"wrapperSha256": "\([0-9a-f]*\)".*/\1/p' "$manifest")
expected_wasm=$(sed -n 's/.*"wasmSha256": "\([0-9a-f]*\)".*/\1/p' "$manifest")
expected_deck=$(sed -n 's/.*"deckRuntimeSha256": "\([0-9a-f]*\)".*/\1/p' "$manifest")

actual_wrapper=$(digest "$repo_dir/pkg/frankenmermaid.js")
actual_wasm=$(digest "$repo_dir/pkg/frankenmermaid_bg.wasm")
actual_deck=$(digest "$repo_dir/crates/fm-cli/src/deck_runtime.js")

if [ "$expected_wrapper" != "$actual_wrapper" ] || \
   [ "$expected_wasm" != "$actual_wasm" ] || \
   [ "$expected_deck" != "$actual_deck" ]; then
    echo "error: FrankenMermaid Apple renderer manifest does not match tracked engine artifacts" >&2
    exit 1
fi

if [ "${1:-}" = "--check" ]; then
    exit 0
fi

destination=${1:?"usage: stage-renderer.sh <built-app-renderer-directory> | --check"}
mkdir -p "$destination"
cp "$repo_dir/pkg/frankenmermaid.js" "$destination/frankenmermaid.js"
cp "$repo_dir/pkg/frankenmermaid_bg.wasm" "$destination/frankenmermaid_bg.wasm"
cp "$repo_dir/pkg/frankenmermaid.d.ts" "$destination/frankenmermaid.d.ts"
cp "$repo_dir/crates/fm-cli/src/deck_runtime.js" "$destination/deck_runtime.js"
cp "$manifest" "$destination/RendererManifest.json"
