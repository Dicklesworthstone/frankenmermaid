// Working mermaid.js head-to-head render harness (cc, 2026-06-29).
//
// The swarm's comparator was recorded as BLOCKED (legacy_mermaid_code/ corpus absent) and the
// `mmdc` CLI is broken in mermaid 11.15.0 (its bundled dist/index.html is an 81-byte stub →
// net::ERR_FILE_NOT_FOUND). This bypasses the CLI and drives mermaid core directly in headless
// chromium, rendering N times to amortize browser startup so the reported ms is RENDER-ONLY
// (the same fair method the old live-CDP comparator used).
//
// Setup (in a scratch dir, NOT the repo — keeps node_modules out of git):
//   mkdir cmp && cd cmp && echo '{"name":"cmp","private":true}' > package.json
//   PUPPETEER_SKIP_DOWNLOAD=true npm install @mermaid-js/mermaid-cli   # pulls mermaid core + puppeteer
//   cp <repo>/scripts/mermaid_headtohead_cc.mjs ./render.mjs
//   node render.mjs <input.mmd> <out.svg>
// Requires a system chromium at /usr/bin/chromium-browser (or edit executablePath).
//
// Measured (wide 16x32 flowchart, 512 nodes / 960 edges, this box):
//   mermaid 11.15.0:  render median 3453.9 ms,  output 1,198,399 bytes
//   frankenmermaid:   full pipeline 4.555 ms,    output 535,831 B (a11y) / 372,075 B (lean)
//   => 758x faster, 2.24x (a11y) / 3.22x (lean) smaller output.
import puppeteer from 'puppeteer';
import { readFileSync, writeFileSync } from 'node:fs';

const mmdPath = process.argv[2];
const outPath = process.argv[3];
const text = readFileSync(mmdPath, 'utf8');
const mermaidJs = readFileSync('./node_modules/mermaid/dist/mermaid.min.js', 'utf8');

const browser = await puppeteer.launch({
  executablePath: '/usr/bin/chromium-browser',
  headless: 'new',
  args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'],
});
const page = await browser.newPage();
await page.setContent('<!DOCTYPE html><html><body><div id="c"></div></body></html>');
await page.addScriptTag({ content: mermaidJs });
await page.evaluate(() => {
  window.mermaid.initialize({ startOnLoad: false, maxEdges: 100000, securityLevel: 'loose' });
});

const result = await page.evaluate(async (mmd) => {
  const m = window.mermaid;
  await m.render('w0', mmd); // warmup
  const N = 5;
  const times = [];
  let svg = '';
  for (let i = 0; i < N; i++) {
    const t0 = performance.now();
    const r = await m.render('g' + i, mmd);
    times.push(performance.now() - t0);
    svg = r.svg;
  }
  times.sort((a, b) => a - b);
  return { medianMs: times[Math.floor(N / 2)], times, bytes: svg.length, svg };
}, text);

writeFileSync(outPath, result.svg);
console.log(JSON.stringify({ medianMs: result.medianMs, times: result.times.map((x) => Math.round(x)), bytes: result.bytes }));
await browser.close();
