#!/usr/bin/env node
// Exercise the shipped WASM WebGPU planning boundary under headless Node.
//
// The browser device pass has its own adapter tests. This test protects the
// boundary before that pass: `planWebGpu` and `renderSvg` must both consume the
// same corpus flowchart and agree on the actual SVG node set.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const wasm = await import(join(root, 'pkg', 'frankenmermaid.js'));
wasm.initSync({ module: readFileSync(join(root, 'pkg', 'frankenmermaid_bg.wasm')) });

const source = readFileSync(
  join(root, 'crates', 'fm-cli', 'tests', 'fixtures', 'frankentui_conformance', 'flowchart_basic.mmd'),
  'utf8',
);

assert.equal(typeof wasm.planWebGpu, 'function', 'WASM package must export planWebGpu');
assert.equal(typeof wasm.renderSvg, 'function', 'WASM package must export renderSvg');

const plan = wasm.planWebGpu(source);
assert.ok(plan && typeof plan === 'object', 'planWebGpu must return a plan summary');
assert.ok(Array.isArray(plan.bounds) && plan.bounds.length === 4, 'plan summary must include bounds');
assert.ok(plan.bounds[2] > 0 && plan.bounds[3] > 0, 'plan bounds must have positive extent');
assert.ok(plan.nodeInstances > 0, 'corpus flowchart must produce WebGPU node instances');
assert.ok(plan.edgeSegments > 0, 'corpus flowchart must produce WebGPU edge segments');
assert.ok(plan.textQuads > 0, 'corpus flowchart must produce WebGPU text quads');

const svg = wasm.renderSvg(source);
assert.equal(typeof svg, 'string', 'renderSvg must return SVG text');
const svgNodeIds = new Set(
  [...svg.matchAll(/<g id="fm-node-[^"]+"[^>]*\bdata-id="([^"]+)"/g)].map((match) => match[1]),
);

assert.deepEqual(
  [...svgNodeIds].sort(),
  ['A', 'B', 'C', 'D', 'E'],
  'SVG backend must render every node in the corpus flowchart',
);
assert.equal(
  plan.nodeInstances,
  svgNodeIds.size,
  'planWebGpu and renderSvg must agree on the actual rendered node set',
);

console.log(
  `WASM WebGPU/SVG agreement: nodes=${plan.nodeInstances} edges=${plan.edgeSegments} text=${plan.textQuads}`,
);
