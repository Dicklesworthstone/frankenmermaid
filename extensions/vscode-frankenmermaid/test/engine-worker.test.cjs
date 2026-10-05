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
  const sources = new Map(), workers = [], fetches = [], revoked = [], timers = new Map();
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
      });
      this.thread.on("error", (error) => {
        for (const fn of this.listeners.get("error") || []) fn({message:error.message,preventDefault(){}});
      });
      this.thread.on("exit", (code) => this.exited.resolve(code));
      workers.push(this);
    }
    addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type,new Set()); this.listeners.get(type).add(fn); }
    removeEventListener(type, fn) { this.listeners.get(type)?.delete(fn); }
    postMessage(message, transfer) { this.thread.postMessage(message, transfer); }
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
  });
  t.after(() => api.destroy());
  return {api,workers,fetches,revoked,timers,sources};
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
