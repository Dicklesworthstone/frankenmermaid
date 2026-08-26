// Ask the PINNED incumbent which state carries which DESCRIPTION, and write the answers down as
// the fixture `crates/fm-render-svg/tests/fixtures/mermaid_state_descriptions.tsv`.
//
// It lives beside the SVG tests, not the parser's, because the test that consumes it has to prove
// the description reaches the DRAWN TEXT — the parser half alone would pass on an IR nobody renders.
//
// WHY A GENERATOR AND NOT A HAND-WRITTEN TABLE. `s1 : text` looks like a node whose name contains a
// colon, and that is exactly how this parser read it (bd-xm62h): the whole line became the label and
// a box was drawn captioned `s1 : text`, with every description line after the first silently
// dropped. The rows nobody guesses are the ones that pin the fix:
//
//   s3 : first / s3 : second        -> ONE state with TWO descriptions, drawn on two lines
//   state "Desc" as s1 / s1 : more  -> the alias label is a description too, and they accumulate
//   A:::bad                         -> NOT a description; the incumbent records nothing for it
//   C --> D: edge label             -> a transition label, which must never become a description
//
//   npm install --prefix scripts        # jsdom, already a declared devDependency there
//   node scripts/headtohead/state_description_battery.mjs
//
// ⚠️ NEEDS A DOM (DOMPurify); see diagram_db_probe.mjs for why a bare node:vm cannot parse these.
//
// Columns, tab separated:
//   case         name of the battery entry
//   diagram      the whole diagram source, newlines written `\n`
//   state        a state id the incumbent reports (its own `root_start`/`root_end` pseudo-states are
//                excluded — the two engines name those differently and that is a separate contract)
//   descriptions the incumbent's `descriptions` array joined with `|`, EMPTY when it has none
//
// Empty rows are carried deliberately. Half of this defect class is drawing a description the
// incumbent does not have, so a fixture that only listed the states WITH descriptions could not
// catch a splitter that fires too often.
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const OUT = path.join(HERE, '../../crates/fm-render-svg/tests/fixtures/mermaid_state_descriptions.tsv');

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

const CASES = {
  no_space_before_colon: 'stateDiagram-v2\ns1: text here\n[*] --> s1',
  space_before_colon: 'stateDiagram-v2\ns1 : text here\n[*] --> s1',
  two_descriptions: 'stateDiagram-v2\ns1 : first\ns1 : second\n[*] --> s1',
  three_descriptions: 'stateDiagram-v2\ns1 : one\ns1 : two\ns1 : three\n[*] --> s1',
  alias_then_description: 'stateDiagram-v2\nstate "Desc" as s1\ns1 : more\ns1 --> [*]',
  description_then_transition: 'stateDiagram-v2\ns1 : described\ns1 --> s2',
  transition_then_description: 'stateDiagram-v2\ns1 --> s2\ns2 : described later',
  description_with_colon: 'stateDiagram-v2\ns1 : ratio 1:2\n[*] --> s1',
  description_with_spaces: 'stateDiagram-v2\ns1 :   padded text   \n[*] --> s1',
  // Negative controls: every one of these has a top-level colon and none is a description.
  class_shorthand_is_not_a_description: 'stateDiagram-v2\nA --> B\nA:::bad',
  transition_label_is_not_a_description: 'stateDiagram-v2\nC --> D: edge label',
  start_transition_label: 'stateDiagram-v2\n[*] --> A: begin',
  note_is_not_a_description: 'stateDiagram-v2\nA --> B\nnote right of A: hello',
  classdef_is_not_a_description: 'stateDiagram-v2\nclassDef bad fill:#f00\nA --> B\nclass A bad',
  plain_states_have_none: 'stateDiagram-v2\nA --> B\nB --> C',
};

const rows = [];
for (const [name, diagram] of Object.entries(CASES)) {
  await mermaid.parse(diagram);
  const parsed = await mermaid.mermaidAPI.getDiagramFromText(diagram);
  const db = parsed.db ?? parsed.getDB();
  const states = db.getStates();
  const entries =
    states instanceof w.Map || states instanceof Map ? [...states.entries()] : Object.entries(states);
  for (const [id, state] of entries) {
    if (id === 'root_start' || id === 'root_end') continue;
    const descriptions = (state.descriptions ?? []).join('|');
    if (descriptions.includes('\t') || descriptions.includes('\n')) {
      console.error(`${name}/${id}: description contains a column separator; the fixture cannot hold it`);
      process.exit(1);
    }
    rows.push([name, diagram.replaceAll('\n', '\\n'), id, descriptions]);
  }
}

const banner = [
  '# GENERATED by scripts/headtohead/state_description_battery.mjs against mermaid 11.15.0 — do not hand-edit.',
  '# Each row is a state the incumbent built, and the descriptions it attached to it.',
  ['# case', 'diagram', 'state', 'descriptions'].join('\t'),
].join('\n');
fs.writeFileSync(OUT, `${banner}\n${rows.map((row) => row.join('\t')).join('\n')}\n`);
const described = rows.filter((row) => row[3] !== '').length;
console.log(`${rows.length} states -> ${OUT}`);
console.log(`  ${described} carry a description, ${rows.length - described} carry none`);
