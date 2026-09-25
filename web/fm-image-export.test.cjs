const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const modulePromise = import('data:text/javascript;base64,' +
  fs.readFileSync(path.join(__dirname, 'fm-image-export.js')).toString('base64'));
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
async function harness(overrides = {}) {
  const { createDiagramExporter } = await modulePromise;
  let source = 'flowchart TD\n a --> b';
  const saves = [], renders = [];
  const exporter = createDiagramExporter({ getSource: () => source,
    renderSource: async input => { renders.push(input); return { svg: '<svg/>' }; },
    makeArtifact: (svg, options) => ({ text: svg, ...options }),
    saveArtifact: async artifact => { saves.push(artifact); }, ...overrides });
  return { exporter, saves, renders, setSource: value => { source = value; } };
}

test('exports a fresh exact source snapshot and never changes the source', async () => {
  const h = await harness();
  const result = await h.exporter.export({ filename: 'my diagram' });
  assert.equal(h.renders[0], 'flowchart TD\n a --> b');
  assert.equal(h.saves.length, 1);
  assert.equal(result.filename, 'my diagram');
  assert.equal(h.exporter.busy, false);
});
test('typing during rendering cannot export an obsolete diagram', async () => {
  const pending = deferred();
  const h = await harness({ renderSource: () => pending.promise });
  const result = h.exporter.export();
  h.setSource('sequenceDiagram\n A->>B: new');
  pending.resolve({ svg: '<svg/>' });
  await assert.rejects(result, { name: 'AbortError' });
  assert.equal(h.saves.length, 0);
});
test('cancellation catches edit-and-revert and document switches with equal source', async () => {
  const pending = deferred();
  const h = await harness({ renderSource: () => pending.promise });
  const result = h.exporter.export();
  h.exporter.cancel();
  pending.resolve({ svg: '<svg/>' });
  await assert.rejects(result, { name: 'AbortError' });
  assert.equal(h.saves.length, 0);
});
test('freshness is checked again after asynchronous artifact preparation', async () => {
  const pending = deferred();
  const h = await harness({ makeArtifact: () => pending.promise });
  const result = h.exporter.export();
  await new Promise(resolve => setImmediate(resolve));
  h.setSource('changed during encoding');
  pending.resolve({ text: '<svg/>' });
  await assert.rejects(result, { name: 'AbortError' });
  assert.equal(h.saves.length, 0);
});
test('superseded requests cannot clear the busy state of the newer request', async () => {
  const first = deferred(), second = deferred();
  let count = 0;
  const h = await harness({ renderSource: () => (++count === 1 ? first : second).promise });
  const old = h.exporter.export();
  const fresh = h.exporter.export();
  first.resolve({ svg: 'old' });
  await assert.rejects(old, { name: 'AbortError' });
  assert.equal(h.exporter.busy, true);
  second.resolve({ svg: 'fresh' });
  await fresh;
  assert.deepEqual(h.saves.map(artifact => artifact.text), ['fresh']);
});
test('disposal aborts encoding and prevents future exports', async () => {
  let signal;
  const pending = deferred();
  const h = await harness({ makeArtifact: (_svg, _options, incoming) => { signal = incoming; return pending.promise; } });
  const result = h.exporter.export();
  await new Promise(resolve => setImmediate(resolve));
  h.exporter.dispose();
  assert.equal(signal.aborted, true);
  pending.resolve({ text: '<svg/>' });
  await assert.rejects(result, { name: 'AbortError' });
  await assert.rejects(h.exporter.export(), { name: 'AbortError' });
  assert.equal(h.saves.length, 0);
});
test('render failures propagate without exporting or retrying another renderer', async () => {
  let calls = 0;
  const h = await harness({ renderSource: async () => { calls += 1; throw new Error('invalid diagram'); } });
  await assert.rejects(h.exporter.export(), /invalid diagram/);
  assert.equal(calls, 1);
  assert.equal(h.saves.length, 0);
  assert.equal(h.exporter.busy, false);
});
test('download failures are reported and the next export can recover', async () => {
  let count = 0;
  const h = await harness({ saveArtifact: () => { if (++count === 1) throw new Error('download blocked'); } });
  await assert.rejects(h.exporter.export(), /download blocked/);
  await h.exporter.export();
  assert.equal(count, 2);
});
test('options are snapshotted before asynchronous rendering', async () => {
  const pending = deferred();
  const h = await harness({ renderSource: () => pending.promise });
  const options = { filename: 'original' };
  const result = h.exporter.export(options);
  options.filename = 'changed';
  pending.resolve({ svg: '<svg/>' });
  assert.equal((await result).filename, 'original');
});
test('filenames are Unicode-safe leaf names with a controlled extension', async () => {
  const { imageFilename } = await modulePromise;
  assert.equal(imageFilename('../folder\\test.mmd', 'svg'), 'test.svg');
  assert.equal(imageFilename('..', 'svg'), 'diagram.svg');
  assert.equal(imageFilename('\0report.png\n', 'svg'), 'report.svg');
  assert.equal(imageFilename('😀'.repeat(181), 'svg'), '😀'.repeat(180) + '.svg');
  assert.throws(() => imageFilename('test', 'html'), /Unsupported/);
});

test('PNG dimensions use explicit resolution, round up, and enforce exact allocation bounds', async () => {
  const { pngDimensions, imageFilename } = await modulePromise;
  assert.deepEqual(pngDimensions(10.1, 20.2, 3), { width: 31, height: 61 });
  assert.deepEqual(pngDimensions(8000, 4000, 1), { width: 8000, height: 4000 });
  assert.deepEqual(pngDimensions(16384, 1, 1), { width: 16384, height: 1 });
  assert.throws(() => pngDimensions(8001, 4000, 1), /allocation limit/);
  assert.throws(() => pngDimensions(16385, 1, 1), /allocation limit/);
  assert.throws(() => pngDimensions(Number.MAX_VALUE, 1, 4), /allocation limit/);
  assert.equal(imageFilename('diagram.mermaid', 'png'), 'diagram.png');
});
test('PNG dimensions reject invalid sizes and unsupported scale values before allocation', async () => {
  const { pngDimensions } = await modulePromise;
  for (const value of [NaN, Infinity, -1, 0, '10', null]) {
    assert.throws(() => pngDimensions(value, 10, 1), /positive finite/);
    assert.throws(() => pngDimensions(10, value, 1), /positive finite/);
  }
  for (const scale of [0, -1, 1.5, 5, '2', Infinity, NaN]) {
    assert.throws(() => pngDimensions(10, 10, scale), /scale/);
  }
});
test('pre-aborted PNG operations do not touch any browser resource', async () => {
  const { pngArtifact } = await modulePromise;
  const controller = new AbortController();
  controller.abort();
  const forbidden = new Proxy({}, { get() { throw new Error('resource accessed after cancellation'); } });
  await assert.rejects(pngArtifact('<svg/>', {}, forbidden, controller.signal), { name: 'AbortError' });
});
test('image format dispatch rejects unknown output types', async () => {
  const { imageArtifact } = await modulePromise;
  assert.throws(() => imageArtifact('<svg/>', { format: 'html' }), /Unsupported image format/);
});
