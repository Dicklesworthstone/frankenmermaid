// Diff the ER crow's-foot CARDINALITY MARKERS both engines draw on a relationship line.
//
//   node scripts/headtohead/er_marker_diff.mjs
//
// WHY THIS EXISTS. mermaid encodes ER cardinality as marker SHAPES, not as text (bd-dun16). Every
// text oracle in this directory is therefore blind to it twice over: it cannot see a missing crow's
// foot, and it cannot see a WRONG one. A diagram that draws "exactly one" where the source said
// "zero or more" is a false statement about the data model, and no `<text>` diff will ever report it.
//
// ⚠️ IDS CANNOT BE COMPARED AND NEITHER CAN `url(#…)`. mermaid namespaces every marker id with the
// render id — `#m_probe0_er-zeroOrMoreStart` — so the two engines never agree on a string here even
// when they draw the identical shape. What is comparable is the GEOMETRY the reference resolves to,
// so each side's `marker-start`/`marker-end` is dereferenced to its `<marker>` and reduced to its
// child shapes. That also makes the check immune to our emitting only the used defs where mermaid
// declares all eight.
//
// ⚠️ AND THE SHAPE MUST BE READ PER END, NOT PER DIAGRAM. `A ||--o{ B` draws two DIFFERENT shapes,
// one at each end. Collecting "the set of markers this diagram uses" would pass on an implementation
// that swapped them — which is the single most likely way to get this wrong, since the start and end
// forms of one cardinality differ only in where the bar sits.
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
const PROFILE_ROOT = path.join(os.homedir(), 'snap', 'chromium', 'common');

/// All four cardinalities appear on BOTH sides across the battery. A form that only ever appeared as
/// a marker-end would leave its start variant — a genuinely different glyph — unmeasured.
const BATTERY = {
  'exactlyOne | exactlyOne': 'erDiagram\n  A ||--|| B : r\n',
  'exactlyOne | zeroOrMore': 'erDiagram\n  A ||--o{ B : r\n',
  'zeroOrOne  | oneOrMore ': 'erDiagram\n  A |o--|{ B : r\n',
  'zeroOrMore | zeroOrOne ': 'erDiagram\n  A }o--o| B : r\n',
  'oneOrMore  | exactlyOne': 'erDiagram\n  A }|--|| B : r\n',
  'labelled relationship  ': 'erDiagram\n  A ||--o{ B : places\n',
};

async function launchChromium() {
  const profile = fs.mkdtempSync(path.join(PROFILE_ROOT, 'fm-ermark-'));
  const proc = spawn(CHROMIUM, [
    '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
    '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', '--no-first-run',
    '--no-default-browser-check', '--disable-extensions', '--disable-background-networking',
    '--disable-sync', '--mute-audio', 'about:blank',
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

/// Dereference the relationship line's markers to the shapes they draw.
///
/// The line is found by "the path that carries a cardinality marker" rather than by class, because
/// the two engines class it differently (`relationshipLine` vs `fm-edge`) and hardcoding either
/// would make this an oracle for one engine's markup instead of for the drawing.
function markerProbe(svg) {
  return `(() => {
    const host = document.createElement('div');
    host.style.position = 'absolute';
    host.style.left = '-10000px';
    document.body.appendChild(host);
    try {
      host.innerHTML = ${JSON.stringify(svg)};
      const norm = (id) => {
        if (!id) return null;
        const ref = /url\\(#(.*)\\)/.exec(id);
        if (!ref) return null;
        const marker = host.querySelector('[id="' + ref[1] + '"]');
        if (!marker) return 'DANGLING:' + ref[1];
        return [...marker.children].map((child) => {
          if (child.tagName === 'path') return 'path:' + child.getAttribute('d').replace(/\\s+/g, ' ').trim();
          if (child.tagName === 'circle') return 'circle:' + child.getAttribute('cx') + ',' + child.getAttribute('cy') + ',r' + child.getAttribute('r');
          return child.tagName;
        }).join(' + ');
      };
      const lines = [...host.querySelectorAll('path')]
        .filter((p) => p.getAttribute('marker-start') || p.getAttribute('marker-end'));
      return JSON.stringify({
        ok: true,
        count: lines.length,
        ends: lines.map((p) => ({
          start: norm(p.getAttribute('marker-start')),
          end: norm(p.getAttribute('marker-end')),
        })),
      });
    } catch (e) {
      return JSON.stringify({ ok: false, error: String((e && e.message) || e) });
    } finally {
      host.remove();
    }
  })()`;
}

const { proc, info, page } = await launchChromium();
const { ws, send, ready } = attach(page);
await ready;
await send('Runtime.enable', {});
await send('Runtime.evaluate', { expression: fs.readFileSync(BUNDLE_PATH, 'utf8') });

const evaluate = async (expression) => {
  const res = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  const value = res.result?.result?.value;
  if (value === undefined) return { ok: false, error: JSON.stringify(res).slice(0, 300) };
  return JSON.parse(value);
};

let agree = 0;
let diverge = 0;
let index = 0;

for (const [name, source] of Object.entries(BATTERY)) {
  index += 1;
  const rendered = await evaluate(`(async () => {
    try {
      mermaid.initialize({ startOnLoad: false });
      const { svg } = await mermaid.render('probe${index}', ${JSON.stringify(source)});
      return JSON.stringify({ ok: true, svg });
    } catch (e) {
      return JSON.stringify({ ok: false, error: String((e && e.message) || e) });
    }
  })()`);
  if (!rendered.ok) {
    console.log(`INCUMBENT-DNF  ${name}  ${rendered.error}`);
    diverge += 1;
    continue;
  }

  const ourSvg = execFileSync(FM_CLI, ['render', '-f', 'svg', '-'], {
    input: source, encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'],
  });

  const theirs = await evaluate(markerProbe(rendered.svg));
  const ours = await evaluate(markerProbe(ourSvg));

  const same = JSON.stringify(theirs.ends) === JSON.stringify(ours.ends);
  console.log(`${same ? 'AGREE  ' : 'DIVERGE'}  ${name}`);
  if (!same) {
    console.log(`    incumbent: ${JSON.stringify(theirs.ends)}`);
    console.log(`    ours     : ${JSON.stringify(ours.ends)}`);
  }
  if (same) agree += 1; else diverge += 1;
}

console.log(`\n${agree} agree, ${diverge} diverge  (chromium ${info.Browser}, bundle mermaid@11.15.0)`);
ws.close();
proc.kill('SIGKILL');
process.exit(diverge === 0 ? 0 : 1);
