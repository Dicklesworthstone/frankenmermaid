import assert from "node:assert/strict";
import test from "node:test";
import { createWorkerMermaid } from "./mermaid-compat.mjs";

// Transport boundary fixture. The browser suite separately runs the production endpoint in
// real DedicatedWorkers, including an infinite WebAssembly loop. No Rust grammar is mocked
// into a claimed parser proof here.
const PROTOCOL = "fm-mermaid-engine/1";
const engineUrl = "https://example.test/pkg/frankenmermaid.js";
const parsed = (source) => ({ ir: { diagram_type: "Flowchart", diagnostics: [] }, warnings: [source] });
async function until(predicate) {
  for (let index = 0; index < 1000; index += 1) {
    if (predicate()) return;
    await new Promise(setImmediate);
  }
  assert.fail("transport did not reach the expected state");
}
function fixture(t, { init, call, construct } = {}, options = {}) {
  const original = Object.getOwnPropertyDescriptor(globalThis, "Worker");
  const workers = [], clients = [];
  class WorkerFixture {
    constructor(url, config) {
      construct?.();
      this.url = url;
      this.config = config;
      this.messages = [];
      this.terminated = 0;
      workers.push(this);
    }
    emit(data) { this.onmessage?.({ data: { protocol: PROTOCOL, ...data } }); }
    result(message, value) { this.emit({ kind: "result", requestId: message.requestId, value }); }
    postMessage(message) {
      this.messages.push(structuredClone(message));
      queueMicrotask(() => {
        if (this.terminated) return;
        if (message.kind === "init") {
          if (init) init(this, message); else this.emit({ kind: "ready" });
        } else if (!call || call(this, message) !== false) {
          this.result(message, message.method === "validateConfig" ? { errors: [] } : parsed(message.args[0]));
        }
      });
    }
    terminate() { this.terminated += 1; }
  }
  globalThis.Worker = WorkerFixture;
  const make = (overrides = {}) => {
    const api = createWorkerMermaid({ engineUrl, requestTimeoutMs: 2000, ...options, ...overrides });
    clients.push(api);
    return api;
  };
  t.after(() => {
    clients.forEach((api) => api.dispose());
    if (original) Object.defineProperty(globalThis, "Worker", original); else delete globalThis.Worker;
  });
  return { api: make(), make, workers, calls: () => workers.flatMap((worker) => worker.messages.filter((m) => m.kind === "call")) };
}

test("lazy instance reuses one module worker and sends only an explicit module URL", async (t) => {
  const f = fixture(t);
  assert.equal(f.workers.length, 0);
  await f.api.run({ nodes: [] });
  assert.equal(f.workers.length, 0);
  assert.deepEqual(await f.api.parse("first"), { diagramType: "flowchart-v2", diagnostics: [], warnings: ["first"] });
  assert.deepEqual((await f.api.parse("second")).warnings, ["second"]);
  assert.equal(f.workers.length, 1);
  assert.equal(f.workers[0].url.hash, "#fm-mermaid-engine");
  assert.match(f.workers[0].url.pathname, /\/mermaid-compat\.mjs$/);
  assert.equal(f.workers[0].config.type, "module");
  assert.deepEqual(f.workers[0].messages[0], { protocol: PROTOCOL, kind: "init", engineUrl });
  assert.deepEqual(f.calls().map((m) => m.method), ["validateConfig", "parse", "parse"]);
});

test("invalid source and timer/loader options fail before worker construction", async (t) => {
  const f = fixture(t);
  for (const source of ["", "\ud800", "x".repeat(2 * 1024 * 1024 + 1)]) await assert.rejects(f.api.parse(source));
  for (const requestTimeoutMs of [0, -1, 0.5, NaN, Infinity, 2 ** 31, "100"]) {
    assert.throws(() => createWorkerMermaid({ engineUrl, requestTimeoutMs }), /timer interval/);
  }
  for (const url of [undefined, "", "javascript:1", "file:///engine.js"]) {
    assert.throws(() => createWorkerMermaid({ engineUrl: url }));
  }
  assert.throws(() => createWorkerMermaid({ engineUrl, loadEngine() {} }), /loadEngine/);
  assert.throws(() => createWorkerMermaid({ engineUrl, wasmInput: new Uint8Array() }), /wasmInput/);
  assert.equal(f.workers.length, 0);
});

test("concurrent diagrams remain FIFO and are not coalesced as keystrokes", async (t) => {
  const held = [];
  const f = fixture(t, { call(worker, message) {
    if (message.method !== "parse") return;
    held.push([worker, message]); return false;
  } });
  const pending = ["a", "b", "c"].map((source) => f.api.parse(source));
  const all = Promise.all(pending);
  for (let index = 0; index < 3; index += 1) {
    await until(() => held.length === index + 1);
    const [worker, message] = held[index];
    assert.equal(message.args[0], ["a", "b", "c"][index]);
    worker.result(message, parsed(message.args[0]));
  }
  assert.deepEqual((await all).map((result) => result.warnings[0]), ["a", "b", "c"]);
  assert.equal(f.workers.length, 1);
});

test("timeout rejects the active call once and recovers independent queued work without replay", async (t) => {
  const f = fixture(t, { call(_worker, message) { if (message.args[0] === "hang") return false; } }, { requestTimeoutMs: 25 });
  const settled = await Promise.allSettled([f.api.parse("hang"), f.api.parse("good")]);
  assert.equal(settled[0].reason.code, "worker-timeout");
  assert.equal(settled[1].value.warnings[0], "good");
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers[0].terminated, 1);
  assert.deepEqual(f.calls().filter((m) => m.method === "parse").map((m) => m.args[0]), ["hang", "good"]);
});

test("a bootstrap failure is shared and does not poison the configuration on retry", async (t) => {
  let attempts = 0;
  const f = fixture(t, { init(worker) {
    attempts += 1;
    worker.emit(attempts === 1 ? { kind: "failed", message: "fixture import failed" } : { kind: "ready" });
  } });
  const settled = await Promise.allSettled([f.api.parse("a"), f.api.parse("b")]);
  assert.ok(settled.every((result) => result.reason.code === "worker-bootstrap"));
  assert.equal(f.workers.length, 1);
  assert.equal(f.calls().length, 0);
  assert.equal((await f.api.parse("retry")).warnings[0], "retry");
  assert.equal(f.workers.length, 2);
});

test("bootstrap deadline terminates an uninitialized engine and permits a fresh call", async (t) => {
  let attempts = 0;
  const f = fixture(t, { init(worker) { if (++attempts > 1) worker.emit({ kind: "ready" }); } }, { requestTimeoutMs: 25 });
  await assert.rejects(f.api.parse("cold"), { code: "worker-timeout" });
  assert.equal(f.workers[0].terminated, 1);
  assert.equal(f.calls().length, 0);
  assert.equal((await f.api.parse("retry")).warnings[0], "retry");
});

test("CSP/constructor failure and absent Workers never fall back to a UI-thread import", async (t) => {
  let fail = true;
  const f = fixture(t, { construct() { if (fail) throw new Error("CSP fixture"); } });
  await assert.rejects(f.api.parse("a"), { code: "worker-bootstrap" });
  fail = false;
  assert.equal((await f.api.parse("b")).warnings[0], "b");
  const second = f.make();
  globalThis.Worker = undefined;
  await assert.rejects(second.parse("c"), { code: "worker-unavailable" });
  assert.equal(f.workers.length, 1);
});

test("ordinary native errors do not kill the worker, while WASM traps retire it", async (t) => {
  const f = fixture(t, { call(worker, message) {
    if (!["error", "trap"].includes(message.args[0])) return;
    worker.emit({ kind: "failed", requestId: message.requestId, code: "worker-error", message: "native failure", fatal: message.args[0] === "trap" });
    return false;
  } });
  await assert.rejects(f.api.parse("error"), /native failure/);
  await f.api.parse("good");
  assert.equal(f.workers.length, 1);
  await assert.rejects(f.api.parse("trap"), /native failure/);
  await f.api.parse("fresh");
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers[0].terminated, 1);
});

test("crash and messageerror retire the generation and preserve queued requests", async (t) => {
  let prevented = 0;
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] === "crash") {
      worker.onerror({ message: "crash fixture", preventDefault() { prevented += 1; } }); return false;
    }
    if (message.args[0] === "decode") { worker.onmessageerror(); return false; }
  } });
  const settled = await Promise.allSettled([f.api.parse("crash"), f.api.parse("decode"), f.api.parse("good")]);
  assert.equal(settled[0].reason.code, "worker-crash");
  assert.equal(settled[1].reason.code, "worker-protocol");
  assert.equal(settled[2].value.warnings[0], "good");
  assert.equal(prevented, 1);
  assert.equal(f.workers.length, 3);
});

test("late replies and errors from a retired generation cannot complete its successor", async (t) => {
  let oldMessage, oldError, nextJob;
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] === "old") {
      oldMessage = worker.onmessage; oldError = worker.onerror;
      worker.onmessageerror(); return false;
    }
    if (message.args[0] === "new") { nextJob = [worker, message]; return false; }
  } });
  await assert.rejects(f.api.parse("old"), { code: "worker-protocol" });
  let finished = false;
  const pending = f.api.parse("new").then((result) => { finished = true; return result; });
  await until(() => nextJob);
  const [worker, message] = nextJob;
  oldMessage({ data: { protocol: PROTOCOL, kind: "result", requestId: message.requestId, value: parsed("wrong") } });
  oldError({ preventDefault() { assert.fail("old error must be ignored"); } });
  await new Promise(setImmediate);
  assert.equal(finished, false);
  assert.equal(worker.terminated, 0);
  worker.result(message, parsed("new"));
  assert.equal((await pending).warnings[0], "new");
});

test("mismatched responses fail closed and a corrected call uses a new generation", async (t) => {
  let wrong = true;
  const f = fixture(t, { call(worker, message) {
    if (message.method === "parse" && wrong) {
      wrong = false;
      worker.emit({ kind: "result", requestId: message.requestId + 1, value: parsed("wrong") }); return false;
    }
  } });
  await assert.rejects(f.api.parse("first"), { code: "worker-protocol" });
  assert.equal((await f.api.parse("second")).warnings[0], "second");
  assert.equal(f.workers.length, 2);
});

test("dispose settles active and queued calls, detaches handlers and remains idempotent", async (t) => {
  const f = fixture(t, { call(_worker, message) { if (message.method === "parse") return false; } });
  const settled = Promise.allSettled([f.api.parse("active"), f.api.parse("queued")]);
  await until(() => f.calls().some((m) => m.method === "parse"));
  f.api.dispose(); f.api.dispose();
  assert.ok((await settled).every((result) => result.reason.code === "disposed"));
  assert.equal(f.workers[0].terminated, 1);
  assert.equal(f.workers[0].onmessage, null);
  await assert.rejects(f.api.parse("late"), { code: "disposed" });
  assert.equal(f.workers.length, 1);
});

test("disposing before loading prevents even creating a worker", async (t) => {
  const f = fixture(t);
  const pending = f.api.parse("early");
  f.api.dispose();
  await assert.rejects(pending, { code: "disposed" });
  assert.equal(f.workers.length, 0);
});

test("bounded queue rejects overload without evicting already admitted work", async (t) => {
  const held = [];
  const f = fixture(t, { call(worker, message) {
    if (message.method === "parse") { held.push([worker, message]); return false; }
  } });
  await f.api.parse("warm").catch(() => {}); // timeout establishes a validated configuration
  const pending = Array.from({ length: 140 }, (_, index) => f.api.parse(String(index)));
  const settled = Promise.allSettled(pending);
  await until(() => held.length === 2);
  f.api.dispose();
  const results = await settled;
  assert.equal(results.filter((result) => result.reason.code === "worker-queue-full").length, 12);
  assert.equal(results.filter((result) => result.reason.code === "disposed").length, 128);
  assert.equal(f.calls().filter((m) => m.method === "parse").length, 2);
});

test("byte budget rejects large queued sources before cloning them into a worker", async (t) => {
  const f = fixture(t, { call(_worker, message) { if (message.method === "parse") return false; } });
  const pending = Array.from({ length: 20 }, () => f.api.parse("x".repeat(2 * 1024 * 1024)));
  const settled = Promise.allSettled(pending);
  await until(() => f.calls().some((m) => m.method === "parse"));
  f.api.dispose();
  const results = await settled;
  assert.equal(results.filter((r) => r.reason.code === "worker-queue-full").length, 5);
  assert.equal(results.filter((r) => r.reason.code === "disposed").length, 15);
});

test("instance configuration snapshots and lifetime are independent", async (t) => {
  const f = fixture(t);
  const second = f.make();
  f.api.initialize({ theme: "dark" }); second.initialize({ theme: "neutral" });
  await Promise.all([f.api.parse("one"), second.parse("two")]);
  f.api.dispose();
  assert.equal((await second.parse("still live")).warnings[0], "still live");
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers.filter((w) => w.terminated).length, 1);
  assert.deepEqual(f.workers.map((w) => JSON.parse(w.messages.find((m) => m.method === "validateConfig").args[0]).theme), ["dark", "neutral"]);
});
