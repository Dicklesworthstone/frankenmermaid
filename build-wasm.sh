#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$ROOT_DIR/crates/fm-wasm"
OUT_DIR="$ROOT_DIR/pkg"
OUT_NAME="frankenmermaid"
WASM_PATH="$OUT_DIR/${OUT_NAME}_bg.wasm"
TARGET_FEATURES="+bulk-memory,+mutable-globals,+nontrapping-fptoint,+sign-ext,+reference-types,+multivalue"
MAX_GZIP_BYTES=$((500 * 1024))

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "error: wasm-pack is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v wasm-opt >/dev/null 2>&1; then
  echo "error: wasm-opt is required but was not found in PATH (install binaryen)" >&2
  exit 1
fi

echo "==> Ensuring wasm32 target is available"
rustup target add wasm32-unknown-unknown >/dev/null

echo "==> Building fm-wasm with wasm-pack"
mkdir -p "$OUT_DIR"
(
  cd "$CRATE_DIR"
  RUSTFLAGS="-C target-feature=${TARGET_FEATURES}" \
    wasm-pack build \
      --release \
      --target web \
      --out-dir "$OUT_DIR" \
      --out-name "$OUT_NAME"
)

if [[ ! -f "$WASM_PATH" ]]; then
  echo "error: expected output wasm not found at $WASM_PATH" >&2
  exit 1
fi

echo "==> Optimizing wasm with wasm-opt"
wasm-opt -Oz --all-features --converge "$WASM_PATH" -o "$WASM_PATH"

RAW_BYTES="$(wc -c < "$WASM_PATH")"
GZIP_BYTES="$(gzip -c "$WASM_PATH" | wc -c)"

echo "==> Output artifacts"
ls -lh "$OUT_DIR"
echo "Raw wasm size: ${RAW_BYTES} bytes"
echo "Gzipped wasm size: ${GZIP_BYTES} bytes"

if (( GZIP_BYTES > MAX_GZIP_BYTES )); then
  echo "error: gzipped wasm (${GZIP_BYTES} bytes) exceeds budget (${MAX_GZIP_BYTES} bytes)" >&2
  exit 1
fi

echo "==> WASM build completed successfully within size budget"
