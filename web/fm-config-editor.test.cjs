const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const modulePromise = import('data:text/javascript;base64,' + fs.readFileSync(path.join(__dirname, 'fm-config-editor.js')).toString('base64'));
const deferred = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const validReport = () => JSON.stringify({ schemaVersion: '1.0.0', errors: [] });
async function harness(loadValidator = async () => ({ validateInitDirectives: validReport })) {
  const { ConfigurationEditSession } = await modulePromise;
  let source = 'flowchart TD\n a[Original] --> b';
  const session = new ConfigurationEditSession({ getSource: () => source, loadValidator });
  session.load();
  return { session, source: () => source, setSource: value => { source = value; } };
}

test('new configuration is inserted before the header, without touching diagram text', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const source = '%% Diagram description\n\nflowchart TD\n a[雪 🙂] --> b';
  const document = configurationDocument(source);
  assert.deepEqual(document.targets, []);
  const plan = planConfigurationEdit(document, '{"theme":"dark"}');
  assert.equal(plan.updatedSource, '%% Diagram description\n\n%%{init: {"theme":"dark"}}%%\nflowchart TD\n a[雪 🙂] --> b');
  assert.equal(source.slice(plan.from, plan.to), '');
  assert.equal(plan.updatedSource.slice(plan.from + plan.replacement.length), source.slice(plan.to));
});

test('BOM, YAML front matter, CRLF, and the unchanged source complement survive', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const source = '\uFEFF---\r\ntitle: Configuration\r\n---\r\n%% Keep\r\nflowchart LR\r\na-->b\r\n';
  const document = configurationDocument(source);
  assert.equal(document.newline, '\r\n');
  const result = planConfigurationEdit(document, '{}').updatedSource;
  assert.equal(result, source.replace('flowchart', '%%{init: {}}%%\r\nflowchart'));
  assert.equal(result.split('\uFEFF').length, 2);
});

test('empty, BOM-only, and unterminated comment-only documents insert a separate directive', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  for (const [source, expected] of [['', '%%{init: {}}%%\n'], ['\uFEFF', '\uFEFF%%{init: {}}%%\n'],
    ['%% Keep', '%% Keep\n%%{init: {}}%%\n']]) {
    assert.equal(planConfigurationEdit(configurationDocument(source), '{}').updatedSource, expected);
  }
});

test('only the selected payload changes, preserving directive spelling and surrounding comments', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const source = '  %%{ /* before */ "init" : { theme: \'dark\', /* keep in draft */ }, }%% %% after\r\nflowchart LR\r\na-->b';
  const document = configurationDocument(source);
  assert.equal(document.targets.length, 1);
  assert.equal(document.targets[0].draft, "{ theme: 'dark', /* keep in draft */ }");
  assert.equal(document.targets[0].line, 1);
  const plan = planConfigurationEdit(document, '{"theme":"forest"}');
  assert.equal(plan.updatedSource, source.replace(document.targets[0].draft, '{"theme":"forest"}'));
  assert.equal(plan.from, document.targets[0].from);
  assert.equal(plan.to, document.targets[0].to);
});

test('compound directives retain deck content and multiple init objects remain independently addressable', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const source = '%%{deck: {title:"Story", slides: []}, init: {"theme":"dark"}}%%\n' +
    '%%{init:{"sequence":{"mirrorActors":false}}}%%\nflowchart TD\na-->b';
  const document = configurationDocument(source);
  assert.equal(document.targets.length, 2);
  assert.deepEqual(document.targets.map(target => target.line), [1, 2]);
  const first = planConfigurationEdit(document, '{}', 0);
  assert.ok(first.updatedSource.includes('deck: {title:"Story", slides: []}'));
  assert.ok(first.updatedSource.includes('%%{init:{"sequence":{"mirrorActors":false}}}%%'));
  const last = planConfigurationEdit(document, '{}');
  assert.ok(last.updatedSource.includes('init: {"theme":"dark"}'));
  assert.ok(last.updatedSource.includes('%%{init:{}}%%'));
});

test('quoted delimiters, escaped quotes, arrays, and JSON5 comments cannot terminate a range', async () => {
  const { configurationDocument } = await modulePromise;
  const draft = String.raw`{theme: 'a\\\'b', themeVariables: {label: "}%%", note: 'init:{x}'},
    /* }%% [ */ other: [{nested: true}], // }%%
    trailing: 3, }`;
  const document = configurationDocument(`%%{init:${draft}}%%\nflowchart TD\na-->b`);
  assert.equal(document.targets.length, 1);
  assert.equal(document.targets[0].draft, draft);
});

test('initialization-looking text after the header is never selected or rewritten', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const source = 'flowchart TD\n A["line one\n%%{init: {fake:true}}%%\nline three"]\n%%{init:{theme:"dark"}}%%';
  const document = configurationDocument(source);
  assert.equal(document.targets.length, 0);
  assert.equal(planConfigurationEdit(document, '{}').updatedSource, '%%{init: {}}%%\n' + source);
});

test('multiline deck and constraints preambles are skipped as intact source ranges', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const prefix = '%%{deck:{slides:[\n {title:"init: fake", nodes:["a"]}\n]}}%%\n%%{constraints:{sameRank:["a","b"]}}%%\n';
  const source = prefix + 'flowchart TD\na-->b';
  const document = configurationDocument(source);
  assert.equal(document.insertion, prefix.length);
  assert.equal(document.targets.length, 0);
  assert.equal(planConfigurationEdit(document, '{}').updatedSource, prefix + '%%{init: {}}%%\nflowchart TD\na-->b');
});

test('malformed envelopes and ambiguous keys fail without proposing a destructive edit', async () => {
  const { configurationDocument } = await modulePromise;
  for (const source of ['%%{init:{x:1}}\nflowchart TD', '%%{init:{x:[1}}}%%', '%%{init:{x:"unclosed}}%%',
    '%%{init:{}}%% flowchart TD', '%%{init:[] }%%', '%%{init:{/* unclosed}}%%',
    '%%{"in\\u0069t":{}}%%', '---\ntitle: unclosed']) {
    assert.throws(() => configurationDocument(source), undefined, source);
  }
});

test('drafts cannot escape their object into other directives or diagram statements', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  const document = configurationDocument('flowchart TD\na-->b');
  for (const draft of ['[]', 'null', '{} }%%\na-->evil\n%%{init:{}', '{}\n{}', '{x:[}', '{} // trailing']) {
    assert.throws(() => planConfigurationEdit(document, draft), undefined, draft);
  }
  assert.throws(() => planConfigurationEdit(document, '{}', 0), /Select/);
  assert.throws(() => planConfigurationEdit(document, '{}', NaN), /Select/);
});

test('source, configuration, nesting and directive-count limits are enforced', async () => {
  const { configurationDocument, planConfigurationEdit } = await modulePromise;
  assert.throws(() => configurationDocument('x'.repeat(8 * 1024 * 1024 + 1)), /size limit/);
  assert.throws(() => configurationDocument('\ud800'), /surrogate/);
  assert.throws(() => configurationDocument('%%{init:{}}%%\n'.repeat(257)), /Too many/);
  assert.throws(() => configurationDocument('%%{' + 'init:{},'.repeat(257) + '}%%'), /Too many/);
  const document = configurationDocument('flowchart TD');
  assert.throws(() => planConfigurationEdit(document, '{"x":"' + 'x'.repeat(65536) + '"}'), /size limit/);
  assert.throws(() => planConfigurationEdit(document, '{"x":' + '['.repeat(130) + '0' + ']'.repeat(130) + '}'), /nesting/);
});

test('prepared edits validate the full proposed source and require a single-use commit token', async () => {
  const inputs = [];
  const h = await harness(async () => ({ validateInitDirectives: source => { inputs.push(source); return validReport(); } }));
  const original = h.source();
  const plan = await h.session.prepare('{"theme":"dark"}');
  assert.equal(h.source(), original);
  assert.equal(inputs[0], plan.updatedSource);
  assert.ok(Object.isFrozen(plan));
  assert.throws(() => h.session.commit({ ...plan }), { name: 'AbortError' });
  const source = h.session.commit(plan);
  assert.ok(source.startsWith('%%{init: {"theme":"dark"}}%%'));
  assert.throws(() => h.session.commit(plan), { name: 'AbortError' });
  h.setSource(source);
  const loaded = h.session.load();
  assert.equal(loaded.targets[0].draft, '{"theme":"dark"}');
});

test('engine field errors are actionable and leave the source and previous preparation uncommittable', async () => {
  const h = await harness(async () => ({ validateInitDirectives: () => JSON.stringify({ schemaVersion: '1.0.0',
    errors: [{field:'flowchart.nodeSpacing', value:'-1', message:'must be nonnegative'}] }) }));
  const before = h.source();
  await assert.rejects(h.session.prepare('{"flowchart":{"nodeSpacing":-1}}'), error => {
    assert.equal(error.name, 'ConfigurationValidationError');
    assert.match(error.message, /flowchart.nodeSpacing: must be nonnegative/);
    assert.ok(Object.isFrozen(error.errors[0]));
    return true;
  });
  assert.equal(h.source(), before);
  assert.throws(() => h.session.commit(null), {name:'AbortError'});
});

test('missing validators, malformed reports and unknown versions fail closed', async () => {
  for (const report of ['not JSON', '{}', '{"schemaVersion":"2.0.0","errors":[]}',
    '{"schemaVersion":"1.0.0","errors":[{"message":"bad"}]}']) {
    const h = await harness(async () => ({ validateInitDirectives: () => report }));
    await assert.rejects(h.session.prepare('{}'));
  }
  const h = await harness(async () => ({}));
  await assert.rejects(h.session.prepare('{}'), /lacks strict directive validation/);
});

test('source changes during validator loading cannot commit stale text', async () => {
  const pending = deferred();
  const h = await harness(() => pending.promise);
  const prepared = h.session.prepare('{}');
  h.setSource('sequenceDiagram\n A->>B: New');
  pending.resolve({ validateInitDirectives: () => { throw new Error('must not validate stale source'); } });
  await assert.rejects(prepared, {name:'AbortError'});
});

test('edit-and-revert and equal-text document switches invalidate in-flight validation', async () => {
  const pending = deferred();
  const h = await harness(async () => ({validateInitDirectives: () => pending.promise}));
  const prepared = h.session.prepare('{}');
  await new Promise(resolve => setImmediate(resolve));
  h.session.sourceChanged();
  pending.resolve(validReport());
  await assert.rejects(prepared, {name:'AbortError'});
  await assert.rejects(h.session.prepare('{}'), {name:'AbortError'});
});

test('new preparations supersede old requests even when the older validator completes later', async () => {
  const one = deferred(), two = deferred();
  let calls = 0;
  const h = await harness(async () => ({validateInitDirectives: () => (++calls === 1 ? one : two).promise}));
  const first = h.session.prepare('{"theme":"dark"}');
  await new Promise(resolve => setImmediate(resolve));
  const second = h.session.prepare('{"theme":"forest"}');
  await new Promise(resolve => setImmediate(resolve));
  two.resolve(validReport());
  const plan = await second;
  one.resolve(validReport());
  await assert.rejects(first, {name:'AbortError'});
  assert.match(h.session.commit(plan), /forest/);
});

test('changing source after validation and disposal both revoke commit authority', async () => {
  const h = await harness();
  const plan = await h.session.prepare('{}');
  h.setSource(h.source() + '\n b-->c');
  assert.throws(() => h.session.commit(plan), {name:'AbortError'});
  h.session.load();
  const next = await h.session.prepare('{}');
  h.session.dispose();
  assert.throws(() => h.session.commit(next), {name:'AbortError'});
  assert.throws(() => h.session.load(), {name:'AbortError'});
});

test('visual field changes preserve unrelated nested values and explicit false', async () => {
  const {patchConfigurationDraft} = await modulePromise;
  const initial = '{"theme":"dark","themeVariables":{"primaryColor":"#123456"},"flowchart":{"nodeSpacing":40},"sequence":{"mirrorActors":false}}';
  const result = JSON.parse(patchConfigurationDraft(initial, ['flowchart','rankSpacing'], 70));
  assert.deepEqual(result, {theme:'dark', themeVariables:{primaryColor:'#123456'}, flowchart:{nodeSpacing:40, rankSpacing:70}, sequence:{mirrorActors:false}});
  assert.deepEqual(JSON.parse(patchConfigurationDraft('{}', ['sequence','showSequenceNumbers'], false)), {sequence:{showSequenceNumbers:false}});
  assert.deepEqual(JSON.parse(patchConfigurationDraft('{"sequence":{"mirrorActors":false}}', ['sequence','mirrorActors'], undefined)), {});
});

test('visual field changes cannot introduce nonfinite values or traverse object prototypes', async () => {
  const {patchConfigurationDraft} = await modulePromise;
  for (const path of [['__proto__','polluted'], ['constructor','prototype','polluted'], [], ['']]) {
    assert.throws(() => patchConfigurationDraft('{}', path, true), /path/);
  }
  assert.equal({}.polluted, undefined);
  for (const value of [NaN, Infinity, -Infinity]) assert.throws(() => patchConfigurationDraft('{}', ['flowchart','nodeSpacing'], value), /finite/);
  assert.throws(() => patchConfigurationDraft('{"flowchart":7}', ['flowchart','nodeSpacing'], 1), /not an object/);
  assert.throws(() => patchConfigurationDraft("{theme:'dark'}", ['theme'], 'forest')); // JSON5 stays text, never guessed.
});
