import assert from "node:assert/strict";
import test from "node:test";
import { getEventListeners } from "node:events";
import { createMermaid, createWorkerMermaid } from "./mermaid-compat.mjs";

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

test("cancelPending terminates active work, rejects the queue and immediately permits retry", async (t) => {
  const f = fixture(t, { call(_worker, message) { if (message.args[0] === "hang") return false; } });
  f.api.initialize({ theme: "dark" });
  const pending = Promise.allSettled([f.api.parse("hang"), f.api.parse("queued")]);
  await until(() => f.calls().some((m) => m.args[0] === "hang"));
  f.api.cancelPending();
  const retry = f.api.parse("corrected"); // no wait for old rejection/cleanup microtasks
  assert.ok((await pending).every((r) => r.reason.code === "cancelled" && r.reason.name === "AbortError"));
  assert.equal((await retry).warnings[0], "corrected");
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers[0].terminated, 1);
  assert.deepEqual(f.calls().filter((m) => m.method === "parse").map((m) => m.args[0]), ["hang", "corrected"]);
  assert.deepEqual(f.calls().filter((m) => m.method === "validateConfig").map((m) => JSON.parse(m.args[0]).theme), ["dark", "dark"]);
});

test("cancel before loading never starts the cancelled request or poisons later work", async (t) => {
  const f = fixture(t);
  const pending = f.api.parse("cancel before load");
  f.api.cancelPending(); f.api.cancelPending();
  await assert.rejects(pending, { name: "AbortError", code: "cancelled" });
  assert.equal(f.workers.length, 0);
  assert.equal((await f.api.parse("new")).warnings[0], "new");
  assert.equal(f.workers.length, 1);
  f.api.dispose(); f.api.cancelPending();
  assert.equal(f.workers[0].terminated, 1);
});

test("cancel during bootstrap retires its late ready notification and retries the same config", async (t) => {
  let lateReady, attempts = 0;
  const f = fixture(t, { init(worker) {
    if (++attempts === 1) lateReady = worker.onmessage;
    else worker.emit({ kind: "ready" });
  } });
  const pending = f.api.parse("old");
  await until(() => lateReady);
  f.api.cancelPending();
  const fresh = f.api.parse("new");
  lateReady({ data: { protocol: PROTOCOL, kind: "ready" } });
  await assert.rejects(pending, { code: "cancelled" });
  assert.equal((await fresh).warnings[0], "new");
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers[0].messages.length, 1);
});

test("cancelling shared pending validation rejects all owners and permits immediate validation retry", async (t) => {
  let block = true;
  const f = fixture(t, { call(_worker, message) { if (block && message.method === "validateConfig") return false; } });
  const pending = Promise.allSettled([f.api.parse("a"), f.api.parse("b")]);
  await until(() => f.calls().length === 1);
  f.api.cancelPending(); block = false;
  const retry = f.api.parse("retry");
  assert.ok((await pending).every((r) => r.reason.code === "cancelled"));
  assert.equal((await retry).warnings[0], "retry");
  assert.deepEqual(f.calls().map((m) => m.method), ["validateConfig", "validateConfig", "parse"]);
});

test("cancellation after a transport reply revokes the still-pending public result", async (t) => {
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] !== "reply then cancel") return;
    worker.result(message, parsed("must not escape"));
    f.api.cancelPending();
    return false;
  } });
  await assert.rejects(f.api.parse("reply then cancel"), { code: "cancelled" });
  assert.equal((await f.api.parse("new")).warnings[0], "new");
});

test("explicit cancellation respects parse suppression and reports unsuppressed operations once", async (t) => {
  const f = fixture(t, { call(_worker, message) { if (message.method === "parse") return false; } });
  const reported = [];
  f.api.parseError = (error) => reported.push(error);
  const quiet = f.api.parse("quiet", { suppressErrors: true });
  const loud = f.api.parse("loud");
  await until(() => f.calls().some((m) => m.method === "parse"));
  f.api.cancelPending();
  assert.equal(await quiet, false);
  await assert.rejects(loud, { code: "cancelled" });
  assert.equal(reported.length, 1);
});

test("cancellation is instance-local even while another worker is active", async (t) => {
  const held = [];
  const f = fixture(t, { call(worker, message) {
    if (message.method === "parse") { held.push([worker, message]); return false; }
  } });
  const second = f.make();
  const one = f.api.parse("one"), two = second.parse("two");
  await until(() => held.length === 2);
  f.api.cancelPending();
  await assert.rejects(one, { code: "cancelled" });
  const [worker, message] = held.find(([, m]) => m.args[0] === "two");
  assert.equal(worker.terminated, 0);
  worker.result(message, parsed("two"));
  assert.equal((await two).warnings[0], "two");
});

test("custom loader cancellation settles promptly and cannot resume old parsing after loading", async (t) => {
  let release;
  const ready = new Promise((resolve) => { release = resolve; });
  const calls = [];
  const api = createMermaid({ loadEngine: () => ready });
  t.after(() => api.dispose());
  const old = api.parse("old");
  api.cancelPending();
  await assert.rejects(old, { code: "cancelled" });
  const current = api.parse("new");
  release({ validateConfig: () => ({ errors: [] }), renderSvg: () => "<svg/>",
    parse: (source) => { calls.push(source); return parsed(source); } });
  assert.equal((await current).warnings[0], "new");
  assert.deepEqual(calls, ["new"]);
});

test("operation waiting preserves even falsy host rejection reasons", async (t) => {
  for (const reason of [undefined, null, false, 0, ""]) {
    const api = createMermaid({ loadEngine: () => Promise.reject(reason) });
    t.after(() => api.dispose());
    const [result] = await Promise.allSettled([api.parse("source")]);
    assert.equal(result.status, "rejected");
    assert.equal(result.reason, reason);
  }
});

test("AbortSignal rejects pre-aborted parse/render without loading or reporting a syntax error", async (t) => {
  const f = fixture(t), controller = new AbortController(), reports = [];
  f.api.parseError = (error) => reports.push(error);
  controller.abort();
  await assert.rejects(f.api.parse("never", { signal: controller.signal, suppressErrors: true }), { name: "AbortError", code: "cancelled" });
  await assert.rejects(f.api.render("never", "never", undefined, { signal: controller.signal }), { name: "AbortError", code: "cancelled" });
  assert.equal(f.workers.length, 0);
  assert.deepEqual(reports, []);
  assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});

test("invalid signals and options fail before admitting work", async (t) => {
  const f = fixture(t);
  for (const signal of [{}, true, "signal", { aborted: false }, new AbortController()]) {
    await assert.rejects(f.api.parse("source", { signal }), TypeError);
    await assert.rejects(f.api.render("id", "source", undefined, { signal }), TypeError);
  }
  for (const options of [null, [], true]) await assert.rejects(f.api.parse("source", options), TypeError);
  assert.equal(f.workers.length, 0);
});

test("aborting the active request retires only its worker and preserves independent FIFO work", async (t) => {
  let lateMessage;
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] === "hang") { lateMessage = worker.onmessage; return false; }
  } });
  const controller = new AbortController();
  const old = f.api.parse("hang", { signal: controller.signal });
  const cancelled = assert.rejects(old, { name: "AbortError", code: "cancelled" });
  const good = [f.api.parse("b"), f.api.parse("c")];
  await until(() => lateMessage);
  controller.abort();
  const results = await Promise.all(good);
  await cancelled;
  lateMessage({ data: { protocol: PROTOCOL, kind: "result", requestId: 1, value: parsed("wrong") } });
  assert.deepEqual(results.map((result) => result.warnings[0]), ["b", "c"]);
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers[0].terminated, 1);
  assert.equal(f.workers[1].terminated, 0);
  assert.deepEqual(f.calls().filter((m) => m.method === "parse").map((m) => m.args[0]), ["hang", "b", "c"]);
  assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});

test("a queued abort never terminates the independent active worker or dispatches cancelled source", async (t) => {
  let held;
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] === "active") { held = [worker, message]; return false; }
  } });
  const controller = new AbortController();
  const first = f.api.parse("active");
  const skipped = assert.rejects(f.api.parse("skip", { signal: controller.signal }), { name: "AbortError" });
  const last = f.api.parse("last");
  await until(() => held);
  controller.abort();
  await skipped;
  assert.equal(f.workers[0].terminated, 0);
  held[0].result(held[1], parsed("active"));
  assert.deepEqual((await Promise.all([first, last])).map((r) => r.warnings[0]), ["active", "last"]);
  assert.equal(f.workers.length, 1);
  assert.deepEqual(f.calls().filter((m) => m.method === "parse").map((m) => m.args[0]), ["active", "last"]);
});

test("a shared signal cannot briefly dispatch its cancelled queued siblings during abort", async (t) => {
  const f = fixture(t, { call(_worker, message) { if (message.args[0] === "active") return false; } });
  await f.api.parse("warm");
  const controller = new AbortController();
  const stopped = Promise.allSettled(["active", "skip1", "skip2"].map((source) => f.api.parse(source, { signal: controller.signal })));
  const survivor = f.api.parse("survivor");
  await until(() => f.calls().some((m) => m.args[0] === "active"));
  controller.abort();
  assert.ok((await stopped).every((r) => r.status === "rejected" && r.reason.name === "AbortError"));
  assert.equal((await survivor).warnings[0], "survivor");
  assert.equal(f.workers.length, 2);
  assert.deepEqual(f.calls().filter((m) => m.method === "parse").map((m) => m.args[0]), ["warm", "active", "survivor"]);
});

test("aborting one validation waiter does not cancel or poison another caller's shared validation", async (t) => {
  let held;
  const f = fixture(t, { call(worker, message) {
    if (message.method === "validateConfig") { held = [worker, message]; return false; }
  } });
  const controller = new AbortController();
  const old = assert.rejects(f.api.parse("old", { signal: controller.signal }), { name: "AbortError" });
  const survivor = f.api.parse("survivor");
  await until(() => held);
  controller.abort();
  await old;
  assert.equal(f.workers[0].terminated, 0);
  held[0].result(held[1], { errors: [] });
  assert.equal((await survivor).warnings[0], "survivor");
  await f.api.parse("cached");
  assert.deepEqual(f.calls().map((m) => m.method), ["validateConfig", "parse", "parse"]);
});

test("the last aborted validation waiter releases a stuck worker and allows immediate config retry", async (t) => {
  let held, block = true;
  const f = fixture(t, { call(worker, message) {
    if (block && message.method === "validateConfig") { held = [worker, message]; return false; }
  } });
  const controller = new AbortController();
  const old = assert.rejects(f.api.parse("old", { signal: controller.signal }), { name: "AbortError" });
  await until(() => held);
  controller.abort(); block = false;
  const retry = f.api.parse("retry");
  await old;
  assert.equal((await retry).warnings[0], "retry");
  assert.equal(f.workers[0].terminated, 1);
  assert.deepEqual(f.calls().map((m) => m.method), ["validateConfig", "validateConfig", "parse"]);
});

test("aborting during bootstrap discards the abandoned generation without waiting for timeout", async (t) => {
  let lateReady, attempts = 0;
  const f = fixture(t, { init(worker) {
    if (++attempts === 1) lateReady = worker.onmessage; else worker.emit({ kind: "ready" });
  } });
  const controller = new AbortController();
  const old = assert.rejects(f.api.parse("old", { signal: controller.signal }), { name: "AbortError" });
  await until(() => lateReady);
  controller.abort();
  const retry = f.api.parse("new");
  await old;
  lateReady({ data: { protocol: PROTOCOL, kind: "ready" } });
  assert.equal((await retry).warnings[0], "new");
  assert.equal(f.workers.length, 2);
  assert.equal(f.workers[0].terminated, 1);
  assert.equal(f.workers[0].messages.length, 1);
});

test("abort after a transport reply revokes publication but does not terminate a completed worker", async (t) => {
  const controller = new AbortController();
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] !== "race") return;
    worker.result(message, parsed("must not escape")); controller.abort(); return false;
  } });
  await assert.rejects(f.api.parse("race", { signal: controller.signal }), { name: "AbortError" });
  assert.equal((await f.api.parse("fresh")).warnings[0], "fresh");
  assert.equal(f.workers.length, 1);
  assert.equal(f.workers[0].terminated, 0);
});

test("successful and failed requests remove abort listeners; later abort cannot kill unrelated work", async (t) => {
  const f = fixture(t, { call(worker, message) {
    if (message.args[0] !== "error") return;
    worker.emit({ kind: "failed", requestId: message.requestId, message: "expected" }); return false;
  } });
  for (const source of ["good", "error"]) {
    const controller = new AbortController();
    const result = await Promise.allSettled([f.api.parse(source, { signal: controller.signal })]);
    assert.equal(result[0].status, source === "good" ? "fulfilled" : "rejected");
    assert.equal(getEventListeners(controller.signal, "abort").length, 0);
    controller.abort();
    assert.equal(f.workers[0].terminated, 0);
  }
  assert.equal((await f.api.parse("next")).warnings[0], "next");
});

test("queued abort returns its admission slot without evicting any other request", async (t) => {
  let hold = false;
  const f = fixture(t, { call(_worker, message) { if (hold && message.method === "parse") return false; } });
  await f.api.parse("warm"); hold = true;
  const controller = new AbortController();
  const jobs = [f.api.parse("active"), f.api.parse("cancel me", { signal: controller.signal }),
    ...Array.from({ length: 126 }, (_, index) => f.api.parse(`queued ${index}`))];
  const settled = Promise.allSettled(jobs);
  await until(() => f.calls().some((m) => m.args[0] === "active"));
  await assert.rejects(f.api.parse("overflow"), { code: "worker-queue-full" });
  controller.abort();
  const admitted = Promise.allSettled([f.api.parse("replacement")]);
  await new Promise(setImmediate);
  f.api.dispose();
  assert.equal((await admitted)[0].reason.code, "disposed", "replacement was admitted, not queue-rejected");
  const results = await settled;
  assert.equal(results.filter((r) => r.reason.name === "AbortError").length, 1);
  assert.equal(results.filter((r) => r.reason.code === "disposed").length, 127);
});

test("custom loader abort settles only its caller and preserves native function signatures", async (t) => {
  let release;
  const calls = [], loading = new Promise((resolve) => { release = resolve; });
  const api = createMermaid({ loadEngine: () => loading });
  t.after(() => api.dispose());
  const controller = new AbortController();
  const old = assert.rejects(api.parse("old", { signal: controller.signal }), { name: "AbortError" });
  const current = api.parse("current");
  controller.abort();
  await old;
  release({ validateConfig(...args) { assert.equal(args.length, 1); return { errors: [] }; },
    renderSvg: () => "<svg/>", parse(...args) { assert.equal(args.length, 1); calls.push(args[0]); return parsed(args[0]); } });
  assert.equal((await current).warnings[0], "current");
  assert.deepEqual(calls, ["current"]);
});

test("aborting a custom asynchronous parse discards its late rejection and never starts SVG rendering", async (t) => {
  let rejectParse, started = false, renders = 0;
  const api = createMermaid({ loadEngine: () => ({ validateConfig: () => ({ errors: [] }),
    parse: () => { started = true; return new Promise((_, reject) => { rejectParse = reject; }); },
    renderSvg: () => { renders += 1; return "<svg/>"; } }) });
  t.after(() => api.dispose());
  const controller = new AbortController();
  const old = assert.rejects(api.render("id", "source", undefined, { signal: controller.signal }), { name: "AbortError" });
  await until(() => started);
  controller.abort(); await old;
  rejectParse(new Error("late native error"));
  await new Promise(setImmediate);
  assert.equal(renders, 0);
  assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});

test("active SVG render abort retires its WASM worker while an independent parse survives", async (t) => {
  let rendering = false;
  const f = fixture(t, { call(_worker, message) {
    if (message.method === "renderSvg") { rendering = true; assert.equal(message.args.length, 2); return false; }
  } });
  const controller = new AbortController();
  const stopped = assert.rejects(f.api.render("id", "source", undefined, { signal: controller.signal }), { name: "AbortError" });
  await until(() => rendering);
  const survivor = f.api.parse("survivor");
  await new Promise(setImmediate);
  controller.abort(); await stopped;
  assert.equal((await survivor).warnings[0], "survivor");
  assert.equal(f.workers.length, 2);
  assert.equal(f.calls().filter((m) => m.method === "renderSvg").length, 1);
  assert.ok(f.calls().every((m) => !Object.hasOwn(m, "signal")), "signals never cross postMessage");
});
