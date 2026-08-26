// Diff the TEXT the two engines actually draw for one diagram.
//
// The cheapest cross-engine parity probe there is: render both, pull every `<text>` leaf, and report
// what one draws and the other does not. It answers the question the db probe cannot — the db says
// what was STORED, and only a render says what reached the reader.
//
//   node scripts/headtohead/drawn_text_diff.mjs <file.mmd> [more.mmd ...]
//
// ⚠️ NEEDS A DOM, and mermaid's own render path needs more of one than `parse` does. Whatever the
// jsdom window lacks is LENT rather than treated as a mermaid verdict: a missing platform global
// reads exactly like "mermaid rejects this input", and in this campaign that shape has impersonated
// an engine defect three times (gitGraph/TextEncoder, pie/structuredClone).
//
// A run that cannot render in the incumbent reports INCUMBENT-DNF and is NOT counted as agreement.
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const FM_CLI = 'target/local/debug/fm-cli';

const files = process.argv.slice(2);
if (files.length === 0) {
  console.error('usage: drawn_text_diff.mjs <file.mmd> [...]');
  process.exit(2);
}

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', {
  runScripts: 'dangerously',
  pretendToBeVisual: true,
});
const w = dom.window;
for (const name of ['TextEncoder', 'TextDecoder', 'crypto', 'structuredClone', 'DOMRect']) {
  if (!w[name] && globalThis[name]) w[name] = globalThis[name];
}
// ⚠️ GEOMETRY STUBS, AND THEY ARE A REAL LIMITATION OF THIS PROBE. jsdom implements no SVG layout,
// so mermaid's renderer dies on `getBBox` / `getComputedTextLength`. Stubbing them lets the render
// COMPLETE, and the text it emits is then comparable — but every position and size it computed from
// them is fiction. That is why this script compares TEXT ONLY and says nothing about geometry.
//
// A width of 0 also means mermaid never wraps a label, so a long label arrives as one run here and
// as several tspans in a real browser. `equivalence.mjs` drives Chromium precisely to avoid that,
// and remains the authority; this is the cheap first pass.
for (const proto of [w.SVGElement.prototype, w.Element.prototype]) {
  if (!proto.getBBox) proto.getBBox = () => ({ x: 0, y: 0, width: 0, height: 0 });
  if (!proto.getComputedTextLength) proto.getComputedTextLength = () => 0;
  if (!proto.getSubStringLength) proto.getSubStringLength = () => 0;
  if (!proto.getScreenCTM) proto.getScreenCTM = () => ({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 });
}

const script = w.document.createElement('script');
script.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(script);
const mermaid = w.mermaid;
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

/** Every `<text>` leaf, escapes undone, blanks dropped. */
function textRuns(svg) {
  const runs = [];
  let rest = svg;
  while (true) {
    const start = rest.indexOf('<text');
    if (start < 0) break;
    rest = rest.slice(start);
    const open = rest.indexOf('>');
    if (open < 0) break;
    const close = rest.indexOf('</text>');
    if (close < 0) break;
    const inner = rest.slice(open + 1, close);
    // ⚠️ A MULTI-LINE LABEL IS ONE `<text>` HOLDING A `<tspan>` PER LINE, so the tspan boundary IS a
    // newline. Stripping the tags without putting one back joins `A` and `10` into `A10` and
    // reports a false divergence against mermaid's `A\n10` — which it did, on every sankey node,
    // until this line existed.
    const plain = inner
      .replace(/<\/tspan>\s*<tspan[^>]*>/g, '\n')
      .replace(/<[^>]*>/g, '')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&amp;/g, '&')
      .trim();
    if (plain) runs.push(plain);
    rest = rest.slice(close + '</text>'.length);
  }
  return runs;
}

let divergent = 0;
for (const file of files) {
  const text = fs.readFileSync(file, 'utf8');
  let theirs = null;
  let note = '';
  try {
    const id = `d${Math.abs([...file].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7))}`;
    const { svg } = await mermaid.render(id, text);
    theirs = textRuns(svg);
  } catch (error) {
    note = String(error?.message ?? error).split('\n')[0].slice(0, 90);
  }

  const ours = textRuns(execFileSync(FM_CLI, ['render', '-f', 'svg', file], { encoding: 'utf8' }));

  if (theirs === null) {
    console.log(`INCUMBENT-DNF  ${file}  -- ${note}`);
    console.log(`  ours (${ours.length}): ${JSON.stringify(ours.slice(0, 12))}`);
    divergent += 1;
    continue;
  }

  const oursSet = new Set(ours);
  const theirsSet = new Set(theirs);
  let missing = theirs.filter((run) => !oursSet.has(run));
  let extra = ours.filter((run) => !theirsSet.has(run));

  // ⚠️ A WHITESPACE-ONLY DIFFERENCE IS NOT REPORTABLE FROM HERE, and saying so is the point.
  // The geometry stubs above make `getComputedTextLength()` return 0, so mermaid's word-wrap sees
  // every string as zero-width and lays text out differently than a browser would. Runs that match
  // once whitespace is collapsed are therefore INDISTINGUISHABLE, from this probe, between a real
  // engine divergence and an artefact of the stub — `timeline_basic` reports `"Event   A"` against
  // our `"Event A"` for exactly this reason, and it is not evidence either way.
  //
  // They are reported separately so nobody files them as parity bugs. `equivalence.mjs` drives
  // Chromium and is the tool that can actually decide them.
  const squash = (run) => run.replace(/\s+/g, ' ').trim();
  const extraBySquash = new Map(extra.map((run) => [squash(run), run]));
  const whitespaceOnly = [];
  for (const run of [...missing]) {
    const twin = extraBySquash.get(squash(run));
    if (twin !== undefined) {
      whitespaceOnly.push([run, twin]);
      missing = missing.filter((r) => r !== run);
      extra = extra.filter((r) => r !== twin);
      extraBySquash.delete(squash(run));
    }
  }

  if (missing.length === 0 && extra.length === 0 && whitespaceOnly.length === 0) {
    console.log(`AGREE          ${file}  (${ours.length} runs)`);
    continue;
  }
  if (missing.length === 0 && extra.length === 0) {
    console.log(`WHITESPACE     ${file}  ${JSON.stringify(whitespaceOnly.slice(0, 4))}`);
    console.log('  undecidable from this probe: the stubs change mermaid word-wrap. Use equivalence.mjs.');
    continue;
  }
  divergent += 1;
  console.log(`DIVERGE        ${file}`);
  if (whitespaceOnly.length > 0) {
    console.log(`  whitespace-only, UNDECIDABLE here: ${JSON.stringify(whitespaceOnly.slice(0, 3))}`);
  }
  if (missing.length > 0) console.log(`  mermaid draws, we do NOT: ${JSON.stringify(missing)}`);
  if (extra.length > 0) console.log(`  we draw, mermaid does NOT: ${JSON.stringify(extra)}`);
}

console.log(`\n${files.length} diagram(s), ${divergent} divergent or undecidable.`);
