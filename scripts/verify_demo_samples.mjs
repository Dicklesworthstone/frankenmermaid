// Verify every diagram example baked into frankenmermaid_demo_showcase.html actually renders
// through the SHIPPED WebAssembly build (pkg/frankenmermaid.js + pkg/frankenmermaid_bg.wasm).
//
// Motivation (GH#3 / GH#4): the demo playground used to default to an example that trapped the
// runtime ("RuntimeError: unreachable"), and its gallery carried ~8 near-duplicate copies of ~10
// families while 14 supported families were absent. This script is the regression guard for both:
//   * it extracts the `DIAGRAM_SAMPLES` array literal verbatim from the HTML (between the
//     `>>> demo-samples:start` / `>>> demo-samples:end` markers), so it checks exactly what ships;
//   * it asserts every family the ENGINE supports (fm-core `DiagramType::support_level()`, mirrored
//     by evidence/capability_matrix.json) appears EXACTLY once — no dupes, none missing;
//   * it renders each example via the real WASM `renderSvg` and fails on any that throws or that
//     silently produces empty output.
//
// GH#5 extended this guard to the OTHER render path: at the time the showcase routed only 10
// `frankenReadyCategories` families through the WASM engine — every other family (and any WASM
// failure) fell through to the mermaid.js baseline. The "Rendering Requirements Trace" tile
// broke on THAT path (mermaid's requirement grammar rejects unquoted values containing `-`)
// while this script stayed green, because it only exercised the WASM path. Step 4 below
// parses every sample through real mermaid.js (same major as the CDN pin in the HTML) under
// jsdom, so a sample that either engine rejects fails the guard.
//
// GH#8/#9: the showcase now routes ALL supported families through the WASM engine (the page
// showcases this engine); mermaid.js remains only as the boot/render-failure fallback and the
// playground comparison surface. The step-4 baseline parse stays: the fallback path must keep
// accepting every shipped sample.
//
// Usage (after `./build-wasm.sh` so pkg/ is current, and `bun install` in scripts/ for the
// mermaid baseline dependencies):
//   node scripts/verify_demo_samples.mjs [--wasm-only]
// Exit code 0 = all good; non-zero prints the offending families.
// `--wasm-only` skips the mermaid.js baseline pass (step 4) for environments without the
// scripts/node_modules install; CI and pre-release runs should NOT pass it.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..");

// 1. Extract the sample literal from the HTML and evaluate just that array.
const html = readFileSync(join(repo, "frankenmermaid_demo_showcase.html"), "utf8");
const start = html.indexOf("// >>> demo-samples:start");
const end = html.indexOf("// >>> demo-samples:end");
if (start === -1 || end === -1) {
  console.error("could not find demo-samples markers in frankenmermaid_demo_showcase.html");
  process.exit(2);
}
const block = html.slice(start, end);
const arrayStart = block.indexOf("[");
const arrayEnd = block.lastIndexOf("]");
const literal = block.slice(arrayStart, arrayEnd + 1);
let DIAGRAM_SAMPLES;
try {
  // The literal is self-contained (template strings, no interpolation, no external refs).
  DIAGRAM_SAMPLES = new Function(`return (${literal});`)();
} catch (error) {
  console.error("failed to evaluate DIAGRAM_SAMPLES literal:", error.message);
  process.exit(2);
}

// 2. The engine's own supported-family taxonomy (fm-core DiagramType::support_level()).
//    `sequence` is the single Partial family; everything else is Supported. The demo `category`
//    keys are the showcase's names for these families.
const EXPECTED_FAMILIES = [
  "flowchart", "sequence", "class", "state", "er", "gantt", "journey", "timeline",
  "pie", "gitGraph", "mindmap", "requirement", "quadrantChart", "sankey", "xyChart",
  "blockBeta", "packetBeta", "architectureBeta", "c4Context", "c4Container",
  "c4Component", "c4Dynamic", "c4Deployment", "kanban",
];

const seen = new Map();
for (const entry of DIAGRAM_SAMPLES) {
  seen.set(entry.category, (seen.get(entry.category) ?? 0) + 1);
}
const problems = [];
for (const family of EXPECTED_FAMILIES) {
  const count = seen.get(family) ?? 0;
  if (count === 0) problems.push(`MISSING family: ${family}`);
  else if (count > 1) problems.push(`DUPLICATE family (${count}x): ${family}`);
}
for (const family of seen.keys()) {
  if (!EXPECTED_FAMILIES.includes(family)) problems.push(`UNKNOWN family: ${family}`);
}

// 3. Render each example through the shipped WASM build.
const initModule = await import(join(repo, "pkg/frankenmermaid.js"));
const wasmBytes = readFileSync(join(repo, "pkg/frankenmermaid_bg.wasm"));
await initModule.default({ module_or_path: wasmBytes });

const renderProblems = [];
for (const entry of DIAGRAM_SAMPLES) {
  let svg;
  try {
    svg = initModule.renderSvg(entry.code, undefined);
  } catch (error) {
    renderProblems.push(`${entry.category} (${entry.label}) THREW: ${String(error).slice(0, 120)}`);
    continue;
  }
  if (typeof svg !== "string" || !svg.includes("<svg")) {
    renderProblems.push(`${entry.category} (${entry.label}) produced no SVG`);
  }
}

// 4. Parse each example through real mermaid.js — the showcase's baseline path for every family
//    the WASM allowlist does not cover, and its fallback when a WASM render throws. `parse` (not
//    `render`) is the right level here: jsdom has no layout engine, and the failure class this
//    guards against (GH#5) is a parse rejection.
const baselineProblems = [];
if (process.argv.includes("--wasm-only")) {
  console.log("NOTE: --wasm-only passed; skipping the mermaid.js baseline parse pass.");
} else {
  let mermaid;
  try {
    const { JSDOM } = await import("jsdom");
    const dom = new JSDOM("<!DOCTYPE html><html><body></body></html>", { pretendToBeVisual: true });
    globalThis.window = dom.window;
    globalThis.document = dom.window.document;
    if (!globalThis.navigator) globalThis.navigator = dom.window.navigator;
    mermaid = (await import("mermaid")).default;
    mermaid.initialize({ startOnLoad: false, securityLevel: "loose" });
  } catch (error) {
    console.error(
      "failed to load the mermaid.js baseline (run `bun install` in scripts/, or pass --wasm-only to skip):",
      String(error).split("\n")[0],
    );
    process.exit(2);
  }
  for (const entry of DIAGRAM_SAMPLES) {
    try {
      await mermaid.parse(entry.code);
    } catch (error) {
      const detail = String(error?.message ?? error).split("\n").slice(0, 3).join(" | ");
      baselineProblems.push(
        `${entry.category} (${entry.label}) REJECTED by mermaid.js baseline: ${detail.slice(0, 200)}`,
      );
    }
  }
}

const all = [...problems, ...renderProblems, ...baselineProblems];
if (all.length > 0) {
  console.error("demo sample verification FAILED:");
  for (const line of all) console.error("  - " + line);
  process.exit(1);
}
console.log(
  `demo sample verification OK: ${DIAGRAM_SAMPLES.length} examples, ` +
    `${EXPECTED_FAMILIES.length} supported families, each rendered once through the WASM build` +
    (process.argv.includes("--wasm-only") ? "." : " and parsed by the mermaid.js baseline.")
);
