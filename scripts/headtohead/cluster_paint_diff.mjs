// Diff the COMPUTED PAINT of flowchart cluster rects across both engines, in a real browser.
//
//   node scripts/headtohead/cluster_paint_diff.mjs            # the built-in style battery
//   node scripts/headtohead/cluster_paint_diff.mjs file.mmd [...]
//
// WHY THIS EXISTS. Every other oracle here diffs TEXT. `drawn_text_diff.mjs`, `equivalence.mjs` and
// `chromium_text_diff.mjs` all answer "does the same writing appear", and all three are structurally
// blind to colour: a subgraph whose declared `fill:#ff0000` is silently discarded draws exactly the
// same words as one that honours it. bd-xfmm was open for two days as "no cluster colour reaches
// either renderer" precisely because nothing measured paint.
//
// ⚠️ IT MUST BE COMPUTED PAINT, NOT THE ATTRIBUTE. The two engines declare the same colour by
// different mechanisms and a string diff of the markup reports a divergence that does not exist:
//
//     mermaid : <rect style="fill:#ff0000 !important">          (no fill attribute at all)
//     ours    : <rect fill="rgba(248,249,250,0.85)" style="fill:#ff0000">
//
// Ours carries a theme `fill` PRESENTATION ATTRIBUTE underneath the declaration. In CSS a `style`
// property beats a presentation attribute, so both paint red — but only `getComputedStyle` says so.
// Reading `getAttribute('fill')` would report ours as grey and mermaid as null, i.e. two wrong
// answers and a fake divergence. mermaid's `!important` is likewise not a difference to port: it
// exists to outrank mermaid's own stylesheet, which we do not emit.
//
// ⚠️ AND CONNECT TO A **PAGE** TARGET, and inject the bundle over CDP rather than loading it from
// disk — the pinned Chromium is snap-confined. Both traps are documented at length in
// `chromium_text_diff.mjs`; this script follows it exactly.
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

/// The battery. Each case pairs a styled diagram with the control that isolates what the style did:
/// asserting only "the cluster is red" cannot tell a working style channel from a theme that happens
/// to be red, which is the vacuity bd-xfmm's own filing warned about.
const BATTERY = {
  'style/fill': 'flowchart TD\n  subgraph one[One]\n    a[A] --> b[B]\n  end\n  style one fill:#ff0000\n',
  'style/none (control)': 'flowchart TD\n  subgraph one[One]\n    a[A] --> b[B]\n  end\n',
  'style/fill+stroke': 'flowchart TD\n  subgraph one[One]\n    a[A] --> b[B]\n  end\n  style one fill:#ff0000,stroke:#00ff00,stroke-width:4px\n',
  'classDef+class': 'flowchart TD\n  subgraph one[One]\n    a[A] --> b[B]\n  end\n  classDef hot fill:#ff0000\n  class one hot\n',
  'style/nested inner': 'flowchart TD\n  subgraph outer[Outer]\n    subgraph inner[Inner]\n      a[A] --> b[B]\n    end\n  end\n  style inner fill:#ff0000\n',
};

const argv = process.argv.slice(2).filter((a) => !a.startsWith('--'));
const cases = argv.length
  ? Object.fromEntries(argv.map((f) => [path.basename(f), fs.readFileSync(f, 'utf8')]))
  : BATTERY;

async function launchChromium() {
  const profile = fs.mkdtempSync(path.join(PROFILE_ROOT, 'fm-paintdiff-'));
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

/// Read the computed paint of every cluster rect in an already-rendered SVG string.
///
/// The host is attached to `document.body`: `getComputedStyle` on a detached subtree returns empty
/// strings for everything, which would report both engines as identical-and-blank — a passing diff
/// that measured nothing.
function paintProbe(svg, selector) {
  return `(() => {
    const host = document.createElement('div');
    host.style.position = 'absolute';
    host.style.left = '-10000px';
    document.body.appendChild(host);
    try {
      host.innerHTML = ${JSON.stringify(svg)};
      const out = [];
      for (const el of host.querySelectorAll(${JSON.stringify(selector)})) {
        const cs = getComputedStyle(el);
        out.push({ fill: cs.fill, stroke: cs.stroke, strokeWidth: cs.strokeWidth });
      }
      return JSON.stringify({ ok: true, rects: out });
    } catch (e) {
      return JSON.stringify({ ok: false, error: String((e && e.message) || e) });
    } finally {
      host.remove();
    }
  })()`;
}

/// mermaid's cluster rect is the unclassed `rect` directly inside `g.cluster`; ours is the rect
/// classed `fm-cluster`. Selecting "every rect" would fold the NODE rects into the comparison and
/// report agreement whenever the node theme agrees, whatever the cluster did.
const MERMAID_CLUSTER_RECT = 'g.cluster > rect';
const OUR_CLUSTER_RECT = 'rect.fm-cluster';

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

/// Each engine's UNSTYLED cluster fill, measured rather than hardcoded — a literal in this file
/// would go stale the moment either theme moved and would then silently reclassify every case.
const UNSTYLED = 'flowchart TD\n  subgraph one[One]\n    a[A] --> b[B]\n  end\n';

async function measureDefaults() {
  const rendered = await evaluate(`(async () => {
    mermaid.initialize({ startOnLoad: false });
    const { svg } = await mermaid.render('paint_default', ${JSON.stringify(UNSTYLED)});
    return JSON.stringify({ ok: true, svg });
  })()`);
  const theirs = await evaluate(paintProbe(rendered.svg, MERMAID_CLUSTER_RECT));
  const ourSvg = execFileSync(FM_CLI, ['render', '-f', 'svg', '-'], {
    input: UNSTYLED, encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'],
  });
  const ours = await evaluate(paintProbe(ourSvg, OUR_CLUSTER_RECT));
  if (!theirs.rects?.length || !ours.rects?.length) {
    throw new Error('could not measure an unstyled cluster fill in one of the engines');
  }
  return { theirs: theirs.rects[0].fill, ours: ours.rects[0].fill };
}

const defaults = await measureDefaults();
console.log(`unstyled cluster fill — incumbent ${defaults.theirs} | ours ${defaults.ours}`);
if (defaults.theirs === defaults.ours) {
  console.log('⚠️  the two themes agree on the default, so this run cannot distinguish '
    + '"the declared colour propagated" from "nothing happened".');
}
console.log('');

let agree = 0;
let diverge = 0;
let dnf = 0;

for (const [name, source] of Object.entries(cases)) {
  const rendered = await evaluate(`(async () => {
    try {
      mermaid.initialize({ startOnLoad: false });
      const { svg } = await mermaid.render(${JSON.stringify(`paint_${agree}_${diverge}_${dnf}`)}, ${JSON.stringify(source)});
      return JSON.stringify({ ok: true, svg });
    } catch (e) {
      return JSON.stringify({ ok: false, error: String((e && e.message) || e) });
    }
  })()`);

  if (!rendered.ok) {
    console.log(`INCUMBENT-DNF  ${name.padEnd(24)} ${rendered.error}`);
    dnf += 1;
    continue;
  }

  let ourSvg;
  try {
    ourSvg = execFileSync(FM_CLI, ['render', '-f', 'svg', '-'], {
      input: source, encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'],
    });
  } catch (error) {
    console.log(`OURS-DNF       ${name.padEnd(24)} ${String(error.message).split('\n')[0]}`);
    dnf += 1;
    continue;
  }

  const theirs = await evaluate(paintProbe(rendered.svg, MERMAID_CLUSTER_RECT));
  const ours = await evaluate(paintProbe(ourSvg, OUR_CLUSTER_RECT));

  if (!theirs.ok || !ours.ok) {
    console.log(`PROBE-DNF      ${name.padEnd(24)} ${theirs.error ?? ours.error}`);
    dnf += 1;
    continue;
  }

  // ⚠️ THE PROPERTY JUDGED IS "THE DECLARED COLOUR PROPAGATES", NOT "THE TWO ENGINES PAINT THE SAME".
  // The two themes disagree about an UNSTYLED cluster by design — mermaid's default is cream
  // `rgb(255, 255, 222)`, ours is slate `rgba(241, 245, 249, 0.65)` — so a raw fill equality check
  // reports DIVERGE on every diagram that contains one unstyled cluster, which is most of them. That
  // is not a finding, it is the comparator measuring the theme; and a check that is red on every run
  // cannot report the regression it exists to catch.
  //
  // So each rect is judged against the engine's OWN default, learned from the control case:
  //   - incumbent painted its default  -> that cluster declared nothing; ours must be OUR default.
  //   - incumbent painted anything else -> the author declared it; ours must match EXACTLY.
  const verdicts = theirs.rects.map((their, index) => {
    const our = ours.rects[index];
    if (!our) return { ok: false, why: 'we draw no cluster rect here' };
    if (their.fill === defaults.theirs) {
      return our.fill === defaults.ours
        ? { ok: true, why: 'unstyled in both' }
        : { ok: false, why: `incumbent left it default but we painted ${our.fill}` };
    }
    return our.fill === their.fill
      ? { ok: true, why: `declared ${their.fill} propagated` }
      : { ok: false, why: `declared ${their.fill}, we painted ${our.fill}` };
  });
  const same = theirs.rects.length === ours.rects.length && verdicts.every((v) => v.ok);

  console.log(`${same ? 'AGREE  ' : 'DIVERGE'}        ${name.padEnd(24)} ${verdicts.map((v) => v.why).join('; ')}`);
  if (!same) {
    console.log(`    incumbent: ${JSON.stringify(theirs.rects)}`);
    console.log(`    ours     : ${JSON.stringify(ours.rects)}`);
  }
  if (same) agree += 1; else diverge += 1;
}

console.log(`\n${agree} agree, ${diverge} diverge, ${dnf} DNF  (chromium ${info.Browser}, bundle mermaid@11.15.0)`);
ws.close();
proc.kill('SIGKILL');
process.exit(diverge === 0 ? 0 : 1);
