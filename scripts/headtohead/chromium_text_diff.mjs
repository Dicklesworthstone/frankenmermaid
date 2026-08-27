// Diff the DRAWN TEXT of both engines using a real browser, for the families no other oracle reaches.
//
//   node scripts/headtohead/chromium_text_diff.mjs file.mmd [more.mmd ...]
//   node scripts/headtohead/chromium_text_diff.mjs --all-goldens
//
// WHY THIS EXISTS, in one line: `drawn_text_diff.mjs` answers the same question far more cheaply and
// reports INCUMBENT-DNF for thirteen diagram families, and `equivalence.mjs` covers most of those
// but only for diagrams in its corpus. The intersection — a family that will not render under jsdom
// AND has no corpus item — had NO oracle at all. That intersection is where every C4 and requirement
// divergence of the last two days was hiding.
//
// Use the cheap oracle first. This one costs a browser launch per invocation; it is the escalation
// for "no other instrument can see this", and for settling a finding that overturns a recorded
// conclusion.
//
// ⚠️ THE BUNDLE IS INJECTED OVER CDP, NOT LOADED FROM DISK. The pinned Chromium is snap-confined and
// cannot read the corpus from a scratch directory (`pins.json` says so, and a `--dump-dom` on a
// file:// URL silently produces nothing). Injecting is also what `mermaid_bench.mjs` does.
//
// ⚠️ AND CONNECT TO A **PAGE** TARGET. `/json/version` hands back the BROWSER endpoint, which has no
// `Runtime` domain: `Runtime.evaluate` there answers
// `{"code":-32601,"message":"'Runtime.evaluate' wasn't found"}`, which reads like a Chromium version
// problem and is not one. The page target comes from `/json/list`.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..');
const BUNDLE_PATH = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const PINS = JSON.parse(fs.readFileSync(path.join(HERE, 'pins.json'), 'utf8'));
const CHROMIUM = process.env.FM_CHROMIUM_BIN ?? PINS.chromium.binary;
const FM_CLI = process.env.FM_CLI ?? path.join(REPO, 'target/local/debug/fm-cli');
const GOLDEN_DIR = path.join(REPO, 'crates/fm-cli/tests/golden');

// Snap confinement denies hidden dirs under $HOME, so the profile goes where pins.json says the
// harness puts it.
const PROFILE_ROOT = path.join(os.homedir(), 'snap', 'chromium', 'common');

const argv = process.argv.slice(2);
const files = argv.includes('--all-goldens')
  ? fs.readdirSync(GOLDEN_DIR).filter((f) => f.endsWith('.mmd')).sort().map((f) => path.join(GOLDEN_DIR, f))
  : argv.filter((a) => !a.startsWith('--'));

if (files.length === 0) {
  console.error('usage: chromium_text_diff.mjs <file.mmd> [...] | --all-goldens');
  process.exit(2);
}

async function launchChromium() {
  const profile = fs.mkdtempSync(path.join(PROFILE_ROOT, 'fm-textdiff-'));
  const proc = spawn(CHROMIUM, [
    '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
    '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
    '--no-first-run', '--no-default-browser-check', '--disable-extensions',
    '--disable-background-networking', '--disable-sync', '--mute-audio', 'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });

  let stderr = '';
  let port = null;
  proc.stderr.on('data', (chunk) => {
    stderr += String(chunk);
    const m = stderr.match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)/);
    if (m) port = Number(m[1]);
  });

  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (port !== null) {
      try {
        const res = await fetch(`http://127.0.0.1:${port}/json/version`);
        if (res.ok) {
          const info = await res.json();
          const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
          const page = list.find((t) => t.type === 'page');
          if (page) return { proc, info, page };
        }
      } catch { /* not up yet */ }
    }
    await new Promise((r) => setTimeout(r, 120));
  }
  proc.kill('SIGKILL');
  throw new Error(`chromium never exposed a devtools port; stderr tail: ${stderr.slice(-400)}`);
}

function attach(page) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  const pending = new Map();
  let id = 0;
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
  };
  const send = (method, params) => new Promise((resolve) => {
    const n = ++id;
    pending.set(n, resolve);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
  const ready = new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  return { ws, send, ready };
}

/** Text runs our own renderer draws, in document order. */
function ourRuns(source) {
  let svg;
  try {
    svg = execFileSync(FM_CLI, ['render', '-f', 'svg', '-'], {
      input: source, encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'],
    });
  } catch (error) {
    return { dnf: `fm-cli failed: ${String(error.message).split('\n')[0]}` };
  }
  const runs = [...svg.matchAll(/<text[^>]*>(.*?)<\/text>/gs)]
    .map((m) => m[1]
      // ⚠️ A SEPARATOR BETWEEN TSPANS, NOT A BARE STRIP. Deleting the tags concatenates the lines of
      // a wrapped label, so `Stores user` + `registration` becomes `Stores userregistration` — a
      // string neither engine ever drew, differing from the other side in a way that looks like a
      // lost space rather than a line break. `drawn_text_diff.mjs` hit exactly this on sankey and
      // the rule is inherited: split first, join with a newline, and let `squash` decide.
      .replace(/<\/tspan>\s*<tspan[^>]*>/g, '\n')
      .replace(/<[^>]+>/g, '')
      .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&').replace(/&quot;/g, '"')
      .trim())
    .filter(Boolean);
  return { runs };
}

/**
 * Compare as MULTISETS, not sequences.
 *
 * ⚠️ DOCUMENT ORDER IS A LEGITIMATE BACKEND DIFFERENCE and comparing sequences would report every
 * diagram as divergent. mermaid emits a C4 boundary's children before the boundary's own caption;
 * we emit the container first. Both draw the same runs. What is NOT legitimate is a run present on
 * one side and absent on the other, which is exactly what a multiset difference isolates.
 */
function multisetDiff(mine, theirs) {
  const count = (xs) => xs.reduce((m, x) => m.set(x, (m.get(x) ?? 0) + 1), new Map());
  const a = count(mine);
  const b = count(theirs);
  const onlyMine = [];
  const onlyTheirs = [];
  for (const [k, n] of a) {
    const extra = n - (b.get(k) ?? 0);
    for (let i = 0; i < extra; i++) onlyMine.push(k);
  }
  for (const [k, n] of b) {
    const extra = n - (a.get(k) ?? 0);
    for (let i = 0; i < extra; i++) onlyTheirs.push(k);
  }
  return { onlyMine, onlyTheirs };
}

/**
 * ⚠️ WHITESPACE-ONLY DIFFERENCES ARE UNDECIDABLE and are reported as such rather than as DIVERGE.
 *
 * The two engines wrap text with different metrics, so `"Event   A"` against `"Event A"` says
 * nothing about correctness. `drawn_text_diff.mjs` learned this the expensive way; the rule is
 * inherited here rather than rediscovered.
 */
const squash = (s) => s.replace(/\s+/g, ' ').trim();

const { proc, info, page } = await launchChromium();
const { ws, send, ready } = attach(page);
await ready;
await send('Runtime.enable');

const evaluate = async (expression) => {
  const res = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (res.error) throw new Error(JSON.stringify(res.error));
  if (res.result?.exceptionDetails) {
    throw new Error(res.result.exceptionDetails.exception?.description
      ?? JSON.stringify(res.result.exceptionDetails));
  }
  return res.result?.result?.value;
};

await evaluate(`${fs.readFileSync(BUNDLE_PATH, 'utf8')}\n;typeof mermaid`);
await evaluate(`mermaid.initialize({ startOnLoad: false, securityLevel: 'loose' }); 'ok'`);

let agree = 0;
let diverge = 0;
let undecidable = 0;
let dnf = 0;

for (const [index, file] of files.entries()) {
  const name = path.basename(file, '.mmd');
  const source = fs.readFileSync(file, 'utf8');

  let theirs;
  try {
    theirs = await evaluate(`(async () => {
      const { svg } = await mermaid.render(${JSON.stringify(`probe${index}`)}, ${JSON.stringify(source)});
      const host = document.createElement('div');
      host.innerHTML = svg;
      // ⚠️ ATTACHED TO THE DOCUMENT ON PURPOSE, so computed styles are real. mermaid emits BOTH a
      // <foreignObject> HTML label AND a fallback <text> for the same content and hides one of them
      // -- journey draws every task and section twice in the markup, exactly once on screen. A
      // detached subtree has no computed style, so a visibility filter would be inert there and the
      // probe would keep reporting the hidden twin as a run mermaid draws and we omit.
      document.body.appendChild(host);
      // ⚠️ NOT JUST <text>. With htmlLabels on — mermaid's DEFAULT for several families — labels are
      // rendered as HTML inside <foreignObject>, and a <text>-only sweep reports the diagram as
      // having drawn NOTHING. That is how this probe first accused mermaid of drawing no text at all
      // for requirement and mindmap: a probe failure impersonating a total engine failure.
      const runs = [];
      for (const el of host.querySelectorAll('text, foreignObject')) {
        // A <text> nested inside a <foreignObject> — or a <foreignObject> inside another — would
        // otherwise be counted twice.
        if (el.tagName.toLowerCase() === 'text' && el.closest('foreignObject')) continue;
        if (el.tagName.toLowerCase() === 'foreignObject' && el.parentElement?.closest('foreignObject')) continue;
        // Only what a reader can actually SEE. This is the whole reason for attaching the host
        // above: it is how the hidden half of mermaid's dual label path gets excluded.
        const style = getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') continue;
        if (el.getClientRects().length === 0) continue;
        let text;
        if (el.tagName.toLowerCase() === 'text') {
          // LEAF TSPANS ONLY. mermaid nests them (text-outer-tspan wrapping text-inner-tspan), so
          // collecting every descendant yields the label once per LEVEL and joins it to itself --
          // "API Gateway API Gateway" -- which reads as a duplicated-label defect and is this reader
          // counting the same text twice. A leaf is a real drawn line; a wrapper is not. Same shape
          // as the nested-div trap below.
          // NOTE: no backticks in this comment. It lives inside a template literal.
          const spans = [...el.querySelectorAll('tspan')].filter((t) => !t.querySelector('tspan'));
          text = (spans.length > 0 ? spans.map((t) => t.textContent).join('\\n') : el.textContent);
        } else {
          // ⚠️ DO NOT COLLECT NESTED ELEMENTS SEPARATELY. An HTML label is a div wrapping a span
          // wrapping the text, so querying 'p, div, span' returns the SAME string once per
          // ancestor — the probe reported "Central Topic Central Topic Central Topic" and read as a
          // triplicated-label defect in mermaid. Mutate a clone so block boundaries become
          // newlines, then take textContent ONCE.
          const clone = el.cloneNode(true);
          for (const br of clone.querySelectorAll('br')) br.replaceWith('\\n');
          for (const block of clone.querySelectorAll('p, div')) block.append('\\n');
          text = clone.textContent;
        }
        text = text.trim();
        if (text) runs.push(text);
      }
      host.remove();
      return runs;
    })()`);
  } catch (error) {
    console.log(`INCUMBENT-DNF  ${name.padEnd(28)} ${String(error.message).split('\n')[0].slice(0, 90)}`);
    dnf += 1;
    continue;
  }

  const mine = ourRuns(source);
  if (mine.dnf) {
    console.log(`FM-DNF         ${name.padEnd(28)} ${mine.dnf}`);
    dnf += 1;
    continue;
  }

  const exact = multisetDiff(mine.runs, theirs);
  if (exact.onlyMine.length === 0 && exact.onlyTheirs.length === 0) {
    console.log(`AGREE          ${name.padEnd(28)} ${mine.runs.length} runs`);
    agree += 1;
    continue;
  }

  const squashed = multisetDiff(mine.runs.map(squash), theirs.map(squash));
  if (squashed.onlyMine.length === 0 && squashed.onlyTheirs.length === 0) {
    console.log(`UNDECIDABLE    ${name.padEnd(28)} whitespace only, both engines wrap differently`);
    undecidable += 1;
    continue;
  }

  console.log(`DIVERGE        ${name.padEnd(28)}`);
  // ⚠️ `--dump` PRINTS BOTH SIDES IN FULL, and triage needs it more often than it looks. A
  // multiset difference reports only the SURPLUS on each side, so a run drawn once by us and twice
  // by mermaid appears under "mermaid draws, we do not" — reading exactly like a run we omit
  // entirely, when in fact we draw it and the count differs. That misread cost a filing once
  // already; the counts settle it.
  if (argv.includes('--dump')) {
    const tally = (xs) => [...xs.reduce((m, x) => m.set(x, (m.get(x) ?? 0) + 1), new Map())]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([text, n]) => (n > 1 ? `${JSON.stringify(text)}x${n}` : JSON.stringify(text)))
      .join(' ');
    console.log(`    incumbent (${theirs.length}): ${tally(theirs)}`);
    console.log(`    ours      (${mine.runs.length}): ${tally(mine.runs)}`);
  }
  if (squashed.onlyTheirs.length) console.log(`    mermaid draws, we do not: ${JSON.stringify(squashed.onlyTheirs.slice(0, 12))}`);
  if (squashed.onlyMine.length) console.log(`    we draw, mermaid does not: ${JSON.stringify(squashed.onlyMine.slice(0, 12))}`);
  diverge += 1;
}

console.log(`\n${agree} agree, ${diverge} diverge, ${undecidable} undecidable, ${dnf} DNF  (chromium ${info.Browser}, bundle mermaid@11.15.0)`);
ws.close();
proc.kill('SIGKILL');
// UNDECIDABLE and DNF are NOT failures: the first means the instrument cannot tell, the second means
// one engine refused the input. Only a real divergence sets a non-zero status.
process.exit(diverge === 0 ? 0 : 1);
