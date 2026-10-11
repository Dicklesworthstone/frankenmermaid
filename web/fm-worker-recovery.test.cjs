"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const source = fs.readFileSync(process.env.FM_SOURCE_EDITOR_MODULE || path.join(__dirname, "fm-source-editor.js"));
const loaded = import(`data:text/javascript;base64,${source.toString("base64")}`);
const ready = { kind: "ready", sourceEditing: true, sourceDeletion: true, sourceInsertion: true,
  sourceBatchEditing: true, deckRendering: true };
const snapshot = { bindings: [], parsed: { warnings: [] } };
const outcome = promise => promise.then(value => ({ value }), error => ({ error }));
async function until(predicate) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    assert.ok(Date.now() < deadline, "worker condition did not complete");
    await new Promise(resolve => setTimeout(resolve, 2));
  }
}
function workerFixture() {
  const workers = [];
  class WorkerFixture {
    constructor() { this.messages = []; this.terminated = false; workers.push(this); }
    postMessage(message) { this.messages.push(structuredClone(message)); }
    terminate() { this.terminated = true; }
    emit(message) { this.onmessage?.({ data: message }); }
    get jobs() { return this.messages.filter(message => message.kind.startsWith("source")); }
    finish(svg = "<svg/>") {
      const job = this.jobs.at(-1);
      assert.ok(job, "a source job was really dispatched");
      this.emit({ kind: "sourceRendered", requestId: job.requestId, svg, snapshot });
    }
  }
  return { workers, WorkerClass: WorkerFixture, workerUrl: "worker.js", moduleUrl: "engine.js",
    initTimeoutMs: 1000, requestTimeoutMs: 1000, cancelGraceMs: 20 };
}
async function clientFixture(t, options = {}) {
  const f = workerFixture();
  const { createSourceWorkerClient } = await loaded;
  const client = createSourceWorkerClient({ ...f, ...options });
  t.after(() => client.dispose());
  return { ...f, client };
}
async function backendFixture(t, options = {}) {
  const f = workerFixture();
  const { createSourceEditorBackend } = await loaded;
  let localCalls = 0;
  const backend = createSourceEditorBackend({ ...f, ...options, loadModule: async () => {
    localCalls += 1;
    return { parseLens: () => snapshot, renderSvg: () => "<svg>LOCAL</svg>" };
  } });
  t.after(() => backend.dispose());
  return { ...f, backend, get localCalls() { return localCalls; } };
}

test("hung authoring work is not replayed on the UI thread and corrected source recovers", async t => {
  const f = await backendFixture(t, { requestTimeoutMs: 20 });
  f.workers[0].emit(ready);
  const failed = await outcome(f.backend.renderSource("expensive input"));
  assert.equal(failed.error?.name, "SourceWorkerInterrupted");
  assert.match(failed.error.message, /timed out/i);
  assert.equal(f.localCalls, 0);
  assert.equal(f.backend.target, "source worker");
  assert.equal(f.workers[0].terminated, true);
  assert.equal(f.workers.length, 1, "failed input is not automatically replayed");
  const result = f.backend.renderSource("corrected input");
  assert.equal(f.workers.length, 2);
  f.workers[1].emit(ready);
  f.workers[1].finish("<svg>corrected</svg>");
  assert.equal((await result).svg, "<svg>corrected</svg>");
  assert.equal(f.localCalls, 0);
});

test("cancelling active work releases CPU even when no replacement render follows", async t => {
  const { client, workers } = await clientFixture(t);
  workers[0].emit(ready);
  const abandoned = outcome(client.renderSource("hang"));
  client.cancel();
  assert.equal((await abandoned).error.name, "AbortError");
  await until(() => workers[0].terminated);
  assert.equal(workers.length, 1, "cancellation alone does not start another worker");
});

test("newest source survives hard cancellation without an unbounded worker message queue", async t => {
  const { client, workers } = await clientFixture(t);
  workers[0].emit(ready);
  const obsolete = [outcome(client.renderSource("running"))];
  for (let index = 0; index < 100; index += 1) obsolete.push(outcome(client.renderSource(`obsolete ${index}`)));
  const result = client.renderSource("newest");
  assert.equal(workers[0].jobs.length, 1, "only the actual running job crosses the worker boundary");
  assert.equal(workers[0].messages.filter(message => message.kind === "cancel").length, 1);
  await until(() => workers.length === 2);
  workers[1].emit(ready);
  assert.equal(workers[1].jobs.length, 1);
  assert.equal(workers[1].jobs[0].input, "newest");
  workers[1].finish("<svg>newest</svg>");
  assert.equal((await result).svg, "<svg>newest</svg>");
  for (const entry of await Promise.all(obsolete)) assert.equal(entry.error.name, "AbortError");
});

test("continuously superseding queued work does not extend the running job's grace deadline", async t => {
  const { client, workers } = await clientFixture(t, { cancelGraceMs: 20 });
  workers[0].emit(ready);
  const results = [outcome(client.renderSource("running"))];
  const timer = setInterval(() => results.push(outcome(client.renderSource("typing"))), 2);
  t.after(() => clearInterval(timer));
  await until(() => workers[0].terminated);
  clearInterval(timer);
  client.cancel();
  for (const entry of await Promise.all(results)) assert.equal(entry.error.name, "AbortError");
});

test("cooperative cancellation reuses the initialized worker and does not publish old SVG", async t => {
  const { client, workers } = await clientFixture(t, { cancelGraceMs: 200 });
  const worker = workers[0];
  worker.emit(ready);
  const old = outcome(client.renderSource("old"));
  const oldId = worker.jobs[0].requestId;
  const result = client.renderSource("current");
  assert.equal(worker.jobs.length, 1);
  worker.emit({ kind: "sourceRendered", requestId: oldId, svg: "<svg>OLD</svg>", snapshot });
  assert.equal(worker.jobs.length, 2);
  assert.equal(worker.jobs[1].input, "current");
  worker.finish("<svg>CURRENT</svg>");
  assert.equal((await old).error.name, "AbortError");
  assert.equal((await result).svg, "<svg>CURRENT</svg>");
  assert.equal(worker.terminated, false);
  assert.equal(workers.length, 1);
});

test("late callbacks from terminated workers cannot settle or kill a new generation", async t => {
  const { client, workers } = await clientFixture(t);
  workers[0].emit(ready);
  const old = outcome(client.renderSource("old"));
  const onMessage = workers[0].onmessage, onError = workers[0].onerror;
  const result = client.renderSource("new");
  await until(() => workers.length === 2);
  workers[1].emit(ready);
  const id = workers[1].jobs[0].requestId;
  onMessage({ data: { kind: "sourceRendered", requestId: id, svg: "<svg>STALE</svg>", snapshot } });
  onError({ message: "late error" });
  assert.equal(workers[1].terminated, false);
  workers[1].finish("<svg>NEW</svg>");
  assert.equal((await result).svg, "<svg>NEW</svg>");
  assert.equal((await old).error.name, "AbortError");
});

test("cancel during initialization coalesces source without a worker restart", async t => {
  const { client, workers } = await clientFixture(t);
  const old = outcome(client.renderSource("old"));
  const result = client.renderSource("new");
  assert.equal(workers[0].jobs.length, 0);
  workers[0].emit(ready);
  assert.equal(workers[0].jobs.length, 1);
  assert.equal(workers[0].jobs[0].input, "new");
  workers[0].finish();
  await result;
  assert.equal((await old).error.name, "AbortError");
  assert.equal(workers.length, 1);
});

for (const failure of ["crash", "messageerror", "malformed"]) {
  test(`active ${failure} stays isolated and next request gets a fresh worker`, async t => {
    const f = await backendFixture(t);
    const old = f.workers[0];
    old.emit(ready);
    const rejected = outcome(f.backend.renderSource("current"));
    if (failure === "crash") old.onerror({ message: "worker crashed" });
    else if (failure === "messageerror") old.onmessageerror();
    else old.emit({ kind: "sourceEdited", requestId: old.jobs[0].requestId });
    assert.equal((await rejected).error.name, "SourceWorkerInterrupted");
    assert.equal(f.localCalls, 0);
    const next = f.backend.renderSource("after failure");
    f.workers[1].emit(ready);
    f.workers[1].finish();
    await next;
    assert.equal(f.localCalls, 0);
  });
}

test("ordinary engine diagnostics reject without retiring a healthy worker", async t => {
  const f = await backendFixture(t);
  const worker = f.workers[0];
  worker.emit(ready);
  const rejected = outcome(f.backend.renderSource("invalid"));
  worker.emit({ kind: "failed", requestId: worker.jobs[0].requestId, reason: "syntax diagnostic" });
  assert.match((await rejected).error.message, /syntax diagnostic/);
  assert.equal(worker.terminated, false);
  const next = f.backend.renderSource("valid");
  worker.finish();
  await next;
  assert.equal(f.workers.length, 1);
  assert.equal(f.localCalls, 0);
});

test("initial Worker unavailability retains the explicit synchronous fallback", async t => {
  const f = await backendFixture(t, { WorkerClass: null });
  assert.equal((await f.backend.renderSource("small")).svg, "<svg>LOCAL</svg>");
  assert.equal(f.localCalls, 1);
  assert.match(f.backend.target, /fallback/);
});

test("initial bootstrap failure can fall back, but a replacement bootstrap failure cannot", async t => {
  const first = await backendFixture(t, { initTimeoutMs: 20 });
  assert.equal((await first.backend.renderSource("initial")).svg, "<svg>LOCAL</svg>");
  assert.equal(first.localCalls, 1);
  const f = await backendFixture(t, { initTimeoutMs: 20, requestTimeoutMs: 20 });
  f.workers[0].emit(ready);
  assert.equal((await outcome(f.backend.renderSource("hang"))).error.name, "SourceWorkerInterrupted");
  const next = await outcome(f.backend.renderSource("retry"));
  assert.equal(next.error.name, "SourceWorkerInterrupted");
  assert.equal(f.localCalls, 0);
});

test("replacement worker construction failure never enables main-thread replay", async t => {
  const fixture = workerFixture();
  let count = 0;
  class FailsOnReplacement extends fixture.WorkerClass {
    constructor() { if (count++ > 0) throw new Error("Worker construction blocked"); super(); }
  }
  const { createSourceEditorBackend } = await loaded;
  let localCalls = 0;
  const backend = createSourceEditorBackend({ ...fixture, WorkerClass: FailsOnReplacement,
    requestTimeoutMs: 20, loadModule: async () => { localCalls += 1; throw new Error("must not load"); } });
  t.after(() => backend.dispose());
  fixture.workers[0].emit(ready);
  await outcome(backend.renderSource("hang"));
  const retry = await outcome(backend.renderSource("retry"));
  assert.equal(retry.error.name, "SourceWorkerInterrupted");
  assert.equal(localCalls, 0);
});

test("restarts preserve the initial configuration snapshot, not later caller mutations", async t => {
  const config = { theme: "dark", layout: { rankSep: 72 } };
  const { client, workers } = await clientFixture(t, { config });
  workers[0].emit(ready);
  const old = outcome(client.renderSource("old"));
  config.theme = "light";
  config.layout.rankSep = 1;
  const next = client.renderSource("new");
  await until(() => workers.length === 2);
  assert.deepEqual(workers[1].messages[0].config, { theme: "dark", layout: { rankSep: 72 } });
  workers[1].emit(ready);
  workers[1].finish();
  await next;
  await old;
});

test("dispose during cancellation rejects queued work and prevents any replacement", async t => {
  const { client, workers } = await clientFixture(t);
  workers[0].emit(ready);
  const old = outcome(client.renderSource("old"));
  const next = outcome(client.renderSource("new"));
  client.dispose();
  assert.equal((await old).error.name, "AbortError");
  assert.equal((await next).error.name, "AbortError");
  assert.equal(workers[0].terminated, true);
  await new Promise(resolve => setTimeout(resolve, 40));
  assert.equal(workers.length, 1);
  assert.equal((await outcome(client.renderSource("after disposal"))).error.name, "AbortError");
});

test("all source operation variants dispatch and retain their response contracts after recovery", async t => {
  const { client, workers } = await clientFixture(t);
  workers[0].emit(ready);
  const old = outcome(client.renderSource("obsolete"));
  const deck = client.renderDeck("deck");
  await until(() => workers.length === 2);
  const worker = workers[1];
  worker.emit(ready);
  worker.emit({ kind: "sourceDeckRendered", requestId: worker.jobs.at(-1).requestId,
    svg: "<svg>DECK</svg>", manifest: { slides: [] }, warnings: [] });
  assert.equal((await deck).svg, "<svg>DECK</svg>");
  for (const [call, reply] of [
    [() => client.editSource("text", "node", "updated"), "sourceEdited"],
    [() => client.deleteSource("text", "node"), "sourceDeleted"],
    [() => client.insertSource("text", "node", "added"), "sourceInserted"],
    [() => client.batchSource("text", [{ elementId: "node", replacement: "batch" }]), "sourceBatchEdited"],
  ]) {
    const pending = call();
    const response = { result: { updatedSource: "changed", changes: [] }, snapshot };
    worker.emit({ kind: reply, requestId: worker.jobs.at(-1).requestId, response });
    assert.deepEqual(await pending, response);
  }
  await old;
});

test("invalid timeout ranges are rejected instead of overflowing browser timers", async () => {
  const { createSourceWorkerClient } = await loaded;
  for (const key of ["initTimeoutMs", "requestTimeoutMs", "cancelGraceMs"]) {
    for (const value of [0, -1, 0.5, NaN, Infinity, 2147483648, Number.MAX_SAFE_INTEGER]) {
      assert.throws(() => createSourceWorkerClient({ ...workerFixture(), [key]: value }), RangeError);
    }
  }
});
