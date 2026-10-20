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

function longEdgeDimensions(svg) {
  const label = 'Souběžně, nikoli jako podmínka hovoru';
  const groups = [...svg.matchAll(/<g\b[^>]*class="fm-edge-labeled"[^>]*>([\s\S]*?)<\/g>/g)];
  const group = groups.find(match => {
    const text = match[1].match(/<text\b[^>]*>([\s\S]*?)<\/text>/)?.[1];
    return text !== undefined && normalize(text.replace(/<[^>]*>/g, ' ')) === label;
  });
  assert.ok(group, 'the reported long edge label must be drawn');
  const width = Number(group[1].match(/<rect\b[^>]*\swidth="([^"]+)"/)?.[1]);
  const fontSize = Number(group[1].match(/<text\b[^>]*\sfont-size="([^"]+)"/)?.[1]);
  assert.ok(Number.isFinite(width) && width > 0, 'long edge backing width');
  assert.ok(Number.isFinite(fontSize) && fontSize > 0, 'long edge font size');
  return { width, fontSize };
}

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
  const result = {
    name, tier, full, textCount: text.length, edgeLabels, clusterLabels,
    ...(full ? { longEdge: longEdgeDimensions(svg) } : {}), sha256: sha256(svg),
  };
  results.push(result);
  process.stdout.write(`${JSON.stringify(result)}\n`);
  return result;
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
  ['font-11-compact', { fontSize: 11, pressure: nominal, detailTier: 'compact' }],
  ['font-22-compact', { fontSize: 22, pressure: nominal, detailTier: 'compact' }],
  ['font-30-compact', { fontSize: 30, pressure: nominal, detailTier: 'compact' }],
  ['nested-font-size-wins', { fontSize: 11, svg: { fontSize: 22 }, pressure: nominal, detailTier: 'compact' }],
  ['font-30-rich', { fontSize: 30, pressure: nominal, detailTier: 'rich' }],
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

const apiPaths = ['renderSvg', 'renderDeck', 'worker'];
const resultFor = name => {
  const result = results.find(row => row.name === name);
  assert.ok(result, `missing API result: ${name}`);
  return result;
};
for (const path of apiPaths) {
  const small = resultFor(`${path}/font-11-compact`).longEdge;
  const large = resultFor(`${path}/font-22-compact`).longEdge;
  assert.ok(Math.abs(large.fontSize - 2 * small.fontSize) < 0.02, `${path}: font override reaches the glyphs`);
  // The broken package already passed all 50 text-count checks at both sizes. Its backing
  // width stayed constant while the text doubled, so count-only font tests cannot catch it.
  assert.ok(large.width > 1.5 * small.width, `${path}: long-label backing must grow with its text`);
  assert.equal(resultFor(`${path}/nested-font-size-wins`).sha256,
    resultFor(`${path}/font-22-compact`).sha256, `${path}: nested font override wins once`);
}

function verifyInitialized(name, config, expectedCase) {
  for (const path of apiPaths) {
    const output = path === 'worker' ? workerSvg(config) : wasm[path](source, config);
    const svg = path === 'renderDeck' ? output.svg : output;
    const result = verify(`${path}/${name}`, svg, { detailTier: 'compact', ...config });
    assert.equal(result.sha256, resultFor(`${path}/${expectedCase}`).sha256,
      `${path}/${name}: initialized and per-call typography must agree`);
  }
}

for (const fontSize of [30, 11, 15]) {
  wasm.init({ fontSize, detailTier: 'compact', preserveLabels: true, pressure: nominal });
  verifyInitialized(`init-font-${fontSize}`, undefined,
    fontSize === 15 ? 'nominal-compact' : `font-${fontSize}-compact`);
}
wasm.init({ fontSize: 11, svg: { fontSize: 22 } });
verifyInitialized('init-nested-font-size-wins', undefined, 'font-22-compact');
verifyInitialized('font-override-after-init', { fontSize: 11 }, 'font-11-compact');
verifyInitialized('font-override-keeps-runtime-default', undefined, 'font-22-compact');
wasm.init({ fontSize: 15, detailTier: 'auto', preserveLabels: true });
assert.equal(results.length, 71);
const report = { source_sha256: sha256(source), wasm_sha256: sha256(wasmBytes), checks: results.length, results };
if (outputPath) await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
process.stdout.write(`PASS: ${results.length} actual WASM API checks${outputPath ? `; evidence ${outputPath}` : ''}\n`);
