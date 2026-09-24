"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { pathToFileURL } = require("node:url");
const vm = require("node:vm");

function loadWorker(diagram, options = {}) {
  const workerPath = path.join(__dirname, "fm-render.worker.js");
  // Exercise the real initialization path, substituting only the module transport.
  const source = fs
    .readFileSync(workerPath, "utf8")
    .replaceAll("import(", "globalThis.__import(");
  const timers = [];
  const messages = [];
  const imports = [];
  const module = {
    chooseCanvasTarget: () => JSON.stringify({ target: "offscreenInWorker" }),
    Diagram: {
      fromOffscreenCanvas: () => diagram,
    },
    ...options.module,
  };
  const context = {
    JSON,
    Number,
    Promise,
    setTimeout: (callback) => {
      timers.push(callback);
      return timers.length;
    },
    self: {
      postMessage: (message) => messages.push(message),
    },
    __import: async (url) => {
      imports.push(url);
      return options.importModule ? options.importModule(url, module) : module;
    },
  };
  context.globalThis = context;
  vm.runInNewContext(source, context, { filename: workerPath });
  return { messages, onMessage: context.self.onmessage, timers, imports };
}

async function flushMicrotasks() {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
}

async function initializeOffscreenWorker(worker, moduleUrl = "../pkg/frankenmermaid.js") {
  await worker.onMessage({
    data: {
      kind: "init",
      capabilities: { canvasTransferred: true, offscreenCanvas: true, worker: true },
      canvas: {},
      moduleUrl,
    },
  });
}

function plain(value) {
  return JSON.parse(JSON.stringify(value));
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

test("playground resolves the worker module URL to the shipped root package", () => {
  const playgroundPath = path.join(__dirname, "playground.html");
  const source = fs.readFileSync(playgroundPath, "utf8");
  const expectedPackage = path.join(__dirname, "..", "pkg", "frankenmermaid.js");

  assert.match(source, /new URL\("\.\.\/pkg\/frankenmermaid\.js", import\.meta\.url\)/);
  assert.equal(
    fs.existsSync(expectedPackage),
    true,
    "the playground must target a shipped package",
  );

  const resolved = new URL("../pkg/frankenmermaid.js", pathToFileURL(playgroundPath));
  assert.equal(path.normalize(resolved.pathname), expectedPackage);
});

test("offscreen worker skips a queued render superseded before synchronous canvas drawing", async () => {
  const inputs = [];
  const worker = loadWorker({
    render: (input) => {
      inputs.push(input);
      return { rendered: input };
    },
  });
  await initializeOffscreenWorker(worker);

  const first = worker.onMessage({ data: { kind: "render", requestId: 1, input: "stale" } });
  await flushMicrotasks();
  const second = worker.onMessage({ data: { kind: "render", requestId: 2, input: "fresh" } });
  await flushMicrotasks();

  worker.timers.shift()();
  await flushMicrotasks();
  worker.timers.shift()();
  await Promise.all([first, second]);

  assert.deepEqual(inputs, ["fresh"]);
  assert.deepEqual(plain(worker.messages.at(-2)), { kind: "noReply", requestId: 1 });
  assert.deepEqual(plain(worker.messages.at(-1)), {
    kind: "completed",
    requestId: 2,
    target: "offscreenInWorker",
    stats: { rendered: "fresh" },
  });
});

test("offscreen worker cancels a queued render before it reaches the canvas", async () => {
  const inputs = [];
  const worker = loadWorker({
    render: (input) => {
      inputs.push(input);
      return { rendered: input };
    },
  });
  await initializeOffscreenWorker(worker);

  const render = worker.onMessage({ data: { kind: "render", requestId: 3, input: "cancelled" } });
  await flushMicrotasks();
  await worker.onMessage({ data: { kind: "cancel", requestId: 3 } });
  worker.timers.shift()();
  await render;

  assert.deepEqual(inputs, []);
  assert.deepEqual(plain(worker.messages.at(-1)), { kind: "noReply", requestId: 3 });
});

test("concurrent messages wait for one complete WASM initialization", async () => {
  const initialized = deferred();
  const inputs = [];
  let initializations = 0;
  let ready = false;
  const worker = loadWorker({ render: (input) => inputs.push(input) }, {
    module: {
      default: async () => {
        initializations += 1;
        await initialized.promise;
        ready = true;
      },
      chooseCanvasTarget: () => {
        assert.equal(ready, true, "WASM exports must not run before initialization");
        return JSON.stringify({ target: "offscreenInWorker" });
      },
    },
  });
  const init = initializeOffscreenWorker(worker);
  await flushMicrotasks();
  const render = worker.onMessage({ data: { kind: "render", requestId: 4, input: "cold start" } });
  await flushMicrotasks();

  assert.equal(initializations, 1);
  assert.deepEqual(worker.imports, ["../pkg/frankenmermaid.js"]);
  assert.deepEqual(worker.messages, []);
  assert.equal(worker.timers.length, 0, "render must wait for initialization, not merely import");

  initialized.resolve();
  await init;
  await flushMicrotasks();
  worker.timers.shift()();
  await render;
  assert.deepEqual(inputs, ["cold start"]);
  assert.equal(worker.messages.at(-1).kind, "completed");
});

test("concurrent cold starts share the in-flight module import", async () => {
  const imported = deferred();
  let initializations = 0;
  const worker = loadWorker({}, {
    importModule: async (_url, module) => {
      await imported.promise;
      return module;
    },
    module: { default: () => { initializations += 1; } },
  });
  const first = worker.onMessage({ data: { kind: "init" } });
  const second = worker.onMessage({ data: { kind: "init" } });
  await flushMicrotasks();
  assert.equal(worker.imports.length, 1);
  imported.resolve();
  await Promise.all([first, second]);
  assert.equal(initializations, 1);
  assert.deepEqual(worker.messages.map((message) => message.kind), ["ready", "ready"]);
});

test("failed WASM initialization rejects all waiters and permits a clean retry", async () => {
  const initialized = deferred();
  let initializations = 0;
  const worker = loadWorker({}, {
    module: {
      default: async () => {
        initializations += 1;
        if (initializations === 1) await initialized.promise;
      },
    },
  });
  const first = worker.onMessage({ data: { kind: "init", requestId: 5 } });
  await flushMicrotasks();
  const second = worker.onMessage({ data: { kind: "init", requestId: 6 } });
  await flushMicrotasks();
  initialized.reject(new Error("WASM download interrupted"));
  await Promise.all([first, second]);
  assert.deepEqual(plain(worker.messages), [
    { kind: "failed", requestId: 5, reason: "WASM download interrupted" },
    { kind: "failed", requestId: 6, reason: "WASM download interrupted" },
  ]);

  await initializeOffscreenWorker(worker);
  assert.equal(initializations, 2, "a rejected initializer must not poison the module cache");
  assert.equal(worker.imports.length, 2);
  assert.equal(worker.messages.at(-1).kind, "ready");
});

test("failed module imports can be retried", async () => {
  let attempts = 0;
  const worker = loadWorker({}, {
    importModule: (_url, module) => {
      attempts += 1;
      if (attempts === 1) throw new Error("module unavailable");
      return module;
    },
  });
  await initializeOffscreenWorker(worker);
  assert.equal(worker.messages.at(-1).kind, "failed");
  await initializeOffscreenWorker(worker);
  assert.equal(attempts, 2);
  assert.equal(worker.messages.at(-1).kind, "ready");
});

test("explicit module URLs are honored even when named frankenmermaid.js", async () => {
  const worker = loadWorker({});
  const moduleUrl = "https://example.invalid/versioned/frankenmermaid.js";
  await initializeOffscreenWorker(worker, moduleUrl);
  assert.deepEqual(worker.imports, [moduleUrl]);
  assert.equal(worker.messages.at(-1).kind, "ready");
});

test("SVG responses preserve Rust diagnostics and timings", async () => {
  const response = {
    kind: "completed",
    requestId: 7,
    svg: "<svg></svg>",
    warnings: [{ message: "recovered input" }],
    timings: { parseMs: 1, layoutMs: 2, renderMs: 3 },
  };
  const worker = loadWorker(null, {
    module: {
      chooseCanvasTarget: () => JSON.stringify({ target: "svgInWorker" }),
      workerHandleMessage: (json) => {
        assert.deepEqual(JSON.parse(json), { kind: "render", requestId: 7, input: "flowchart LR" });
        return JSON.stringify(response);
      },
    },
  });
  await worker.onMessage({ data: { kind: "init" } });
  const render = worker.onMessage({ data: { kind: "render", requestId: 7, input: "flowchart LR" } });
  await flushMicrotasks();
  worker.timers.shift()();
  await render;
  assert.deepEqual(plain(worker.messages.at(-1)), response);
});

// Run the shipped integration, replacing only browser I/O and the WASM module transport.
function loadPlayground(options = {}) {
  const html = fs.readFileSync(path.join(__dirname, "playground.html"), "utf8");
  const script = html.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
    .replaceAll("import.meta.url", JSON.stringify("https://example.invalid/web/playground.html"))
    .replaceAll("import(", "globalThis.__import(");
  const elements = {};
  function element() {
    return {
      textContent: "", innerHTML: "", value: "flowchart LR\nA --> B",
      children: [], listeners: {}, removed: false,
      get childElementCount() { return this.children.length; },
      get firstElementChild() { return this.children[0]; },
      append(child) { this.children.push(child); child.parent = this; },
      remove() {
        this.removed = true;
        if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1);
      },
      addEventListener(name, fn) { this.listeners[name] = fn; },
    };
  }
  for (const id of ["src", "out", "log", "status", "canvas"]) elements[id] = element();
  if (options.transfer) elements.canvas.transferControlToOffscreen = options.transfer;
  const timers = new Map();
  let timerId = 0;
  const listeners = {};
  const workers = [];
  const inputs = [];
  const imports = [];
  let initializations = 0;
  const module = {
    default: async () => {
      initializations += 1;
      if (options.initialize) await options.initialize(initializations);
    },
    workerHandleMessage: (json) => {
      const request = JSON.parse(json);
      inputs.push(request.input);
      return JSON.stringify({ kind: "completed", requestId: request.requestId, svg: `<svg>${request.input}</svg>` });
    },
  };
  class MockWorker {
    constructor() {
      if (options.constructorError) throw new Error(options.constructorError);
      this.messages = [];
      this.terminated = false;
      workers.push(this);
    }
    postMessage(message, transfer) {
      if (options.postError && options.postError(message)) throw new Error("postMessage failed");
      this.messages.push({ ...message, transfer });
    }
    terminate() { this.terminated = true; }
    emit(data) { this.onmessage({ data }); }
  }
  const context = {
    URL, JSON, Promise,
    performance: { now: () => 0 },
    document: { getElementById: (id) => elements[id], createElement: element },
    setTimeout: (callback, delay) => {
      const id = ++timerId;
      timers.set(id, { callback, delay });
      return id;
    },
    clearTimeout: (id) => timers.delete(id),
    addEventListener: (name, fn) => { listeners[name] = fn; },
    __import: async (url) => {
      imports.push(url);
      if (options.importModule) return options.importModule(module);
      return module;
    },
  };
  if (!options.noWorker) context.Worker = MockWorker;
  if (options.transfer) context.OffscreenCanvas = class {};
  context.globalThis = context;
  vm.runInNewContext(script, context, { filename: "playground.html" });
  return {
    elements, workers, inputs, imports, timers, listeners,
    initializations: () => initializations,
    edit(input) { elements.src.value = input; elements.src.listeners.input(); },
    ready() { workers[0].emit({ kind: "ready", requested: "svgInWorker", target: "svgInWorker" }); },
    runTimers(delay) {
      for (const [id, timer] of [...timers]) {
        if (timer.delay === delay) { timers.delete(id); timer.callback(); }
      }
    },
  };
}

async function finishMainRender(page) {
  await flushMicrotasks();
  page.runTimers(0);
  await flushMicrotasks();
}

test("playground renders SVG without Worker support and reuses initialized WASM", async () => {
  const page = loadPlayground({ noWorker: true });
  await finishMainRender(page);
  assert.equal(page.inputs.length, 1);
  assert.match(page.elements.out.innerHTML, /<svg>/);
  assert.match(page.elements.status.textContent, /main-thread SVG/);
  page.edit("new diagram");
  await finishMainRender(page);
  assert.equal(page.elements.out.innerHTML, "<svg>new diagram</svg>");
  assert.equal(page.imports.length, 1);
  assert.equal(page.initializations(), 1);
});

test("playground recovers from blocked constructors and initialization transport failures", async () => {
  for (const options of [
    { constructorError: "CSP blocked worker" },
    { postError: (message) => message.kind === "init" },
  ]) {
    const page = loadPlayground(options);
    await finishMainRender(page);
    assert.equal(page.inputs.length, 1);
    assert.equal(page.timers.size, 0);
    if (page.workers.length) assert.equal(page.workers[0].terminated, true);
  }
});

test("playground handles init failure without requestId, crashes, decode errors and timeouts", async () => {
  const fail = [
    (page) => page.workers[0].emit({ kind: "failed", reason: "WASM unavailable" }),
    (page) => page.workers[0].onerror({ message: "worker crashed" }),
    (page) => page.workers[0].onmessageerror(),
    (page) => page.runTimers(10000),
  ];
  for (const trigger of fail) {
    const page = loadPlayground();
    trigger(page);
    await finishMainRender(page);
    assert.equal(page.inputs.length, 1);
    assert.equal(page.workers[0].terminated, true);
    assert.equal(page.timers.size, 0);
  }
});

test("playground waits for worker readiness and sends only current source", () => {
  const page = loadPlayground();
  page.edit("obsolete");
  page.edit("latest");
  assert.deepEqual(page.workers[0].messages.map((msg) => msg.kind), ["init"]);
  page.ready();
  const renders = page.workers[0].messages.filter((msg) => msg.kind === "render");
  assert.equal(renders.length, 1);
  assert.equal(renders[0].input, "latest");
  assert.equal(page.imports.length, 0);
  assert.equal(page.timers.size, 0);
});

test("playground discards stale responses and cancels the previous request on edit", () => {
  const page = loadPlayground();
  page.ready();
  const first = page.workers[0].messages.at(-1);
  page.edit("latest");
  const second = page.workers[0].messages.at(-1);
  assert.equal(page.workers[0].messages.at(-2).kind, "cancel");
  assert.equal(page.workers[0].messages.at(-2).requestId, first.requestId);
  page.workers[0].emit({ kind: "completed", requestId: second.requestId, svg: "<svg>latest</svg>" });
  page.workers[0].emit({ kind: "completed", requestId: first.requestId, svg: "<svg>obsolete</svg>" });
  assert.equal(page.elements.out.innerHTML, "<svg>latest</svg>");
  page.workers[0].emit({ kind: "completed", requestId: second.requestId, svg: "" });
  assert.equal(page.elements.out.innerHTML, "");
});

test("playground recovers latest source after a worker crash and ignores its late events", async () => {
  const page = loadPlayground();
  page.ready();
  page.edit("latest input");
  const requestId = page.workers[0].messages.at(-1).requestId;
  page.workers[0].onerror({ message: "worker crashed" });
  await finishMainRender(page);
  page.workers[0].emit({ kind: "completed", requestId, svg: "stale worker SVG" });
  page.workers[0].onerror({ message: "late error" });
  assert.deepEqual(page.inputs, ["latest input"]);
  assert.equal(page.elements.out.innerHTML, "<svg>latest input</svg>");
  assert.equal(page.imports.length, 1);
});

test("playground coalesces edits during main WASM startup and retries failed initialization", async () => {
  const initialized = deferred();
  const page = loadPlayground({ noWorker: true, initialize: () => initialized.promise });
  page.edit("obsolete");
  page.edit("latest");
  await flushMicrotasks();
  assert.equal(page.initializations(), 1);
  assert.deepEqual(page.inputs, []);
  initialized.resolve();
  await finishMainRender(page);
  assert.deepEqual(page.inputs, ["latest"]);

  const retry = loadPlayground({ noWorker: true, initialize: (attempt) => {
    if (attempt === 1) throw new Error("WASM download interrupted");
  } });
  await finishMainRender(retry);
  assert.match(retry.elements.status.textContent, /retry/);
  retry.edit("retry input");
  await finishMainRender(retry);
  assert.deepEqual(retry.inputs, ["retry input"]);
  assert.equal(retry.initializations(), 2);
});

test("playground retains worker SVG rendering when canvas transfer fails", () => {
  const page = loadPlayground({ transfer: () => { throw new Error("transfer refused"); } });
  const init = page.workers[0].messages[0];
  assert.equal(init.capabilities.canvasTransferred, false);
  assert.equal(init.transfer.length, 0);
  page.ready();
  assert.equal(page.workers[0].terminated, false);
  assert.equal(page.imports.length, 0);
  assert.equal(page.elements.canvas.removed, true);
});

test("playground reports diagram errors without changing a healthy rendering transport", () => {
  const page = loadPlayground();
  page.ready();
  const requestId = page.workers[0].messages.at(-1).requestId;
  page.workers[0].emit({ kind: "failed", requestId, reason: "diagram parse failed" });
  assert.match(page.elements.log.children.at(-1).textContent, /diagram parse failed/);
  assert.equal(page.imports.length, 0);
  assert.equal(page.workers[0].terminated, false);
});

test("playground teardown stops the worker and discards queued fallback work", async () => {
  const page = loadPlayground();
  page.listeners.pagehide({ persisted: true });
  assert.equal(page.workers[0].terminated, false);
  page.listeners.pagehide({ persisted: false });
  page.ready();
  assert.equal(page.workers[0].terminated, true);
  assert.equal(page.workers[0].messages.length, 1);
  assert.equal(page.timers.size, 0);

  const fallback = loadPlayground({ noWorker: true });
  await flushMicrotasks();
  fallback.listeners.pagehide({ persisted: false });
  await finishMainRender(fallback);
  assert.deepEqual(fallback.inputs, []);
});

test("canvas setup failure retains worker SVG rendering and initialization config", async () => {
  const requests = [];
  const worker = loadWorker(null, {
    module: {
      Diagram: { fromOffscreenCanvas: () => { throw new Error("2D context unavailable"); } },
      workerHandleMessage: (json) => {
        const request = JSON.parse(json);
        requests.push(request);
        return JSON.stringify({ kind: "completed", requestId: request.requestId, svg: "<svg/>" });
      },
    },
  });
  await worker.onMessage({ data: {
    kind: "init", canvas: {}, config: { theme: "dark" },
    capabilities: { offscreenCanvas: true, worker: true, canvasTransferred: true },
  } });
  assert.equal(worker.messages.at(-1).kind, "ready");
  assert.equal(worker.messages.at(-1).target, "svgInWorker");
  assert.match(worker.messages.at(-1).fallbackReason, /2D context unavailable/);

  for (const message of [
    { kind: "render", requestId: 1, input: "default config" },
    { kind: "render", requestId: 2, input: "override config", configJson: "{}" },
  ]) {
    const render = worker.onMessage({ data: message });
    await flushMicrotasks();
    worker.timers.shift()();
    await render;
    assert.equal(worker.messages.at(-1).kind, "completed");
  }
  assert.deepEqual(JSON.parse(requests[0].configJson), { theme: "dark" });
  assert.equal(requests[1].configJson, "{}");
});

test("canvas cancellation cannot revive a queued render when an ID is reused", async () => {
  const inputs = [];
  const worker = loadWorker({ render: (input) => inputs.push(input) });
  await initializeOffscreenWorker(worker);
  const first = worker.onMessage({ data: { kind: "render", requestId: 8, input: "cancelled" } });
  await flushMicrotasks();
  await worker.onMessage({ data: { kind: "cancel", requestId: 8 } });
  const second = worker.onMessage({ data: { kind: "render", requestId: 8, input: "replacement" } });
  await flushMicrotasks();
  worker.timers.shift()();
  await flushMicrotasks();
  worker.timers.shift()();
  await Promise.all([first, second]);
  assert.deepEqual(inputs, ["replacement"]);
  assert.deepEqual(worker.messages.map((msg) => msg.kind), ["ready", "noReply", "completed"]);
});

test("canvas reinitialization releases old renderers and invalidates queued draws", async () => {
  const inputs = [];
  const freed = [];
  let created = 0;
  const worker = loadWorker(null, {
    module: {
      chooseCanvasTarget: (json) => JSON.stringify({
        target: JSON.parse(json).canvasTransferred ? "offscreenInWorker" : "svgInWorker",
      }),
      Diagram: { fromOffscreenCanvas: () => {
        const id = ++created;
        return { free: () => freed.push(id), render: (input) => inputs.push({ id, input }) };
      } },
      workerHandleMessage: () => JSON.stringify({ kind: "completed", requestId: 11, svg: "<svg/>" }),
    },
  });
  await initializeOffscreenWorker(worker);
  const old = worker.onMessage({ data: { kind: "render", requestId: 9, input: "obsolete canvas" } });
  await flushMicrotasks();
  await initializeOffscreenWorker(worker);
  worker.timers.shift()();
  await old;
  assert.deepEqual(inputs, []);
  assert.deepEqual(freed, [1]);
  assert.equal(worker.messages.at(-1).kind, "noReply");

  const fresh = worker.onMessage({ data: { kind: "render", requestId: 10, input: "new canvas" } });
  await flushMicrotasks();
  worker.timers.shift()();
  await fresh;
  assert.deepEqual(inputs, [{ id: 2, input: "new canvas" }]);
  await worker.onMessage({ data: { kind: "init" } });
  assert.deepEqual(freed, [1, 2]);
  assert.equal(worker.messages.at(-1).target, "svgInWorker");
  const svg = worker.onMessage({ data: { kind: "render", requestId: 11, input: "SVG mode" } });
  await flushMicrotasks();
  worker.timers.shift()();
  await svg;
  assert.equal(worker.messages.at(-1).svg, "<svg/>");
});

test("canvas rejects negative request IDs without scheduling a draw", async () => {
  const inputs = [];
  const worker = loadWorker({ render: (input) => inputs.push(input) });
  await initializeOffscreenWorker(worker);
  // Do not await an invalid request before checking timers: the regression wrongly schedules it.
  const result = worker.onMessage({ data: { kind: "render", requestId: -1, input: "invalid" } });
  await flushMicrotasks();
  assert.equal(worker.timers.length, 0);
  await result;
  assert.equal(worker.messages.at(-1).kind, "failed");
  assert.deepEqual(inputs, []);
});

test("canvas drawing failure does not poison the next render", async () => {
  const inputs = [];
  const worker = loadWorker({ render: (input) => {
    inputs.push(input);
    if (input === "fail") throw new Error("draw failed");
    return { ok: true };
  } });
  await initializeOffscreenWorker(worker);
  for (const [requestId, input] of [[12, "fail"], [13, "recover"]]) {
    const result = worker.onMessage({ data: { kind: "render", requestId, input } });
    await flushMicrotasks();
    worker.timers.shift()();
    await result;
  }
  assert.deepEqual(inputs, ["fail", "recover"]);
  assert.equal(worker.messages.at(-2).kind, "failed");
  assert.equal(worker.messages.at(-1).kind, "completed");
});
