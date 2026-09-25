"use strict";

// Structural source transactions and transport contracts. The examples below supply explicit
// Rust-shaped responses; they are NOT a Mermaid parser or evidence of a Rust/WASM build.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const loaded = import(`data:text/javascript;base64,${Buffer.from(
  fs.readFileSync(path.join(__dirname, "fm-source-editor.js"), "utf8")).toString("base64")}`);
const bytes = (text) => Buffer.byteLength(text);
const emptySnapshot = () => ({ bindings: [], parsed: { warnings: [] } });

function example({ source = "%% café 🐉\r\nflowchart TD\r\n\ta[A]\r\n  b[B]\r\n", span = "a[A]",
  kind = "delete", text = "c[世界]", start, end, replacement } = {}) {
  const selectedStart = source.indexOf(span);
  assert.ok(selectedStart >= 0);
  const selectedEnd = selectedStart + span.length;
  const lineStart = source.slice(0, selectedStart).lastIndexOf("\n") + 1;
  const next = source.indexOf("\n", selectedEnd);
  const lineEnd = next < 0 ? source.length : next + 1;
  start ??= kind === "delete" ? lineStart : lineEnd;
  end ??= kind === "delete" ? lineEnd : start;
  replacement ??= kind === "delete" ? "" : "\tc[世界]\r\n";
  const updated = source.slice(0, start) + replacement + source.slice(end);
  return {
    source, kind, text,
    snapshot: { bindings: [{ elementId: "a", sourceId: "A", kind: "node", snippet: span,
      textRange: { startByte: bytes(source.slice(0, selectedStart)), endByte: bytes(source.slice(0, selectedEnd)) } }],
      parsed: { warnings: [] } },
    response: { result: { elementId: "a", replacedRange: { startByte: bytes(source.slice(0, start)), endByte: bytes(source.slice(0, end)) },
      previousSnippet: source.slice(start, end), replacement, updatedSource: updated }, snapshot: emptySnapshot() },
  };
}
async function prepare(data) {
  const { SourceEditSession } = await loaded;
  const session = new SourceEditSession();
  session.setSnapshot(data.source, data.snapshot);
  const selection = session.select("a");
  return { session, selection, run: () => session.prepareStructural(selection, data.kind, data.text, data.response) };
}

test("deletion is preview-only until confirmation, removes the full empty line and is single-use", async () => {
  const data = example();
  const { session, run } = await prepare(data);
  const preview = run();
  assert.equal(session.source, data.source);
  assert.equal(preview.previousSnippet, "\ta[A]\r\n");
  assert.equal(preview.replacement, "");
  assert.ok(Object.isFrozen(preview));
  assert.equal(session.commitStructural(preview), "%% café 🐉\r\nflowchart TD\r\n  b[B]\r\n");
  assert.throws(() => session.commitStructural(preview), /stale/);
});

test("an inline span may be deleted without removing its shared statement or next line", async () => {
  const source = "flowchart TD\n  a[A] --> b[B]\n  c[C]\n";
  const start = source.indexOf("a[A]");
  const data = example({ source, start, end: start + 4 });
  const { session, run } = await prepare(data);
  assert.equal(session.commitStructural(run()), "flowchart TD\n   --> b[B]\n  c[C]\n");
});

test("whole-statement deletion is represented honestly, not as semantic node removal", async () => {
  const data = example({ source: "flowchart TD\n  a[A] --> b[B]\n  b --> c\n", span: "a[A] --> b[B]" });
  const { session, run } = await prepare(data);
  const preview = run();
  assert.equal(preview.previousSnippet, "  a[A] --> b[B]\n");
  assert.equal(session.commitStructural(preview), "flowchart TD\n  b --> c\n");
});

test("insertion preserves exact Unicode, CRLF and indentation without rewriting existing bytes", async () => {
  const data = example({ kind: "insert" });
  data.response.snapshot.parsed.warnings = ["example recovery warning"];
  const { session, run } = await prepare(data);
  const preview = run();
  assert.equal(preview.startByte, preview.endByte);
  assert.deepEqual(preview.warnings, ["example recovery warning"]);
  assert.equal(session.source, data.source);
  assert.equal(session.commitStructural(preview), data.source.replace("\ta[A]\r\n", "\ta[A]\r\n\tc[世界]\r\n"));
});

test("insertion accepts EOF framing, multiline input, empty text and newline-only text", async () => {
  for (const [text, replacement] of [
    ["c[C]", "\n  c[C]\n"], ["c[C]\n  c --> a", "\n  c[C]\n  c --> a\n"],
    ["", "\n  \n"], ["\n", "\n  \n\n"], ["\r", "\n  \r\n"],
  ]) {
    const data = example({ source: "flowchart TD\n  a[A]", kind: "insert", text, replacement });
    const { session, run } = await prepare(data);
    assert.equal(session.commitStructural(run()), data.source + replacement);
  }
});

test("deletion may consume Unicode whitespace but must not remove a BOM or unrelated content", async () => {
  for (const [prefix, allowed] of [["\u0085", true], ["\ufeff", false], ["keep ", false]]) {
    const data = example({ source: `flowchart TD\n${prefix}a[A]\n` });
    const { run } = await prepare(data);
    if (allowed) assert.doesNotThrow(run);
    else assert.throws(run, /outside/);
  }
});

test("malformed transaction metadata, UTF-8 interiors and unrelated edits never commit", async () => {
  const mutations = [
    (r) => { r.result.elementId = "other"; },
    (r) => { r.result.previousSnippet = "wrong"; },
    (r) => { r.result.replacedRange.startByte = -1; },
    (r) => { r.result.replacedRange.endByte = 999; },
    (r) => { r.result.replacedRange.startByte = 8; }, // interior of é
    (r) => { r.result.updatedSource += "unrelated"; },
    (r) => { r.result.replacement = "not a deletion"; },
    (r) => { r.snapshot.bindings = null; },
    (r) => { r.snapshot.bindings = [{ elementId: "x", textRange: { startByte: 0, endByte: 1 }, snippet: "wrong" }]; },
  ];
  for (const mutate of mutations) {
    const data = example();
    mutate(data.response);
    const { session, run } = await prepare(data);
    assert.throws(run);
    assert.equal(session.source, data.source);
  }
});

test("a self-consistent transaction cannot remove a neighboring line", async () => {
  const source = "flowchart TD\n  a[A]\n  b[B]\n";
  const data = example({ source, start: source.indexOf("  a"), end: source.length });
  const { session, run } = await prepare(data);
  assert.throws(run, /outside/);
  assert.equal(session.source, source);
});

test("insertion rejects wrong location, changed submitted text and invented content", async () => {
  for (const options of [
    { start: 0, end: 0 }, { replacement: "\tc[wrong]\r\n" },
    { replacement: "\tc[世界]\nextra statement\n" }, { replacement: "c[世界]" },
  ]) {
    const data = example({ kind: "insert", ...options });
    const { session, run } = await prepare(data);
    assert.throws(run, /Insertion/);
    assert.equal(session.source, data.source);
  }
});

test("prepared snapshots detach from caller mutation and publish new bindings only on commit", async () => {
  const data = example();
  const updated = data.response.result.updatedSource;
  const start = updated.indexOf("b[B]");
  data.response.snapshot.bindings.push({ elementId: "new-b", kind: "node", snippet: "b[B]",
    textRange: { startByte: bytes(updated.slice(0, start)), endByte: bytes(updated.slice(0, start + 4)) } });
  const { session, run } = await prepare(data);
  const preview = run();
  data.response.result.updatedSource = "tampered";
  data.response.snapshot.bindings[0].snippet = "tampered";
  assert.equal(session.bindings[0].elementId, "a");
  session.commitStructural(preview);
  assert.equal(session.select("new-b").snippet, "b[B]");
  assert.equal(session.source, updated);
});

test("cancel, reselection, newer preview, foreign token and ABA source changes invalidate confirmation", async () => {
  for (const mutate of [
    (s) => s.cancelStructural(), (s) => s.select("a"),
    (s, d, run) => run(),
    (s, d) => { s.setSnapshot("", emptySnapshot()); s.setSnapshot(d.source, d.snapshot); s.select("a"); },
  ]) {
    const data = example();
    const { session, run } = await prepare(data);
    const preview = run();
    mutate(session, data, run);
    assert.throws(() => session.commitStructural(preview), /stale/);
    assert.equal(session.source, data.source);
  }
  const first = await prepare(example());
  const second = await prepare(example());
  const token = first.run();
  second.run();
  assert.throws(() => first.session.commitStructural({ ...token }), /stale/);
  assert.throws(() => second.session.commitStructural(token), /stale/);
});

test("structural edits have exact source undo/redo even without a new render", async () => {
  const { SourceHistory } = await loaded;
  const data = example({ kind: "insert" });
  const { session, run } = await prepare(data);
  const history = new SourceHistory(data.source);
  const updated = session.commitStructural(run());
  history.record(updated);
  assert.equal(history.undo(), data.source);
  assert.equal(history.redo(), updated);
});

class FixtureWorker {
  static latest;
  constructor() { FixtureWorker.latest = this; this.messages = []; this.terminated = false; }
  postMessage(message) { this.messages.push(message); }
  terminate() { this.terminated = true; }
  emit(message) { this.onmessage({ data: message }); }
  ready(extra = {}) { this.emit({ kind: "ready", sourceEditing: true, sourceDeletion: true, sourceInsertion: true, ...extra }); }
}
function reply(worker, kind, response) {
  worker.emit({ kind, requestId: worker.messages.at(-1).requestId, response });
}

test("the client sends structural operations and discriminates their typed replies", async () => {
  const { createSourceWorkerClient } = await loaded;
  const client = createSourceWorkerClient({ WorkerClass: FixtureWorker, workerUrl: "fixture" });
  const worker = FixtureWorker.latest;
  try {
    const pending = client.insertSource("input", "a", "C");
    assert.equal(worker.messages.length, 1, "wait for capabilities before sending");
    worker.ready();
    assert.deepEqual(worker.messages.at(-1), { kind: "sourceInsert", requestId: 1, input: "input", elementId: "a", text: "C" });
    const response = example().response;
    reply(worker, "sourceInserted", response);
    assert.equal(await pending, response);
    const deletion = client.deleteSource("input", "a");
    reply(worker, "sourceDeleted", response);
    assert.equal(await deletion, response);
  } finally { client.dispose(); }
});

test("structural capability absence and engine failures do not run synchronous fallback", async () => {
  const { createSourceEditorBackend } = await loaded;
  for (const missing of [true, false]) {
    const backend = createSourceEditorBackend({ WorkerClass: FixtureWorker, workerUrl: "fixture",
      loadModule: () => assert.fail("engine/capability error must not escape worker isolation") });
    const worker = FixtureWorker.latest;
    try {
      worker.ready({ sourceDeletion: !missing });
      const operation = backend.deleteSource("input", "a");
      const rejected = assert.rejects(operation, missing ? /lacks/ : /fixture refusal/);
      if (!missing) worker.emit({ kind: "failed", requestId: worker.messages.at(-1).requestId, reason: "fixture refusal" });
      await rejected;
      assert.equal(backend.target, "source worker");
      assert.equal(worker.terminated, false);
    } finally { backend.dispose(); }
  }
});

test("wrong operation replies are transport errors, not an accepted mutation", async () => {
  const { createSourceWorkerClient } = await loaded;
  const client = createSourceWorkerClient({ WorkerClass: FixtureWorker, workerUrl: "fixture" });
  const worker = FixtureWorker.latest;
  try {
    worker.ready();
    const pending = assert.rejects(client.deleteSource("input", "a"), /mismatched/);
    reply(worker, "sourceEdited", example().response);
    await pending;
    assert.equal(worker.terminated, true);
  } finally { client.dispose(); }
});

test("structural requests cancel older work and ignore its late replies", async () => {
  const { createSourceWorkerClient } = await loaded;
  const client = createSourceWorkerClient({ WorkerClass: FixtureWorker, workerUrl: "fixture" });
  const worker = FixtureWorker.latest;
  try {
    worker.ready();
    const obsolete = assert.rejects(client.deleteSource("old", "a"), { name: "AbortError" });
    const current = client.insertSource("new", "a", "B");
    worker.emit({ kind: "sourceDeleted", requestId: 1, response: example().response });
    assert.deepEqual(worker.messages[2], { kind: "cancel", requestId: 1 });
    const response = example({ kind: "insert" }).response;
    reply(worker, "sourceInserted", response);
    assert.equal(await current, response);
    await obsolete;
  } finally { client.dispose(); }
});

test("transport failure replays the requested structural operation through the exact local API", async () => {
  const { createSourceEditorBackend } = await loaded;
  const calls = [];
  const response = example().response;
  const backend = createSourceEditorBackend({ WorkerClass: FixtureWorker, workerUrl: "fixture", loadModule: () => ({
    applyParseLensDelete: (...args) => { calls.push(["delete", ...args]); return response; },
    applyParseLensInsertLineAfter: (...args) => { calls.push(["insert", ...args]); return response; },
  }) });
  const worker = FixtureWorker.latest;
  try {
    worker.ready();
    const operation = backend.deleteSource("source", "a");
    worker.onerror({ message: "fixture transport crash" });
    assert.equal(await operation, response);
    assert.equal(await backend.insertSource("source", "a", "C"), response);
    assert.deepEqual(calls, [["delete", "source", "a"], ["insert", "source", "a", "C"]]);
    assert.match(backend.target, /synchronous/);
    assert.equal(worker.terminated, true);
  } finally { backend.dispose(); }
});

test("local structural APIs reject invalid text/IDs and missing exports instead of rewriting source", async () => {
  const { createSourceEditorBackend } = await loaded;
  const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: () => ({}) });
  try {
    await assert.rejects(backend.deleteSource("source", "a"), /applyParseLensDelete/);
    await assert.rejects(backend.insertSource("source", "a", "C"), /applyParseLensInsertLineAfter/);
    await assert.rejects(backend.insertSource("source", "a", "\ud800"), /surrogate/);
    await assert.rejects(backend.deleteSource("source", ""), /element ID/);
  } finally { backend.dispose(); }
});

test("cancellation during fallback loading prevents obsolete structural API entry", async () => {
  const { createSourceEditorBackend } = await loaded;
  let resolve;
  const loadedModule = new Promise((done) => { resolve = done; });
  const backend = createSourceEditorBackend({ WorkerClass: null, loadModule: () => loadedModule });
  try {
    const pending = assert.rejects(backend.deleteSource("source", "a"), { name: "AbortError" });
    backend.cancel();
    resolve({ applyParseLensDelete: () => assert.fail("cancelled edit entered local WASM") });
    await pending;
  } finally { backend.dispose(); }
});
