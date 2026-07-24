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
// Usage (after `./build-wasm.sh` so pkg/ is current):
//   node scripts/verify_demo_samples.mjs
// Exit code 0 = all good; non-zero prints the offending families.

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

const all = [...problems, ...renderProblems];
if (all.length > 0) {
  console.error("demo sample verification FAILED:");
  for (const line of all) console.error("  - " + line);
  process.exit(1);
}
console.log(
  `demo sample verification OK: ${DIAGRAM_SAMPLES.length} examples, ` +
    `${EXPECTED_FAMILIES.length} supported families, each rendered once through the WASM build.`
);
