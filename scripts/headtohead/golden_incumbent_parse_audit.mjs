// Ask the PINNED incumbent whether it can parse each of our own golden fixtures.
//
// ⚠️ A GOLDEN THE INCUMBENT REJECTS CANNOT BE CHECKED AGAINST IT. `equivalence.mjs` compares our SVG
// against mermaid's for the same input; if mermaid will not parse that input there is nothing to
// compare, and the family's "parity" is asserted only by our own goldens agreeing with themselves.
// That is a dialect, not compatibility, and it is invisible from inside this repo.
//
//   node scripts/headtohead/golden_incumbent_parse_audit.mjs [--dir <path>]
//
// ⚠️ NEEDS A DOM (DOMPurify); see diagram_db_probe.mjs.
//
// REJECTED is not automatically a defect. This engine's stated goal is best-effort parsing of
// Mermaid-LIKE input, so accepting more than the incumbent can be deliberate. What is never
// deliberate is a REFERENCE FIXTURE written in syntax the reference rejects, because that fixture
// then pins our behaviour to nobody's specification.
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const dirIndex = argv.indexOf('--dir');
const DIR = dirIndex >= 0 ? argv[dirIndex + 1] : path.join(HERE, '../../crates/fm-cli/tests/golden');

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', {
  runScripts: 'dangerously',
});
const w = dom.window;
for (const name of ['TextEncoder', 'TextDecoder', 'crypto', 'structuredClone']) {
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

const files = fs.readdirSync(DIR).filter((name) => name.endsWith('.mmd')).sort();
const rejected = [];
for (const name of files) {
  const text = fs.readFileSync(path.join(DIR, name), 'utf8');
  let verdict = 'PARSED';
  let detail = '';
  try {
    await mermaid.parse(text);
  } catch (error) {
    const message = String(error?.message ?? error);
    verdict = /Parse error|Expecting |Lexical error|Unrecognized text|No diagram type detected/i.test(message)
      ? 'REJECTED'
      : 'RUNTIME ERROR';
    detail =
      message.split('\n').find((line) => /Expecting|Unrecognized|No diagram type/.test(line))
      ?? message.split('\n')[0];
  }
  if (verdict !== 'PARSED') {
    rejected.push({ name, verdict, detail: detail.trim().slice(0, 90) });
  }
  console.log(`${verdict.padEnd(14)} ${name}${detail ? `  -- ${detail.trim().slice(0, 80)}` : ''}`);
}

console.log(`\n${files.length} golden fixtures, ${rejected.length} the incumbent will not parse.`);
for (const row of rejected) {
  console.log(`  ${row.name}: ${row.detail}`);
}
