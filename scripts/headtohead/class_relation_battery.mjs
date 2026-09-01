// Ask the PINNED incumbent what EVERY class relation spelling means, and write the answers down as
// the fixture `crates/fm-parser/tests/fixtures/mermaid_class_relations.tsv`.
//
// WHY A CROSS-PRODUCT AND NOT A LIST. mermaid's class relation is not an enumeration of arrow
// spellings, it is a PRODUCT, and every list we have written down has been short. `CLASS_OPERATORS`
// has now been patched four separate times for a spelling somebody forgot — `<--` (bd-lfucm),
// `o--`/`*--` (bd-92b6), `..|>`/`<|..` (bd-u9hcc), `()--` (bd-lkm9i) — and each time the next
// missing one was found by accident. The grammar is:
//
//   relation : relationType lineType relationType | relationType lineType
//            | lineType relationType             | lineType
//   relationType ::= 0 AGGREGATION | 1 EXTENSION | 2 COMPOSITION | 3 DEPENDENCY | 4 LOLLIPOP
//   lineType     ::= 0 LINE (solid) | 1 DOTTED_LINE
//
// So this battery does not ask about the spellings we happen to know. It asks about ALL of them:
// every start marker x every line type x every end marker, and records the incumbent's verdict for
// each — including the REJECTIONS, which are just as load-bearing. A fixture that listed only the
// accepted forms could not stop us from "fixing" a spelling mermaid does not have.
//
//   npm install --prefix scripts        # jsdom, already a declared devDependency there
//   node scripts/headtohead/class_relation_battery.mjs
//
// ⚠️ NEEDS A DOM. `scripts/headtohead/parse_probe.mjs` runs the bundle under bare `node:vm` and
// CANNOT parse a classDiagram at all — mermaid's class db calls DOMPurify's `addHook` during setup,
// so every classDiagram comes back `RUNTIME ERROR: Ro.addHook is not a function` regardless of its
// syntax. That verdict is not a syntax verdict, and reading it as one is how a spelling gets
// declared unsupported when it parses fine.
//
// Columns, tab separated:
//   op       the operator as written between the two class names
//   ids      the class ids the incumbent's db ends up holding, comma separated
//   type1    start-end relation type: none | 0..4 as above
//   type2    end-end relation type
//   line     0 solid, 1 dotted
//
// ⚠️ `ids` IS THE COLUMN THIS FIXTURE EXISTS FOR. The recurring defect is not a missing marker, it
// is a PHANTOM NODE: our table-driven scan splits `Alpha o--o Beta` at its `o--` entry and keeps
// `o Beta` as the endpoint, inventing a class the author never declared. The incumbent holds
// exactly [Alpha,Beta] for every spelling it accepts, and that is the invariant to hold us to.
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const OUT = path.join(HERE, '../../crates/fm-parser/tests/fixtures/mermaid_class_relations.tsv');

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', {
  runScripts: 'dangerously',
});
const w = dom.window;
const script = w.document.createElement('script');
script.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(script);
const mermaid = w.mermaid;
if (typeof mermaid?.parse !== 'function') {
  console.error('pinned bundle did not expose window.mermaid.parse');
  process.exit(1);
}
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

// The marker halves as they are actually SPELLED at each end. `<|` opens a triangle and `|>` closes
// one, and DEPENDENCY is a bare `<` / `>`, so neither of those two markers is the same bytes on
// both sides; aggregation, composition and the lollipop are.
//
// ⚠️ THE FIRST VERSION OF THIS LIST LEFT DEPENDENCY OUT and produced a 50-row fixture that looked
// complete. It was the same short-list mistake `CLASS_OPERATORS` has now made four times, committed
// in the very file written to stop it — which is why the counts are asserted below rather than
// eyeballed: 6 starts x 6 ends x 2 line types, minus the two duplicate bare line types, is 71.
const STARTS = ['', 'o', '*', '<|', '()', '<'];
const ENDS = ['', 'o', '*', '|>', '()', '>'];
const LINES = ['--', '..'];

const ops = [];
for (const line of LINES) {
  for (const start of STARTS) {
    for (const end of ENDS) {
      const op = `${start}${line}${end}`;
      if (!ops.includes(op)) ops.push(op);
    }
  }
}

const rows = [];
for (const op of ops) {
  const text = `classDiagram\n    Alpha ${op} Beta\n`;
  try {
    await mermaid.parse(text);
    const diagram = await mermaid.mermaidAPI.getDiagramFromText(text);
    const db = diagram.db ?? diagram.getDB();
    const classes = db.getClasses();
    const ids = [...(classes.keys ? classes.keys() : Object.keys(classes))];
    const relations = db.getRelations();
    if (relations.length !== 1) {
      rows.push([op, ids.join(','), 'REJECTED', 'REJECTED', 'REJECTED']);
      continue;
    }
    const { type1, type2, lineType } = relations[0].relation;
    rows.push([op, ids.join(','), String(type1), String(type2), String(lineType)]);
  } catch {
    // A throw here is the grammar refusing the spelling. Recorded, not skipped: the rejections are
    // half the contract — see the header.
    rows.push([op, '', 'REJECTED', 'REJECTED', 'REJECTED']);
  }
}

const header = ['op', 'ids', 'type1', 'type2', 'line'].join('\t');
fs.mkdirSync(path.dirname(OUT), { recursive: true });
fs.writeFileSync(OUT, `${header}\n${rows.map((r) => r.join('\t')).join('\n')}\n`);
// The cross-product must be COMPLETE, not merely large: an entry silently dropped from STARTS or
// ENDS shrinks the fixture without failing anything, and a shorter oracle is how this defect keeps
// coming back. 6 starts x 2 line types x 6 ends = 72, all distinct — the empty start and empty end
// combine to the bare `--` / `..`, which no other pair spells.
const EXPECTED = STARTS.length * LINES.length * ENDS.length;
if (rows.length !== EXPECTED) {
  console.error(`expected ${EXPECTED} spellings, built ${rows.length} — STARTS/ENDS/LINES drifted`);
  process.exit(1);
}
const accepted = rows.filter((r) => r[2] !== 'REJECTED').length;
console.log(`wrote ${rows.length} rows (${accepted} accepted, ${rows.length - accepted} rejected) -> ${OUT}`);
