// Ask the PINNED incumbent what every `commit` clause combination MEANS, and write the answers down
// as the fixture `crates/fm-parser/tests/fixtures/mermaid_gitgraph_commits.tsv`.
//
// ⚠️ `tag:` IS REPEATABLE, AND THAT IS THE WHOLE REASON THIS IS A GENERATOR. mermaid's db stores
// `tags: string[]` and appends, so `commit id: "two" tag: "v1.0" tag: "stable"` means BOTH tags. A
// hand-written table modelled on the syntax reads `tag:` as one field, keeps the last clause, and
// passes every single-tag row while silently dropping a release tag off every multi-tag one. The
// battery therefore goes up to THREE tags: a two-tag case alone cannot tell "keeps the last" from
// "keeps the first", and cannot catch a reversed list at all.
//
//   npm install --prefix scripts        # jsdom, already a declared devDependency there
//   node scripts/headtohead/gitgraph_commit_battery.mjs
//
// ⚠️ NEEDS A DOM (DOMPurify), AND NEEDS TextEncoder LENT TO IT; see diagram_db_probe.mjs. gitGraph
// hashes commit ids through TextEncoder, which jsdom's window does not provide — without the loan
// every row here fails as a spurious `RUNTIME ERROR` that reads exactly like mermaid rejecting the
// input.
//
// Every commit is given an EXPLICIT id. mermaid generates `<seq>-<hash>` ids otherwise, which would
// bake a hash of this file's phrasing into the fixture and make it churn for no reason.
//
// Columns, tab separated:
//   spec     the text written after `commit`
//   verdict  PARSED | REJECTED
//   id       the id mermaid resolved for the commit — empty unless PARSED
//   message  mermaid's `message` field (empty when no `msg:` clause)
//   tags     mermaid's `tags` array, JSON-encoded, so a tag containing a comma or a pipe survives
//   type     mermaid's numeric commit type: 0 NORMAL, 1 REVERSE, 2 HIGHLIGHT
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const OUT = path.join(HERE, '../../crates/fm-parser/tests/fixtures/mermaid_gitgraph_commits.tsv');

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', {
  runScripts: 'dangerously',
});
const w = dom.window;
for (const name of ['TextEncoder', 'TextDecoder', 'crypto']) {
  if (!w[name] && globalThis[name]) w[name] = globalThis[name];
}
const script = w.document.createElement('script');
script.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(script);
const mermaid = w.mermaid;
if (typeof mermaid?.parse !== 'function') {
  console.error('pinned bundle did not expose window.mermaid.parse');
  process.exit(1);
}
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

// The cross product of the four optional clauses. `tag:` gets 0..3 repetitions with DISTINCT values
// so a dropped or reordered entry is visible in the fixture rather than absorbed by a duplicate.
const TAG_RUNS = [[], ['v1.0'], ['v1.0', 'stable'], ['v1.0', 'stable', 'lts']];
const MESSAGES = [null, 'ship it'];
const TYPES = [null, 'NORMAL', 'REVERSE', 'HIGHLIGHT'];

const cases = [];
let n = 0;
for (const tags of TAG_RUNS) {
  for (const message of MESSAGES) {
    for (const type of TYPES) {
      const id = `c${n++}`;
      const clauses = [`id: "${id}"`];
      if (message !== null) clauses.push(`msg: "${message}"`);
      for (const tag of tags) clauses.push(`tag: "${tag}"`);
      if (type !== null) clauses.push(`type: ${type}`);
      cases.push({ id, spec: clauses.join(' ') });
    }
  }
}

const rows = [];
for (const { id, spec } of cases) {
  const text = `gitGraph\n    commit ${spec}\n`;
  let commit = null;
  try {
    await mermaid.parse(text);
    const diagram = await mermaid.mermaidAPI.getDiagramFromText(text);
    const db = diagram.db ?? diagram.getDB();
    const commits = db.getCommits();
    const entries = commits instanceof w.Map || commits instanceof Map
      ? [...commits.values()]
      : Object.values(commits);
    commit = entries.find((entry) => entry.id === id) ?? null;
  } catch {
    commit = null;
  }
  if (!commit) {
    rows.push([spec, 'REJECTED', '', '', '', ''].join('\t'));
    continue;
  }
  rows.push(
    [
      spec,
      'PARSED',
      commit.id,
      commit.message ?? '',
      JSON.stringify(commit.tags ?? []),
      String(commit.type ?? 0),
    ].join('\t'),
  );
}

const header = ['spec', 'verdict', 'id', 'message', 'tags', 'type'].join('\t');
fs.writeFileSync(OUT, `${header}\n${rows.join('\n')}\n`);

const parsed = rows.filter((row) => row.split('\t')[1] === 'PARSED').length;
const multi = rows.filter((row) => {
  const tags = row.split('\t')[4];
  return tags && JSON.parse(tags).length > 1;
}).length;
console.log(`${OUT}: ${rows.length} rows, ${parsed} PARSED, ${multi} carrying more than one tag`);
