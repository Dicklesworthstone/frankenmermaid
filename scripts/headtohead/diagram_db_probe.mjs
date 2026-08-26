// Dump what the PINNED incumbent actually built from a diagram — its own diagram db — for any
// family, so a divergence hunt compares semantics instead of guessing from syntax.
//
// `parse_probe.mjs` answers PARSED / SYNTAX ERROR / RUNTIME ERROR. That is the right tool for "is
// this real mermaid?", and the wrong one for "does it MEAN the same thing to both engines?" — a
// diagram can parse in both and still produce different states, different relations, or a dropped
// note. This dumps the db so the second question has an oracle too.
//
//   npm install --prefix scripts        # jsdom, already a declared devDependency there
//   node scripts/headtohead/diagram_db_probe.mjs --file some.mmd
//   node scripts/headtohead/diagram_db_probe.mjs 'stateDiagram-v2\n[*] --> A'
//
// ⚠️ NEEDS A DOM. mermaid's state/class/sequence db paths run text through `sanitizeText` into
// DOMPurify, which without a real `document` bails to a stub with no `addHook`, and the parse dies
// with `Ro.addHook is not a function`. mermaid_bench.mjs deliberately refuses to stub DOMPurify
// because for TIMING that would flatter us; there is no timing here, so jsdom is the honest way to
// give the incumbent the environment it asks for.
//
// The bundle must be loaded through a <script> element with `runScripts: 'dangerously'`.
// `window.eval` does NOT work — the bundle's namespace global never appears.
import fs from 'node:fs';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';

const argv = process.argv.slice(2);
if (argv.length === 0) {
  console.error("usage: diagram_db_probe.mjs <diagram text with \\n escapes> | --file <path>");
  process.exit(2);
}
const text = argv[0] === '--file' ? fs.readFileSync(argv[1], 'utf8') : argv.join(' ').replace(/\\n/g, '\n');

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', {
  runScripts: 'dangerously',
});
const w = dom.window;
// jsdom's window lacks a few Node platform globals the bundle reaches for (gitGraph's id hashing
// wants TextEncoder). Lend it ours rather than let a family fail as a spurious RUNTIME ERROR.
for (const name of ['TextEncoder', 'TextDecoder', 'crypto']) {
  if (!w[name] && globalThis[name]) w[name] = globalThis[name];
}
const script = w.document.createElement('script');
script.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(script);
const mermaid = w.mermaid;
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

/** Maps, Sets and class instances all have to survive JSON.stringify or the dump lies by omission. */
function plain(value, depth = 0) {
  if (depth > 6) return '<deep>';
  if (value === null || typeof value !== 'object') {
    return typeof value === 'function' ? undefined : value;
  }
  if (value instanceof w.Map || value instanceof Map) {
    return Object.fromEntries([...value.entries()].map(([k, v]) => [String(k), plain(v, depth + 1)]));
  }
  if (value instanceof w.Set || value instanceof Set) return [...value].map((v) => plain(v, depth + 1));
  if (Array.isArray(value)) return value.map((v) => plain(v, depth + 1));
  const out = {};
  for (const key of Object.keys(value)) {
    const next = plain(value[key], depth + 1);
    if (next !== undefined) out[key] = next;
  }
  return out;
}

try {
  await mermaid.parse(text);
} catch (error) {
  const message = String(error?.message ?? error).split('\n').slice(0, 4).join('\n');
  const syntax = /Parse error|Expecting |Lexical error|Unrecognized text|No diagram type detected/i.test(message);
  console.log(JSON.stringify({ verdict: syntax ? 'SYNTAX ERROR' : 'RUNTIME ERROR', message }, null, 2));
  process.exit(1);
}

const diagram = await mermaid.mermaidAPI.getDiagramFromText(text);
const db = diagram.db ?? diagram.getDB();
// Ask every zero-argument getter the db exposes. Enumerating accessors rather than naming the ones
// we expect is the point: a getter we did not think to call is exactly where a dropped construct
// hides.
const dump = {};
const seen = new Set();
for (let proto = db; proto && proto !== Object.prototype; proto = Object.getPrototypeOf(proto)) {
  for (const key of Object.getOwnPropertyNames(proto)) {
    if (seen.has(key) || !/^get[A-Z]/.test(key)) continue;
    seen.add(key);
    const fn = db[key];
    if (typeof fn !== 'function' || fn.length !== 0) continue;
    try {
      dump[key] = plain(fn.call(db));
    } catch (error) {
      dump[key] = `<threw: ${String(error?.message ?? error).split('\n')[0]}>`;
    }
  }
}
for (const key of Object.keys(db)) {
  if (seen.has(key) || typeof db[key] === 'function') continue;
  dump[`field:${key}`] = plain(db[key]);
}
console.log(JSON.stringify({ verdict: 'PARSED', db: dump }, null, 2));
