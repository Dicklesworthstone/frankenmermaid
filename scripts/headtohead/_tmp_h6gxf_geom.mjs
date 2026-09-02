// bd-h6gxf: read the PINNED INCUMBENT's viewBox for the schema_catalog_25 revisions whose layout
// algorithm the guardrail quality gate moves, so the flip is judged against the reference rather
// than against a stopwatch or a golden.
//
// Height is not the interesting axis here: both of our arms produce IDENTICAL heights on this
// family, so a height comparison would report agreement no matter which arm ran. Width is the whole
// of the difference and is what the reader sees.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BUNDLE_PATH = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const PINS = JSON.parse(fs.readFileSync(path.join(HERE, 'pins.json'), 'utf8'));
const CHROMIUM = process.env.FM_CHROMIUM_BIN ?? PINS.chromium.binary;
const PROFILE_ROOT = path.join(os.homedir(), 'snap', 'chromium', 'common');

const corpusPath = process.argv[2];
const revs = process.argv.slice(3).map(Number);
const corpus = JSON.parse(fs.readFileSync(corpusPath, 'utf8'))[0];

async function launchChromium() {
  const profile = fs.mkdtempSync(path.join(PROFILE_ROOT, 'fm-h6gxf-'));
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

console.log(`chromium ${info.Browser}, bundle mermaid@11.15.0, corpus ${corpus.id}`);
for (const rev of revs) {
  const source = corpus.texts[rev];
  const out = await evaluate(`(async () => {
    try {
      mermaid.initialize({ startOnLoad: false, maxTextSize: 5000000, maxEdges: 10000 });
      const { svg } = await mermaid.render('h6gxf${rev}', ${JSON.stringify(source)});
      const m = /viewBox="([^"]+)"/.exec(svg);
      return JSON.stringify({ ok: true, viewBox: m ? m[1] : null, bytes: svg.length });
    } catch (e) {
      return JSON.stringify({ ok: false, error: String((e && e.message) || e) });
    }
  })()`);
  if (!out.ok || !out.viewBox) {
    console.log(`rev${String(rev).padStart(2, '0')}  INCUMBENT-DNF  ${out.error ?? 'no viewBox'}`);
    continue;
  }
  const [, , w, h] = out.viewBox.split(/\s+/).map(Number);
  console.log(`rev${String(rev).padStart(2, '0')}  incumbent_w=${w.toFixed(1)}  incumbent_h=${h.toFixed(1)}  bytes=${out.bytes}`);
}
ws.close();
proc.kill('SIGKILL');
