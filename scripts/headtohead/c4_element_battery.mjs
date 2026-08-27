// Ask the PINNED incumbent what type string each C4 element macro actually stores, and compare it
// with what we draw.
//
//   node scripts/headtohead/c4_element_battery.mjs
//
// ⚠️ WHY A BATTERY AND NOT A GUESS. mermaid's C4 renderer draws the stored type verbatim —
// `.text("<<" + t.typeC4Shape.text + ">>")` — so the type string IS the label. The bundle's theme
// config happens to contain twenty `<type>FontSize` keys, which is enough to learn that twenty type
// strings exist and NOT enough to learn which macro produces which. Reading the config and inferring
// the mapping is exactly the shape of error that has bitten this campaign before, so this asks the
// grammar instead: parse one diagram per macro and read the db.
//
// ⚠️ NEEDS A DOM for the same reason `diagram_db_probe.mjs` does — mermaid's db paths route text
// through DOMPurify. There is no timing here, so lending it jsdom costs nothing.
//
// C4 renders under NEITHER existing oracle: it has no head-to-head corpus item (so `equivalence.mjs`
// has nothing to compare) and its renderer cannot run under jsdom (so `drawn_text_diff.mjs` reports
// INCUMBENT-DNF). The db is the only oracle this family has.
import fs from 'node:fs';
import { execFileSync } from 'node:child_process';
import { JSDOM } from 'jsdom';

const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const FM_CLI = process.env.FM_CLI ?? 'target/local/debug/fm-cli';

/** Every element macro mermaid's C4 grammar accepts, in the bundle's own declaration order. */
const MACROS = [
  'Person', 'Person_Ext',
  'System', 'System_Ext', 'SystemDb', 'SystemDb_Ext', 'SystemQueue', 'SystemQueue_Ext',
  'Container', 'Container_Ext', 'ContainerDb', 'ContainerDb_Ext', 'ContainerQueue', 'ContainerQueue_Ext',
  'Component', 'Component_Ext', 'ComponentDb', 'ComponentDb_Ext', 'ComponentQueue', 'ComponentQueue_Ext',
];

const diagram = (macro) => `C4Context\n    title T\n    ${macro}(a, "A", "d")\n`;

const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>', { runScripts: 'dangerously' });
const w = dom.window;
for (const name of ['TextEncoder', 'TextDecoder', 'crypto', 'structuredClone']) {
  if (!w[name] && globalThis[name]) w[name] = globalThis[name];
}
const script = w.document.createElement('script');
script.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(script);
const mermaid = w.mermaid;
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

/** What we draw: the `<<…>>` runs in our own SVG for the same source. */
function ours(text) {
  let svg;
  try {
    svg = execFileSync(FM_CLI, ['render', '-f', 'svg', '-'], { input: text, encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] });
  } catch {
    return '<FM-DNF>';
  }
  const runs = [...svg.matchAll(/<text[^>]*>(.*?)<\/text>/gs)]
    .map((m) => m[1].replace(/<[^>]+>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').trim())
    .filter((run) => run.startsWith('<<') && run.endsWith('>>'));
  return runs[0] ?? '<none>';
}

const rows = [];
for (const macro of MACROS) {
  const text = diagram(macro);
  let theirs;
  try {
    await mermaid.parse(text);
    const parsed = await mermaid.mermaidAPI.getDiagramFromText(text);
    const db = parsed.db ?? parsed.getDB();
    const shape = db.getC4ShapeArray?.()?.[0];
    theirs = shape ? `<<${shape.typeC4Shape?.text}>>` : '<no shape>';
  } catch (error) {
    theirs = `<INCUMBENT-DNF: ${String(error?.message ?? error).split('\n')[0]}>`;
  }
  const mine = ours(text);
  rows.push({ macro, theirs, ours: mine, agree: theirs === mine });
}

const width = Math.max(...rows.map((r) => r.macro.length));
for (const row of rows) {
  const mark = row.agree ? 'AGREE  ' : 'DIVERGE';
  console.log(`${mark} ${row.macro.padEnd(width)}  incumbent=${row.theirs.padEnd(28)} ours=${row.ours}`);
}
const diverged = rows.filter((r) => !r.agree);
console.log(`\n${rows.length - diverged.length}/${rows.length} agree`);
process.exit(diverged.length === 0 ? 0 : 1);
