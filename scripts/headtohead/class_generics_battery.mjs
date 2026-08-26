// Ask the PINNED incumbent what a class-diagram MEMBER with generic type parameters DISPLAYS as,
// and write the answers down as the fixture
// `crates/fm-render-svg/tests/fixtures/mermaid_class_generics.tsv`.
//
// WHY A GENERATOR AND NOT A HAND-WRITTEN TABLE. mermaid's `~T~` -> `<T>` rewrite is not the
// substitution anyone would guess. Measured against 11.15.0:
//
//   List~int~ items         -> List<int> items          the documented case
//   a~T~ b~U~               -> a<T< b>U>                NOT two groups: outermost pair first
//   List~List~int~~ nested  -> List<List<int>> nested   nesting falls out of the same loop
//   weird~ x                -> weird~ x                 a lone tilde is left alone
//   Map~String, int~ lookup -> Map<String, int> lookup  a comma INSIDE the group still converts
//   Pair~A, B, C~ p         -> Pair~A, B, C~ p          but only when the comma splits it in TWO
//
// A table written from what the syntax LOOKS like gets at least the third, fourth and sixth rows
// wrong, which is the same defect class as bd-lrl48 (the flowchart link spelling table).
//
//   npm install --prefix scripts        # jsdom, already a declared devDependency there
//   node scripts/headtohead/class_generics_battery.mjs
//
// ⚠️ THIS ONE NEEDS A DOM, unlike parse_probe.mjs / link_battery.mjs. mermaid's classDb runs every
// member through `sanitizeText`, which reaches DOMPurify, which needs a real `document` — in a bare
// `node:vm` the class path dies with `Ro.addHook is not a function`. Stubbing DOMPurify is what
// mermaid_bench.mjs refuses to do (it would flatter our timings); here there is no timing at all
// and jsdom is the honest way to give the incumbent the environment it asks for.
//
// Columns, tab separated:
//   member    the member line as written inside `class Foo { … }`
//   id        the member id mermaid parsed out of it — the string it feeds to parseGenericTypes
//   display   mermaid's own `getDisplayDetails().displayText`, i.e. what it draws
//
// Every row is an ATTRIBUTE (no parens) carrying an EXPLICIT visibility marker, so `display` is
// exactly `visibility + parseGenericTypes(id)` with no other transform mixed in. Methods add a
// `(params)` and a ` : returnType` tail that mermaid converts FIELD BY FIELD (measured:
// `+f~T(x~) void` leaves BOTH tildes alone because neither field has two), and the spacing of that
// tail is a separate open divergence — keeping it out of this fixture keeps the fixture about the
// rewrite and nothing else.
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const OUT = path.join(HERE, '../../crates/fm-render-svg/tests/fixtures/mermaid_class_generics.tsv');

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

// The battery is the tilde grammar's shape, not a list of members anyone would write: zero, one and
// two tildes; an even count that is NOT a nesting; a real nesting; an empty group; a comma inside a
// group; a comma that splits the group in three; a comma with an odd count on one side; and each of
// the four visibility markers, because the marker is consumed BEFORE the rewrite sees the string.
const MEMBERS = [
  '+plainField',
  '+String name',
  '+weird~ x',
  '+x ~y',
  '+x~',
  '+~x',
  '+List~int~ items',
  '+items List~int~',
  '+tilde~~ x',
  '+~T~ x',
  '+a~T~ b~U~',
  '+List~List~int~~ nested',
  '+Map~String, int~ lookup',
  '+Map~String,int~ lookup',
  '+Pair~A, B, C~ p',
  '+m Map~String, List~int~~',
  '+Map~String, Map~String, int~~ deep',
  '+A~a, b~ and B~c, d~',
  '+a,b',
  '-List~int~ xs',
  '#Set~T~ s',
  '~pkg List~int~',
];

const rows = [];
for (const member of MEMBERS) {
  const text = `classDiagram\nclass Foo {\n${member}\n}\n`;
  await mermaid.parse(text);
  const diagram = await mermaid.mermaidAPI.getDiagramFromText(text);
  const db = diagram.db ?? diagram.getDB();
  const classes = db.getClasses();
  const foo = classes.get ? classes.get('Foo') : classes.Foo;
  if (foo.methods.length !== 0 || foo.members.length !== 1) {
    console.error(
      `${member}: expected exactly one ATTRIBUTE, got ${foo.members.length} attrs / ${foo.methods.length} methods`
    );
    process.exit(1);
  }
  const [attr] = foo.members;
  const display = attr.getDisplayDetails().displayText;
  // The marker is stripped by mermaid BEFORE the rewrite, so the fixture is only honest if the
  // display really is marker + f(id). Assert it here rather than letting a row that breaks the
  // assumption sit in the file looking like evidence.
  if (!display.startsWith(attr.visibility)) {
    console.error(`${member}: display ${JSON.stringify(display)} does not start with its visibility`);
    process.exit(1);
  }
  rows.push([member, attr.id, display]);
}

const banner = [
  '# GENERATED by scripts/headtohead/class_generics_battery.mjs against mermaid 11.15.0 — do not hand-edit.',
  '# Each row is what the incumbent ACTUALLY displays for `classDiagram / class Foo { <member> }`.',
  ['# member', 'id', 'display'].join('\t'),
].join('\n');
fs.writeFileSync(OUT, `${banner}\n${rows.map((row) => row.join('\t')).join('\n')}\n`);
const rewritten = rows.filter((row) => row[2].includes('<')).length;
console.log(`${rows.length} members -> ${OUT}`);
console.log(`  ${rewritten} rewritten to angle brackets, ${rows.length - rewritten} left as written`);
