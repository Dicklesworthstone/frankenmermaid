// Ask the PINNED incumbent whether it accepts a given diagram — no DOM, no browser, no network.
//
// Every capability bead starts with the same question: is this syntax real mermaid, or did we
// invent it? Reading the minified bundle answers that slowly and sometimes wrongly (its jison
// tables are symbol ids, not readable productions). Asking the parser answers it in seconds.
//
// This exists because a claim about the incumbent got upgraded from inferred to measured by it:
// `linkStyle 0,1 stroke:#f00` PARSES, which proved our single-`parse::<usize>()` was dropping valid
// input. It also caught mermaid's dead bounds guard — `linkStyle 9` on a one-edge diagram throws
// `Cannot set properties of undefined` rather than its own "out of bounds" message, because the
// guard tests `typeof i == "number"` and the grammar hands it a string.
//
//   node scripts/headtohead/parse_probe.mjs 'flowchart LR\n A --> B\n linkStyle 0,1 stroke:#f00'
//   node scripts/headtohead/parse_probe.mjs --file some.mmd
//
// PARSED / ERROR is the whole verdict. It says the grammar accepts the input, NOT that the input
// renders the way you expect — a diagram can parse and still draw nothing. Anything about geometry
// or output bytes needs the render harness, not this.
//
// Diagrams whose parse path touches the DOM (DOMPurify and friends) report ERROR here for reasons
// that have nothing to do with your syntax. Treat an ERROR mentioning `document`, `window` or
// `sanitize` as DID-NOT-FINISH, not as a verdict.
import fs from 'node:fs';
import vm from 'node:vm';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';

const argv = process.argv.slice(2);
if (argv.length === 0) {
  console.error('usage: parse_probe.mjs <diagram text with \\n escapes> | --file <path>');
  process.exit(2);
}
const text =
  argv[0] === '--file'
    ? fs.readFileSync(argv[1], 'utf8')
    : argv.join(' ').replace(/\\n/g, '\n');

// The bundle is an IIFE that hangs itself off a namespace global, so a bare vm context with the
// handful of globals esbuild's runtime touches is enough. No jsdom, nothing installed.
const ctx = { console, setTimeout, clearTimeout, queueMicrotask, TextEncoder, TextDecoder, URL };
ctx.globalThis = ctx;
ctx.self = ctx;
ctx.window = ctx;
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(BUNDLE, 'utf8'), ctx);
const mermaid = ctx.__esbuild_esm_mermaid_nm.mermaid.default ?? ctx.__esbuild_esm_mermaid_nm.mermaid;

try {
  await mermaid.parse(text);
  console.log('PARSED');
} catch (err) {
  const message = String(err?.message ?? err);
  const domish = /document|window|sanitize|DOMPurify|getComputedStyle/i.test(message);
  console.log(domish ? 'DNF (DOM-dependent parse path, not a syntax verdict)' : 'ERROR');
  console.log(message.split('\n').slice(0, 4).join('\n'));
  process.exit(1);
}
