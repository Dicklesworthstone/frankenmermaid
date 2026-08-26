// Ask the PINNED incumbent what a battery of state- and sequence-diagram constructs MEANS, and
// print one compact semantic line per construct.
//
// The 2026-08-18 keyword sweep established that our parser ACCEPTS these families' syntax. Accepting
// is not agreeing: the class-diagram generics defect (db49a252) parsed fine in both engines and drew
// different text. This battery asks the second question — same input, same meaning? — by reading
// mermaid's own diagram db rather than its error strings.
//
//   npm install --prefix scripts        # jsdom, already a declared devDependency there
//   node scripts/headtohead/state_sequence_battery.mjs [state|sequence]
//
// ⚠️ NEEDS A DOM (DOMPurify); see diagram_db_probe.mjs for why a bare node:vm cannot run these.
import fs from 'node:fs';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', {
  runScripts: 'dangerously',
});
const w = dom.window;
const script = w.document.createElement('script');
script.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(script);
const mermaid = w.mermaid;
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

const STATE = {
  alias_state: 'stateDiagram-v2\nstate "Long description" as s1\n[*] --> s1',
  description_line: 'stateDiagram-v2\ns1 : first line\ns1 : second line\n[*] --> s1',
  transition_label: 'stateDiagram-v2\nA --> B: go now',
  label_with_colon: 'stateDiagram-v2\nA --> B: ratio 1:2',
  start_label: 'stateDiagram-v2\n[*] --> A: begin',
  end_label: 'stateDiagram-v2\nA --> [*]: done',
  choice: 'stateDiagram-v2\nstate c <<choice>>\nA --> c\nc --> B: yes\nc --> C: no',
  fork_join: 'stateDiagram-v2\nstate f <<fork>>\nstate j <<join>>\nA --> f\nf --> B\nf --> C\nB --> j\nC --> j\nj --> D',
  composite: 'stateDiagram-v2\nstate Outer {\n  [*] --> In\n  In --> [*]\n}\nA --> Outer',
  nested_composite: 'stateDiagram-v2\nstate A {\n  state B {\n    [*] --> C\n  }\n}',
  concurrency: 'stateDiagram-v2\nstate Active {\n  [*] --> One\n  --\n  [*] --> Two\n}',
  note_right: 'stateDiagram-v2\nA --> B\nnote right of A: hello',
  note_left: 'stateDiagram-v2\nA --> B\nnote left of B: hello',
  note_block: 'stateDiagram-v2\nA --> B\nnote right of A\n  line one\n  line two\nend note',
  direction: 'stateDiagram-v2\ndirection LR\nA --> B',
  classdef: 'stateDiagram-v2\nclassDef bad fill:#f00\nA --> B\nclass A bad',
  class_shorthand: 'stateDiagram-v2\nA --> B\nA:::bad',
  state_with_desc_and_note: 'stateDiagram-v2\nstate "Desc" as s\nnote left of s: n\ns --> [*]',
  v1_keyword: 'stateDiagram\nA --> B: v1 syntax',
};

const SEQUENCE = {
  participant_alias: 'sequenceDiagram\nparticipant A as Alice\nA ->> B: hi',
  actor_keyword: 'sequenceDiagram\nactor Alice\nAlice ->> Bob: hi',
  arrow_solid: 'sequenceDiagram\nA -> B: m',
  arrow_dotted: 'sequenceDiagram\nA --> B: m',
  arrow_solid_head: 'sequenceDiagram\nA ->> B: m',
  arrow_dotted_head: 'sequenceDiagram\nA -->> B: m',
  arrow_cross: 'sequenceDiagram\nA -x B: m',
  arrow_dotted_cross: 'sequenceDiagram\nA --x B: m',
  arrow_open_async: 'sequenceDiagram\nA -) B: m',
  arrow_dotted_async: 'sequenceDiagram\nA --) B: m',
  bidirectional: 'sequenceDiagram\nA <<->> B: m',
  bidirectional_dotted: 'sequenceDiagram\nA <<-->> B: m',
  activation_suffix: 'sequenceDiagram\nA ->>+ B: m\nB -->>- A: r',
  activate_keyword: 'sequenceDiagram\nA ->> B: m\nactivate B\nB -->> A: r\ndeactivate B',
  loop_block: 'sequenceDiagram\nloop every day\n  A ->> B: m\nend',
  alt_else: 'sequenceDiagram\nalt is ok\n  A ->> B: yes\nelse not ok\n  A ->> B: no\nend',
  opt_block: 'sequenceDiagram\nopt maybe\n  A ->> B: m\nend',
  par_and: 'sequenceDiagram\npar one\n  A ->> B: m\nand two\n  A ->> C: m\nend',
  critical_option: 'sequenceDiagram\ncritical connect\n  A ->> B: m\noption timeout\n  A ->> B: retry\nend',
  break_block: 'sequenceDiagram\nA ->> B: m\nbreak failure\n  B ->> A: err\nend',
  rect_block: 'sequenceDiagram\nrect rgb(200,200,255)\n  A ->> B: m\nend',
  note_over: 'sequenceDiagram\nA ->> B: m\nNote over A,B: shared',
  note_right: 'sequenceDiagram\nA ->> B: m\nNote right of B: text',
  autonumber: 'sequenceDiagram\nautonumber\nA ->> B: m\nA ->> B: n',
  box_group: 'sequenceDiagram\nbox Aqua Team\n  participant A\n  participant B\nend\nA ->> B: m',
  create_destroy: 'sequenceDiagram\nA ->> B: m\ncreate participant C\nB ->> C: spawn\ndestroy C\nC -->> B: bye',
  links_block: 'sequenceDiagram\nparticipant A\nlinks A: {"Dashboard": "https://x"}\nA ->> B: m',
  multiline_message: 'sequenceDiagram\nA ->> B: line one<br/>line two',
};

function summarizeState(db) {
  const states = db.getStates();
  const rows = [];
  const entries = states instanceof w.Map || states instanceof Map ? [...states.entries()] : Object.entries(states);
  for (const [id, state] of entries) {
    const note = state.note ? ` note[${state.note.position}]="${[].concat(state.note.text ?? []).join('|')}"` : '';
    const descriptions = (state.descriptions ?? []).join('|');
    rows.push(
      `${id}{type=${state.type ?? ''}${descriptions ? ` desc="${descriptions}"` : ''}` +
        `${state.classes?.length ? ` classes=${state.classes.join(',')}` : ''}` +
        `${state.doc ? ` doc=${state.doc.length}` : ''}${note}}`,
    );
  }
  const relations = (db.getRelations() ?? []).map(
    (relation) =>
      `${relation.id1 ?? relation.state1?.id}->${relation.id2 ?? relation.state2?.id}` +
      `${relation.relationTitle ? `:"${relation.relationTitle}"` : ''}`,
  );
  return `states=[${rows.join(' ')}] relations=[${relations.join(' ')}] dir=${db.getDirection?.() ?? ''}`;
}

function summarizeSequence(db) {
  const actors = db.getActors();
  const entries = actors instanceof w.Map || actors instanceof Map ? [...actors.entries()] : Object.entries(actors);
  const actorRows = entries.map(
    ([id, actor]) => `${id}{desc="${actor.description ?? ''}" type=${actor.type ?? ''}${actor.links && Object.keys(actor.links).length ? ' links' : ''}}`,
  );
  const messages = (db.getMessages() ?? []).map(
    (message) => `${message.from ?? ''}->${message.to ?? ''}:t${message.type}${message.message ? `:"${message.message}"` : ''}`,
  );
  const boxes = (db.getBoxes?.() ?? []).map((box) => `${box.name ?? ''}[${(box.actorKeys ?? []).join(',')}]`);
  return `actors=[${actorRows.join(' ')}] messages=[${messages.join(' ')}]${boxes.length ? ` boxes=[${boxes.join(' ')}]` : ''}`;
}

const which = process.argv[2] ?? 'all';
const families = [];
if (which === 'all' || which === 'state') families.push(['state', STATE, summarizeState]);
if (which === 'all' || which === 'sequence') families.push(['sequence', SEQUENCE, summarizeSequence]);

for (const [family, battery, summarize] of families) {
  console.log(`### ${family}`);
  for (const [name, text] of Object.entries(battery)) {
    try {
      await mermaid.parse(text);
      const diagram = await mermaid.mermaidAPI.getDiagramFromText(text);
      const db = diagram.db ?? diagram.getDB();
      console.log(`${name}\tPARSED\t${summarize(db)}`);
    } catch (error) {
      const message = String(error?.message ?? error).split('\n')[0];
      const syntax = /Parse error|Expecting |Lexical error|Unrecognized text|No diagram type detected/i.test(message);
      console.log(`${name}\t${syntax ? 'REJECTED' : 'RUNTIME'}\t${message}`);
    }
  }
}
