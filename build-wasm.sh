#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$ROOT_DIR/crates/fm-wasm"
OUT_DIR="$ROOT_DIR/pkg"
OUT_NAME="frankenmermaid"
PACKAGE_NAME="@frankenmermaid/core"
PACKAGE_DESCRIPTION="Rust-first Mermaid-compatible diagram engine for WebAssembly and browser rendering."
PACKAGE_REPOSITORY_URL="git+https://github.com/Dicklesworthstone/frankenmermaid.git"
PACKAGE_HOMEPAGE="https://github.com/Dicklesworthstone/frankenmermaid#readme"
PACKAGE_BUGS_URL="https://github.com/Dicklesworthstone/frankenmermaid/issues"
CAPABILITY_MATRIX_JSON="$ROOT_DIR/evidence/capability_matrix.json"
WASM_PATH="$OUT_DIR/${OUT_NAME}_bg.wasm"
TARGET_FEATURES="+bulk-memory,+mutable-globals,+nontrapping-fptoint,+sign-ext,+reference-types,+multivalue"
RUST_SIZE_FLAGS="-Zlocation-detail=none -Zfmt-debug=none"
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
  RUSTFLAGS="-C target-feature=${TARGET_FEATURES} ${RUST_SIZE_FLAGS}" \
    wasm-pack build \
      --release \
      --target web \
      --out-dir "$OUT_DIR" \
      --out-name "$OUT_NAME" \
      -- --config 'profile.release.package.fm-layout.opt-level="z"'
)

if [[ ! -f "$WASM_PATH" ]]; then
  echo "error: expected output wasm not found at $WASM_PATH" >&2
  exit 1
fi

echo "==> Optimizing wasm with wasm-opt"
wasm-opt -Oz --all-features --converge "$WASM_PATH" -o "$WASM_PATH"

echo "==> Syncing npm package metadata"
cp "$ROOT_DIR/README.md" "$OUT_DIR/README.md"
PACKAGE_JSON="$OUT_DIR/package.json" \
PACKAGE_NAME="$PACKAGE_NAME" \
PACKAGE_DESCRIPTION="$PACKAGE_DESCRIPTION" \
PACKAGE_REPOSITORY_URL="$PACKAGE_REPOSITORY_URL" \
PACKAGE_HOMEPAGE="$PACKAGE_HOMEPAGE" \
PACKAGE_BUGS_URL="$PACKAGE_BUGS_URL" \
PACKAGE_JS="$OUT_DIR/${OUT_NAME}.js" \
PACKAGE_DTS="$OUT_DIR/${OUT_NAME}.d.ts" \
CAPABILITY_MATRIX_JSON="$CAPABILITY_MATRIX_JSON" \
python3 - <<'PY'
import json
import os
from pathlib import Path

package_json = Path(os.environ["PACKAGE_JSON"])
payload = json.loads(package_json.read_text())
payload["name"] = os.environ["PACKAGE_NAME"]
payload["description"] = os.environ["PACKAGE_DESCRIPTION"]
payload["repository"] = {
    "type": "git",
    "url": os.environ["PACKAGE_REPOSITORY_URL"],
}
payload["homepage"] = os.environ["PACKAGE_HOMEPAGE"]
payload["bugs"] = {"url": os.environ["PACKAGE_BUGS_URL"]}
payload["keywords"] = ["mermaid", "diagram", "wasm", "svg", "canvas"]
payload["files"] = [
    "README.md",
    "frankenmermaid_bg.wasm",
    "frankenmermaid.js",
    "frankenmermaid.d.ts",
    "frankenmermaid_bg.wasm.d.ts",
]
package_json.write_text(json.dumps(payload, indent=2) + "\n")

package_js = Path(os.environ["PACKAGE_JS"])
package_dts = Path(os.environ["PACKAGE_DTS"])
capability_matrix = json.loads(Path(os.environ["CAPABILITY_MATRIX_JSON"]).read_text())
capability_matrix_json = json.dumps(capability_matrix, separators=(",", ":"))
source_spans_helper = """
const CAPABILITY_MATRIX = __CAPABILITY_MATRIX__;

function hasKnownSpan(span) {
  if (!span || !span.start || !span.end) {
    return false;
  }

  return Boolean(
    span.start.line || span.start.column || span.start.byte ||
    span.end.line || span.end.column || span.end.byte
  );
}

function sanitizeFragment(raw) {
  let out = "";
  let lastWasDash = false;

  for (const ch of String(raw ?? "")) {
    if ((ch >= "0" && ch <= "9") || (ch >= "A" && ch <= "Z") || (ch >= "a" && ch <= "z")) {
      out += ch.toLowerCase();
      lastWasDash = false;
    } else if (!lastWasDash && out.length > 0) {
      out += "-";
      lastWasDash = true;
    }
  }

  return out.replace(/^-+|-+$/g, "");
}

function nodeElementId(nodeId, index) {
  const fragment = sanitizeFragment(nodeId);
  return fragment ? `fm-node-${fragment}-${index}` : `fm-node-${index}`;
}

function stringifySourceId(value) {
  if (value == null) {
    return undefined;
  }
  if (typeof value === "number" || typeof value === "string") {
    return String(value);
  }
  if (Array.isArray(value) && value.length > 0) {
    return String(value[0]);
  }
  if (typeof value === "object" && 0 in value) {
    return String(value[0]);
  }
  return String(value);
}

export function sourceSpans(input) {
  const parsed = parse(input);
  const ir = parsed && parsed.ir ? parsed.ir : {};
  const records = [];
  const nodes = Array.isArray(ir.nodes) ? ir.nodes : [];
  const edges = Array.isArray(ir.edges) ? ir.edges : [];
  const clusters = Array.isArray(ir.clusters) ? ir.clusters : [];

  nodes.forEach((node, index) => {
    const span = node?.span_primary ?? node?.spanPrimary;
    if (!hasKnownSpan(span)) {
      return;
    }
    const sourceId = typeof node?.id === "string" && node.id.length > 0 ? node.id : undefined;
    records.push({
      kind: "node",
      index,
      id: sourceId,
      elementId: nodeElementId(sourceId ?? "", index),
      span,
    });
  });

  edges.forEach((edge, index) => {
    if (!hasKnownSpan(edge?.span)) {
      return;
    }
    records.push({
      kind: "edge",
      index,
      elementId: `fm-edge-${index}`,
      span: edge.span,
    });
  });

  clusters.forEach((cluster, index) => {
    if (!hasKnownSpan(cluster?.span)) {
      return;
    }
    records.push({
      kind: "cluster",
      index,
      id: stringifySourceId(cluster?.id),
      elementId: `fm-cluster-${index}`,
      span: cluster.span,
    });
  });

  return records;
}

export function capabilityMatrix() {
  return CAPABILITY_MATRIX;
}
""".replace("__CAPABILITY_MATRIX__", capability_matrix_json)
package_js.write_text(
    package_js.read_text() + "\n\n" + source_spans_helper + "\n"
)
package_dts.write_text(
    package_dts.read_text()
    + "\n"
    + "export function sourceSpans(input: string): any[];\n"
    + "/**\n"
    + " * @returns {any}\n"
    + " */\n"
    + "export function capabilityMatrix(): any;\n"
)
PY

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
