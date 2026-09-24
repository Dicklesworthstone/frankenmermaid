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
