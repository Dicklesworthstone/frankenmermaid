// Does the published WASM `parse()` return IR for every diagram family, or does it throw?
//
// ⚠️ THIS GUARD CANNOT LIVE IN A RUST TEST, and that is the reason this file exists. `fm-wasm`
// serializes with `serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true)` — required,
// because the published TS contract declares `Record<string, …>` and the default JS `Map` output
// silently broke consumers (bd-tm1q7). That serializer REJECTS a non-string map key. No native
// serializer reproduces the constraint: `serde_json` and `toml` both stringify integer keys without
// complaint (verified — toml emits `[commit_lanes]\n0 = 0`), so a Rust test passes either way. Only
// the real boundary can fail.
//
// WHAT IT CAUGHT: `parse()` threw `Map key is not a string and cannot be an object key` for EVERY
// gitGraph (`IrGitGraphMeta::commit_lanes`, keyed by node index) and for EVERY diagram using a
// markdown label (`MermaidDiagramIr::label_markup`, keyed by `IrLabelId`) — while `renderSvg` on the
// same input succeeded, which is why the gap survived: nothing that rendered was broken.
//
//   node scripts/headtohead/wasm_parse_conformance.mjs
//   node scripts/headtohead/wasm_parse_conformance.mjs /path/to/fresh/wasm/package
//
// Exit 0 = parsing and semantic edits crossed the real JS/WASM boundary for every case.
import assert from "node:assert/strict";
import fs from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG = process.argv[2] ? resolve(process.argv[2]) : join(HERE, "..", "..", "pkg");
const fm = await import(pathToFileURL(join(PKG, "frankenmermaid.js")).href);
fm.initSync({ module: fs.readFileSync(join(PKG, "frankenmermaid_bg.wasm")) });

// One case per family, plus the two that carry a non-string-keyed map. The markdown-label cases are
// listed separately from the plain flowchart on purpose: a plain label populates no `label_markup`,
// so a sweep that only tested `A --> B` reported flowchart healthy while markdown labels threw.
const CASES = [
  ["flowchart", "flowchart LR\n  A --> B"],
  ["flowchart markdown label", 'flowchart LR\n  A["`**bold**`"] --> B'],
  ["class markdown label", 'classDiagram\n  class A["`**bold**`"]'],
  ["gitGraph commits", "gitGraph\n  commit\n  commit"],
  [
    "gitGraph branch/merge",
    "gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n  checkout main\n  merge dev",
  ],
  ["sequence", "sequenceDiagram\n  A->>B: x"],
  ["state", "stateDiagram-v2\n  a --> b"],
  ["class", "classDiagram\n  A <|-- B"],
  ["er", "erDiagram\n  A ||--o{ B : has"],
  ["pie", 'pie\n  "a" : 1'],
  ["mindmap", "mindmap\n  root((a))\n    b"],
  ["timeline", "timeline\n  title t\n  2021 : a"],
  ["journey", "journey\n  title j\n  section s\n    T: 5: Me"],
  ["gantt", "gantt\n  title g\n  section s\n  a: a1, 2024-01-01, 3d"],
  ["kanban", "kanban\n  Todo\n    t[x]"],
  ["quadrant", "quadrantChart\n  x-axis Low --> High"],
  ["xychart", "xychart-beta\n  line [1, 2, 3]"],
  ["sankey", "sankey-beta\n\na,b,10"],
  ["block", "block-beta\n  columns 2\n  a b"],
  ["packet", 'packet-beta\n  0-7: "src"'],
  [
    "requirement",
    "requirementDiagram\n  requirement r {\n    id: 1\n    text: t\n    risk: high\n    verifymethod: test\n  }",
  ],
  ["C4", 'C4Context\n  title c\n  Person(a, "A", "d")'],
];

let failures = 0;
for (const [name, source] of CASES) {
  let verdict;
  try {
    const parsed = fm.parse(source);
    // Reaching the IR is the whole point: the failure mode is serialization, not parsing, so a
    // result that never materialises as a JS object is exactly what this has to catch.
    const nodes = (parsed.ir.nodes ?? []).length;
    verdict = `ok   type=${parsed.ir.diagram_type} nodes=${nodes}`;
  } catch (error) {
    failures += 1;
    verdict = `FAIL ${String(error).slice(0, 90)}`;
  }
  console.log(`${name.padEnd(26)} ${verdict}`);
}

// ── FRONT MATTER MUST SURVIVE THE wasm32 BUILD ────────────────────────────────────────────────
//
// ⚠️ ANOTHER GUARD THAT CANNOT LIVE IN A RUST TEST, for the same structural reason as the rest of
// this file: the bug was a `#[cfg(target_arch = "wasm32")]` early return in
// `parse_front_matter_config` that discarded the ENTIRE front-matter block. Every native test
// compiles the other branch, so all of them passed while the shipped browser bundle silently threw
// away `title`, `config: theme:` and every other key. Only the real wasm32 artifact can fail here.
const FRONT_MATTER_CASES = [
  [
    "front matter title",
    "---\ntitle: FMTITLE\n---\nflowchart LR\n  A --> B",
    (ir) => (ir.meta?.title === "FMTITLE" ? null : `title=${JSON.stringify(ir.meta?.title)}`),
  ],
  // A `title:` nested under `config:` is NOT the document title -- an indentation-blind scan would
  // caption the diagram with it.
  [
    "nested title is not the title",
    "---\nconfig:\n  title: NESTED\n---\nflowchart LR\n  A --> B",
    (ir) =>
      ir.meta?.title === undefined || ir.meta?.title === null
        ? null
        : `a nested key became the document title: ${JSON.stringify(ir.meta?.title)}`,
  ],
  // ⚠️ ASSERTS THE DIAGNOSTIC, NOT AN `init` RECORD. `config:` is deliberately still native-only
  // here (serde_yaml is excluded from the wasm build to hold the gzip ceiling), so the contract
  // this case pins is that the reader is TOLD. An earlier draft checked `ir.meta.init` existed --
  // which passes even with the config ignored, because recording the diagnostic itself creates
  // that record. It would have gone green whether or not anything worked.
  [
    "front matter config notice",
    "---\nconfig:\n  theme: dark\n---\nflowchart LR\n  A --> B",
    (ir, warnings) =>
      warnings.some((w) => w.includes("wasm32"))
        ? null
        : `config keys dropped with no warning: ${JSON.stringify(warnings)}`,
  ],
  // The title-only block must NOT draw that warning: there is nothing left to ignore.
  [
    "front matter title is silent",
    "---\ntitle: FMTITLE\n---\nflowchart LR\n  A --> B",
    (ir, warnings) =>
      warnings.some((w) => w.includes("wasm32"))
        ? `a title-only block warned about ignored config: ${JSON.stringify(warnings)}`
        : null,
  ],
];

for (const [name, source, check] of FRONT_MATTER_CASES) {
  let verdict;
  try {
    const parsed = fm.parse(source);
    const complaint = check(parsed.ir, parsed.warnings ?? []);
    if (complaint) {
      failures += 1;
      verdict = `FAIL ${complaint}`;
    } else {
      verdict = "ok   front matter survived the wasm32 boundary";
    }
  } catch (error) {
    failures += 1;
    verdict = `FAIL ${String(error).slice(0, 90)}`;
  }
  console.log(`${name.padEnd(26)} ${verdict}`);
}

// Semantic putback must accept the exact object emitted by parseLens, including maps and
// family-specific metadata. JSON.stringify/parse would bypass the boundary this test protects.
const LENS_CASES = [
  {
    name: "flowchart semantic lens",
    source: 'flowchart LR\r\n  %% retain this comment\r\n  A["`**Client**`"] -->|request| B[Server]\r\n',
    before: "request",
    after: "authenticated 🦀 request",
    check: ir => assert.ok(Object.keys(ir.label_markup ?? {}).length > 0),
  },
  {
    name: "sequence semantic lens",
    source: 'sequenceDiagram\r\n  autonumber 10 2\r\n  participant A as "Client"\r\n  participant B as Server\r\n  alt ready\r\n    A->>+B: request\r\n    B-->>-A: response\r\n  else waiting\r\n    A->>B: retry\r\n  end\r\n  Note right of A: retained note\r\n',
    before: "request",
    after: "authenticated 🦀 request",
    check: ir => assert.equal(ir.sequence_meta.autonumber_start, 10),
  },
  {
    name: "class semantic lens",
    source: 'classDiagram\r\n  A["Client"]\r\n  B[Server]\r\n  A : +String name\r\n  A : +send() bool\r\n  A "1" *-- "many" B : request\r\n',
    before: "request",
    after: "authenticated 🦀 request",
    check: ir => {
      assert.equal(ir.nodes[0].class_meta.attributes.length, 1);
      assert.equal(ir.nodes[0].class_meta.methods.length, 1);
      assert.equal(ir.edges[0].extras.source_cardinality, "1");
      assert.equal(ir.edges[0].extras.target_cardinality, "many");
    },
  },
  {
    name: "state semantic lens",
    source: 'stateDiagram-v2\r\n  state "Session" as A\r\n  state "Done" as B\r\n  A --> B: "finish [ready] / cleanup()"\r\n  note right of A: retained note\r\n',
    before: "finish [ready] / cleanup()",
    after: "terminé 🦀 [ready] / cleanup()",
    check: ir => {
      assert.equal(ir.edges[0].extras.guard, "ready");
      assert.equal(ir.edges[0].extras.action, "cleanup()");
      assert.equal(ir.state_notes.length, 1);
    },
  },
  {
    name: "ER semantic lens",
    source: 'erDiagram\r\n  A["Client"] {\r\n    int id PK\r\n  }\r\n  B[Server]\r\n  A ||--o{ B : request\r\n',
    before: "request",
    after: "authenticated 🦀 request",
    check: ir => {
      assert.equal(ir.nodes[0].members.length, 1);
      assert.equal(ir.edges[0].extras.er_notation, "||--o{");
    },
  },
];

for (const { name, source, before, after, check } of LENS_CASES) {
  let verdict;
  try {
    assert.equal(typeof fm.applyIrLensEdit, "function", "missing semantic edit export");
    const snapshot = fm.parseLens(source);
    check(snapshot.parsed.ir);
    const noop = fm.applyIrLensEdit(source, structuredClone(snapshot.parsed.ir));
    assert.equal(noop.updatedSource, source);
    assert.deepEqual(noop.snapshot, snapshot);

    const edited = structuredClone(snapshot.parsed.ir);
    const labelIndex = edited.edges[0].label;
    assert.equal(edited.labels[labelIndex].text, before);
    edited.labels[labelIndex].text = after;
    const unchangedInput = structuredClone(edited);
    const result = fm.applyIrLensEdit(source, edited);
    assert.equal(result.updatedSource, source.replace(before, after));
    assert.deepEqual(edited, unchangedInput, "putback must not mutate the caller's IR");
    assert.deepEqual(result.snapshot, fm.parseLens(result.updatedSource));
    assert.equal(
      result.snapshot.parsed.ir.labels[result.snapshot.parsed.ir.edges[0].label].text,
      after,
    );
    check(result.snapshot.parsed.ir);

    const restored = structuredClone(result.snapshot.parsed.ir);
    restored.labels[restored.edges[0].label].text = before;
    assert.equal(fm.applyIrLensEdit(result.updatedSource, restored).updatedSource, source);
    verdict = "ok   full IR, formatting, metadata, and repeated edits survived";
  } catch (error) {
    failures += 1;
    verdict = `FAIL ${String(error).slice(0, 160)}`;
  }
  console.log(`${name.padEnd(26)} ${verdict}`);
}

try {
  const source = "sequenceDiagram\nA->>B: request\n";
  const edited = structuredClone(fm.parseLens(source).parsed.ir);
  edited.labels[edited.edges[0].label].text = "changed\nB->>A: injected";
  assert.throws(() => fm.applyIrLensEdit(source, edited), /re-emit/);
  assert.throws(() => fm.applyIrLensEdit(source, {}), /invalid edited diagram IR/);
  const unsupported = 'pie\n"Share": 10\n';
  assert.throws(
    () => fm.applyIrLensEdit(unsupported, fm.parseLens(unsupported).parsed.ir),
    /unsupported for pie/,
  );
  console.log("semantic lens rejections   ok   malformed, injected, and unsupported edits rejected");
} catch (error) {
  failures += 1;
  console.log(`semantic lens rejections   FAIL ${String(error).slice(0, 160)}`);
}

console.log(
  failures === 0
    ? "\nALL PARSE AND SEMANTIC EDIT CASES CROSS THE BOUNDARY"
    : `\n${failures} PARSE OR SEMANTIC EDIT CASES FAILED`,
);
process.exit(failures === 0 ? 0 : 1);
