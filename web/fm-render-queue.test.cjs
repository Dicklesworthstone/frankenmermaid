"use strict";

// Execute the actual worker host. Only module transport, WASM exports and the timer clock
// are fixtures; no native rendering or browser performance claim is made by these tests.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

async function microtasks() {
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
}

function host(options = {}) {
  const workerPath = path.join(__dirname, "fm-render.worker.js");
  const source = fs.readFileSync(workerPath, "utf8").replaceAll("import(", "globalThis.__import(");
  const timers = [];
  const messages = [];
  const requests = [];
  const canvasInputs = [];
  const releases = [];
  let imports = 0;
  let canvasCount = 0;
  const module = {
    chooseCanvasTarget: (json) => JSON.stringify({
      target: JSON.parse(json).canvasTransferred ? "offscreenInWorker" : "svgInWorker",
    }),
    Diagram: {
      fromOffscreenCanvas: () => {
        const id = ++canvasCount;
        return {
          render: (input) => { canvasInputs.push({ id, input }); return { nodeCount: 1 }; },
          free: () => releases.push(id),
        };
      },
    },
    workerHandleMessage: (json) => {
      const request = JSON.parse(json);
      requests.push(request);
      if (options.handle) return options.handle(request);
      if (request.kind === "cancel") return null;
      if (request.kind !== "render" || typeof request.input !== "string") {
        return JSON.stringify({ kind: "failed", requestId: request.requestId, reason: "invalid request" });
      }
      return JSON.stringify({ kind: "completed", requestId: request.requestId,
        svg: `<svg>${request.input}</svg>`, diagnostics: [{ severity: "warning", message: "recovered" }],
        timings: { parseMs: 1, layoutMs: 2, renderMs: 3 }, nodeCount: 1, edgeCount: 0 });
    },
    ...options.module,
  };
  const context = {
    JSON, Number, Promise,
    setTimeout: (callback) => { timers.push(callback); return timers.length; },
    self: { postMessage: (message) => messages.push(JSON.parse(JSON.stringify(message))) },
    __import: async () => {
      imports += 1;
      if (options.importModule) return options.importModule(module, imports);
      return module;
    },
  };
  context.globalThis = context;
  vm.runInNewContext(source, context, { filename: workerPath });
  return {
    messages, requests, canvasInputs, releases, timers,
    imports: () => imports,
    send: (message) => context.self.onmessage({ data: message }),
    init: (config, offscreen = false) => context.self.onmessage({ data: {
      kind: "init", config, canvas: offscreen ? {} : undefined,
      capabilities: { worker: true, offscreenCanvas: offscreen, canvasTransferred: offscreen },
    } }),
    async drain() {
      for (let turn = 0; turn < 1000; turn += 1) {
        await microtasks();
        if (!timers.length) return;
        timers.shift()();
      }
      throw new Error("worker did not drain its timer queue");
    },
    replies: (requestId) => messages.filter((message) => message.requestId === requestId),
    inputs: () => requests.filter((request) => request.kind === "render").map((request) => request.input),
  };
}

async function complete(worker, message) {
  const pending = worker.send(message);
  await worker.drain();
  await pending;
}

test("a burst of 100 queued SVG edits enters synchronous WASM only once", async () => {
  const worker = host();
  await worker.init();
  const pending = [];
  for (let requestId = 1; requestId <= 100; requestId += 1) {
    pending.push(worker.send({ kind: "render", requestId, input: `diagram ${requestId}` }));
    await microtasks();
  }
  await worker.drain();
  await Promise.all(pending);
  assert.deepEqual(worker.inputs(), ["diagram 100"]);
  for (let id = 1; id < 100; id += 1) {
    assert.deepEqual(worker.replies(id), [{ kind: "noReply", requestId: id }]);
  }
  assert.equal(worker.replies(100).length, 1);
  assert.equal(worker.replies(100)[0].kind, "completed");
});

test("cancellation before the yield prevents SVG parse, layout and render entry", async () => {
  const worker = host();
  await worker.init();
  const render = worker.send({ kind: "render", requestId: 1, input: "cancelled" });
  await microtasks();
  const cancel = worker.send({ kind: "cancel", requestId: 1 });
  await worker.drain();
  await Promise.all([render, cancel]);
  assert.deepEqual(worker.inputs(), []);
  assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
});

test("cancelling an old request cannot cancel a newer queued SVG render", async () => {
  const worker = host();
  await worker.init();
  const first = worker.send({ kind: "render", requestId: 1, input: "old" });
  await microtasks();
  const second = worker.send({ kind: "render", requestId: 2, input: "current" });
  await microtasks();
  const cancel = worker.send({ kind: "cancel", requestId: 1 });
  await worker.drain();
  await Promise.all([first, second, cancel]);
  assert.deepEqual(worker.inputs(), ["current"]);
  assert.equal(worker.replies(2).at(-1).kind, "completed");
});

test("lower queued request IDs cannot displace a newer valid render", async () => {
  const worker = host();
  await worker.init();
  const newer = worker.send({ kind: "render", requestId: 20, input: "current" });
  await microtasks();
  const older = worker.send({ kind: "render", requestId: 19, input: "late obsolete request" });
  await worker.drain();
  await Promise.all([newer, older]);
  assert.deepEqual(worker.inputs(), ["current"]);
  assert.deepEqual(worker.replies(19), [{ kind: "noReply", requestId: 19 }]);
});

test("operation identity prevents ID reuse from reviving a cancelled source", async () => {
  const worker = host();
  await worker.init();
  const old = worker.send({ kind: "render", requestId: 7, input: "cancelled" });
  await microtasks();
  const cancel = worker.send({ kind: "cancel", requestId: 7 });
  await microtasks();
  const replacement = worker.send({ kind: "render", requestId: 7, input: "replacement" });
  await worker.drain();
  await Promise.all([old, cancel, replacement]);
  assert.deepEqual(worker.inputs(), ["replacement"]);
  assert.deepEqual(worker.replies(7).map((message) => message.kind), ["noReply", "completed"]);
});

test("SVG edits queued during cold module initialization coalesce", async () => {
  const initialized = deferred();
  const worker = host({ module: { default: () => initialized.promise } });
  const init = worker.init();
  const pending = [1, 2, 3].map((requestId) => worker.send({
    kind: "render", requestId, input: `cold ${requestId}`,
  }));
  await microtasks();
  assert.equal(worker.imports(), 1);
  assert.deepEqual(worker.inputs(), []);
  initialized.resolve();
  await worker.drain();
  await Promise.all([init, ...pending]);
  assert.deepEqual(worker.inputs(), ["cold 3"]);
});

test("a cold-start cancellation prevents the cancelled render entering WASM", async () => {
  const initialized = deferred();
  const worker = host({ module: { default: () => initialized.promise } });
  const init = worker.init();
  const render = worker.send({ kind: "render", requestId: 4, input: "cancelled cold" });
  const cancel = worker.send({ kind: "cancel", requestId: 4 });
  initialized.resolve();
  await worker.drain();
  await Promise.all([init, render, cancel]);
  assert.deepEqual(worker.inputs(), []);
  assert.deepEqual(worker.replies(4), [{ kind: "noReply", requestId: 4 }]);
});

test("render exceptions release the gate and permit a subsequent successful render", async () => {
  const worker = host({ handle: (request) => {
    if (request.input === "invalid") throw new Error("parse failed");
    return JSON.stringify({ kind: "completed", requestId: request.requestId, svg: "<svg/>" });
  } });
  await worker.init();
  await complete(worker, { kind: "render", requestId: 1, input: "invalid" });
  await complete(worker, { kind: "render", requestId: 2, input: "valid" });
  assert.deepEqual(worker.replies(1), [{ kind: "failed", requestId: 1, reason: "parse failed" }]);
  assert.equal(worker.replies(2).at(-1).kind, "completed");
});

test("coalescing preserves the live response diagnostics, timings and graph counts", async () => {
  const worker = host();
  await worker.init();
  await complete(worker, { kind: "render", requestId: 1, input: "live" });
  assert.deepEqual(worker.replies(1), [{
    kind: "completed", requestId: 1, svg: "<svg>live</svg>",
    diagnostics: [{ severity: "warning", message: "recovered" }],
    timings: { parseMs: 1, layoutMs: 2, renderMs: 3 }, nodeCount: 1, edgeCount: 0,
  }]);
});

test("Rust no-response results remain noReply, never synthetic render completions", async () => {
  for (const result of [null, undefined]) {
    const worker = host({ handle: () => result });
    await worker.init();
    await complete(worker, { kind: "render", requestId: 1, input: "stale in Rust" });
    assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
  }
});

test("initialization config survives SVG fallback and explicit render config wins", async () => {
  const worker = host({ module: { Diagram: {
    fromOffscreenCanvas: () => { throw new Error("context unavailable"); },
  } } });
  await worker.init({ theme: "dark" }, true);
  assert.equal(worker.messages.at(-1).target, "svgInWorker");
  await complete(worker, { kind: "render", requestId: 1, input: "inherited" });
  await complete(worker, { kind: "render", requestId: 2, input: "override", configJson: "{}" });
  assert.deepEqual(JSON.parse(worker.requests[0].configJson), { theme: "dark" });
  assert.equal(worker.requests[1].configJson, "{}");
});

test("invalid protocol messages do not evict a valid queued render", async () => {
  const worker = host();
  await worker.init();
  const live = worker.send({ kind: "render", requestId: 1, input: "keep me" });
  await microtasks();
  const invalid = worker.send({ kind: "render", requestId: 2, input: null });
  await worker.drain();
  await Promise.all([live, invalid]);
  assert.equal(worker.replies(1).at(-1).kind, "completed");
  assert.equal(worker.replies(2).at(-1).kind, "failed");
});

test("unmatched cancels are still forwarded unchanged to the Rust protocol", async () => {
  const worker = host();
  await worker.init({ theme: "dark" });
  await complete(worker, { kind: "cancel", requestId: 9 });
  assert.deepEqual(worker.requests, [{ kind: "cancel", requestId: 9 }]);
  assert.deepEqual(worker.replies(9), [{ kind: "noReply", requestId: 9 }]);
});

test("offscreen pre-draw supersession still draws only the current canvas request", async () => {
  const worker = host();
  await worker.init(undefined, true);
  const old = worker.send({ kind: "render", requestId: 1, input: "old" });
  await microtasks();
  const current = worker.send({ kind: "render", requestId: 2, input: "current" });
  await worker.drain();
  await Promise.all([old, current]);
  assert.deepEqual(worker.canvasInputs, [{ id: 1, input: "current" }]);
  assert.deepEqual(worker.inputs(), []);
});

test("completed renders are not retained as cancellable queued work", async () => {
  const worker = host();
  await worker.init();
  await complete(worker, { kind: "render", requestId: 1, input: "finished" });
  await complete(worker, { kind: "cancel", requestId: 1 });
  assert.deepEqual(worker.requests.map((request) => request.kind), ["render", "cancel"]);
});
