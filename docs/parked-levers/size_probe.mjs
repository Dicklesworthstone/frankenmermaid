import { CORPUS, generate } from '/data/projects/frankenmermaid/scripts/headtohead/corpus.mjs';

const item = CORPUS.find((c) => c.id === 'docs_site_50');
const texts = generate(item);
const arr = Array.isArray(texts) ? texts : [texts];

const rows = arr.map((src) => {
  const lines = src.split('\n').map((s) => s.trim()).filter(Boolean);
  const header = (lines[0] || '').split(/\s+/)[0];
  return { header, lines: lines.length };
});

const byType = {};
for (const r of rows) byType[r.header] = (byType[r.header] || 0) + 1;
console.log('diagram types:', JSON.stringify(byType));

const sizes = rows.map((r) => r.lines).sort((a, b) => a - b);
const pct = (p) => sizes[Math.min(sizes.length - 1, Math.floor(p * sizes.length))];
console.log(
  'statement-lines per diagram: min', sizes[0],
  'p25', pct(0.25), 'median', pct(0.5), 'p75', pct(0.75), 'p90', pct(0.9),
  'max', sizes[sizes.length - 1],
);

const total = sizes.reduce((a, b) => a + b, 0);
console.log('total statement lines across', sizes.length, 'diagrams:', total,
  'mean', (total / sizes.length).toFixed(1));

for (const cut of [10, 20, 30]) {
  const n = sizes.filter((s) => s <= cut).length;
  const share = sizes.filter((s) => s <= cut).reduce((a, b) => a + b, 0);
  console.log(`diagrams with <=${cut} lines: ${n} (${(100 * n / sizes.length).toFixed(0)}% of diagrams, ` +
    `${(100 * share / total).toFixed(0)}% of all lines)`);
}
