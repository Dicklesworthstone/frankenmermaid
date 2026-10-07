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
RUST_SIZE_FLAGS="-Zlocation-detail=none -Zfmt-debug=none -Zunstable-options -Cpanic=immediate-abort"
# Gzipped-wasm ceiling. Raised 500K -> 540K on 2026-07-24: the committed pkg/ had drifted ~2
# months behind source (last regenerated May), so it predated the IncrementalLayoutEngine
# integration and the parseLens/applyParseLensEdit bindings. The first regeneration since
# (which also carries the GH#3 web-time/Instant fix) legitimately lands at ~506K gzip; the
# growth is accumulated fm-wasm/fm-layout functionality, not bloat from this change. wasm-opt
# already runs -Oz --converge, so this reflects real code size. Keep tightening opportunistically.
#
# Raised 590K -> 650K on 2026-08-24 for the accumulated workspace and layout features
# (24 diagram types, DOT bridge, CGA edge routing, incremental Adapton caching, lens system).
#
# Raised 650K -> 680K on 2026-08-26 for the graph-deck feature (bd-cb9oi): the renderDeck
# export links fm-render-svg's deck scene-resolution/projection plus DeckManifest
# serialization, measured at +26.8K gzip (642.7K -> 669.6K). Shrink attempts logged in the
# bead: routing serialization through serde_json + JSON.parse instead of serde_wasm_bindgen
# recovered only 254 raw bytes (reverted for API uniformity) — the cost is the feature's
# resolution logic, not the serializer. Headroom after raise: ~10K.
#
# Raised 680K -> 690K on 2026-08-27 for the radar-beta diagram family (bd-sk4dv). MEASURED across
# this session's three builds, so the cost is attributed rather than assumed:
#   686057 gzip  before treemap
#   691733 gzip  after treemap  (+5676, and it PASSED under the 680K ceiling — no raise needed)
#   697170 gzip  after radar    (+5437, 850 bytes over the ceiling, i.e. 0.12%)
# So this raise is attributable to radar alone: a whole diagram family — parser, polar layout,
# cardinal-spline renderer — for 5.3K gzip. wasm-opt already runs -Oz --converge, so this is real
# code size, not slack. Headroom after raise: ~9.2K.
#
# Raised 690K -> 700K on 2026-08-27 while repairing a stale pkg/: the committed package had been
# rebuilt from a pre-morph tree (schema 1.0.0, no nodeGeometry/edgeEndpoints, Map-typed manifest
# fields), so a fresh build of the ACTUAL head was the first honest measurement in several
# features' worth of landings. Measured: 708507 gzip, 1937 bytes (0.27%) over the 690K ceiling.
# Attribution: the morph manifest joins were already inside the ceiling at 684717 on their own
# build; the remainder is the parser wave that landed against the stale pkg without paying here —
# transitive forward-declared subgraph endpoints (bd-dw2a9), empty-subgraph-as-rect (bd-kat55),
# labelled-endpoint subgraph resolution (bd-honvo), and cluster classDef propagation (bd-6cdzy).
# wasm-opt still runs -Oz --converge. Headroom after raise: ~8.3K.
#
# Raised 700K -> 701K on 2026-08-29 (bd-zrz6r): the parity/support-matrix wave since the 700K
# raise (bd-zlmow) consumed the ~8.3K headroom and left HEAD 180 bytes over. MEASURED at HEAD:
# 716980 gzip, +180 (0.025%) over the 700K ceiling. Attribution: ~60 engine commits since
# bd-zlmow, dominated by the bd-1buv incumbent-parity sweep (title table, subgraph fixed-point,
# autonumber circles, ER crow's-foot markers, sankey rounding), the bd-5k51.1 support-matrix
# wave (C4 directional relationships/legends/numbering, WebGPU sequence fragments, canvas
# dividers), the bd-1t7l lens family (rank-local LayoutLens, wasm DiagramLens composition), and
# the bd-6qd.4 config-schema validators. Shrink levers were attempted FIRST, per the doctrine
# above: 20d8d9b4 already trimmed the embedded style payload for this bead; re-running wasm-opt
# (-Oz --converge, and with --gufa) on the already-optimized module GROWS output (718159-719789
# gzip); the module carries no name/producers/DWARF sections to strip; fm-render-term is already
# outside the wasm surface. Headroom after that raise was ~820 bytes. The next structural lever was:
# -Zbuild-std=std,panic_abort with immediate-abort panic machinery.
#
# Applied on 2026-09-03 after the defensive LayoutLens validation and parser/a11y correctness
# fixes moved the package from 716954 to 720164 gzip bytes (+3210), 2340 bytes over this ratchet.
# Rebuilding the WASM-only standard library with the nightly immediate-abort panic strategy reduced
# the same source to 699884 bytes (-20280 versus the ordinary std build). Browser-visible errors
# already cross the wasm-bindgen Result boundary; unwinding is unavailable on this target, so the
# removed panic formatting/unwind machinery is not part of the package API.
# Headroom after applying the lever: 17940 bytes (~17.5 KiB), without raising the ceiling.
#
# ⚠️ RAISED IN THE COMMIT THAT NEEDED IT, which is what the raises above also did, and it is
# stated here rather than left to be inferred from a diff. The ceiling is a resource ratchet with a
# measured-justification procedure, not a correctness gate to be quietly relaxed — if a future
# change cannot say what it cost and why, it should shrink instead of raising.
#
# Raised 701K -> 706K on 2026-10-06 for two new diagram families, MEASURED per family from the
# committed pkg/ at each step (`gzip -c | wc -c`, the measurement below):
#   702544 gzip  before (1e5ed69)
#   709388 gzip  + ishikawa and treeView-beta families, terminal text-block fix  (+6844, PASSED)
#   722087 gzip  + venn-beta family and canvas `fill-opacity`                    (+12699, 4263 over)
# So the raise is attributable to venn-beta alone: a lexer/statement parser, the IR meta and its
# serialisation, and a geometry engine (lens-area inversion, greedy placement, stress refinement,
# largest-margin region search, text grids). Shrink levers were applied FIRST and are what brought
# it from 726452 to 722087 (-4365): dropping the generic `[String]::sort` and an index `sort_by`
# for hand-rolled scans, building styles through the shared CSS parser instead of a new
# BTreeMap-from-iterator instantiation, f32 geometry instead of f64 libm trig, and one shared
# warning formatter. Measured function-by-function against the pre-venn build with names kept, the
# remainder is the family's own inlined layout (~8.4K pre-opt) and parser (~6.6K pre-opt).
# Headroom after raise: 857 bytes.
#
# Raised 706K -> 712K on 2026-10-06 for four syntax gaps in existing families, measured the same
# way from the committed pkg/ (c9ad966, 722087 gzip):
#   728708 gzip  + `xychart-beta horizontal`, gitGraph `branch … order:`, quadrant point styling
#                  (`radius`/`color`/`stroke-*`, `classDef` + `:::class`), requirement `style`
#                  statements                                                    (+6621, 5764 over)
#   727729 gzip  after shrink levers                                             (+5642, 4785 over)
# The levers: a hand-rolled stable insertion sort for the branch-order remap instead of a `sort_by`
# instantiation (~3.3K pre-opt of drift/quicksort/smallsort monomorphs), and `strip_suffix("px")`
# instead of `trim_end_matches("px")`, which pulled in a reverse `StrSearcher` (~1.2K pre-opt).
# Function-by-function against c9ad966 with names kept, what remains is the features' own code:
# the parser (~4.9K pre-opt: quadrant classDef/`:::` resolution and node styling, the branch-order
# remap), the horizontal axis furniture in `render_xychart_svg` (~3.5K pre-opt; the vertical path
# is the hand-streamed hot path and was deliberately not refactored for bytes), the xychart layout
# branch and plot-bounds helpers (~1.0K), and quadrant point style parse/serialise/paint (~1.6K).
# Headroom after raise: 1359 bytes.
#
# Raised 712K -> 726K on 2026-10-06 for the `wardley-beta` family, measured the same way from the
# committed pkg/ (6b8e3ea, 728519 gzip):
#   742177 gzip  + wardley-beta: grammar, IR meta + serialisation, projection layout,
#                  linear guardrail pricing, renderer inline-style fix        (+13658, 13089 over)
#   742134 gzip  after merging the four `Name [v, e]` statement arms        (-43)
# Function-by-function against 6b8e3ea with names kept: `parse_wardley` with its inlined builder
# and link splitter ~15.4K pre-opt (the whole statement grammar — size, evolution stages and
# boundaries, anchors, components with labels / sourcing / inertia, pipelines, six link spellings
# with flows and annotations, evolve, notes, annotations, (de)accelerators — plus lowering to
# ~10 kinds of marks), the projection inlined into `compute_traced_layout_with_config_and_guardrails`
# ~4.4K, and serde for the five new IR structs ~2.5K. No shared machinery was added; the remainder
# is the family. Headroom after raise: 1290 bytes.
#
# Raised 726K -> 733K on 2026-10-06 for the `eventmodeling` family, the last mermaid 11.15.0 type
# this renderer lacked, measured the same way from the committed pkg/ (5ac1939, 742134 gzip):
#   749987 gzip  + eventmodeling: whitespace-insensitive lexer and statement grammar, lane keying,
#                  labels with markup, flow rules, IR meta + serialisation, swimlane layout (+7853)
#   748995 gzip  after replacing the lane `sort_unstable_by_key` with an insertion sort (-992;
#                  ~1.6K pre-opt of quicksort/smallsort/ipnsort instantiations)
# Function-by-function against 5ac1939 with names kept: the parser inlined into
# `parse_mermaid_with_detection_and_config` ~6.3K pre-opt plus the lexer's data reader 0.7K, the
# layout inlined into the guardrail dispatcher ~2.9K, serde for the new meta ~0.4K. The remainder
# is the family. Headroom after raise: 1597 bytes.
#
# Raised 733K -> 734K on 2026-10-07 for sequence diagrams, measured from the committed pkg/:
#   748995 gzip  (53f8063)
#   750542 gzip  + source-order timeline: notes, fragment headers and branches get their own
#                  rows, note-only regions, created participants at their message (dcb63ff, fit)
#   751261 gzip  + `actor` drawn as a stick figure in its own `render_node` arm   (+719, 669 over)
# No lever was found inside the figure itself — a circle, one path and the shared label writer —
# so the raise is that arm. Headroom after raise: 355 bytes.
#
# Raised 734K -> 737K on 2026-10-07 for class/ER relationship ends, measured against 700a609
# (751261 gzip, rebuilt by this script in the same session):
#   753918 gzip  + ends sharing one point on a node side are fanned apart, cardinality labels are
#                  stepped out of the node along the path, start diamonds face along the path
#   753572 gzip  after replacing the fan's stable `sort_by` with an insertion sort (-346; ~11.6K
#                  pre-opt of driftsort/quicksort/smallsort instantiations)
# Function-by-function against 700a609 with names kept: the fan inlined into
# `compute_traced_layout_with_config_and_guardrails` ~2.8K pre-opt, the direction-aware
# cardinality writer +0.3K net. Headroom after raise: 1116 bytes, of which the same commit's
# flowchart `~~~` invisible links then took 1033 (+461 pre-opt: the tilde run in the link grammar,
# the skip in the SVG edge writer and scene builder, and leaving them out of the `<desc>`):
#   754944 gzip  with the skip in `render_edge_into`, which changed how wasm-opt inlined it
#   754605 gzip  after moving the skip onto the arrow already read in `render_edge_body_into` and
#                folding the `<desc>` filter into its existing `filter_map`              (-339)
MAX_GZIP_BYTES=$((737 * 1024))

compute_source_sha256() {
  python3 - "$ROOT_DIR" <<'PY'
import hashlib
import sys
from pathlib import Path

root = Path(sys.argv[1])
fixed_inputs = [
    ".cargo/config.toml",
    "Cargo.lock",
    "Cargo.toml",
    "build-wasm.sh",
    "evidence/capability_matrix.json",
    "rust-toolchain.toml",
]
crate_inputs = [
    "fm-core",
    "fm-layout",
    "fm-parser",
    "fm-render-canvas",
    "fm-render-svg",
    "fm-wasm",
]

inputs = [root / relative for relative in fixed_inputs]
for crate in crate_inputs:
    crate_dir = root / "crates" / crate
    inputs.append(crate_dir / "Cargo.toml")
    inputs.extend(path for path in (crate_dir / "src").rglob("*") if path.is_file())

hasher = hashlib.sha256()
for path in sorted(inputs, key=lambda item: item.relative_to(root).as_posix()):
    relative = path.relative_to(root).as_posix()
    data = path.read_bytes()
    if relative == "crates/fm-wasm/src/lib.rs":
        marker = b"\n#[cfg(test)]\nmod tests {"
        if marker not in data:
            raise SystemExit("error: fm-wasm test-module marker not found")
        data = data.split(marker, 1)[0]
    hasher.update(relative.encode())
    hasher.update(b"\0")
    hasher.update(len(data).to_bytes(8, "little"))
    hasher.update(data)

print(hasher.hexdigest())
PY
}

SOURCE_SHA256_BEFORE="$(compute_source_sha256)"
echo "==> Source input SHA-256: $SOURCE_SHA256_BEFORE"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "error: wasm-pack is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v wasm-opt >/dev/null 2>&1; then
  echo "error: wasm-opt is required but was not found in PATH (install binaryen)" >&2
  exit 1
fi

echo "==> Ensuring wasm32 target and Rust sources are available"
rustup target add wasm32-unknown-unknown >/dev/null
rustup component add rust-src >/dev/null

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
      . \
      -- -Zbuild-std=std,panic_abort \
         --config 'profile.release.package.fm-layout.opt-level="z"' \
         --config 'profile.release.package.fm-parser.opt-level="z"' \
         --config 'profile.release.package.fm-render-svg.opt-level="z"' \
         --config 'profile.release.package.fm-core.opt-level="z"'
)

if [[ ! -f "$WASM_PATH" ]]; then
  echo "error: expected output wasm not found at $WASM_PATH" >&2
  exit 1
fi

echo "==> Optimizing wasm with wasm-opt"
# Enable exactly the features the build targets (TARGET_FEATURES above), NOT --all-features:
# binaryen 125's --all-features lets optimization passes rewrite types with `exact` heap types
# (custom-descriptors proposal), producing a module Node and current browsers refuse to
# instantiate ("invalid heap type 'exact'"). The explicit list keeps the output inside the
# widely-shipped wasm feature set.
wasm-opt -Oz \
  --enable-bulk-memory \
  --enable-mutable-globals \
  --enable-nontrapping-float-to-int \
  --enable-sign-ext \
  --enable-reference-types \
  --enable-multivalue \
  --converge "$WASM_PATH" -o "$WASM_PATH"

SOURCE_SHA256_AFTER="$(compute_source_sha256)"
if [[ "$SOURCE_SHA256_AFTER" != "$SOURCE_SHA256_BEFORE" ]]; then
  echo "error: WASM source inputs changed during the build" >&2
  echo "before: $SOURCE_SHA256_BEFORE" >&2
  echo "after:  $SOURCE_SHA256_AFTER" >&2
  exit 1
fi

echo "==> Syncing npm package metadata"
cp "$ROOT_DIR/README.md" "$OUT_DIR/README.md"
cp "$ROOT_DIR/LICENSE" "$OUT_DIR/LICENSE"
PACKAGE_JSON="$OUT_DIR/package.json" \
PACKAGE_NAME="$PACKAGE_NAME" \
PACKAGE_DESCRIPTION="$PACKAGE_DESCRIPTION" \
PACKAGE_REPOSITORY_URL="$PACKAGE_REPOSITORY_URL" \
PACKAGE_HOMEPAGE="$PACKAGE_HOMEPAGE" \
PACKAGE_BUGS_URL="$PACKAGE_BUGS_URL" \
PACKAGE_JS="$OUT_DIR/${OUT_NAME}.js" \
PACKAGE_DTS="$OUT_DIR/${OUT_NAME}.d.ts" \
CAPABILITY_MATRIX_JSON="$CAPABILITY_MATRIX_JSON" \
SOURCE_SHA256="$SOURCE_SHA256_AFTER" \
python3 - <<'PY'
import json
import os
from pathlib import Path

package_json = Path(os.environ["PACKAGE_JSON"])
payload = json.loads(package_json.read_text())
payload["name"] = os.environ["PACKAGE_NAME"]
payload["license"] = "SEE LICENSE IN LICENSE"
payload["description"] = os.environ["PACKAGE_DESCRIPTION"]
payload["repository"] = {
    "type": "git",
    "url": os.environ["PACKAGE_REPOSITORY_URL"],
}
payload["homepage"] = os.environ["PACKAGE_HOMEPAGE"]
payload["bugs"] = {"url": os.environ["PACKAGE_BUGS_URL"]}
payload["keywords"] = ["mermaid", "diagram", "wasm", "svg", "canvas"]
payload["frankenmermaidSourceSha256"] = os.environ["SOURCE_SHA256"]
payload["files"] = [
    "LICENSE",
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
