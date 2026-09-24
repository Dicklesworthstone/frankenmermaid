"use strict";

// Tests the browser's revision and UTF-8 boundary, NOT the Rust parser. The small fixture
// adapter below implements only the documented ParseLens JSON contract; it is never shipped.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const moduleSource = fs.readFileSync(path.join(__dirname, "fm-source-editor.js"), "utf8");
const loaded = import(`data:text/javascript;base64,${Buffer.from(moduleSource).toString("base64")}`);

function snapshot(source) {
  const bindings = [];
  for (const match of source.matchAll(/\[[^\]]*\]/gu)) {
    bindings.push({
      elementId: `fm-node-${bindings.length}`,
      sourceId: `node-${bindings.length}`,
      kind: "Node",
      textRange: {
        startByte: Buffer.byteLength(source.slice(0, match.index)),
        endByte: Buffer.byteLength(source.slice(0, match.index + match[0].length)),
      },
      snippet: match[0],
    });
  }
  return { bindings, parsed: { warnings: [] } };
}

function adapter() {
  const calls = [];
  return {
    calls,
    parseLens: snapshot,
    applyParseLensEdit(source, elementId, replacement) {
      calls.push({ source, elementId, replacement });
      const binding = snapshot(source).bindings.find((item) => item.elementId === elementId);
      assert.ok(binding, "the adapter must receive a real fixture binding");
      const bytes = Buffer.from(source);
      const updatedSource = Buffer.concat([
        bytes.subarray(0, binding.textRange.startByte), Buffer.from(replacement),
        bytes.subarray(binding.textRange.endByte),
      ]).toString("utf8");
      return { result: { updatedSource }, snapshot: snapshot(updatedSource) };
    },
  };
}

async function session(api = adapter()) {
  const { SourceEditSession } = await loaded;
  return new SourceEditSession(api);
}

test("UTF-8 ranges select the correct UTF-16 text after accented and astral characters", async () => {
  const source = "%% café 🐉\r\nflowchart TD\r\n  a[世界 😀] --> b[Beta]\r\n";
  const api = adapter();
  const edit = await session(api);
  edit.setSource(source);
  const selected = edit.select("fm-node-0");
  assert.equal(source.slice(selected.start, selected.end), "[世界 😀]");
  assert.equal(selected.start, source.indexOf("["));
  assert.equal(edit.bindingAt(selected.start + 1).elementId, selected.elementId);
  const updated = edit.replace(selected, "[Renamed 🌍]");
  assert.equal(updated, source.replace("[世界 😀]", "[Renamed 🌍]"));
  assert.equal(api.calls.length, 1, "the mutation must go through the WASM adapter");
  assert.ok(updated.endsWith("b[Beta]\r\n"));
  assert.equal(edit.select("fm-node-1").start, updated.indexOf("[Beta]"));
});

test("identity edits preserve comments, directives, spacing, and line endings", async () => {
  for (const source of [
    "%%{init: {'theme':'forest'}}%%\r\nflowchart TD\r\n  %% comment\r\n  a[Alpha]   --> b[Beta]",
    "flowchart TD\n\n a[é😀] --> b[Beta]\n",
  ]) {
    const edit = await session();
    edit.setSource(source);
    const selected = edit.select("fm-node-0");
    assert.equal(edit.replace(selected, selected.snippet), source);
  }
});

test("new source invalidates tokens even when the text later returns to the same value", async () => {
  const api = adapter();
  const edit = await session(api);
  edit.setSource("a[A]");
  const selected = edit.select("fm-node-0");
  edit.setSource("b[B]");
  edit.setSource("a[A]");
  assert.throws(() => edit.replace(selected, "[X]"), /stale/);
  assert.equal(api.calls.length, 0);
  assert.equal(edit.source, "a[A]");
});

test("successful edits, changed selections, and forged tokens cannot reuse an old range", async () => {
  const edit = await session();
  edit.setSource("a[A] --> b[B]");
  const first = edit.select("fm-node-0");
  const second = edit.select("fm-node-1");
  assert.throws(() => edit.replace(first, "[X]"), /stale/);
  assert.throws(() => edit.replace({ ...second }, "[X]"), /stale/);
  assert.equal(edit.replace(second, "[Longer]"), "a[A] --> b[Longer]");
  assert.throws(() => edit.replace(second, "[Y]"), /stale/);
});

test("foreign-session tokens and unknown elements are refused", async () => {
  const first = await session();
  const second = await session();
  first.setSource("a[A]");
  second.setSource("a[A]");
  const token = first.select("fm-node-0");
  second.select("fm-node-0");
  assert.throws(() => second.replace(token, "[B]"), /stale/);
  assert.throws(() => first.select("missing"), /no editable source span/);
});

test("malformed, mismatched, duplicate, and non-boundary ranges fail closed", async () => {
  const badSnapshots = [
    null, {}, { bindings: null },
    { bindings: [{ elementId: "x", textRange: { startByte: 1, endByte: 2 } }] },
    { bindings: [{ elementId: "x", textRange: { startByte: -1, endByte: 0 } }] },
    { bindings: [{ elementId: "x", textRange: { startByte: 0, endByte: 999 } }] },
    { bindings: [{ elementId: "x", textRange: { startByte: 2, endByte: 0 } }] },
    { bindings: [{ elementId: "x", textRange: { startByte: 0, endByte: 2 }, snippet: "wrong" }] },
    { bindings: [0, 1].map(() => ({ elementId: "x", textRange: { startByte: 0, endByte: 2 } })) },
  ];
  for (const bad of badSnapshots) {
    const api = adapter();
    const edit = await session(api);
    edit.setSource("a[A]");
    const selected = edit.select("fm-node-0");
    api.parseLens = () => bad;
    assert.throws(() => edit.setSource("é[A]"));
    assert.equal(edit.source, "é[A]", "typed source must not roll back on parse failure");
    assert.deepEqual(edit.bindings, []);
    assert.throws(() => edit.replace(selected, "[B]"), /stale/);
  }
});

test("synthetic unbound elements are omitted without inventing source spans", async () => {
  const api = adapter();
  api.parseLens = () => ({ bindings: [{ elementId: "synthesized", textRange: null }] });
  const edit = await session(api);
  edit.setSource("");
  assert.deepEqual(edit.bindings, []);
  assert.equal(edit.bindingAt(0), null);
});

test("parser failures and unpaired surrogates invalidate previous bindings", async () => {
  for (const bad of ["throw", "[\ud800]"]) {
    const api = adapter();
    api.parseLens = (source) => {
      if (source === "throw") throw new Error("fixture parse failed");
      return snapshot(source);
    };
    const edit = await session(api);
    edit.setSource("a[A]");
    const selected = edit.select("fm-node-0");
    assert.throws(() => edit.setSource(bad));
    assert.throws(() => edit.replace(selected, "[B]"), /stale/);
    assert.deepEqual(edit.bindings, []);
  }
});

test("an invalid edit response cannot commit damage outside the selected span", async () => {
  for (const failSnapshot of [false, true]) {
    const api = adapter();
    api.applyParseLensEdit = () => ({
      result: { updatedSource: failSnapshot ? "a[X] --> b[B]" : "deleted everything" },
      snapshot: failSnapshot ? {} : snapshot("deleted everything"),
    });
    const edit = await session(api);
    const source = "a[A] --> b[B]";
    edit.setSource(source);
    const selected = edit.select("fm-node-0");
    assert.throws(() => edit.replace(selected, "[X]"));
    assert.equal(edit.source, source);
    assert.equal(edit.bindings.length, 2);
  }
});

test("selection lookup uses containing half-open ranges and prefers the narrowest span", async () => {
  const api = adapter();
  api.parseLens = () => ({ bindings: [
    { elementId: "wide", textRange: { startByte: 0, endByte: 8 } },
    { elementId: "narrow", textRange: { startByte: 2, endByte: 5 } },
  ] });
  const edit = await session(api);
  edit.setSource("abcdefgh");
  assert.equal(edit.bindingAt(3).elementId, "narrow");
  assert.equal(edit.bindingAt(1, 6).elementId, "wide");
  assert.equal(edit.bindingAt(5).elementId, "wide");
  assert.equal(edit.bindingAt(8), null);
  assert.equal(edit.bindingAt(-1), null);
  assert.equal(edit.bindingAt(3, 2), null);
  assert.equal(edit.bindingAt(NaN), null);
});

test("missing WASM APIs and invalid replacement text are actionable failures", async () => {
  await assert.rejects(session({}), /lacks source-editing APIs/);
  const api = adapter();
  const edit = await session(api);
  edit.setSource("a[A]");
  const selected = edit.select("fm-node-0");
  assert.throws(() => edit.replace(selected, "[\udfff]"), /surrogate/);
  assert.throws(() => edit.replace(selected, null), /must be text/);
  assert.equal(api.calls.length, 0);
});

test("history restores exact source, including invalid Mermaid and mixed line endings", async () => {
  const { SourceHistory } = await loaded;
  const first = "%% café 🐉\r\nflowchart TD\r\n a[A]\n";
  const second = "flowchart TD\n a[B]\n";
  const broken = "flowchart TD\n a[";
  const history = new SourceHistory(first);
  history.record(second);
  history.record(broken);
  assert.equal(history.undo(), second);
  assert.equal(history.undo(), first);
  assert.equal(history.undo(), first);
  assert.equal(history.redo(), second);
  assert.equal(history.redo(), broken);
  assert.equal(history.redo(), broken);
});

test("unchanged renders preserve redo, while new edits discard the abandoned future", async () => {
  const { SourceHistory } = await loaded;
  const history = new SourceHistory("A");
  history.record("B");
  history.record("C");
  assert.equal(history.undo(), "B");
  history.record("B");
  assert.equal(history.canRedo, true);
  assert.equal(history.redo(), "C");
  history.undo();
  history.record("D");
  assert.equal(history.canRedo, false);
  assert.equal(history.redo(), "D");
  assert.equal(history.undo(), "B");
  assert.equal(history.undo(), "A");
});

test("history enforces the entry limit without truncating the current source", async () => {
  const { SourceHistory } = await loaded;
  const history = new SourceHistory("A", { maxEntries: 3 });
  for (const source of ["B", "C", "D", "E"]) history.record(source);
  assert.equal(history.undo(), "D");
  assert.equal(history.undo(), "C");
  assert.equal(history.canUndo, false);
  assert.equal(history.redo(), "D");
  assert.equal(history.redo(), "E");
});

test("history enforces its text budget and retains an oversized document intact", async () => {
  const { SourceHistory } = await loaded;
  const history = new SourceHistory("AAA", { maxCodeUnits: 8 });
  history.record("BBB");
  history.record("CCC");
  assert.equal(history.undo(), "BBB");
  assert.equal(history.canUndo, false);
  history.redo();
  const oversized = "😀".repeat(100);
  history.record(oversized);
  assert.equal(history.source, oversized);
  assert.equal(history.canUndo, false);
  history.record("D");
  assert.equal(history.source, "D");
  assert.equal(history.canUndo, false);
});

test("history budgets remain correct after undo, branching, and eviction", async () => {
  const { SourceHistory } = await loaded;
  const history = new SourceHistory("11", { maxCodeUnits: 6 });
  history.record("22");
  history.record("33");
  history.undo();
  history.record("44"); // Discard 33; it must not count against the memory budget.
  assert.equal(history.undo(), "22");
  assert.equal(history.undo(), "11");
  history.redo();
  history.redo();
  history.record("55");
  assert.equal(history.undo(), "44");
  assert.equal(history.undo(), "22");
  assert.equal(history.canUndo, false);
});

test("history rejects invalid limits and non-text updates without changing its state", async () => {
  const { SourceHistory } = await loaded;
  for (const options of [{ maxEntries: 0 }, { maxEntries: 1.5 }, { maxCodeUnits: -1 }, { maxCodeUnits: Infinity }]) {
    assert.throws(() => new SourceHistory("a", options), /limits/);
  }
  const history = new SourceHistory("a");
  assert.throws(() => history.record(null), /must be text/);
  assert.equal(history.source, "a");
  assert.equal(history.canUndo, false);
});
