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
// THREE verdicts, and the distinction is the whole point of the tool:
//
//   PARSED         the grammar accepted the input and the db callbacks ran clean.
//   SYNTAX ERROR   the grammar REJECTED it. This is the only verdict that means "not real mermaid".
//   RUNTIME ERROR  the grammar ACCEPTED it — parsing got far enough to execute — and then something
//                  threw. Two very different causes live here, so read the message:
//                    - an ENVIRONMENT gap: no DOM. `click A "url" "tip"` throws
//                      `Ro.addHook is not a function`, which is a minified DOMPurify. Says nothing
//                      about your syntax; the syntax was accepted.
//                    - REAL incumbent behaviour: `linkStyle 9` on a one-edge diagram throws
//                      `Cannot set properties of undefined (setting 'style')` because mermaid walks
//                      off the end of its edges array. That is a genuine finding.
//
// ⚠️ The classification keys on the GRAMMAR's own error text, not on a list of DOM-ish words. An
// earlier version matched /document|window|sanitize|DOMPurify/ and misfiled the `click` case as a
// hard ERROR — the bundle is minified, so DOMPurify never appears by name. Enumerating symbol names
// in minified code does not work; the jison error strings survive minification because they are
// data, not identifiers.
//
// None of the three says anything about RENDERING. A diagram can parse and still draw nothing;
// geometry and output bytes need the render harness.
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
  // jison's own rejection text. These strings are data in the bundle, so they survive minification
  // intact — unlike any identifier we might have tried to match on.
  const syntax = /Parse error|Expecting |Lexical error|Unrecognized text|No diagram type detected/i
    .test(message);
  console.log(
    syntax
      ? 'SYNTAX ERROR (grammar rejected the input)'
      : 'RUNTIME ERROR (grammar ACCEPTED the input; execution threw — read the message)'
  );
  console.log(message.split('\n').slice(0, 4).join('\n'));
  process.exit(1);
}
