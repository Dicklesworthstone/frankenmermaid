'use strict';
// Pure composition/session tests. Only the unused browser-backend import is replaced;
// renderer results are explicit fixtures, not evidence of native Mermaid parsing.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const code = fs.readFileSync(path.join(__dirname, 'fm-deck-editor.js'), 'utf8');
const specifier = 'import { createSourceEditorBackend } from "./fm-source-editor.js";';
assert.equal(code.split(specifier).length, 2);
const modulePromise = import('data:text/javascript;base64,' + Buffer.from(code.replace(specifier,
  'const createSourceEditorBackend = () => { throw Error("unused browser fixture"); };')).toString('base64'));
const draft = (changes = {}) => JSON.stringify({ slides: [{ id: 'intro', title: 'Introduction', nodes: ['a', 'b'], ...changes }] });
const rect = { x: 0, y: 0, width: 100, height: 80 };
function slide(id = 'intro', overrides = {}) {
  return { id, title: id === 'intro' ? 'Introduction' : id, bounds: { ...rect }, fitMargin: 150, zoomMax: 1.4,
    nodes: [{ sourceId: 'a', elementId: 'fm-node-a-0', step: 0 }, { sourceId: 'b', elementId: 'fm-node-b-1', step: 0 }],
    maxStep: 0, ...overrides };
}
function result(slides = [slide()]) {
  return { svg: '<svg xmlns="http://www.w3.org/2000/svg"><g id="fm-node-a-0"/><g id="fm-node-b-1"/></svg>',
    warnings: [], manifest: slides === null ? null : { schemaVersion: '1.1.0', viewBox: { ...rect },
      options: { fitMargin: 150, zoomMax: 1.4, dimOpacity: .07, autoAdvanceMs: 0 }, slides } };
}
const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
async function session({ source = 'flowchart LR\n a --> b', before = result(null), after = result(), render, check } = {}) {
  const { DeckAuthoringSession } = await modulePromise;
  const calls = [], validations = [];
  let cancels = 0;
  const s = new DeckAuthoringSession({ renderDeck: async input => { calls.push(input); return render ? render(input, calls.length) : calls.length % 2 ? before : after; },
    cancelRender: () => { cancels++; }, checkElements: check || (deck => { validations.push(deck); }) });
  s.setSource(source);
  return { s, calls, validations, cancels: () => cancels };
}

test('draft supports selector groups, reveal, edge policies, tips and complete presentation options', async () => {
  const { deckDraft } = await modulePromise;
  const spec = { title: 'Architecture 雪', options: { fitMargin: 100, zoomMax: 2, dimOpacity: .1, autoAdvanceMs: 3000 },
    tips: { a: 'Go here' }, overview: { enabled: true, tour: false, title: 'All', caption: 'Overview' },
    slides: [{ id: 'intro', nodes: ['a', 'subgraph:storage', '*'], reveal: [['a'], ['subgraph:storage']], edges: 'touching', fitMargin: 0, zoomMax: 3 }] };
  const parsed = deckDraft(JSON.stringify(spec));
  assert.deepEqual(parsed, spec);
  assert.ok(Object.isFrozen(parsed.slides[0].reveal[0]));
});

test('source append preserves BOM, mixed line endings, JSON5 comments and every existing byte', async () => {
  const { appendDeckDraft } = await modulePromise;
  const source = '\uFEFF---\r\ntitle: Existing\r\n---\r\n%%{deck: {slides: [], /* keep */}}%%\nflowchart LR\r\n a["}%%"] --> b';
  const plan = appendDeckDraft(source, draft());
  assert.equal(plan.source, source);
  assert.equal(plan.updatedSource.slice(0, source.length), source);
  assert.equal(plan.updatedSource.slice(source.length), '\r\n' + plan.block);
  assert.ok(plan.block.endsWith('\r\n'));
  assert.equal(plan.block.split('\r\n').length, 2);
});

test('caption and identifier delimiter text cannot escape an authored directive', async () => {
  const { appendDeckDraft } = await modulePromise;
  const caption = 'quote " backslash \\ }%%\n%%{init: {}}%%\u2028雪';
  const plan = appendDeckDraft('flowchart TD\n a', draft({ caption, nodes: ['a}%%'] }));
  assert.equal(plan.block.match(/}\%\%/g).length, 1);
  assert.equal(plan.block.split('\n').length, 2);
  const decoded = JSON.parse(plan.block.slice('%%{deck: '.length, plan.block.lastIndexOf('}%%')));
  assert.equal(decoded.slides[0].caption, caption);
  assert.deepEqual(decoded.slides[0].nodes, ['a}%%']);
});

test('bounded draft rejects malformed or silently lossy schema inputs', async () => {
  const { deckDraft } = await modulePromise;
  for (const input of ['null', '[]', '{}', '{"slides":[]}', '{"slides":[],"__proto__":{}}',
    draft({ nodes: [] }), draft({ nodes: ['a', 'a'] }), draft({ id: '' }), draft({ nodes: ['\ud800'] }),
    draft({ edges: 'all' }), draft({ reveal: 'random' }), draft({ reveal: [['a'], ['a']] }),
    draft({ reveal: [[]] }), draft({ title: '😀'.repeat(513) }), draft({ fitMargin: -1 }),
    draft({ zoomMax: 0 }), draft({ typo: true }), '{"options":{"zoomMax":1e999},"slides":[{"id":"s","nodes":["a"]}]}']) {
    assert.throws(() => deckDraft(input), Error, input.slice(0, 90));
  }
  assert.throws(() => deckDraft(JSON.stringify({ slides: [JSON.parse(draft()).slides[0], JSON.parse(draft()).slides[0]] })), /unique/);
});

test('limits count UTF-8 bytes after escaping and permit exactly 64 short slides', async () => {
  const { appendDeckDraft, deckDraft } = await modulePromise;
  const slides = Array.from({ length: 64 }, (_, i) => ({ id: `s${i}`, nodes: ['a'] }));
  assert.equal(deckDraft(JSON.stringify({ slides })).slides.length, 64);
  assert.throws(() => deckDraft(JSON.stringify({ slides: [...slides, { id: 'more', nodes: ['a'] }] })), /1–64/);
  const large = JSON.stringify({ slides: Array.from({ length: 30 }, (_, i) => ({ id: `s${i}`, nodes: ['a'], caption: '雪'.repeat(512) })) });
  assert.ok(large.length < 32768);
  assert.throws(() => appendDeckDraft('a', large), /32 KiB/);
  assert.throws(() => appendDeckDraft('x'.repeat(8 * 1024 * 1024), draft()), /Updated source/);
});

test('prepare resolves the exact base and candidate without changing authoritative source', async () => {
  const h = await session();
  const prepared = await h.s.prepare(draft());
  assert.deepEqual(h.calls, [prepared.source, prepared.updatedSource]);
  assert.equal(h.s.source, prepared.source);
  assert.equal(h.validations.length, 1);
  assert.equal(prepared.existingSlides, 0);
  assert.ok(Object.isFrozen(prepared.deck.manifest.slides[0].nodes));
});

test('commit is an identity-bound single-use token and never accepts a forged preview', async () => {
  const h = await session();
  const prepared = await h.s.prepare(draft());
  assert.throws(() => h.s.commit({ ...prepared }), { name: 'AbortError' });
  assert.equal(h.s.commit(prepared), prepared.updatedSource);
  assert.equal(h.s.source, prepared.updatedSource);
  assert.equal(h.s.ownsComposition, true);
  assert.throws(() => h.s.commit(prepared), { name: 'AbortError' });
});

test('revising a committed composition replaces only its owned suffix rather than appending duplicates', async () => {
  const h = await session();
  const first = await h.s.prepare(draft());
  h.s.commit(first);
  const second = await h.s.prepare(draft());
  assert.equal(second.source, first.updatedSource);
  assert.equal(second.updatedSource, first.updatedSource);
  assert.equal(h.calls[2], first.source);
  assert.equal(second.updatedSource.match(/%%\{deck:/g).length, 1);
});

test('external source replacement clears suffix ownership and revokes an old review even for equal text', async () => {
  const h = await session();
  const prepared = await h.s.prepare(draft());
  h.s.setSource(h.s.source);
  assert.throws(() => h.s.commit(prepared), { name: 'AbortError' });
  assert.equal(h.s.ownsComposition, false);
  const fresh = await h.s.prepare(draft());
  h.s.commit(fresh);
  h.s.setSource(h.s.source);
  assert.equal(h.s.ownsComposition, false);
});

test('invalid external text revokes a prepared review before rejecting the new source', async () => {
  const h = await session();
  const prepared = await h.s.prepare(draft());
  assert.throws(() => h.s.setSource('\ud800'), /surrogate/);
  assert.throws(() => h.s.commit(prepared), { name: 'AbortError' });
});

test('existing slides are retained in order and new IDs cannot alias an existing slide', async () => {
  const before = result([slide('old')]);
  const h = await session({ before, after: result([slide('old'), slide()]) });
  const prepared = await h.s.prepare(draft());
  assert.equal(prepared.existingSlides, 1);
  assert.deepEqual(prepared.deck.manifest.slides.map(s => s.id), ['old', 'intro']);
  const collision = await session({ before: result([slide()]) });
  await assert.rejects(collision.s.prepare(draft()), /already used/);
  assert.equal(collision.calls.length, 1);
});

test('existing presentation capacity is checked before rendering a truncated candidate', async () => {
  const h = await session({ before: result(Array.from({ length: 64 }, (_, i) => slide(`s${i}`))) });
  await assert.rejects(h.s.prepare(draft()), /exceeds 64/);
  assert.equal(h.calls.length, 1);
});

test('dropped, reordered, renamed, empty and text-truncated slides cannot authorize source changes', async () => {
  for (const after of [result([]), result([slide('renamed')]), result([slide('intro', { nodes: [] })]),
    result([slide('intro', { title: 'truncated' })]), result([slide('intro', { caption: 'unexpected' })]),
    result([slide('intro', { nodes: [slide().nodes[0]] })])]) {
    const h = await session({ after });
    await assert.rejects(h.s.prepare(draft()));
    assert.equal(h.s.source, 'flowchart LR\n a --> b');
  }
  const h = await session({ before: result([slide('old')]), after: result([slide(), slide('old')]) });
  await assert.rejects(h.s.prepare(draft()), /order/);
});

test('manual reveals are checked against actual member steps, not recomputed graph ranks', async () => {
  const actual = slide('intro', { nodes: [slide().nodes[0], { ...slide().nodes[1], step: 1 }], maxStep: 1 });
  const good = await session({ after: result([actual]) });
  assert.equal((await good.s.prepare(draft({ reveal: [['b']] }))).deck.manifest.slides[0].maxStep, 1);
  const bad = await session();
  await assert.rejects(bad.s.prepare(draft({ reveal: [['b']] })), /reveal 'b'/);
});

test('no-reveal and no-edge requests are not silently changed by the engine', async () => {
  const h = await session({ after: result([slide('intro', { maxStep: 1 })]) });
  await assert.rejects(h.s.prepare(draft()), /unexpected reveal/);
  const edges = await session({ after: result([slide('intro', { edges: [{ elementId: 'edge', step: 0 }] })]) });
  await assert.rejects(edges.s.prepare(draft({ edges: 'none' })), /still contains edges/);
});

test('engine-owned group expansion and auto reveal remain in the review with warnings', async () => {
  const after = result([slide('intro', { maxStep: 2 })]);
  after.warnings = [{ message: 'a warning to review', severity: 'Warning' }];
  const h = await session({ after });
  const reviewed = await h.s.prepare(draft({ nodes: ['subgraph:storage', '*'], reveal: 'auto' }));
  assert.equal(reviewed.deck.manifest.slides[0].maxStep, 2);
  assert.deepEqual(reviewed.deck.warnings, after.warnings);
  after.warnings[0].message = 'transport mutation';
  after.manifest.slides[0].nodes[0].sourceId = 'changed';
  assert.equal(reviewed.deck.warnings[0].message, 'a warning to review');
  assert.equal(reviewed.deck.manifest.slides[0].nodes[0].sourceId, 'a');
});

test('unsupported family or malformed engine response cannot produce a review', async () => {
  for (const after of [result(null), null, {}, { svg: '', warnings: 'wrong' }]) {
    const h = await session({ after });
    await assert.rejects(h.s.prepare(draft()));
  }
  const h = await session({ before: {} });
  await assert.rejects(h.s.prepare(draft()), /Invalid original/);
  assert.equal(h.calls.length, 1);
});

test('failed SVG-element validation prevents a commit and preserves source', async () => {
  const h = await session({ check: () => { throw new Error('missing actual SVG element'); } });
  await assert.rejects(h.s.prepare(draft()), /missing actual/);
  assert.equal(h.s.ownsComposition, false);
});

test('typing during the base render cancels before the candidate can enter the engine', async () => {
  const gate = deferred();
  const h = await session({ render: () => gate.promise });
  const pending = h.s.prepare(draft());
  h.s.setSource('new source');
  gate.resolve(result(null));
  await assert.rejects(pending, { name: 'AbortError' });
  assert.equal(h.calls.length, 1);
});

test('cancellation after candidate rendering or during asynchronous SVG checking revokes publication', async () => {
  for (const at of ['render', 'check']) {
    const gate = deferred();
    const h = await session({ render: (_s, n) => n === 1 ? result(null) : at === 'render' ? gate.promise : result(),
      check: () => at === 'check' ? gate.promise : undefined });
    const pending = h.s.prepare(draft());
    await tick();
    h.s.cancel();
    gate.resolve(result());
    await assert.rejects(pending, { name: 'AbortError' });
  }
});

test('superseded review cannot replace the newer request and its immutable source', async () => {
  const gate = deferred();
  const h = await session({ render: (_s, n) => n === 1 ? gate.promise : n === 2 ? result(null) : result() });
  const old = h.s.prepare(draft());
  const fresh = await h.s.prepare(draft());
  gate.resolve(result(null));
  await assert.rejects(old, { name: 'AbortError' });
  assert.equal(h.s.commit(fresh), fresh.updatedSource);
});

test('disposal cancels pending work and permanently revokes all edit methods', async () => {
  const gate = deferred(), h = await session({ render: () => gate.promise });
  const pending = h.s.prepare(draft());
  h.s.dispose(); h.s.dispose();
  gate.resolve(result(null));
  await assert.rejects(pending, { name: 'AbortError' });
  await assert.rejects(h.s.prepare(draft()), { name: 'AbortError' });
  assert.throws(() => h.s.setSource('x'), { name: 'AbortError' });
  assert.throws(() => h.s.commit({}), { name: 'AbortError' });
});
