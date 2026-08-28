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
//
// Exit 0 = every case returned IR. Exit 1 = at least one family cannot cross the boundary.
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG = join(HERE, '..', '..', 'pkg');
const fm = await import(join(PKG, 'frankenmermaid.js'));
fm.initSync({ module: fs.readFileSync(join(PKG, 'frankenmermaid_bg.wasm')) });

// One case per family, plus the two that carry a non-string-keyed map. The markdown-label cases are
// listed separately from the plain flowchart on purpose: a plain label populates no `label_markup`,
// so a sweep that only tested `A --> B` reported flowchart healthy while markdown labels threw.
const CASES = [
  ['flowchart', 'flowchart LR\n  A --> B'],
  ['flowchart markdown label', 'flowchart LR\n  A["`**bold**`"] --> B'],
  ['class markdown label', 'classDiagram\n  class A["`**bold**`"]'],
  ['gitGraph commits', 'gitGraph\n  commit\n  commit'],
  ['gitGraph branch/merge', 'gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n  checkout main\n  merge dev'],
  ['sequence', 'sequenceDiagram\n  A->>B: x'],
  ['state', 'stateDiagram-v2\n  a --> b'],
  ['class', 'classDiagram\n  A <|-- B'],
  ['er', 'erDiagram\n  A ||--o{ B : has'],
  ['pie', 'pie\n  "a" : 1'],
  ['mindmap', 'mindmap\n  root((a))\n    b'],
  ['timeline', 'timeline\n  title t\n  2021 : a'],
  ['journey', 'journey\n  title j\n  section s\n    T: 5: Me'],
  ['gantt', 'gantt\n  title g\n  section s\n  a: a1, 2024-01-01, 3d'],
  ['kanban', 'kanban\n  Todo\n    t[x]'],
  ['quadrant', 'quadrantChart\n  x-axis Low --> High'],
  ['xychart', 'xychart-beta\n  line [1, 2, 3]'],
  ['sankey', 'sankey-beta\n\na,b,10'],
  ['block', 'block-beta\n  columns 2\n  a b'],
  ['packet', 'packet-beta\n  0-7: "src"'],
  ['requirement', 'requirementDiagram\n  requirement r {\n    id: 1\n    text: t\n    risk: high\n    verifymethod: test\n  }'],
  ['C4', 'C4Context\n  title c\n  Person(a, "A", "d")'],
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
  ['front matter title', '---\ntitle: FMTITLE\n---\nflowchart LR\n  A --> B',
    (ir) => (ir.meta?.title === 'FMTITLE' ? null : `title=${JSON.stringify(ir.meta?.title)}`)],
  // A `title:` nested under `config:` is NOT the document title -- an indentation-blind scan would
  // caption the diagram with it.
  ['nested title is not the title', '---\nconfig:\n  title: NESTED\n---\nflowchart LR\n  A --> B',
    (ir) => (ir.meta?.title === undefined || ir.meta?.title === null
      ? null
      : `a nested key became the document title: ${JSON.stringify(ir.meta?.title)}`)],
  // ⚠️ ASSERTS THE DIAGNOSTIC, NOT AN `init` RECORD. `config:` is deliberately still native-only
  // here (serde_yaml is excluded from the wasm build to hold the gzip ceiling), so the contract
  // this case pins is that the reader is TOLD. An earlier draft checked `ir.meta.init` existed --
  // which passes even with the config ignored, because recording the diagnostic itself creates
  // that record. It would have gone green whether or not anything worked.
  ['front matter config notice', '---\nconfig:\n  theme: dark\n---\nflowchart LR\n  A --> B',
    (ir, warnings) => (warnings.some((w) => w.includes('wasm32'))
      ? null
      : `config keys dropped with no warning: ${JSON.stringify(warnings)}`)],
  // The title-only block must NOT draw that warning: there is nothing left to ignore.
  ['front matter title is silent', '---\ntitle: FMTITLE\n---\nflowchart LR\n  A --> B',
    (ir, warnings) => (warnings.some((w) => w.includes('wasm32'))
      ? `a title-only block warned about ignored config: ${JSON.stringify(warnings)}`
      : null)],
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
      verdict = 'ok   front matter survived the wasm32 boundary';
    }
  } catch (error) {
    failures += 1;
    verdict = `FAIL ${String(error).slice(0, 90)}`;
  }
  console.log(`${name.padEnd(26)} ${verdict}`);
}

console.log(failures === 0 ? '\nALL FAMILIES CROSS THE BOUNDARY' : `\n${failures} FAMILIES CANNOT CROSS THE BOUNDARY`);
process.exit(failures === 0 ? 0 : 1);
