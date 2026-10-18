// Actual browser/WASM rendering contract for GitHub issue #14.
// Usage: node scripts/issue14_wasm_regression.mjs [packageDir] [reportPath]
// Rebuild the WASM package first; no browser or third-party Node packages are required.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = fileURLToPath(new URL('../', import.meta.url));
const packageDir = resolve(process.argv[2] ?? join(repoRoot, 'pkg'));
const outputPath = process.argv[3] ? resolve(process.argv[3]) : undefined;
const source = await readFile(join(repoRoot, 'crates/fm-render-svg/tests/fixtures/issue14_omnichannel.mmd'), 'utf8');
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
assert.equal(sha256(source), '3258bf1dd770ee1d7c9715069f4dc463bcd64faa119646645afe2792ee177b4c');
const normalize = text => text.replace(/\s+/g, ' ').trim();
const escaped = text => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const expected = [...source.matchAll(/"([^"]+)"/g), ...source.matchAll(/\|([^|]+)\|/g)]
  .map(match => normalize(escaped(match[1].replace(/<br\s*\/?>/g, ' '))))
  .sort();
assert.equal(expected.length, 50);

const wasmBytes = await readFile(join(packageDir, 'frankenmermaid_bg.wasm'));
const wasm = await import(pathToFileURL(join(packageDir, 'frankenmermaid.js')).href);
await wasm.default({ module_or_path: wasmBytes });
const results = [];

function verify(name, svg, config, full = true) {
  assert.equal(typeof svg, 'string', `${name}: SVG response`);
  // Full labels already existed in the broken artifact's accessibility metadata.
  // Only drawn text and tspan contents count toward this independent oracle.
  const text = [...svg.matchAll(/<text\b[^>]*>([\s\S]*?)<\/text>/g)]
    .map(match => normalize(match[1].replace(/<\/?tspan\b[^>]*>/g, ' ')))
    .sort();
  const root = svg.match(/<svg\b[^>]*>/)?.[0];
  assert.ok(root, `${name}: SVG opening tag`);
  const tier = root.match(/data-detail-tier="([^"]+)"/)?.[1];
  const edgeLabels = (svg.match(/class="edge-label"/g) ?? []).length;
  const clusterLabels = (svg.match(/class="fm-cluster-label"/g) ?? []).length;
  const precisionRule = '[data-label-detail="full"] text{text-rendering:geometricPrecision}';
  assert.equal(svg.includes(precisionRule), full, `${name}: scoped export text metrics`);
  if (full) assert.ok(svg.indexOf(precisionRule) > svg.lastIndexOf('text-rendering:optimizeLegibility'), `${name}: precision wins cascade`);
  assert.match(root, /data-nodes="24"/, name);
  assert.match(root, /data-edges="43"/, name);
  if (full) {
    assert.deepEqual(text, expected, `${name}: complete drawn label multiset`);
    assert.match(root, /data-label-detail="full"/, name);
    assert.equal(edgeLabels, 24, name);
    assert.equal(clusterLabels, 2, name);
    const sms = svg.split('data-id="S"')[1]?.split('</g>')[0];
    assert.ok(sms, `${name}: SMS node`);
    assert.equal((sms.match(/<tspan /g) ?? []).length, 3, `${name}: authored line breaks`);
  } else {
    assert.equal(tier, 'compact', name);
    assert.equal(text.length, 24, name);
    assert.equal(edgeLabels, 0, name);
    assert.equal(clusterLabels, 0, name);
    assert.doesNotMatch(root, /data-label-detail="full"/, name);
  }
  if (config?.pressure?.workerSaturationPermille === 1000) {
    assert.doesNotMatch(svg, /<filter id="drop-shadow"/, `${name}: pressure still reduces decoration`);
  }
  if ((config?.svg?.detailTier ?? config?.detailTier) === 'compact') {
    assert.equal(tier, 'compact', `${name}: explicit compact presentation`);
  }
  if (config?.pressure?.frameTimeMs === 0 && config?.detailTier) {
    assert.equal(tier, config.detailTier, `${name}: requested tier with nominal telemetry`);
  }
  const result = { name, tier, full, textCount: text.length, edgeLabels, clusterLabels, sha256: sha256(svg) };
  results.push(result);
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

const critical = { frameBudgetMs: 16, frameTimeMs: 64, workerSaturationPermille: 1000 };
const nominal = { frameBudgetMs: 16, frameTimeMs: 0, eventLoopLagMs: 0, workerSaturationPermille: 0 };
const cases = [
  ['default', undefined],
  ['auto', { detailTier: 'auto' }],
  ['compact', { detailTier: 'compact' }],
  ['normal', { detailTier: 'normal' }],
  ['rich', { detailTier: 'rich' }],
  ['critical', { pressure: critical }],
  ['critical-rich-request', { pressure: critical, detailTier: 'rich' }],
  ['nested-preservation-wins', { preserveLabels: false, detailTier: 'rich', svg: { preserveLabels: true, detailTier: 'compact' } }],
  ['explicit-overview', { detailTier: 'compact', preserveLabels: false }, false],
  ['nominal-compact', { pressure: nominal, detailTier: 'compact' }],
  ['nominal-normal', { pressure: nominal, detailTier: 'normal' }],
  ['nominal-rich', { pressure: nominal, detailTier: 'rich' }],
];

for (const path of ['renderSvg', 'renderDeck']) {
  for (const [name, config, full = true] of cases) {
    const output = wasm[path](source, config);
    verify(`${path}/${name}`, path === 'renderSvg' ? output : output.svg, config, full);
  }
}

let requestId = 1400;
function workerSvg(config) {
  const response = JSON.parse(wasm.workerHandleMessage(JSON.stringify({
    kind: 'render', requestId: ++requestId, input: source,
    ...(config === undefined ? {} : { configJson: JSON.stringify(config) }),
  })));
  assert.equal(response.kind, 'completed', JSON.stringify(response));
  assert.equal(response.node_count, 24);
  assert.equal(response.edge_count, 43);
  return response.svg;
}

for (const [name, config, full = true] of cases) {
  verify(`worker/${name}`, workerSvg(config), config, full);
}
wasm.init({ preserveLabels: false, detailTier: 'rich', svg: { preserveLabels: true, detailTier: 'compact' } });
verify('init/nested-preservation-wins', workerSvg(), { detailTier: 'compact' });
wasm.init({ detailTier: 'compact', preserveLabels: false });
verify('init/explicit-overview', workerSvg(), { detailTier: 'compact' }, false);
wasm.init({ detailTier: 'auto', preserveLabels: true });
assert.equal(results.length, 38);
const report = { source_sha256: sha256(source), wasm_sha256: sha256(wasmBytes), checks: results.length, results };
if (outputPath) await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
process.stdout.write(`PASS: ${results.length} actual WASM API checks${outputPath ? `; evidence ${outputPath}` : ''}\n`);
