"use strict";
const assert = require("node:assert/strict");
const test = require("node:test");
const fs = require("node:fs");
const path = require("node:path");
const { Worker } = require("node:worker_threads");
const { create, attach } = require("../media/engine-worker.js");
const workerSource = fs.readFileSync(path.join(__dirname, "../media/engine-worker.js"), "utf8");
// Actual WebAssembly: (module (func (export "spin") (loop br 0))). No timer or fake
// pending promise substitutes for an executing, non-returning WASM call in the cancellation tests.
const bytes = Uint8Array.from([0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,
  7,8,1,4,115,112,105,110,0,0,10,9,1,7,0,3,64,12,0,11,11]);
const fixture = `
let wasm;
async function __wbg_init({module_or_path}) { wasm = (await WebAssembly.instantiate(module_or_path)).instance; }
function renderSvg(source) {
  if (source === 'HANG') { self.postMessage({entered: true}); wasm.exports.spin(); }
  if (source === 'THROW') throw new Error('fixture render rejected');
  return '<svg>' + source + '</svg>';
}
function parse(source) { return { warnings: ['notice'], ir: { diagnostics: [{message:'diagnostic'}], nodes: ['not copied'] } }; }
function parseLens(source) { return { bindings: [{elementId:'node', snippet:source}], parsed:parse(source) }; }
function applyParseLensEdit(source, id, replacement) {
  return {result:{elementId:id,previousSnippet:source,replacement,updatedSource:replacement,
    replacedRange:{startByte:0,endByte:new TextEncoder().encode(source).length}},snapshot:parseLens(replacement)};
}
export { __wbg_init as default, renderSvg, parse, parseLens, applyParseLensEdit };
`;
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
function harness(t, options = {}) {
  const sources = new Map(), workers = [], fetches = [], revoked = [], timers = new Map(), requests = [];
  let timerId = 0;
  // Only the platform boundary is adapted. Production create() builds its real self-contained
  // worker module, then a Node worker thread runs that module and production attach().
  class BlobBoundary { constructor(parts) { this.source = parts.join(""); } }
  class WorkerBoundary {
    constructor(url, config) {
      assert.equal(config.type, "module");
      this.listeners = new Map(); this.entered = deferred(); this.exited = deferred();
      this.terminated = false;
      const prefix = `import {parentPort} from 'node:worker_threads';
        globalThis.self = {addEventListener: (type, fn) => parentPort.on(type, data => fn({data})),
          postMessage: value => parentPort.postMessage(value)};\n`;
      this.thread = new Worker(new URL("data:text/javascript;base64," + Buffer.from(prefix + sources.get(url)).toString("base64")));
      this.thread.on("message", (data) => {
        if (data.entered) this.entered.resolve();
        for (const fn of this.listeners.get("message") || []) fn({data});
        options.afterMessage?.(data);
      });
      this.thread.on("error", (error) => {
        for (const fn of this.listeners.get("error") || []) fn({message:error.message,preventDefault(){}});
      });
      this.thread.on("exit", (code) => this.exited.resolve(code));
      workers.push(this);
    }
    addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type,new Set()); this.listeners.get(type).add(fn); }
    removeEventListener(type, fn) { this.listeners.get(type)?.delete(fn); }
    postMessage(message, transfer) {
      requests.push({ method: message.method, source: message.source });
      this.thread.postMessage(message, transfer);
    }
    terminate() { this.terminated = true; void this.thread.terminate(); }
  }
  const api = create({ moduleUrl:"module", workerUrl:"worker", binaryUrl:"binary",
    BlobType:BlobBoundary, WorkerType:WorkerBoundary,
    urls:{createObjectURL(blob){const id=`blob:${sources.size}`;sources.set(id,blob.source);return id;},revokeObjectURL(url){revoked.push(url);}},
    fetchFile:async (url, config) => {
      fetches.push(url);
      if (options.fetchFile) return options.fetchFile(url,config);
      return {ok:true,status:200,text:async () => url === "module" ? options.fixture || fixture : workerSource,
        arrayBuffer:async () => bytes.slice().buffer};
    },
    ...(options.manualTimers ? {setTimer(fn){const id=++timerId;timers.set(id,fn);return id;},clearTimer(id){timers.delete(id);}} : {}),
    ...options.cacheOptions,
  });
  t.after(() => api.destroy());
  return {api,workers,fetches,revoked,timers,sources,requests};
}

test("real worker loads explicit WASM bytes and preserves Unicode receipts without copying graph IR", async (t) => {
  const h = harness(t); await h.api.default();
  assert.equal(await h.api.renderSvg('café 😀'), '<svg>café 😀</svg>');
  const lens = await h.api.parseLens('café 😀');
  assert.deepEqual(lens.bindings, [{elementId:'node',snippet:'café 😀'}]);
  assert.deepEqual(lens.parsed, {warnings:['notice'],ir:{diagnostics:[{message:'diagnostic'}]}});
  const edit = await h.api.applyParseLensEdit('café 😀','node','新しい');
  assert.equal(edit.result.previousSnippet,'café 😀');
  assert.equal(edit.result.replacedRange.endByte,Buffer.byteLength('café 😀'));
  assert.equal(edit.result.updatedSource,'新しい');
  assert.equal(edit.snapshot.parsed.ir.nodes,undefined);
  assert.equal(h.workers.length,1); assert.equal(h.fetches.length,3);
});

test("cancel terminates actually executing WASM, leaves the caller responsive, and restarts cached bytes", {timeout:5000}, async (t) => {
  const h = harness(t); await h.api.default();
  const hung = h.api.renderSvg('HANG'); const rejected = assert.rejects(hung,{name:'AbortError'});
  await h.workers[0].entered.promise;
  let responsive = false; await new Promise((done) => setImmediate(() => {responsive=true;done();}));
  assert.equal(responsive,true);
  h.api.cancelPending(); await rejected; await h.workers[0].exited.promise;
  assert.equal(h.workers[0].terminated,true);
  assert.equal(await h.api.renderSvg('new'),'<'+'svg>new</svg>');
  assert.equal(h.workers.length,2); assert.equal(h.fetches.length,3);
});

test("watchdog kills a running WASM call and rejects all queued operations before restart", {timeout:5000}, async (t) => {
  const h = harness(t,{manualTimers:true}); await h.api.default();
  const hung = h.api.renderSvg('HANG'); const rejected = assert.rejects(hung,/time budget/);
  await h.workers[0].entered.promise;
  const queued = h.api.parse('later'); const queuedRejected = assert.rejects(queued,/time budget/);
  await new Promise((done) => setImmediate(done));
  assert.equal(h.timers.size,2);
  h.timers.values().next().value(); await Promise.all([rejected,queuedRejected]);
  assert.equal(h.timers.size,0); await h.workers[0].exited.promise;
  assert.equal(await h.api.renderSvg('recovered'),'<svg>recovered</svg>');
});

test("a render error does not poison the worker or erase valid sibling results", async (t) => {
  const h = harness(t); await h.api.default();
  await assert.rejects(h.api.renderSvg('THROW'),/fixture render rejected/);
  assert.equal(await h.api.renderSvg('valid'),'<svg>valid</svg>');
  h.api.cancelPending(); assert.equal(h.workers[0].terminated,false,'idle workers are reusable');
  assert.equal(await h.api.renderSvg('idle'),'<svg>idle</svg>');
  assert.equal(h.workers.length,1);
});

test("disposal settles running work, revokes its blob once, and never resurrects a worker", {timeout:5000}, async (t) => {
  const h = harness(t); await h.api.default();
  const hung = h.api.renderSvg('HANG'); const rejected = assert.rejects(hung,{name:'AbortError'});
  await h.workers[0].entered.promise; h.api.destroy(); await rejected; h.api.destroy();
  await h.workers[0].exited.promise;
  assert.equal(h.revoked.length,1);
  await assert.rejects(h.api.renderSvg('late'),{name:'AbortError'});
  assert.equal(h.workers.length,1);
});

test("oversized UTF-8 inputs fail before starting worker work", async (t) => {
  const h = harness(t);
  await assert.rejects(h.api.renderSvg('😀'.repeat(530000)),/size limit/);
  assert.equal(h.fetches.length,0); assert.equal(h.workers.length,0);
  await h.api.default();
  await assert.rejects(h.api.applyParseLensEdit('x','node','x'.repeat(2*1024*1024+1)),/size limit/);
  assert.equal(await h.api.renderSvg('next'),'<svg>next</svg>');
});

test("asset loading has a real deadline even when the fetch never settles", async (t) => {
  const h = harness(t,{manualTimers:true,fetchFile:() => new Promise(() => {})});
  const pending = h.api.default(); const rejected=assert.rejects(pending,/asset loading exceeded/);
  h.timers.values().next().value(); await rejected;
  assert.equal(h.timers.size,0); assert.equal(h.workers.length,0);
});

test("disposal aborts a hanging download without leaking a worker or blob", async (t) => {
  const h = harness(t,{manualTimers:true,fetchFile:() => new Promise(() => {})});
  const pending=h.api.default(); const rejected=assert.rejects(pending,{name:'AbortError'});
  h.api.destroy(); await rejected;
  assert.equal(h.timers.size,0); assert.equal(h.sources.size,0); assert.equal(h.workers.length,0);
});

test("failed downloads can be retried; late cancelled callers cannot start a worker", async (t) => {
  let fail=true;
  const h=harness(t,{fetchFile:async (url) => ({ok:!fail,status:fail?404:200,
    text:async () => url==='module'?fixture:workerSource,arrayBuffer:async () => bytes.slice().buffer})});
  await assert.rejects(h.api.default(),/HTTP 404/); fail=false;
  const pending=h.api.renderSvg('stale'); const rejected=assert.rejects(pending,{name:'AbortError'});
  h.api.cancelPending(); await rejected;
  assert.equal(h.workers.length,0);
  await h.api.default(); assert.equal(await h.api.renderSvg('current'),'<svg>current</svg>');
});

test("failed WASM initialization tears down the worker and permits a fresh attempt", async (t) => {
  // Alter only the synthetic fixture, never the production source or execution implementation.
  const broken=fixture.replace('wasm = (await WebAssembly.instantiate(module_or_path)).instance;',"throw new Error('bad binary');");
  const h=harness(t,{fixture:broken});
  await assert.rejects(h.api.default(),/bad binary/);
  assert.equal(h.workers[0].terminated,true);
  await assert.rejects(h.api.default(),/bad binary/);
  assert.equal(h.workers.length,2); assert.equal(h.workers[1].terminated,true);
});

test("worker endpoint refuses unknown operations and work before initialization", async () => {
  let receive; const replies=[];
  attach({addEventListener(type,fn){receive=fn;},postMessage(value){replies.push(value);}},{});
  await receive({data:{id:1,method:'renderSvg',source:'x'}});
  await receive({data:{id:2,method:'constructor',source:'x'}});
  assert.deepEqual(replies.map((r)=>r.ok),[false,false]);
  assert.match(replies[0].error,/Unavailable/);
});

const reads = (h, method, source) => h.requests.filter(request => request.method === method
  && (source === undefined || request.source === source)).length;

test("unchanged diagrams reuse their exact SVG and source-inspection results across edits", async (t) => {
  const h = harness(t); await h.api.default();
  const sources = Array.from({ length: 64 }, (_, index) => `flowchart LR\nN${index}[café 😀]`);
  const render = async (inputs) => {
    const results = [];
    for (const source of inputs) results.push([await h.api.parseLens(source), await h.api.renderSvg(source)]);
    return results;
  };
  const first = await render(sources);
  assert.equal(reads(h, 'renderSvg'), 64); assert.equal(reads(h, 'parseLens'), 64);
  h.api.cancelPending(); // The preview invalidates on every document change, including prose.
  const proseOnly = await render(sources);
  assert.deepEqual(proseOnly, first);
  assert.equal(reads(h, 'renderSvg'), 64); assert.equal(reads(h, 'parseLens'), 64);
  const edited = [...sources]; edited[31] += '\nN31-->new';
  h.api.cancelPending();
  const second = await render(edited);
  assert.notDeepEqual(second[31], first[31]);
  assert.deepEqual(second.filter((_, i) => i !== 31), first.filter((_, i) => i !== 31));
  assert.equal(reads(h, 'renderSvg'), 65); assert.equal(reads(h, 'parseLens'), 65);
  h.api.cancelPending();
  assert.deepEqual(await render([...sources].reverse()), [...first].reverse());
  assert.equal(reads(h, 'renderSvg'), 65); assert.equal(reads(h, 'parseLens'), 65);
});

test("cache identity includes exact Unicode, line endings, directives, whitespace and method", async (t) => {
  const h = harness(t); await h.api.default();
  const sources = ['A', 'A ', 'A\nB', 'A\r\nB', 'é', 'e\u0301', '😀', 'parse:A',
    '__proto__', 'A\u0000B', '%%{init: {theme:"dark"}}%%\nA'];
  for (const source of sources) {
    const svg = await h.api.renderSvg(source), lens = await h.api.parseLens(source);
    const parsed = await h.api.parse(source);
    assert.equal(svg, `<svg>${source}</svg>`); assert.equal(lens.bindings[0].snippet, source);
    assert.equal(await h.api.renderSvg(source), svg);
    assert.deepEqual(await h.api.parseLens(source), lens);
    assert.deepEqual(await h.api.parse(source), parsed);
  }
  for (const method of ['renderSvg','parseLens','parse']) assert.equal(reads(h, method), sources.length);
});

test("cache never lends mutable binding or diagnostic objects to callers", async (t) => {
  const h = harness(t); await h.api.default();
  const original = await h.api.parseLens('A[😀]'), expected = structuredClone(original);
  original.bindings[0].snippet = 'wrong source'; original.bindings.push({elementId:'invented'});
  original.parsed.ir.diagnostics[0].message = 'corrupted'; original.parsed.warnings.push('invented');
  const cached = await h.api.parseLens('A[😀]');
  assert.deepEqual(cached, expected); assert.notEqual(cached, original);
  cached.bindings[0].elementId = 'another'; cached.parsed.ir.diagnostics.length = 0;
  assert.deepEqual(await h.api.parseLens('A[😀]'), expected);
  assert.equal(reads(h, 'parseLens'), 1);
});

test("cached reads survive cancellation of real WASM without restarting the worker", {timeout:5000}, async (t) => {
  const h = harness(t); await h.api.default();
  const expected = await h.api.parseLens('safe'); await h.api.renderSvg('safe');
  const hung = h.api.renderSvg('HANG'), rejected = assert.rejects(hung, {name:'AbortError'});
  await h.workers[0].entered.promise; h.api.cancelPending(); await rejected;
  await h.workers[0].exited.promise;
  assert.equal(await h.api.renderSvg('safe'), '<svg>safe</svg>');
  assert.deepEqual(await h.api.parseLens('safe'), expected);
  assert.equal(h.workers.length, 1); assert.equal(reads(h, 'renderSvg', 'safe'), 1);
  assert.equal(reads(h, 'parseLens', 'safe'), 1);
  assert.equal(await h.api.renderSvg('changed'), '<svg>changed</svg>');
  assert.equal(h.workers.length, 2); assert.equal(h.fetches.length, 3);
});

test("cache hits still reject synchronous cancellation and disposal before delivery", async (t) => {
  const h = harness(t); await h.api.default(); await h.api.renderSvg('cached');
  const cancelled = h.api.renderSvg('cached'); h.api.cancelPending();
  await assert.rejects(cancelled, {name:'AbortError'});
  assert.equal(await h.api.renderSvg('cached'), '<svg>cached</svg>');
  const disposed = h.api.renderSvg('cached'); h.api.destroy();
  await assert.rejects(disposed, {name:'AbortError'});
  await assert.rejects(h.api.renderSvg('cached'), {name:'AbortError'});
  assert.equal(reads(h, 'renderSvg'), 1);
});

test("cancellation between worker response and delivery cannot seed a stale cache entry", async (t) => {
  let api, cancelOnce = true;
  const h = harness(t, {afterMessage(message) {
    if (cancelOnce && message.value === '<svg>racing</svg>') { cancelOnce = false; api.cancelPending(); }
  }});
  api = h.api; await api.default();
  await assert.rejects(api.renderSvg('racing'), {name:'AbortError'});
  assert.equal(await api.renderSvg('racing'), '<svg>racing</svg>');
  assert.equal(reads(h, 'renderSvg', 'racing'), 2);
});

test("worker errors are retried while successful sibling results remain reusable", async (t) => {
  const h = harness(t); await h.api.default(); await h.api.renderSvg('good');
  for (let i = 0; i < 3; i++) await assert.rejects(h.api.renderSvg('THROW'), /fixture render rejected/);
  assert.equal(reads(h, 'renderSvg', 'THROW'), 3);
  assert.equal(await h.api.renderSvg('good'), '<svg>good</svg>');
  assert.equal(reads(h, 'renderSvg', 'good'), 1);
});

test("source-edit receipts are never cached, including identical repeated replacements", async (t) => {
  const h = harness(t); await h.api.default(); await h.api.parseLens('before');
  const a = await h.api.applyParseLensEdit('before', 'node', 'after');
  const b = await h.api.applyParseLensEdit('before', 'node', 'after');
  assert.deepEqual(a, b); assert.notEqual(a, b);
  assert.equal(reads(h, 'applyParseLensEdit'), 2);
  assert.equal((await h.api.parseLens('before')).bindings[0].snippet, 'before');
  assert.equal(reads(h, 'parseLens'), 1);
});

test("LRU eviction keeps recently read entries, not just recently written ones", async (t) => {
  const h = harness(t, {cacheOptions:{maxCachedEntries:2}}); await h.api.default();
  await h.api.renderSvg('A'); await h.api.renderSvg('B'); await h.api.renderSvg('A');
  await h.api.renderSvg('C'); await h.api.renderSvg('A');
  assert.equal(reads(h, 'renderSvg', 'A'), 1);
  await h.api.renderSvg('B'); assert.equal(reads(h, 'renderSvg', 'B'), 2);
});

test("retained UTF-16 key-plus-JSON payload obeys the configured byte budget", async (t) => {
  const cost = source => 2 * (`renderSvg:${source}`.length + JSON.stringify(`<svg>${source}</svg>`).length);
  const h = harness(t, {cacheOptions:{maxCachedBytes:cost('é😀')}}); await h.api.default();
  await h.api.renderSvg('é😀'); await h.api.renderSvg('é😀');
  assert.equal(reads(h, 'renderSvg', 'é😀'), 1);
  await h.api.renderSvg('B'); // There is only room for one of these two entries.
  await h.api.renderSvg('é😀'); assert.equal(reads(h, 'renderSvg', 'é😀'), 2);
  const tooSmall = harness(t, {cacheOptions:{maxCachedBytes:cost('é😀') - 1}}); await tooSmall.api.default();
  await tooSmall.api.renderSvg('é😀'); await tooSmall.api.renderSvg('é😀');
  assert.equal(reads(tooSmall, 'renderSvg'), 2);
});

test("oversized results pass through without evicting the useful small cache", async (t) => {
  const h = harness(t, {cacheOptions:{maxCachedBytes:256}}); await h.api.default();
  await h.api.renderSvg('small'); const big = 'large'.repeat(1000);
  assert.equal(await h.api.renderSvg(big), `<svg>${big}</svg>`);
  assert.equal(await h.api.renderSvg(big), `<svg>${big}</svg>`);
  await h.api.renderSvg('small');
  assert.equal(reads(h, 'renderSvg', big), 2); assert.equal(reads(h, 'renderSvg', 'small'), 1);
});

for (const cacheOptions of [{maxCachedEntries:0}, {maxCachedBytes:0}]) {
  test(`zero cache capacity disables retention: ${JSON.stringify(cacheOptions)}`, async (t) => {
    const h = harness(t, {cacheOptions}); await h.api.default();
    await h.api.renderSvg('same'); await h.api.renderSvg('same');
    assert.equal(reads(h, 'renderSvg'), 2);
  });
}

test("result caches are client-local and cannot leak between engine asset sets", async (t) => {
  const a = harness(t), b = harness(t); await a.api.default(); await b.api.default();
  await a.api.renderSvg('identical'); await b.api.renderSvg('identical');
  assert.equal(reads(a, 'renderSvg'), 1); assert.equal(reads(b, 'renderSvg'), 1);
  a.api.destroy(); assert.equal(await b.api.renderSvg('identical'), '<svg>identical</svg>');
  assert.equal(reads(b, 'renderSvg'), 1);
});

for (const [name, value, verify] of [
  ['undefined property', '{message:"diagnostic", optional:undefined}', result => assert.ok(Object.hasOwn(result,'optional'))],
  ['negative zero', '{message:"diagnostic", value:-0}', result => assert.ok(Object.is(result.value,-0))],
  ['nonfinite number', '{message:"diagnostic", value:NaN}', result => assert.ok(Number.isNaN(result.value))],
  ['Map', '{message:"diagnostic", value:new Map([["node","café"]])}', result => assert.equal(result.value.get('node'),'café')],
  ['sparse array', '{message:"diagnostic", value:Array(2)}', result => assert.equal(0 in result.value,false)],
  ['shared references', '(() => { const value={}; return {message:"diagnostic", a:value,b:value}; })()', result => assert.equal(result.a,result.b)],
  ['cycle', '(() => { const value={message:"diagnostic"}; value.self=value; return value; })()', result => assert.equal(result.self,result)],
]) {
  test(`non-JSON ${name} retains structured-clone semantics without caching`, async (t) => {
    const custom = fixture.replace("[{message:'diagnostic'}]", `[${value}]`);
    const h = harness(t,{fixture:custom}); await h.api.default();
    verify((await h.api.parse('A')).ir.diagnostics[0]);
    verify((await h.api.parse('A')).ir.diagnostics[0]);
    assert.equal(reads(h,'parse'),2);
  });
}

test("invalid retention limits fail before downloading or creating a worker", (t) => {
  for (const cacheOptions of [{maxCachedBytes:-1}, {maxCachedBytes:NaN}, {maxCachedBytes:1.5},
    {maxCachedBytes:129*1024*1024}, {maxCachedEntries:-1}, {maxCachedEntries:Infinity},
    {maxCachedEntries:2049}]) assert.throws(() => harness(t, {cacheOptions}), /read-cache limits/);
});
