"use strict";

// Exercises the shipped worker; only module loading/WASM exports and the clock are mocked.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const plain = (value) => JSON.parse(JSON.stringify(value));
const snapshot = (input) => ({
  bindings: [{ elementId: "fm-node-a-0", textRange: { startByte: 0, endByte: Buffer.byteLength(input) }, snippet: input }],
  parsed: { warnings: ["a parser warning"], ir: { nodes: ["not needed by the host"] } },
});

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function tick() {
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
}
function harness(options = {}) {
  const messages = [], timers = [], calls = [];
  const module = {
    chooseCanvasTarget: () => JSON.stringify({ target: "svgInWorker" }),
    parseLens: (input) => { calls.push(["parse", input]); return snapshot(input); },
    renderSvg: (input, config) => { calls.push(["render", input, config]); return `<svg>${input}</svg>`; },
    applyParseLensEdit: (input, id, replacement) => {
      calls.push(["edit", input, id, replacement]);
      return { result: { updatedSource: replacement, previousSnippet: input, replacement }, snapshot: snapshot(replacement) };
    },
    workerHandleMessage: (json) => {
      calls.push(["protocol", JSON.parse(json)]);
      return JSON.stringify({ kind: "completed", requestId: JSON.parse(json).requestId, svg: "<svg/>" });
    },
    ...options.module,
  };
  const workerPath = process.env.FM_WORKER_SOURCE || path.join(__dirname, "fm-render.worker.js");
  const source = fs.readFileSync(workerPath, "utf8").replaceAll("import(", "globalThis.__import(");
  const context = {
    setTimeout: (callback) => timers.push(callback),
    __import: async () => {
      if (options.imported) await options.imported.promise;
      return module;
    },
    self: { postMessage: (message) => messages.push(plain(message)) },
  };
  vm.runInNewContext(source, context, { filename: workerPath });
  return {
    messages, calls, module,
    send: (message) => context.self.onmessage({ data: message }),
    async drain() {
      await tick();
      while (timers.length) { timers.shift()(); await tick(); }
    },
  };
}
const render = (requestId, input, extra = {}) => ({ kind: "sourceRender", requestId, input, ...extra });
const edit = (requestId, input, replacement) => ({ kind: "sourceEdit", requestId, input, elementId: "fm-node-a-0", replacement });

async function complete(worker, message) {
  const pending = worker.send(message);
  await worker.drain();
  await pending;
  return worker.messages.at(-1);
}

test("source render returns the matching SVG, Rust bindings and warnings together", async () => {
  const worker = harness();
  const response = await complete(worker, render(1, "A[雪 🦀]"));
  assert.equal(response.kind, "sourceRendered");
  assert.equal(response.requestId, 1);
  assert.equal(response.svg, "<svg>A[雪 🦀]</svg>");
  assert.deepEqual(response.snapshot.bindings, snapshot("A[雪 🦀]").bindings);
  assert.deepEqual(response.snapshot.parsed.warnings, ["a parser warning"]);
  assert.equal(response.snapshot.parsed.ir, undefined, "do not clone an unused IR to the UI");
  assert.deepEqual(worker.calls.map((call) => call[0]), ["parse", "render"]);
});

test("source edits delegate to Rust and retain its exact result and fresh snapshot", async () => {
  const worker = harness();
  const response = await complete(worker, edit(2, "old\r\n", "new 🦀\r\n"));
  assert.equal(response.kind, "sourceEdited");
  assert.deepEqual(worker.calls, [["edit", "old\r\n", "fm-node-a-0", "new 🦀\r\n"]]);
  assert.equal(response.response.result.updatedSource, "new 🦀\r\n");
  assert.equal(response.response.snapshot.bindings[0].snippet, "new 🦀\r\n");
});

test("a burst arriving during a cold import executes only the latest authoring request", async () => {
  const imported = deferred();
  const worker = harness({ imported });
  const pending = [worker.send(render(1, "old")), worker.send(edit(2, "old", "older edit")), worker.send(render(3, "latest"))];
  await tick();
  assert.deepEqual(worker.calls, []);
  imported.resolve();
  await worker.drain();
  await Promise.all(pending);
  assert.deepEqual(worker.calls.map((call) => call.slice(0, 2)), [["parse", "latest"], ["render", "latest"]]);
  assert.deepEqual(worker.messages.map(({ kind, requestId }) => ({ kind, requestId })), [
    { kind: "noReply", requestId: 1 }, { kind: "noReply", requestId: 2 }, { kind: "sourceRendered", requestId: 3 },
  ]);
});

test("queued source operations supersede one another after initialization too", async () => {
  const worker = harness();
  await worker.send({ kind: "init" });
  const first = worker.send(render(1, "old"));
  await tick();
  const second = worker.send(render(2, "new"));
  await worker.drain();
  await Promise.all([first, second]);
  assert.deepEqual(worker.calls.map((call) => call[1]), ["new", "new"]);
  assert.equal(worker.messages.filter((message) => message.requestId === 1).length, 1);
});

test("cancel during import immediately acknowledges and prevents subsequent parsing", async () => {
  const imported = deferred();
  const worker = harness({ imported });
  const request = worker.send(render(5, "cancel me"));
  await worker.send({ kind: "cancel", requestId: 5 });
  assert.deepEqual(worker.messages, [{ kind: "noReply", requestId: 5 }]);
  imported.resolve();
  await worker.drain();
  await request;
  assert.deepEqual(worker.calls, []);
  assert.equal(worker.messages.length, 1);
});

test("reusing a cancelled ID cannot revive old source or old edits", async () => {
  const worker = harness();
  const first = worker.send(edit(5, "old", "wrong"));
  await tick();
  await worker.send({ kind: "cancel", requestId: 5 });
  const second = worker.send(render(5, "new"));
  await worker.drain();
  await Promise.all([first, second]);
  assert.deepEqual(worker.calls.map((call) => call[1]), ["new", "new"]);
  assert.deepEqual(worker.messages.map((message) => message.kind), ["noReply", "sourceRendered"]);
});

test("reinitialization invalidates queued source work before changing configuration", async () => {
  const worker = harness();
  const pending = worker.send(render(9, "old"));
  await tick();
  await worker.send({ kind: "init", config: { theme: "forest" } });
  await worker.drain();
  await pending;
  assert.deepEqual(worker.calls, []);
  assert.equal(worker.messages.filter((message) => message.requestId === 9).length, 1);
  await complete(worker, render(10, "new"));
  assert.deepEqual(plain(worker.calls.at(-1)), ["render", "new", { theme: "forest" }]);
});

test("source rendering honors init configuration and explicit request overrides", async () => {
  const worker = harness();
  await worker.send({ kind: "init", config: { theme: "dark" } });
  await complete(worker, render(1, "A"));
  await complete(worker, render(2, "B", { configJson: '{"theme":"forest"}' }));
  assert.deepEqual(plain(worker.calls.filter((call) => call[0] === "render")), [
    ["render", "A", { theme: "dark" }], ["render", "B", { theme: "forest" }],
  ]);
});

test("invalid messages fail without superseding valid queued work or entering Rust", async () => {
  for (const invalid of [render(-1, "A"), render(1.5, "A"), render(Number.MAX_SAFE_INTEGER + 1, "A"),
    render(2, {}), { ...edit(3, "A", "B"), elementId: "" }, { ...edit(3, "A", "B"), replacement: 7 }]) {
    const worker = harness();
    const pending = worker.send(render(20, "valid"));
    await worker.send(invalid);
    assert.equal(worker.messages.at(-1).kind, "failed");
    await worker.drain();
    await pending;
    assert.equal(worker.messages.at(-1).kind, "sourceRendered");
    assert.deepEqual(worker.calls.map((call) => call[1]), ["valid", "valid"]);
  }
});

test("engine exceptions and malformed results fail explicitly and permit the next render", async () => {
  for (const method of ["parseLens", "renderSvg", "applyParseLensEdit"]) {
    const worker = harness();
    const original = worker.module[method];
    worker.module[method] = () => { throw new Error("engine failure"); };
    const response = await complete(worker, method === "applyParseLensEdit" ? edit(1, "A", "B") : render(1, "A"));
    assert.equal(response.kind, "failed");
    assert.match(response.reason, /engine failure/);
    worker.module[method] = original;
    assert.equal((await complete(worker, render(2, "healthy"))).kind, "sourceRendered");
  }
  for (const module of [{ parseLens: () => ({}) }, { renderSvg: () => null }, { applyParseLensEdit: () => ({}) }]) {
    const worker = harness({ module });
    const response = await complete(worker, module.applyParseLensEdit ? edit(1, "A", "B") : render(1, "A"));
    assert.equal(response.kind, "failed");
  }
});

test("a superseded initializer failure has one terminal reply per operation and can retry", async () => {
  const imported = deferred();
  const worker = harness({ imported });
  const a = worker.send(render(1, "old")), b = worker.send(render(2, "latest"));
  imported.reject(new Error("offline"));
  await Promise.all([a, b]);
  assert.deepEqual(worker.messages.map(({ kind, requestId }) => ({ kind, requestId })), [
    { kind: "noReply", requestId: 1 }, { kind: "failed", requestId: 2 },
  ]);
  imported.promise = Promise.resolve();
  assert.equal((await complete(worker, render(3, "retry"))).kind, "sourceRendered");
});

test("source authoring coexists with the original SVG protocol and advertises capability", async () => {
  const worker = harness();
  await worker.send({ kind: "init" });
  assert.equal(worker.messages.at(-1).sourceEditing, true);
  await complete(worker, { kind: "render", requestId: 5, input: "original protocol" });
  assert.equal(worker.messages.at(-1).kind, "completed");
  assert.equal((await complete(worker, render(6, "authoring"))).kind, "sourceRendered");
  const oldModule = harness({ module: { parseLens: undefined } });
  await oldModule.send({ kind: "init" });
  assert.equal(oldModule.messages.at(-1).sourceEditing, false);
});

const editorSource = fs.readFileSync(path.join(__dirname, "fm-source-editor.js"), "utf8");
const editorModule = import(`data:text/javascript;base64,${Buffer.from(editorSource).toString("base64")}`);
function mockTransport() {
  const instances = [];
  class WorkerClass {
    constructor(url, options) { this.url = url; this.options = options; this.messages = []; instances.push(this); }
    postMessage(message) {
      if (this.failPost) throw new Error("transport closed");
      this.messages.push(message);
    }
    terminate() { this.terminated = true; }
    emit(data) { this.onmessage({ data }); }
    ready() { this.emit({ kind: "ready", target: "svgInWorker", sourceEditing: true }); }
    finish(input = "A") {
      const request = this.messages.findLast((message) => message.kind === "sourceRender" || message.kind === "sourceEdit");
      this.emit(request.kind === "sourceRender"
        ? { kind: "sourceRendered", requestId: request.requestId, svg: `<svg>${input}</svg>`, snapshot: snapshot(input) }
        : { kind: "sourceEdited", requestId: request.requestId,
          response: { result: { updatedSource: input }, snapshot: snapshot(input) } });
    }
  }
  return { instances, WorkerClass, workerUrl: "https://example.invalid/web/fm-render.worker.js" };
}

test("client keeps only the newest request before readiness and matches reply IDs", async () => {
  const { createSourceWorkerClient } = await editorModule;
  const transport = mockTransport();
  const client = createSourceWorkerClient(transport);
  const worker = transport.instances[0];
  try {
    const first = client.renderSource("old");
    const cancelled = assert.rejects(first, { name: "AbortError" });
    const second = client.renderSource("new");
    await cancelled;
    assert.deepEqual(worker.messages.map((message) => message.kind), ["init"]);
    worker.ready();
    assert.equal(worker.messages.at(-1).input, "new");
    worker.emit({ kind: "sourceRendered", requestId: 1, svg: "obsolete", snapshot: snapshot("old") });
    worker.finish("new");
    assert.equal((await second).svg, "<svg>new</svg>");
    assert.equal(worker.options.type, "module");
  } finally { client.dispose(); }
});

test("client cancels sent operations, ignores late errors, and transports edits", async () => {
  const { createSourceWorkerClient } = await editorModule;
  const transport = mockTransport(), client = createSourceWorkerClient(transport), worker = transport.instances[0];
  try {
    worker.ready();
    const first = client.renderSource("A");
    const rejected = assert.rejects(first, { name: "AbortError" });
    client.cancel();
    await rejected;
    assert.equal(worker.messages.at(-1).kind, "cancel");
    const pending = client.editSource("A", "fm-node-a-0", "B");
    worker.emit({ kind: "failed", requestId: 1, reason: "late old failure" });
    worker.finish("B");
    assert.equal((await pending).result.updatedSource, "B");
  } finally { client.dispose(); }
});

test("client classifies engine errors separately and remains usable", async () => {
  const { createSourceWorkerClient } = await editorModule;
  const transport = mockTransport(), client = createSourceWorkerClient(transport), worker = transport.instances[0];
  try {
    worker.ready();
    const failed = client.renderSource("bad");
    worker.emit({ kind: "failed", requestId: 1, reason: "invalid diagram" });
    await assert.rejects(failed, (error) => error.name === "Error" && /invalid diagram/.test(error.message));
    const next = client.renderSource("good");
    worker.finish("good");
    assert.equal((await next).svg, "<svg>good</svg>");
    assert.notEqual(worker.terminated, true);
  } finally { client.dispose(); }
});

test("client fails and terminates on crash, decode failure, stale package and bad response", async () => {
  const { createSourceWorkerClient } = await editorModule;
  for (const trigger of [
    (worker) => worker.onerror({ message: "crashed" }),
    (worker) => worker.onmessageerror(),
    (worker) => worker.emit({ kind: "ready", sourceEditing: false }),
    (worker) => { worker.ready(); worker.emit({ kind: "sourceEdited", requestId: 1, response: {} }); },
    (worker) => { worker.failPost = true; worker.ready(); },
  ]) {
    const transport = mockTransport(), client = createSourceWorkerClient(transport), worker = transport.instances[0];
    const pending = client.renderSource("A");
    trigger(worker);
    await assert.rejects(pending, { name: "SourceWorkerUnavailable" });
    assert.equal(worker.terminated, true);
    await assert.rejects(client.renderSource("B"), { name: "SourceWorkerUnavailable" });
    client.dispose();
  }
});

test("client initialization and request deadlines settle pending callers", async () => {
  const { createSourceWorkerClient } = await editorModule;
  for (const ready of [false, true]) {
    const transport = mockTransport();
    const client = createSourceWorkerClient({ ...transport, initTimeoutMs: 10, requestTimeoutMs: 10 });
    const worker = transport.instances[0];
    if (ready) worker.ready();
    await assert.rejects(client.renderSource("A"), /timed out/);
    assert.equal(worker.terminated, true);
    client.dispose();
  }
});

test("client disposal settles startup and active requests without late callbacks", async () => {
  const { createSourceWorkerClient } = await editorModule;
  for (const ready of [false, true]) {
    const transport = mockTransport(), client = createSourceWorkerClient(transport), worker = transport.instances[0];
    if (ready) worker.ready();
    const pending = client.renderSource("A");
    client.dispose();
    await assert.rejects(pending, { name: "AbortError" });
    worker.ready();
    worker.emit({ kind: "sourceRendered", requestId: 1, svg: "late", snapshot: snapshot("A") });
    await assert.rejects(client.renderSource("B"), { name: "AbortError" });
    assert.equal(worker.terminated, true);
  }
});

test("backend renders and edits through the worker without loading main-thread WASM", async () => {
  const { createSourceEditorBackend } = await editorModule;
  const transport = mockTransport();
  let mainLoads = 0;
  const backend = createSourceEditorBackend({ ...transport, loadModule: () => { mainLoads += 1; throw new Error("must stay off-thread"); } });
  try {
    const worker = transport.instances[0];
    const renderPromise = backend.renderSource("A");
    worker.ready(); worker.finish("A");
    assert.equal((await renderPromise).svg, "<svg>A</svg>");
    const editPromise = backend.editSource("A", "fm-node-a-0", "B");
    worker.finish("B");
    assert.equal((await editPromise).result.updatedSource, "B");
    assert.equal(mainLoads, 0);
    assert.equal(backend.target, "source worker");
  } finally { backend.dispose(); }
});

test("backend falls back after transport failure, not after an engine failure", async () => {
  const { createSourceEditorBackend } = await editorModule;
  for (const transportFailure of [false, true]) {
    const transport = mockTransport();
    let mainLoads = 0;
    const backend = createSourceEditorBackend({ ...transport, config: { theme: "dark" }, loadModule: () => {
      mainLoads += 1;
      return { parseLens: snapshot, renderSvg: (input, config) => { assert.equal(config.theme, "dark"); return `<svg>${input}</svg>`; } };
    } });
    try {
      const worker = transport.instances[0], pending = backend.renderSource("A");
      worker.ready();
      if (transportFailure) {
        worker.onerror({ message: "crashed" });
        assert.equal((await pending).svg, "<svg>A</svg>");
        assert.match(backend.target, /main-thread.*synchronous/);
        assert.match(backend.fallbackReason, /crashed/);
        await backend.renderSource("B");
        assert.equal(mainLoads, 1);
      } else {
        worker.emit({ kind: "failed", requestId: 1, reason: "parse failed" });
        await assert.rejects(pending, /parse failed/);
        assert.equal(mainLoads, 0);
        assert.equal(backend.target, "source worker");
      }
    } finally { backend.dispose(); }
  }
});

test("fallback coalesces startup edits, retries failed imports, and honours disposal", async () => {
  const { createSourceEditorBackend } = await editorModule;
  const imported = deferred();
  let loads = 0;
  const inputs = [];
  const api = { parseLens: snapshot, renderSvg: (input) => { inputs.push(input); return "<svg/>"; } };
  const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: () => { loads += 1; return imported.promise; } });
  const first = backend.renderSource("old");
  const rejected = assert.rejects(first, { name: "AbortError" });
  const second = backend.renderSource("new");
  imported.resolve(api);
  await Promise.all([rejected, second]);
  assert.deepEqual(inputs, ["new"]);
  assert.equal(loads, 1);
  backend.dispose();
  await assert.rejects(backend.renderSource("closed"), { name: "AbortError" });

  let attempts = 0;
  const retry = createSourceEditorBackend({ WorkerClass: null, loadModule: () => {
    attempts += 1;
    if (attempts === 1) throw new Error("download failed");
    return api;
  } });
  try {
    await assert.rejects(retry.renderSource("A"), /download failed/);
    await retry.renderSource("B");
    assert.equal(attempts, 2);
  } finally { retry.dispose(); }
});

test("snapshot adoption and remote edits retain exact-splice and stale-selection checks", async () => {
  const { SourceEditSession } = await editorModule;
  const session = new SourceEditSession();
  session.setSnapshot("雪 🦀", snapshot("雪 🦀"));
  const selected = session.select("fm-node-a-0");
  assert.equal(session.replaceWithResponse(selected, "新", { result: { updatedSource: "新" }, snapshot: snapshot("新") }), "新");
  assert.throws(() => session.replaceWithResponse(selected, "old", {}), /stale/);
  const current = session.select("fm-node-a-0");
  assert.throws(() => session.replaceWithResponse(current, "new", { result: { updatedSource: "wrong" } }), /outside/);
  assert.equal(session.source, "新");
  assert.throws(() => session.setSnapshot("latest", {}), /snapshot/);
  assert.deepEqual(session.bindings, []);
  assert.throws(() => session.replaceWithResponse(current, "new", {}), /stale/);
});

// Real threads execute the production worker, with only the browser Worker adapter and WASM
// exports supplied by fixtures. Atomics.wait deliberately blocks the worker until the parent
// releases it; a main-thread implementation would deadlock instead of passing this test.
const { Worker: NodeWorker } = require("node:worker_threads");
const { pathToFileURL } = require("node:url");
const threadFixture = `
import { workerData, threadId } from 'node:worker_threads';
if (threadId === 0) throw new Error('WASM fixture ran on main thread');
const gate = new Int32Array(workerData.gate);
const snapshot = input => ({bindings: [{elementId:'fm-node-a-0', kind:'Node',
  snippet: input, textRange: {startByte:0, endByte:Buffer.byteLength(input)}}], parsed:{warnings:[]}});
function block(input) {
  if (!input.includes('BLOCK_WORKER')) return;
  Atomics.store(gate, 0, 1);
  self.postMessage({kind:'fixtureStarted', threadId});
  if (Atomics.wait(gate, 1, 0, 5000) === 'timed-out') throw new Error('test gate was never released');
  Atomics.store(gate, 0, 2);
}
export default async function init() {}
export function chooseCanvasTarget() { return JSON.stringify({target:'svgInWorker'}); }
export function parseLens(input) {
  if (input === 'PARSE_ERROR') throw new Error('threaded parse error');
  return snapshot(input);
}
export function renderSvg(input) { block(input); return '<svg>' + input + '</svg>'; }
export function applyParseLensEdit(input, id, replacement) {
  if (id !== 'fm-node-a-0') throw new Error('unknown element');
  block(replacement);
  return {result:{updatedSource:replacement}, snapshot:snapshot(replacement)};
}
export function workerHandleMessage() { return null; }
`;

function threadedTransport() {
  const gate = new Int32Array(new SharedArrayBuffer(8));
  const started = deferred();
  const instances = [];
  const bootstrap = `
    const {parentPort, workerData} = require('node:worker_threads');
    globalThis.self = {postMessage: message => parentPort.postMessage(message)};
    import(workerData.workerUrl).then(() => {
      parentPort.on('message', data => self.onmessage({data}));
    });
  `;
  class ThreadWorker {
    constructor() {
      this.thread = new NodeWorker(bootstrap, { eval: true, workerData: {
        workerUrl: pathToFileURL(path.join(__dirname, "fm-render.worker.js")).href, gate: gate.buffer,
      } });
      this.thread.on("message", (data) => {
        if (data.kind === "fixtureStarted") started.resolve(data);
        this.onmessage?.({ data });
      });
      this.thread.on("error", (error) => this.onerror?.({ message: error.message }));
      this.thread.on("messageerror", () => this.onmessageerror?.());
      instances.push(this);
    }
    postMessage(message) { this.thread.postMessage(message); }
    terminate() { this.stopped = this.thread.terminate(); }
  }
  return {
    gate, started, instances, WorkerClass: ThreadWorker, workerUrl: "unused-by-node-adapter",
    moduleUrl: `data:text/javascript;base64,${Buffer.from(threadFixture).toString("base64")}`,
    release() { Atomics.store(gate, 1, 1); Atomics.notify(gate, 1); },
  };
}

test("real worker CPU cannot block the parent event loop or publish an obsolete render", { timeout: 10000 }, async () => {
  const { createSourceEditorBackend } = await editorModule;
  const transport = threadedTransport();
  let mainLoads = 0, heartbeat = 0;
  const backend = createSourceEditorBackend({ ...transport, loadModule() { mainLoads++; throw new Error("main WASM must not run"); } });
  const clock = setInterval(() => heartbeat++, 2);
  try {
    const old = backend.renderSource("BLOCK_WORKER old source");
    const rejected = assert.rejects(old, { name: "AbortError" });
    const start = await transport.started.promise;
    assert.ok(start.threadId > 0, "the renderer must run in a separate thread");
    const before = heartbeat;
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.ok(heartbeat > before, "parent timers must run while the renderer is blocked");
    assert.equal(Atomics.load(transport.gate, 0), 1, "the synchronous worker is still blocked");
    const latest = backend.renderSource("latest source 雪 🦀");
    await rejected;
    transport.release();
    const result = await latest;
    assert.equal(result.svg, "<svg>latest source 雪 🦀</svg>");
    assert.equal(result.snapshot.bindings[0].snippet, "latest source 雪 🦀");
    assert.equal(mainLoads, 0);
    assert.equal(backend.target, "source worker");
  } finally {
    clearInterval(clock);
    transport.release();
    backend.dispose();
    await transport.instances[0]?.stopped;
  }
});

test("real worker edits round-trip through source validation and cannot overwrite a newer revision", { timeout: 10000 }, async () => {
  const { createSourceEditorBackend, SourceEditSession } = await editorModule;
  const transport = threadedTransport();
  const backend = createSourceEditorBackend({ ...transport, loadModule() { throw new Error("no main-thread fallback"); } });
  try {
    const source = "original 雪\r\n";
    const rendered = await backend.renderSource(source);
    const session = new SourceEditSession();
    session.setSnapshot(source, rendered.snapshot);
    const selected = session.select("fm-node-a-0");
    const response = await backend.editSource(source, selected.elementId, "updated 🦀\r\n");
    assert.equal(session.replaceWithResponse(selected, "updated 🦀\r\n", response), "updated 🦀\r\n");
    const nextSelection = session.select("fm-node-a-0");
    const old = backend.editSource(session.source, nextSelection.elementId, "BLOCK_WORKER old edit");
    const rejected = assert.rejects(old, { name: "AbortError" });
    await transport.started.promise;
    const typed = backend.renderSource("typed after edit");
    await rejected;
    transport.release();
    const latest = await typed;
    session.setSnapshot("typed after edit", latest.snapshot);
    assert.throws(() => session.replaceWithResponse(nextSelection, "old", {}), /stale/);
    assert.equal(session.source, "typed after edit");
    await assert.rejects(backend.renderSource("PARSE_ERROR"), /threaded parse error/);
    assert.equal(backend.target, "source worker", "an engine error must not move parsing to the UI thread");
    assert.equal((await backend.renderSource("recovered")).svg, "<svg>recovered</svg>");
  } finally {
    transport.release();
    backend.dispose();
    await transport.instances[0]?.stopped;
  }
});

test("disposing during synchronous worker execution terminates it and settles the caller", { timeout: 10000 }, async () => {
  const { createSourceEditorBackend } = await editorModule;
  const transport = threadedTransport();
  const backend = createSourceEditorBackend({ ...transport, loadModule() { throw new Error("no fallback after dispose"); } });
  try {
    const pending = backend.renderSource("BLOCK_WORKER until disposed");
    const rejected = assert.rejects(pending, { name: "AbortError" });
    await transport.started.promise;
    backend.dispose();
    await rejected;
    await transport.instances[0].stopped;
    assert.equal(Atomics.load(transport.gate, 0), 1, "termination stopped execution before releasing the gate");
    await assert.rejects(backend.renderSource("late"), { name: "AbortError" });
  } finally {
    transport.release();
    backend.dispose();
  }
});
