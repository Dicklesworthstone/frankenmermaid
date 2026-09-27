// Real session/client/backend code, with a deliberately small WASM-boundary fixture.
// This is source-transaction and transport coverage, not a Mermaid grammar conformance claim.
// Run: node --test web/fm-source-batch-client.test.cjs
"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { test } = require("node:test");
const sourceCode = fs.readFileSync(path.join(__dirname, "fm-source-editor.js"), "utf8");
const editor = import(`data:text/javascript;base64,${Buffer.from(sourceCode).toString("base64")}`);
const SOURCE = "\ufeff%% café 🦀\r\nflowchart TB\r\nA[old]\r\nB[two]\r\n%% keep\r\n";
const bytes = (text) => Buffer.byteLength(text, "utf8");
const clone = (value) => JSON.parse(JSON.stringify(value));
const tick = () => new Promise((resolve) => setImmediate(resolve));

function snapshot(source) {
  return { bindings: [...source.matchAll(/^([A-Z][0-9]*)\[[^\r\n]*\]/gm)].map((match) => ({
    elementId: `node-${match[1]}-${source.length}`, sourceId: match[1], kind: "node", snippet: match[0],
    textRange: { startByte: bytes(source.slice(0, match.index)), endByte: bytes(source.slice(0, match.index + match[0].length)) },
  })), parsed: { warnings: ["fixture warning"] } };
}
function requests(source = SOURCE, replacements = { A: "A[日本語]", B: "B[😀 longer]" }) {
  return Object.entries(replacements).map(([id, replacement]) => ({
    elementId: snapshot(source).bindings.find((binding) => binding.sourceId === id).elementId, replacement,
  }));
}
function responseFor(source = SOURCE, edits = requests(source)) {
  const bindings = snapshot(source).bindings;
  const entries = edits.map((edit) => ({ ...bindings.find((binding) => binding.elementId === edit.elementId), ...edit }));
  let updated = Buffer.from(source);
  for (const entry of [...entries].sort((a, b) => b.textRange.startByte - a.textRange.startByte)) {
    updated = Buffer.concat([updated.subarray(0, entry.textRange.startByte), Buffer.from(entry.replacement), updated.subarray(entry.textRange.endByte)]);
  }
  return { result: { updatedSource: updated.toString(), changes: entries.filter((entry) => entry.replacement !== entry.snippet).map((entry) => ({
    elementId: entry.elementId, appliedElementId: entry.elementId, previousSnippet: entry.snippet,
    replacement: entry.replacement, replacedRange: entry.textRange,
  })) }, snapshot: snapshot(updated.toString()) };
}
function apiFixture(options = {}) {
  const calls = [];
  const parses = [];
  return {
    calls, parses,
    parseLens(source) { parses.push(source); return options.snapshot ? options.snapshot(source) : snapshot(source); },
    renderSvg: (source) => `<svg data-length="${source.length}"/>`,
    applyParseLensEdit(source, elementId, replacement) {
      const binding = snapshot(source).bindings.find((entry) => entry.elementId === elementId);
      assert.ok(binding, "a current binding is required after every reparse");
      calls.push({ source, elementId, replacement });
      const response = responseFor(source, [{ elementId, replacement }]);
      response.result = { updatedSource: response.result.updatedSource, elementId, replacement,
        previousSnippet: binding.snippet, replacedRange: binding.textRange };
      options.onEdit?.(calls.length, response);
      return response;
    },
  };
}
async function session(source = SOURCE, value = snapshot(source)) {
  const { SourceEditSession } = await editor;
  const instance = new SourceEditSession();
  instance.setSnapshot(source, value);
  return instance;
}
function transport() {
  const workers = [];
  class WorkerClass {
    constructor() { this.sent = []; this.terminated = false; workers.push(this); }
    postMessage(message) { this.sent.push(clone(message)); }
    terminate() { this.terminated = true; }
    emit(message) { this.onmessage({ data: message }); }
    ready(extra = {}) { this.emit({ kind: "ready", sourceEditing: true, sourceBatchEditing: true, ...extra }); }
  }
  return { workers, WorkerClass, workerUrl: "https://example.invalid/fm-render.worker.js", requestTimeoutMs: 1000 };
}

// Session laws and independent validation at the user-visible commit boundary.
test("batch preview is read-only and commits the exact Unicode/CRLF transaction once", async () => {
  const s = await session();
  const transaction = s.beginBatch(requests());
  const response = responseFor();
  const preview = s.prepareBatch(transaction, response);
  assert.equal(s.source, SOURCE);
  assert.equal(preview.changes.length, 2);
  assert.equal(preview.changes[0].startByte, bytes(SOURCE.slice(0, SOURCE.indexOf("A["))));
  assert.deepEqual(preview.warnings, ["fixture warning"]);
  assert.equal(s.commitBatch(preview), SOURCE.replace("A[old]", "A[日本語]").replace("B[two]", "B[😀 longer]"));
  assert.deepEqual(s.bindings.map((binding) => binding.snippet), ["A[日本語]", "B[😀 longer]"]);
  assert.throws(() => s.commitBatch(preview), /stale/);
});

test("selection changes do not alter the original batch revision or payload", async () => {
  const s = await session();
  const edits = requests();
  const transaction = s.beginBatch(edits);
  edits[0].replacement = "A[wrong]";
  edits.pop();
  s.select(s.bindings[1].elementId);
  assert.equal(transaction.edits.length, 2);
  assert.equal(transaction.edits[0].replacement, "A[日本語]");
  const preview = s.prepareBatch(transaction, responseFor());
  assert.ok(Object.isFrozen(preview) && Object.isFrozen(preview.changes) && Object.isFrozen(preview.changes[0]));
  assert.ok(Object.isFrozen(transaction.edits[0]));
  const response = responseFor();
  assert.equal(s.commitBatch(preview), response.result.updatedSource);
});

test("resetting even to identical source invalidates pending transactions and previews", async () => {
  for (const prepared of [false, true]) {
    const s = await session();
    const transaction = s.beginBatch(requests());
    const preview = prepared ? s.prepareBatch(transaction, responseFor()) : null;
    s.setSnapshot(SOURCE, snapshot(SOURCE));
    assert.throws(() => s.prepareBatch(transaction, responseFor()), /stale/);
    if (preview) assert.throws(() => s.commitBatch(preview), /stale/);
    assert.equal(s.source, SOURCE);
  }
});

test("cancel, another batch and a single edit all invalidate older batch tokens", async () => {
  for (const invalidate of [
    (s) => s.cancelBatch(),
    (s) => s.beginBatch(requests()),
    (s) => {
      const selected = s.select(s.bindings[0].elementId);
      s.replaceWithResponse(selected, "A[new]", responseFor(SOURCE, requests(SOURCE, { A: "A[new]" })));
    },
  ]) {
    const s = await session();
    const transaction = s.beginBatch(requests());
    const preview = s.prepareBatch(transaction, responseFor());
    invalidate(s);
    assert.throws(() => s.prepareBatch(transaction, responseFor()), /stale/);
    assert.throws(() => s.commitBatch(preview), /stale/);
  }
});

test("transactions and previews cannot be forged or transferred between sessions", async () => {
  const a = await session();
  const b = await session();
  const transaction = a.beginBatch(requests());
  b.beginBatch(requests());
  assert.throws(() => a.prepareBatch({ ...transaction }, responseFor()), /stale/);
  assert.throws(() => b.prepareBatch(transaction, responseFor()), /stale/);
  const preview = a.prepareBatch(transaction, responseFor());
  assert.throws(() => a.commitBatch({ ...preview }), /stale/);
  assert.throws(() => b.commitBatch(preview), /stale/);
});

test("source and transaction metadata are independently checked before preview", async () => {
  const mutations = [
    (r) => { r.result.updatedSource += "\nphantom"; },
    (r) => { r.result.changes.pop(); },
    (r) => { r.result.changes.reverse(); },
    (r) => { r.result.changes[1] = r.result.changes[0]; },
    (r) => { r.result.changes[0].elementId = "wrong"; },
    (r) => { r.result.changes[0].appliedElementId = ""; },
    (r) => { r.result.changes[0].previousSnippet = "wrong"; },
    (r) => { r.result.changes[0].replacement = "wrong"; },
    (r) => { r.result.changes[0].replacedRange.startByte -= 1; },
    (r) => { r.result.changes[0].replacedRange.endByte += 1; },
  ];
  for (const mutate of mutations) {
    const s = await session();
    const transaction = s.beginBatch(requests());
    const r = responseFor(); mutate(r);
    assert.throws(() => s.prepareBatch(transaction, r), /batch response/);
    assert.equal(s.source, SOURCE);
  }
});

test("stale, malformed or wrong new bindings cannot be committed", async () => {
  for (const mutate of [
    (r) => { r.snapshot = snapshot(SOURCE); },
    (r) => { r.snapshot.bindings[0].textRange.endByte = 1e9; },
    (r) => { r.snapshot.bindings[0].snippet = "wrong"; },
    (r) => { r.snapshot.bindings.push(r.snapshot.bindings[0]); },
    (r) => { r.snapshot.bindings = null; },
  ]) {
    const s = await session();
    const transaction = s.beginBatch(requests());
    const r = responseFor(); mutate(r);
    assert.throws(() => s.prepareBatch(transaction, r), /source map|source-map|snapshot/);
    assert.equal(s.source, SOURCE);
  }
});

test("mutating a successful response cannot change the reviewed result", async () => {
  const s = await session();
  const transaction = s.beginBatch(requests());
  const r = responseFor();
  const preview = s.prepareBatch(transaction, r);
  r.result.updatedSource = "corrupt";
  r.result.changes[0].replacement = "corrupt";
  r.snapshot.bindings[0].snippet = "corrupt";
  r.snapshot.parsed.warnings.push("corrupt");
  assert.equal(s.commitBatch(preview), responseFor().result.updatedSource);
  assert.equal(s.bindings[0].snippet, "A[日本語]");
  assert.deepEqual(preview.warnings, ["fixture warning"]);
});

test("failed response revalidation retires any earlier preview", async () => {
  const s = await session();
  const transaction = s.beginBatch(requests());
  const old = s.prepareBatch(transaction, responseFor());
  assert.throws(() => s.prepareBatch(transaction, { result: {} }), /batch response/);
  assert.throws(() => s.commitBatch(old), /stale/);
});

test("duplicate IDs, missing spans, overlap and malformed edits fail before dispatch", async () => {
  const s = await session();
  for (const edits of [null, [{}], [{ elementId: "missing", replacement: "x" }],
    [...requests(), requests()[0]], [{ elementId: requests()[0].elementId, replacement: "\ud800" }],
    new Array(1), Array.from({ length: 1025 }, (_, i) => ({ elementId: `x${i}`, replacement: "" }))]) {
    assert.throws(() => s.beginBatch(edits));
    assert.equal(s.source, SOURCE);
  }
  for (const contained of [false, true]) {
    const value = snapshot(SOURCE);
    const binding = value.bindings[0];
    value.bindings.push({ ...binding, elementId: "edge-alias", kind: "edge",
      snippet: contained ? "old" : binding.snippet,
      textRange: contained ? { startByte: binding.textRange.startByte + 2, endByte: binding.textRange.endByte - 1 } : binding.textRange });
    const overlapping = await session(SOURCE, value);
    assert.throws(() => overlapping.beginBatch([...requests(), { elementId: "edge-alias", replacement: "x" }]), /Overlapping/);
  }
});

test("empty and no-op transactions preserve bytes and report no changes", async () => {
  for (const edits of [[], requests(SOURCE, { A: "A[old]", B: "B[two]" })]) {
    const s = await session();
    const tx = s.beginBatch(edits);
    const preview = s.prepareBatch(tx, responseFor(SOURCE, edits));
    assert.deepEqual(preview.changes, []);
    assert.equal(s.commitBatch(preview), SOURCE);
  }
});

test("adjacent spans are legal but coincident insertion points are ambiguous", async () => {
  const source = "abc";
  const value = { bindings: [
    { elementId: "a", snippet: "a", textRange: { startByte: 0, endByte: 1 } },
    { elementId: "b", snippet: "b", textRange: { startByte: 1, endByte: 2 } },
  ] };
  const s = await session(source, value);
  assert.doesNotThrow(() => s.beginBatch([{ elementId: "a", replacement: "A" }, { elementId: "b", replacement: "B" }]));
  value.bindings[0].textRange = { startByte: 1, endByte: 1 }; value.bindings[0].snippet = "";
  const bad = await session(source, value);
  assert.throws(() => bad.beginBatch([{ elementId: "a", replacement: "A" }, { elementId: "b", replacement: "B" }]), /Overlapping/);
});

// Protocol and capability negotiation, including cold initialization and mutable caller input.
test("client waits for capability negotiation and sends a captured sourceBatch", async (t) => {
  const { createSourceWorkerClient } = await editor;
  const host = transport();
  const client = createSourceWorkerClient(host); t.after(() => client.dispose());
  const edits = requests();
  const pending = client.batchSource(SOURCE, edits);
  edits[0].replacement = "A[mutated]"; edits.pop();
  const worker = host.workers[0];
  assert.equal(worker.sent.length, 1);
  worker.ready();
  assert.deepEqual(worker.sent[1].edits, requests());
  worker.emit({ kind: "sourceBatchEdited", requestId: worker.sent[1].requestId, response: responseFor() });
  assert.deepEqual(await pending, responseFor());
});

test("old worker capability failure is explicit and leaves single-edit/render transport usable", async (t) => {
  const { createSourceWorkerClient } = await editor;
  const host = transport();
  const client = createSourceWorkerClient(host); t.after(() => client.dispose());
  host.workers[0].ready({ sourceBatchEditing: false });
  await assert.rejects(client.batchSource(SOURCE, requests()), /lacks atomic batch editing/);
  assert.equal(host.workers[0].sent.length, 1);
  assert.equal(host.workers[0].terminated, false);
  const pending = client.renderSource(SOURCE);
  host.workers[0].emit({ kind: "sourceRendered", requestId: host.workers[0].sent.at(-1).requestId,
    svg: "<svg/>", snapshot: snapshot(SOURCE) });
  assert.equal((await pending).svg, "<svg/>");
});

test("malformed batch requests do not cancel a valid client request", async (t) => {
  const { createSourceWorkerClient } = await editor;
  const host = transport(); const client = createSourceWorkerClient(host); t.after(() => client.dispose());
  host.workers[0].ready();
  const pending = client.batchSource(SOURCE, requests());
  await assert.rejects(client.batchSource(SOURCE, [{}]), /requires/);
  assert.equal(host.workers[0].sent.length, 2);
  host.workers[0].emit({ kind: "sourceBatchEdited", requestId: 1, response: responseFor() });
  assert.equal((await pending).result.updatedSource, responseFor().result.updatedSource);
});

test("new requests cancel old batches and delayed replies cannot settle the new request", async (t) => {
  const { createSourceWorkerClient } = await editor;
  const host = transport(); const client = createSourceWorkerClient(host); t.after(() => client.dispose());
  const worker = host.workers[0]; worker.ready();
  const old = client.batchSource(SOURCE, requests());
  const rejected = assert.rejects(old, { name: "AbortError" });
  const next = client.batchSource(SOURCE, requests(SOURCE, { A: "A[new]" }));
  await rejected;
  assert.deepEqual(worker.sent[2], { kind: "cancel", requestId: 1 });
  worker.emit({ kind: "sourceBatchEdited", requestId: 1, response: responseFor() });
  worker.emit({ kind: "noReply", requestId: 1 });
  const response = responseFor(SOURCE, requests(SOURCE, { A: "A[new]" }));
  worker.emit({ kind: "sourceBatchEdited", requestId: 2, response });
  assert.deepEqual(await next, response);
});

test("client rejects mismatched batch reply kinds and missing changes arrays", async () => {
  const { createSourceWorkerClient } = await editor;
  for (const malformed of ["kind", "changes"]) {
    const host = transport(); const client = createSourceWorkerClient(host);
    const worker = host.workers[0]; worker.ready();
    const pending = client.batchSource(SOURCE, requests());
    const rejection = assert.rejects(pending, /mismatched source worker/);
    const response = responseFor();
    if (malformed === "changes") delete response.result.changes;
    worker.emit({ kind: malformed === "kind" ? "sourceEdited" : "sourceBatchEdited", requestId: 1, response });
    await rejection; assert.equal(worker.terminated, true); client.dispose();
  }
});

test("batch disposal and timeouts settle callers instead of hanging", async () => {
  const { createSourceWorkerClient } = await editor;
  const host = transport(); const client = createSourceWorkerClient({ ...host, requestTimeoutMs: 5 });
  host.workers[0].ready();
  await assert.rejects(client.batchSource(SOURCE, requests()), /timed out/);
  assert.equal(host.workers[0].terminated, true); client.dispose();
  const second = createSourceWorkerClient(host);
  const pending = second.batchSource(SOURCE, requests());
  second.dispose(); await assert.rejects(pending, { name: "AbortError" });
});

// Main-thread fallback: the real host code calls the fixture's Rust-shaped functions.
test("fallback batch preserves exact source and rebinds IDs after right-to-left edits", async (t) => {
  const { createSourceEditorBackend } = await editor;
  const api = apiFixture(); let loads = 0;
  const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: async () => { loads++; return api; } });
  t.after(() => backend.dispose());
  const response = await backend.batchSource(SOURCE, requests());
  assert.equal(response.result.updatedSource, responseFor().result.updatedSource);
  assert.deepEqual(api.calls.map((call) => call.replacement), ["B[😀 longer]", "A[日本語]"]);
  assert.notEqual(api.calls[1].elementId, requests()[0].elementId);
  assert.deepEqual(response.result.changes.map((change) => change.elementId), requests().map((edit) => edit.elementId));
  const s = await session(); s.commitBatch(s.prepareBatch(s.beginBatch(requests()), response));
  assert.equal(s.source, responseFor().result.updatedSource);
  await backend.renderSource(SOURCE);
  assert.equal(loads, 1); assert.match(backend.target, /synchronous/);
});

test("fallback copies edits before a slow module load", async (t) => {
  const { createSourceEditorBackend } = await editor;
  let resolve; const loaded = new Promise((done) => { resolve = done; });
  const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: () => loaded });
  t.after(() => backend.dispose());
  const edits = requests(); const pending = backend.batchSource(SOURCE, edits);
  edits[0].replacement = "wrong"; edits.pop(); resolve(apiFixture());
  assert.equal((await pending).result.updatedSource, responseFor().result.updatedSource);
});

test("fallback cancellation between edits never returns a partial document", async (t) => {
  const { createSourceEditorBackend } = await editor;
  let backend;
  const api = apiFixture({ onEdit(number) { if (number === 1) queueMicrotask(() => backend.cancel()); } });
  backend = createSourceEditorBackend({ WorkerClass: null, loadModule: async () => api });
  t.after(() => backend.dispose());
  await assert.rejects(backend.batchSource(SOURCE, requests()), { name: "AbortError" });
  assert.equal(api.calls.length, 1);
});

test("fallback cancellation while loading performs no source work", async () => {
  const { createSourceEditorBackend } = await editor;
  for (const dispose of [false, true]) {
    const api = apiFixture(); let resolve;
    const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: () => new Promise((done) => { resolve = done; }) });
    const pending = backend.batchSource(SOURCE, requests());
    await tick();
    dispose ? backend.dispose() : backend.cancel(); resolve(api);
    await assert.rejects(pending, { name: "AbortError" });
    assert.equal(api.parses.length, 0); backend.dispose();
  }
});

test("a newer fallback render supersedes a batch", async (t) => {
  const { createSourceEditorBackend } = await editor;
  let backend; let next;
  const api = apiFixture({ onEdit(number) { if (number === 1) queueMicrotask(() => { next = backend.renderSource("flowchart LR\nZ[new]"); }); } });
  backend = createSourceEditorBackend({ WorkerClass: null, loadModule: async () => api }); t.after(() => backend.dispose());
  await assert.rejects(backend.batchSource(SOURCE, requests()), { name: "AbortError" });
  assert.ok((await next).svg.includes("data-length")); assert.equal(api.calls.length, 1);
});

test("fallback refuses missing or ambiguous rebound identities after an earlier edit", async (t) => {
  const { createSourceEditorBackend } = await editor;
  for (const ambiguous of [false, true]) {
    const api = apiFixture({ onEdit(number, response) {
      if (number !== 1) return;
      const a = response.snapshot.bindings[0];
      if (ambiguous) response.snapshot.bindings.push({ ...a, elementId: "alias" });
      else response.snapshot.bindings.shift();
    } });
    const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: async () => api }); t.after(() => backend.dispose());
    await assert.rejects(backend.batchSource(SOURCE, requests()), /changed or became ambiguous/);
    assert.equal(api.calls.length, 1);
  }
});

test("fallback rejects corrupt per-edit metadata, out-of-span changes and invalid snapshots", async (t) => {
  const { createSourceEditorBackend } = await editor;
  for (const mutate of [
    (r) => { r.result.elementId = "wrong"; },
    (r) => { r.result.replacedRange.startByte--; },
    (r) => { r.result.previousSnippet = "wrong"; },
    (r) => { r.result.updatedSource += "phantom"; },
    (r) => { r.snapshot.bindings[0].snippet = "wrong"; },
    () => { throw new Error("fixture engine failure"); },
  ]) {
    const api = apiFixture({ onEdit(_number, response) { mutate(response); } });
    const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: async () => api }); t.after(() => backend.dispose());
    await assert.rejects(backend.batchSource(SOURCE, requests()));
    assert.equal(api.calls.length, 1);
  }
});

test("empty and unchanged fallback batches do no edit work", async (t) => {
  const { createSourceEditorBackend } = await editor;
  const api = apiFixture(); const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: async () => api });
  t.after(() => backend.dispose());
  for (const edits of [[], requests(SOURCE, { A: "A[old]" })]) {
    const r = await backend.batchSource(SOURCE, edits);
    assert.equal(r.result.updatedSource, SOURCE); assert.deepEqual(r.result.changes, []);
  }
  assert.equal(api.calls.length, 0);
});

test("transport failure retries the whole batch privately, not a partial worker result", async (t) => {
  const { createSourceEditorBackend } = await editor;
  const host = transport(); const api = apiFixture();
  const backend = createSourceEditorBackend({ ...host, loadModule: async () => api }); t.after(() => backend.dispose());
  const worker = host.workers[0]; worker.ready();
  const pending = backend.batchSource(SOURCE, requests());
  worker.onerror({ message: "fixture transport crash" });
  const response = await pending;
  assert.equal(response.result.updatedSource, responseFor().result.updatedSource);
  assert.equal(api.calls[0].source, SOURCE);
  assert.match(backend.fallbackReason, /transport crash/);
});

test("engine and missing-capability errors are not retried synchronously", async (t) => {
  const { createSourceEditorBackend } = await editor;
  for (const capability of [true, false]) {
    const host = transport(); let loads = 0;
    const backend = createSourceEditorBackend({ ...host, loadModule: async () => { loads++; return apiFixture(); } });
    t.after(() => backend.dispose());
    const worker = host.workers[0]; worker.ready({ sourceBatchEditing: capability });
    const pending = backend.batchSource(SOURCE, requests());
    if (capability) worker.emit({ kind: "failed", requestId: 1, reason: "fixture engine rejected edit" });
    await assert.rejects(pending); assert.equal(loads, 0);
  }
});
