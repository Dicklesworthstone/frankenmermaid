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

test("SVG reinitialization invalidates queued work instead of applying the new config to it", async () => {
  const worker = host();
  await worker.init({ theme: "old" });
  const stale = worker.send({ kind: "render", requestId: 1, input: "old document" });
  await microtasks();
  await worker.init({ theme: "new" });
  await worker.drain();
  await stale;
  assert.deepEqual(worker.inputs(), []);
  assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
  await complete(worker, { kind: "render", requestId: 2, input: "new document" });
  assert.deepEqual(worker.inputs(), ["new document"]);
  assert.deepEqual(JSON.parse(worker.requests[0].configJson), { theme: "new" });
});

test("an old queued SVG cannot overwrite a newly transferred canvas", async () => {
  const worker = host();
  await worker.init();
  const stale = worker.send({ kind: "render", requestId: 1, input: "old SVG" });
  await microtasks();
  await worker.init(undefined, true);
  await worker.drain();
  await stale;
  assert.deepEqual(worker.inputs(), []);
  await complete(worker, { kind: "render", requestId: 2, input: "new canvas" });
  assert.deepEqual(worker.canvasInputs, [{ id: 1, input: "new canvas" }]);
});

test("reinitialization during cold import invalidates earlier renders before backend selection", async () => {
  const loaded = deferred();
  const worker = host({ importModule: async (module) => { await loaded.promise; return module; } });
  const stale = worker.send({ kind: "render", requestId: 1, input: "before init" });
  await microtasks();
  const init = worker.init({ theme: "new" });
  loaded.resolve();
  await worker.drain();
  await Promise.all([stale, init]);
  assert.deepEqual(worker.inputs(), []);
  assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
});

test("renders arriving after a cold init are kept while renders from the previous session are dropped", async () => {
  const loaded = deferred();
  const worker = host({ importModule: async (module) => { await loaded.promise; return module; } });
  const stale = worker.send({ kind: "render", requestId: 1, input: "before init" });
  const init = worker.init({ theme: "fresh" }, true);
  const current = worker.send({ kind: "render", requestId: 2, input: "after init" });
  loaded.resolve();
  await worker.drain();
  await Promise.all([stale, init, current]);
  assert.deepEqual(worker.canvasInputs, [{ id: 1, input: "after init" }]);
  assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
  assert.equal(worker.replies(2).at(-1).kind, "completed");
});

test("failed cold imports settle old-session renders as superseded, not duplicate failures", async () => {
  const loaded = deferred();
  const worker = host({ importModule: async (module, attempt) => {
    if (attempt === 1) await loaded.promise;
    return module;
  } });
  const stale = worker.send({ kind: "render", requestId: 1, input: "old session" });
  const init = worker.init();
  loaded.reject(new Error("network failed"));
  await Promise.all([stale, init]);
  assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
  assert.equal(worker.messages.filter((message) => message.kind === "failed").length, 1);
  await worker.init();
  await complete(worker, { kind: "render", requestId: 2, input: "retry" });
  assert.deepEqual(worker.inputs(), ["retry"]);
});

test("canvas to SVG mode changes release the old renderer and never draw queued old pixels", async () => {
  const worker = host();
  await worker.init(undefined, true);
  const stale = worker.send({ kind: "render", requestId: 1, input: "old pixels" });
  await microtasks();
  await worker.init();
  await worker.drain();
  await stale;
  assert.deepEqual(worker.canvasInputs, []);
  assert.deepEqual(worker.releases, [1]);
  assert.deepEqual(worker.replies(1), [{ kind: "noReply", requestId: 1 }]);
  await complete(worker, { kind: "render", requestId: 2, input: "new SVG" });
  assert.deepEqual(worker.inputs(), ["new SVG"]);
});

test("SVG session invalidation does not invent failures for malformed protocol messages", async () => {
  const loaded = deferred();
  const worker = host({ importModule: async (module) => { await loaded.promise; return module; } });
  const invalid = worker.send({ kind: "render", requestId: 1, input: null });
  const init = worker.init();
  loaded.resolve();
  await worker.drain();
  await Promise.all([invalid, init]);
  assert.equal(worker.replies(1).at(-1).kind, "failed");
  assert.deepEqual(worker.requests, [{ kind: "render", requestId: 1, input: null }]);
});

test("reinitialization invalidates source-authoring work independently of SVG render queues", async () => {
  const calls = [];
  const worker = host({ module: {
    parseLens: (input) => { calls.push(input); return { bindings: [], parsed: { warnings: [] } }; },
    renderSvg: (input) => `<svg>${input}</svg>`,
  } });
  await worker.init();
  const source = worker.send({ kind: "sourceRender", requestId: 10, input: "source preview" });
  const svg = worker.send({ kind: "render", requestId: 11, input: "diagram preview" });
  await microtasks();
  await worker.init();
  await worker.drain();
  await Promise.all([source, svg]);
  assert.deepEqual(calls, []);
  assert.deepEqual(worker.inputs(), []);
  assert.deepEqual(worker.replies(10), [{ kind: "noReply", requestId: 10 }]);
  assert.deepEqual(worker.replies(11), [{ kind: "noReply", requestId: 11 }]);
});

// Real message-port/import/microtask delivery, with a controlled module-readiness barrier.
// These are Node worker threads, not a browser or an actual Rust/WASM rendering benchmark.
async function threadedHost(t) {
  const { Worker } = require("node:worker_threads");
  const moduleUrl = `data:text/javascript,${encodeURIComponent(`
    export default async function () {
      self.postMessage({ kind: "testInitializing" });
      await globalThis.__testModuleReady;
    }
    export function chooseCanvasTarget() {
      return JSON.stringify({ target: "svgInWorker" });
    }
    export function workerHandleMessage(json) {
      const request = JSON.parse(json);
      self.postMessage({ kind: "testEntered", request });
      if (request.kind === "cancel") return null;
      return JSON.stringify({ kind: "completed", requestId: request.requestId, svg: "<svg/>" });
    }
  `)}`;
  const worker = new Worker(`
    const { parentPort, workerData } = require("node:worker_threads");
    let release;
    globalThis.__testModuleReady = new Promise((resolve) => { release = resolve; });
    globalThis.self = { postMessage: (message) => parentPort.postMessage(message) };
    require(workerData.workerPath);
    parentPort.on("message", (message) => {
      if (message.kind === "testRelease") release();
      else void self.onmessage({ data: message });
    });
    self.postMessage({ kind: "testHostReady" });
  `, { eval: true, workerData: { workerPath: path.join(__dirname, "fm-render.worker.js") } });
  const messages = [];
  const waiters = new Set();
  let failure = null;
  function fail(error) {
    failure = error;
    for (const waiter of [...waiters]) waiter.reject(error);
  }
  worker.on("message", (message) => {
    messages.push(message);
    for (const waiter of [...waiters]) {
      if (waiter.matches(message)) waiter.resolve(message);
    }
  });
  worker.on("error", fail);
  worker.on("exit", (code) => { if (waiters.size) fail(new Error(`worker exited early (${code})`)); });
  t.after(async () => {
    fail(new Error("worker test disposed"));
    await worker.terminate();
  });
  function wait(matches) {
    const found = messages.find(matches);
    if (found) return Promise.resolve(found);
    if (failure) return Promise.reject(failure);
    return new Promise((resolve, reject) => {
      let timer;
      const waiter = {
        matches,
        resolve(message) { clearTimeout(timer); waiters.delete(waiter); resolve(message); },
        reject(error) { clearTimeout(timer); waiters.delete(waiter); reject(error); },
      };
      waiters.add(waiter);
      timer = setTimeout(() => waiter.reject(new Error("worker response timed out")), 5000);
    });
  }
  await wait((message) => message.kind === "testHostReady");
  worker.postMessage({ kind: "init", moduleUrl });
  await wait((message) => message.kind === "testInitializing");
  return {
    messages,
    send: (message) => worker.postMessage(message),
    release: () => worker.postMessage({ kind: "testRelease" }),
    wait,
    inputs: () => messages.filter((message) => message.kind === "testEntered" &&
      message.request.kind === "render").map((message) => message.request.input),
  };
}

test("real worker port: a cold burst settles every request but enters WASM only for the newest", async (t) => {
  const worker = await threadedHost(t);
  for (let id = 1; id <= 100; id += 1) {
    worker.send({ kind: "render", requestId: id, input: `diagram ${id}` });
  }
  worker.release();
  await worker.wait((message) => message.kind === "completed" && message.requestId === 100);
  assert.deepEqual(worker.inputs(), ["diagram 100"]);
  for (let id = 1; id < 100; id += 1) {
    assert.deepEqual(worker.messages.filter((message) => message.requestId === id), [
      { kind: "noReply", requestId: id },
    ]);
  }
});

test("real worker port: cancellation posted during module loading prevents rendering", async (t) => {
  const worker = await threadedHost(t);
  worker.send({ kind: "render", requestId: 1, input: "cancelled" });
  worker.send({ kind: "cancel", requestId: 1 });
  worker.release();
  await worker.wait((message) => message.kind === "noReply" && message.requestId === 1);
  assert.deepEqual(worker.inputs(), []);
});

test("real worker port: cold reinitialization retains only the new session and its config", async (t) => {
  const worker = await threadedHost(t);
  worker.send({ kind: "render", requestId: 1, input: "old document" });
  worker.send({ kind: "init", config: { theme: "fresh" } });
  worker.send({ kind: "render", requestId: 2, input: "new document" });
  worker.release();
  await worker.wait((message) => message.kind === "completed" && message.requestId === 2);
  assert.deepEqual(worker.inputs(), ["new document"]);
  assert.deepEqual(worker.messages.filter((message) => message.requestId === 1), [
    { kind: "noReply", requestId: 1 },
  ]);
  const entered = worker.messages.find((message) => message.kind === "testEntered");
  assert.deepEqual(JSON.parse(entered.request.configJson), { theme: "fresh" });
});

test("a cancel without an ID cannot accidentally match an absent pending operation", async () => {
  const worker = host();
  await worker.init();
  await complete(worker, { kind: "cancel" });
  assert.deepEqual(worker.requests, [{ kind: "cancel" }], "Rust must receive malformed protocol messages");
  assert.deepEqual(worker.messages.at(-1), { kind: "noReply" });
});
