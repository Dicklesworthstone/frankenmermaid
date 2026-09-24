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
