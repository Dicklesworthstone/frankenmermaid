"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");
const { createRenderSnapshot, checkedSourceBindings, planSourceEdit, planSourceBatch } = require("../preview-contract.cjs");
const { createPreviewController } = require("../media/preview.js");

// These tests execute the production host/controller. Only VS Code, DOM, and Rust-WASM are
// boundary doubles; passing them is not a claim of an installed VS Code or real WASM run.
const settle = () => new Promise((resolve) => setImmediate(resolve));
class Uri {
  constructor(filePath) { this.path = filePath; this.scheme = "file"; }
  toString() { return `file://${this.path}`; }
  static joinPath(uri, ...parts) { return new Uri(path.posix.join(uri.path, ...parts)); }
}
class Range {
  constructor(sl, sc, el, ec) {
    this.start = typeof sl === "object" ? sl : { line: sl, character: sc };
    this.end = typeof sl === "object" ? sc : { line: el, character: ec };
  }
}
function makeDocument(text, fileName = "/work/graph.mmd", eol = 1) {
  return { text, fileName, languageId: fileName.endsWith(".md") ? "markdown" : "mermaid",
    version: 3, eol, isClosed: false, uri: new Uri(fileName),
    getText() { return this.text; },
    offsetAt(position) {
      const lines = this.text.split(/(?<=\n)/u);
      return lines.slice(0, position.line).join("").length + position.character;
    },
  };
}
function bindingFor(source, snippet, elementId = "fm-node-a-0") {
  const offset = source.indexOf(snippet);
  assert.ok(offset >= 0, `missing test snippet ${snippet}`);
  const startByte = Buffer.byteLength(source.slice(0, offset));
  return { elementId, sourceId: "A", kind: "node", snippet,
    textRange: { startByte, endByte: startByte + Buffer.byteLength(snippet) } };
}
function receipt(source, binding, replacement) {
  const bytes = Buffer.from(source);
  const { startByte, endByte } = binding.textRange;
  return { result: { elementId: binding.elementId, replacement,
    previousSnippet: binding.snippet, replacedRange: { startByte, endByte },
    updatedSource: Buffer.concat([bytes.subarray(0, startByte), Buffer.from(replacement), bytes.subarray(endByte)]).toString() },
    snapshot: { bindings: [], parsed: { warnings: [] } } };
}
function editFixture(doc, snippet, replacement, diagramId = 0) {
  const snapshot = createRenderSnapshot(doc, 1, doc.fileName);
  const block = snapshot.blocks[diagramId];
  const binding = bindingFor(block.source, snippet);
  const reports = new Map([[diagramId, { bindings: checkedSourceBindings(block, [binding]) }]]);
  const message = { type: "apply-source-edit", requestId: 1, documentVersion: doc.version,
    diagramId, elementId: binding.elementId, editId: 1, replacement,
    result: receipt(block.source, binding, replacement).result };
  return { snapshot, binding, reports, message };
}
function plan(doc, snippet, replacement, diagramId) {
  const f = editFixture(doc, snippet, replacement, diagramId);
  return planSourceEdit(doc, f.snapshot, f.reports, f.message);
}

test("source replacement preserves Unicode and all text outside a shared statement", () => {
  const doc = makeDocument("%% café 😀\nflowchart LR\nA[😀] --> B\nC --> D\n");
  const result = plan(doc, "A[😀] --> B", "A[新しい] --> Z");
  assert.equal(result.after, "%% café 😀\nflowchart LR\nA[新しい] --> Z\nC --> D\n");
  assert.deepEqual(result.range, { start: { line: 2, character: 0 }, end: { line: 2, character: 11 } });
  assert.equal(result.newText, "A[新しい] --> Z");
});

test("multiline Markdown replacement restores fence indent and CRLF without touching siblings", () => {
  const doc = makeDocument("# prose 😀\r\n  ```mermaid\r\n  flowchart LR\r\n   A --> B\r\n   C --> D\r\n  ```\r\nTail\r\n~~~mermaid\r\npie\r\n~~~\r\n", "/work/readme.md", 2);
  const result = plan(doc, "A --> B\n C --> D", "A[😀] --> Q\n R --> S\n T --> U");
  assert.equal(result.newText, "A[😀] --> Q\r\n   R --> S\r\n   T --> U");
  assert.equal(result.after, "# prose 😀\r\n  ```mermaid\r\n  flowchart LR\r\n   A[😀] --> Q\r\n   R --> S\r\n   T --> U\r\n  ```\r\nTail\r\n~~~mermaid\r\npie\r\n~~~\r\n");
});

test("edits can target the second diagram without rewriting the first or unclosed EOF", () => {
  const doc = makeDocument("```mermaid\nA\n```\ntext\n  ~~~mermaid\n  flowchart LR\n  A --> B", "/work/readme.md");
  assert.equal(plan(doc, "A --> B", "A --> C", 1).after,
    "```mermaid\nA\n```\ntext\n  ~~~mermaid\n  flowchart LR\n  A --> C");
});

test("standalone CRLF, no-op and empty replacement retain exact splice semantics", () => {
  const doc = makeDocument("flowchart LR\r\nA --> B\r\nC --> D\r\n", undefined, 2);
  assert.equal(plan(doc, "A --> B", "A --> Q\r\nQ --> B").after,
    "flowchart LR\r\nA --> Q\r\nQ --> B\r\nC --> D\r\n");
  assert.equal(plan(doc, "A --> B", "A --> B").changed, false);
  assert.equal(plan(doc, "A --> B", "").after, "flowchart LR\r\n\r\nC --> D\r\n");
});

test("malformed receipts, Unicode loss, oversized replacements and stale sources fail closed", () => {
  const doc = makeDocument("flowchart LR\nA --> B\n");
  for (const corrupt of [
    (m) => { m.result.elementId = "another"; },
    (m) => { m.result.previousSnippet = "not source"; },
    (m) => { m.result.replacement = "other"; },
    (m) => { m.result.replacedRange.endByte += 1; },
    (m) => { m.result.updatedSource += "\nC --> D"; },
    (m) => { m.elementId = "unknown"; },
    (m) => { m.diagramId = -1; },
    (m) => { m.replacement = "\ud800"; },
    (m) => { m.replacement = "x".repeat(2 * 1024 * 1024 + 1); },
  ]) {
    const f = editFixture(doc, "A --> B", "A --> Z"); corrupt(f.message);
    assert.throws(() => planSourceEdit(doc, f.snapshot, f.reports, f.message));
  }
  const f = editFixture(doc, "A --> B", "A --> Z"); doc.text += "external";
  assert.throws(() => planSourceEdit(doc, f.snapshot, f.reports, f.message), /Source changed/u);
});

test("Markdown fence escape and incompatible native line endings cannot change the document", () => {
  const doc = makeDocument("```mermaid\nflowchart LR\nA --> B\n```\nprose", "/work/readme.md");
  assert.throws(() => plan(doc, "A --> B", "A\n```\nreplaced prose\n```mermaid\nB"), /fence/u);
  const mixed = makeDocument("flowchart LR\r\nA --> B", undefined, 2);
  assert.throws(() => plan(mixed, "A --> B", "A\nB"), /line endings/u);
});

async function hostHarness(t, doc = makeDocument("flowchart LR\nA --> B\n")) {
  const messages = [], errors = [], editCalls = [], listeners = new Map();
  const disposable = { dispose() {} };
  let receive, onDispose, command;
  const controls = { doc, messages, errors, editCalls, rejectEdit: false, openEditor: null };
  const editor = { document: doc, viewColumn: 1, selection: new Range(0, 0, 0, 0), revealRange() {},
    async edit(build, options) {
      const changes = [];
      build({ replace: (range, text) => changes.push({ range, text }) });
      editCalls.push({ changes, options });
      if (controls.rejectEdit) return false;
      const original = doc.text;
      const ordered = changes.map((change) => ({ ...change, start: doc.offsetAt(change.range.start), end: doc.offsetAt(change.range.end) }))
        .sort((a, b) => b.start - a.start);
      for (const change of ordered) doc.text = doc.text.slice(0, change.start) + change.text + doc.text.slice(change.end);
      controls.undo = () => { doc.text = original; doc.version += 1; };
      controls.redo = () => { doc.text = controls.after; doc.version += 1; };
      controls.after = doc.text;
      doc.version += 1;
      listeners.get("change")({ document: doc, contentChanges: changes });
      return true;
    },
  };
  const panel = { reveal() {}, dispose() { onDispose?.(); }, onDidDispose(fn) { onDispose = fn; return disposable; },
    webview: { cspSource: "vscode-webview-resource:", asWebviewUri: (uri) => uri.toString(),
      onDidReceiveMessage(fn) { receive = fn; return disposable; }, postMessage(message) { messages.push(message); return Promise.resolve(true); } } };
  const vscode = { Uri, Range, Selection: Range, ViewColumn: { One: 1, Beside: 2 },
    Diagnostic: class { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
    DiagnosticSeverity: { Error: 0, Warning: 1, Information: 2, Hint: 3 }, TextEditorRevealType: { InCenterIfOutsideViewport: 1 },
    languages: { createDiagnosticCollection: () => ({ set() {}, delete() {}, dispose() {} }) },
    commands: { registerCommand(name, fn) { command = fn; return disposable; } },
    window: { activeTextEditor: editor, createWebviewPanel: () => panel,
      showTextDocument: async () => controls.openEditor ? controls.openEditor() : editor,
      showWarningMessage: (value) => errors.push(value), showErrorMessage: (value) => errors.push(value),
      onDidChangeTextEditorSelection: () => disposable },
    workspace: { fs: { stat: async () => ({ type: 1 }) }, asRelativePath: () => doc.fileName,
      getConfiguration: () => ({ get: () => 0 }),
      onDidChangeTextDocument: (fn) => { listeners.set("change", fn); return disposable; },
      onDidChangeConfiguration: () => disposable, onDidCloseTextDocument: () => disposable },
  };
  const filename = path.join(__dirname, "..", "extension.js");
  const sandbox = { module: { exports: {} }, require: (name) => name === "vscode" ? vscode
    : name === "./preview-contract.cjs" ? require("../preview-contract.cjs") : require(name), Buffer, setTimeout, clearTimeout };
  vm.runInNewContext(fs.readFileSync(filename, "utf8"), sandbox, { filename });
  const extension = sandbox.module.exports;
  extension.activate({ extensionUri: new Uri("/repo/extensions/vscode-frankenmermaid"), subscriptions: [] });
  t.after(() => extension.deactivate());
  await command(); await receive({ type: "ready" });
  controls.editor = editor;
  controls.receive = (message) => receive(message);
  controls.change = (text) => { doc.text = text; doc.version += 1; listeners.get("change")({ document: doc, contentChanges: [{}] }); };
  controls.render = () => messages.filter((m) => m.type === "render").at(-1);
  controls.report = async (snippet = "A --> B") => {
    const m = controls.render();
    const binding = bindingFor(m.diagrams[0].source, snippet);
    await receive({ type: "rendered", requestId: m.requestId, documentVersion: m.documentVersion,
      reports: [{ id: 0, bindings: [binding], diagnostics: [] }] });
    return binding;
  };
  return controls;
}
function messageFor(h, binding, replacement, editId = 1) {
  const m = h.render();
  return { type: "apply-source-edit", requestId: m.requestId, documentVersion: m.documentVersion, diagramId: 0,
    elementId: binding.elementId, editId, replacement, result: receipt(m.diagrams[0].source, binding, replacement).result };
}

test("production extension applies exactly one native undoable transaction and rejects replay", async (t) => {
  const h = await hostHarness(t); const binding = await h.report();
  const message = messageFor(h, binding, "A --> C");
  await h.receive(message);
  assert.equal(h.doc.text, "flowchart LR\nA --> C\n");
  assert.equal(h.editCalls.length, 1);
  assert.equal(h.editCalls[0].changes.length, 1);
  assert.equal(h.editCalls[0].options.undoStopBefore, true);
  assert.equal(h.editCalls[0].options.undoStopAfter, true);
  assert.equal(h.messages.filter((m) => m.type === "source-edit-result").at(-1).ok, true);
  await h.receive(message); assert.equal(h.editCalls.length, 1);
  h.undo(); assert.equal(h.doc.text, "flowchart LR\nA --> B\n");
  h.redo(); assert.equal(h.doc.text, "flowchart LR\nA --> C\n");
});

test("production host rejects typing during editor opening and concurrent apply requests", async (t) => {
  const h = await hostHarness(t); const binding = await h.report();
  let resolve;
  h.openEditor = () => new Promise((done) => { resolve = done; });
  const first = h.receive(messageFor(h, binding, "A --> C"));
  await h.receive(messageFor(h, binding, "A --> D", 2));
  assert.match(h.messages.at(-1).message, /Another source edit/u);
  h.change("flowchart LR\nA --> typed\n");
  resolve(h.editor); await first;
  assert.equal(h.editCalls.length, 0);
  assert.equal(h.doc.text, "flowchart LR\nA --> typed\n");
  assert.equal(h.messages.at(-1).ok, false);
});

test("a rejected native write releases the lock and a no-op creates no undo entry", async (t) => {
  const h = await hostHarness(t); const binding = await h.report();
  await h.receive(messageFor(h, binding, "A --> B"));
  assert.equal(h.editCalls.length, 0);
  h.rejectEdit = true; await h.receive(messageFor(h, binding, "A --> C", 2));
  assert.equal(h.messages.at(-1).ok, false);
  assert.equal(h.doc.text, "flowchart LR\nA --> B\n");
  h.rejectEdit = false; await h.receive(messageFor(h, binding, "A --> D", 3));
  assert.equal(h.doc.text, "flowchart LR\nA --> D\n");
});

test("invalid host receipts and an editor for another document never reach native edit", async (t) => {
  const h = await hostHarness(t); const binding = await h.report();
  const message = messageFor(h, binding, "A --> C"); message.result.updatedSource += "escape";
  await h.receive(message); assert.equal(h.editCalls.length, 0);
  h.openEditor = () => ({ ...h.editor, document: makeDocument("other", "/work/other.mmd") });
  await h.receive(messageFor(h, binding, "A --> C", 2));
  assert.equal(h.editCalls.length, 0);
});

class Element {
  constructor(tag) { this.localName = tag; this.children = []; this.attrs = new Map(); this.listeners = new Map(); this._text = ""; this._value = ""; }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  set textContent(value) { this._text = String(value); this.children = []; }
  get value() { return this._value; }
  set value(value) { this._value = String(value).replace(/\r\n|\r/gu, "\n"); }
  get attributes() { return [...this.attrs].map(([name, value]) => ({ name, value })); }
  get id() { return this.getAttribute("id") || ""; }
  setAttribute(name, value) { this.attrs.set(name, String(value)); }
  getAttribute(name) { return this.attrs.get(name) ?? null; }
  removeAttribute(name) { this.attrs.delete(name); }
  append(...nodes) { for (const node of nodes) { node.parent = this; this.children.push(node); } }
  replaceChildren(...nodes) { this.children = []; this._text = ""; this.append(...nodes); }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter((node) => node !== this); }
  focus() { this.focused = true; }
  addEventListener(type, fn) { this.listeners.set(type, fn); }
  querySelectorAll(selector) {
    return this.children.flatMap((node) => [
      ...(selector === "*" || node.localName === selector || (selector === "[id]" && node.id) ? [node] : []), ...node.querySelectorAll(selector),
    ]);
  }
  attachShadow() { this.shadowRoot = new Element("shadow-root"); return this.shadowRoot; }
  click() { this.listeners.get("click")?.({ preventDefault() {}, stopPropagation() {} }); }
}
async function controllerHarness(t, source = "flowchart LR\nA --> B\n",
  fragments = [{ snippet: "A --> B", id: "fm-node-a-0" }]) {
  const root = new Element("main"), status = new Element("div"), messages = [], calls = [], listeners = new Map();
  const engine = { default: async () => {}, renderSvg: () => "<svg/>",
    parseLens(input) { return { parsed: { warnings: [] }, bindings: fragments.map((item) => bindingFor(input, item.snippet, item.id)) }; },
    applyParseLensEdit(input, id, replacement) { calls.push({ input, id, replacement });
      return receipt(input, bindingFor(input, fragments.find((item) => item.id === id).snippet, id), replacement); },
  };
  const document = { body: { dataset: { wasmModule: "module", wasmBinary: "bytes", styleNonce: "nonce" } },
    getElementById: (id) => id === "preview" ? root : status, createElement: (tag) => new Element(tag), importNode: (node) => node };
  const window = { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: (name) => listeners.delete(name),
    DOMParser: class { parseFromString() { const svg = new Element("svg"); svg.namespaceURI = "http://www.w3.org/2000/svg";
      for (const fragment of fragments) { const node = new Element("g"); node.setAttribute("id", fragment.id); svg.append(node); }
      return { documentElement: svg, querySelector: () => null }; } },
    XMLSerializer: class { serializeToString() { return '<svg xmlns="http://www.w3.org/2000/svg"/>'; } },
  };
  const controller = createPreviewController({ document, window, vscode: { postMessage: (m) => messages.push(m) },
    loadEngine: async () => engine, yieldToHost: async () => {} });
  t.after(() => controller.dispose()); await controller.start();
  const send = (message) => listeners.get("message")({ data: message });
  const render = (requestId = 1) => send({ type: "render", requestId, documentVersion: requestId + 2,
    title: "graph.mmd", diagrams: [{ id: 0, source, startLine: 0 }] });
  render(); await settle();
  const button = (label) => root.querySelectorAll("button").find((node) => node.textContent === label);
  const open = (index = 0) => {
    root.children[0].children[1].shadowRoot.querySelectorAll("[id]")[index].click();
    button("Edit selected source fragment").click();
  };
  return { root, status, messages, calls, engine, send, render, button, open };
}

test("production webview calls Rust with the captured source and waits for host acknowledgement", async (t) => {
  const h = await controllerHarness(t, "flowchart LR\r\nA --> B\r\n"); h.open();
  const input = h.root.querySelectorAll("textarea")[0]; input.value = "A[😀] --> C\nC --> B";
  h.button("Apply source edit").click();
  assert.deepEqual(h.calls, [{ input: "flowchart LR\r\nA --> B\r\n", id: "fm-node-a-0", replacement: "A[😀] --> C\r\nC --> B" }]);
  const request = h.messages.at(-1);
  assert.equal(request.type, "apply-source-edit");
  assert.equal(h.button("Apply source edit").disabled, true);
  h.button("Apply source edit").click(); assert.equal(h.calls.length, 1);
  h.send({ type: "source-edit-result", requestId: 1, documentVersion: 3, editId: request.editId + 1, ok: true, message: "wrong" });
  assert.equal(input.readOnly, true); // The submitted text cannot change during an async host write.
  h.send({ type: "source-edit-result", requestId: 1, documentVersion: 3, editId: request.editId, ok: true, message: "Applied" });
  assert.equal(input.readOnly, true);
  assert.equal(h.button("Discard draft").disabled, false);
});

test("engine errors and host rejections preserve the draft and permit a retry", async (t) => {
  const h = await controllerHarness(t); h.open();
  const input = h.root.querySelectorAll("textarea")[0]; input.value = "A --> C";
  const original = h.engine.applyParseLensEdit;
  h.engine.applyParseLensEdit = () => { throw new Error("Rust refused this fragment"); };
  h.button("Apply source edit").click(); assert.match(h.root.textContent, /Rust refused/u);
  assert.equal(input.value, "A --> C");
  h.engine.applyParseLensEdit = original; h.button("Apply source edit").click();
  const request = h.messages.at(-1);
  h.send({ type: "source-edit-result", requestId: 1, documentVersion: 3, editId: request.editId, ok: false, message: "Read only" });
  assert.equal(input.value, "A --> C"); assert.equal(h.button("Apply source edit").disabled, false);
  assert.equal(input.readOnly, false);
  h.button("Apply source edit").click(); assert.equal(h.calls.length, 2);
});

test("new renders keep a dirty draft visible but make its source binding non-actionable", async (t) => {
  const h = await controllerHarness(t); h.open();
  const input = h.root.querySelectorAll("textarea")[0]; input.value = "keep this work";
  h.render(2); await settle();
  assert.equal(h.root.querySelectorAll("textarea")[0], input);
  assert.equal(input.value, "keep this work");
  assert.equal(h.button("Apply source edit").disabled, true);
  h.button("Apply source edit").click(); assert.equal(h.calls.length, 0);
  h.open(); assert.equal(h.root.querySelectorAll("textarea")[0], input);
  h.button("Discard draft").click(); h.open();
  assert.notEqual(h.root.querySelectorAll("textarea")[0], input);
});

test("missing editing APIs and malformed engine receipts never dispatch a document write", async (t) => {
  const h = await controllerHarness(t); h.engine.applyParseLensEdit = undefined; h.open();
  assert.equal(h.root.querySelectorAll("textarea").length, 0);
  h.engine.applyParseLensEdit = () => ({ result: {} }); h.open();
  h.root.querySelectorAll("textarea")[0].value = "new"; h.button("Apply source edit").click();
  assert.match(h.root.textContent, /complete source-edit receipt/u);
  assert.equal(h.messages.some((message) => message.type === "apply-source-edit"), false);
});

function batchFixture(doc, changes) {
  const snapshot = createRenderSnapshot(doc, 1, doc.fileName);
  const raw = new Map(snapshot.blocks.map((block) => [block.id, []]));
  const edits = changes.map(({ diagramId = 0, snippet, replacement, id }, index) => {
    const block = snapshot.blocks[diagramId];
    const binding = bindingFor(block.source, snippet, id || `element-${index}`);
    raw.get(diagramId).push(binding);
    return { diagramId, elementId: binding.elementId, replacement,
      result: receipt(block.source, binding, replacement).result };
  });
  const reports = new Map(snapshot.blocks.map((block) => [block.id,
    { bindings: checkedSourceBindings(block, raw.get(block.id)), diagnostics: [] }]));
  const message = { type: "apply-source-batch", requestId: 1, documentVersion: doc.version, editId: 1, edits };
  return { snapshot, reports, raw, message };
}
function batchPlan(doc, changes) {
  const f = batchFixture(doc, changes);
  return planSourceBatch(doc, f.snapshot, f.reports, f.message);
}

test("batch uses original Unicode offsets even when an earlier replacement changes length", () => {
  const doc = makeDocument("flowchart LR\nA[😀] --> B\nC --> D\n");
  const result = batchPlan(doc, [
    { snippet: "A[😀] --> B", replacement: "A[longer label 東京] --> E\nE --> B" },
    { snippet: "C --> D", replacement: "C --> Z" },
  ]);
  assert.equal(result.after, "flowchart LR\nA[longer label 東京] --> E\nE --> B\nC --> Z\n");
  assert.equal(result.edits.length, 2);
  assert.equal(result.edits[1].range.start.line, 2);
});

test("one batch spans multiple Markdown diagrams and keeps unrelated content byte-exact", () => {
  const doc = makeDocument("Text\r\n  ```mermaid\r\n  flowchart LR\r\n  A --> B\r\n  ```\r\nDo not touch 😀\r\n~~~mermaid\r\nflowchart LR\r\nC --> D\r\n~~~\r\n", "/work/readme.md", 2);
  const result = batchPlan(doc, [
    { diagramId: 1, snippet: "C --> D", replacement: "C --> F" },
    { snippet: "A --> B", replacement: "A --> E\nE --> B" },
  ]);
  assert.equal(result.after, "Text\r\n  ```mermaid\r\n  flowchart LR\r\n  A --> E\r\n  E --> B\r\n  ```\r\nDo not touch 😀\r\n~~~mermaid\r\nflowchart LR\r\nC --> F\r\n~~~\r\n");
});

test("shared-statement aliases, overlapping fragments and repeated IDs abort the whole batch", () => {
  const doc = makeDocument("flowchart LR\nA --> B\nC --> D\n");
  for (const snippets of [["A --> B", "A --> B"], ["A --> B", "B"]]) {
    assert.throws(() => batchPlan(doc, snippets.map((snippet) => ({ snippet, replacement: "new" }))), /overlap|share/u);
  }
  const f = batchFixture(doc, [{ snippet: "A --> B", replacement: "A --> C" }]);
  f.message.edits.push(f.message.edits[0]);
  assert.throws(() => planSourceBatch(doc, f.snapshot, f.reports, f.message), /repeat/u);
});

test("combined fence escapes are rejected even though each individual edit is allowed", () => {
  const doc = makeDocument("```mermaid\nflowchart LR\nAB\n```\nuntouched prose", "/work/readme.md");
  const f = batchFixture(doc, [{ snippet: "A", replacement: "``" }, { snippet: "B", replacement: "`" }]);
  for (const edit of f.message.edits) assert.doesNotThrow(() => planSourceEdit(doc, f.snapshot, f.reports, edit));
  assert.throws(() => planSourceBatch(doc, f.snapshot, f.reports, f.message), /Combined edits/u);
});

test("batch cardinality, payload and final document budgets are enforced", () => {
  const doc = makeDocument("flowchart LR\nA --> B\nC --> D\n");
  const f = batchFixture(doc, [{ snippet: "A --> B", replacement: "A --> C" }]);
  for (const edits of [[], null, Array(65).fill(f.message.edits[0])]) {
    assert.throws(() => planSourceBatch(doc, f.snapshot, f.reports, { edits }), /1 and 64/u);
  }
  f.message.edits[0].result.updatedSource = "x".repeat(16 * 1024 * 1024);
  assert.throws(() => planSourceBatch(doc, f.snapshot, f.reports, f.message), /16 MiB/u);
  const large = "x".repeat(1100000);
  assert.throws(() => batchPlan(doc, [{ snippet: "A --> B", replacement: large },
    { snippet: "C --> D", replacement: large }]), /2 MiB/u);
});

test("no-op batch members do not create extra editor changes", () => {
  const doc = makeDocument("flowchart LR\nA --> B\nC --> D\n");
  const result = batchPlan(doc, [{ snippet: "A --> B", replacement: "A --> B" },
    { snippet: "C --> D", replacement: "C --> E" }]);
  assert.equal(result.edits.length, 1);
  const unchanged = batchPlan(doc, [{ snippet: "A --> B", replacement: "A --> B" }]);
  assert.equal(unchanged.changed, false);
  assert.equal(unchanged.edits.length, 0);
});

async function acceptBatch(h, changes) {
  const f = batchFixture(h.doc, changes);
  const render = h.render();
  await h.receive({ type: "rendered", requestId: render.requestId, documentVersion: render.documentVersion,
    reports: f.snapshot.blocks.map((block) => ({ id: block.id, bindings: f.raw.get(block.id), diagnostics: [] })) });
  f.message.requestId = render.requestId;
  f.message.documentVersion = render.documentVersion;
  return f.message;
}

test("production host applies two fragments as one native transaction, with whole-batch undo", async (t) => {
  const before = "flowchart LR\nA --> B\nC --> D\n";
  const h = await hostHarness(t, makeDocument(before));
  const m = await acceptBatch(h, [{ snippet: "A --> B", replacement: "A --> E\nE --> B" },
    { snippet: "C --> D", replacement: "C --> Z" }]);
  await h.receive(m);
  assert.equal(h.doc.text, "flowchart LR\nA --> E\nE --> B\nC --> Z\n");
  assert.equal(h.editCalls.length, 1);
  assert.equal(h.editCalls[0].changes.length, 2);
  h.undo(); assert.equal(h.doc.text, before);
  h.redo(); assert.equal(h.doc.text, "flowchart LR\nA --> E\nE --> B\nC --> Z\n");
  await h.receive(m); assert.equal(h.editCalls.length, 1);
});

test("a bad last receipt never partially applies earlier fragments; refused batches remain retryable", async (t) => {
  const before = "flowchart LR\nA --> B\nC --> D\n";
  const h = await hostHarness(t, makeDocument(before));
  const m = await acceptBatch(h, [{ snippet: "A --> B", replacement: "A --> E" },
    { snippet: "C --> D", replacement: "C --> Z" }]);
  const original = m.edits[1].result.updatedSource;
  m.edits[1].result.updatedSource += "extra";
  await h.receive(m); assert.equal(h.editCalls.length, 0); assert.equal(h.doc.text, before);
  m.edits[1].result.updatedSource = original;
  h.rejectEdit = true; await h.receive(m); assert.equal(h.doc.text, before);
  h.rejectEdit = false; await h.receive(m); assert.equal(h.doc.text, "flowchart LR\nA --> E\nC --> Z\n");
});

test("production controller stages separate Rust receipts, then dispatches only one batch", async (t) => {
  const h = await controllerHarness(t, "flowchart LR\nA --> B\nC --> D\n",
    [{ snippet: "A --> B", id: "fm-node-a-0" }, { snippet: "C --> D", id: "fm-node-c-1" }]);
  h.open(); h.root.querySelectorAll("textarea")[0].value = "A --> E"; h.button("Stage fragment").click();
  assert.equal(h.messages.some((m) => m.type.startsWith("apply-source")), false);
  h.open(1); h.root.querySelectorAll("textarea")[0].value = "C --> Z";
  h.button("Apply source edit").click(); assert.match(h.root.textContent, /apply all staged edits together/u);
  h.button("Apply staged edits").click(); assert.match(h.root.textContent, /Stage or discard the open/u);
  h.button("Stage fragment").click();
  assert.equal(h.calls.length, 2);
  assert.ok(h.calls.every((call) => call.input === "flowchart LR\nA --> B\nC --> D\n"));
  h.button("Apply staged edits").click();
  const message = h.messages.at(-1);
  assert.equal(message.type, "apply-source-batch");
  assert.equal(message.edits.length, 2);
  assert.deepEqual(message.edits.map((edit) => edit.replacement), ["A --> E", "C --> Z"]);
  h.button("Apply staged edits").click();
  assert.equal(h.messages.filter((m) => m.type === "apply-source-batch").length, 1);
});

test("stale batches retain replacement text but cannot write after a new render", async (t) => {
  const h = await controllerHarness(t); h.open();
  h.root.querySelectorAll("textarea")[0].value = "A[retain my work] --> E"; h.button("Stage fragment").click();
  h.render(2); await settle();
  assert.match(h.root.textContent, /retain my work/u);
  assert.equal(h.button("Apply staged edits").disabled, true);
  h.button("Apply staged edits").click();
  assert.equal(h.messages.some((m) => m.type === "apply-source-batch"), false);
  h.button("Discard staged edits").click(); assert.equal(h.button("Apply staged edits"), undefined);
});

test("shared spans are rejected at staging and rejected host batches keep every staged change", async (t) => {
  const h = await controllerHarness(t, "flowchart LR\nA --> B\n",
    [{ snippet: "A --> B", id: "fm-node-a-0" }, { snippet: "A --> B", id: "fm-node-b-1" }]);
  h.open(); h.root.querySelectorAll("textarea")[0].value = "A --> E"; h.button("Stage fragment").click();
  h.open(1); h.root.querySelectorAll("textarea")[0].value = "A --> F"; h.button("Stage fragment").click();
  assert.match(h.root.textContent, /overlaps a staged statement/u);
  assert.equal(h.root.querySelectorAll("textarea")[0].value, "A --> F");
  h.button("Discard draft").click(); h.button("Apply staged edits").click();
  const request = h.messages.at(-1);
  h.send({ type: "source-edit-result", requestId: 1, documentVersion: 3, editId: request.editId, ok: false, message: "Refused" });
  assert.match(h.root.textContent, /A --> E/u);
  assert.equal(h.button("Apply staged edits").disabled, false);
  h.button("Remove staged fragment").click();
  assert.equal(h.button("Apply staged edits").disabled, true);
});

test("a batch refused after asynchronous editor opening leaves every original fragment intact", async (t) => {
  const before = "flowchart LR\nA --> B\nC --> D\n";
  const h = await hostHarness(t, makeDocument(before));
  const m = await acceptBatch(h, [{ snippet: "A --> B", replacement: "A --> E" },
    { snippet: "C --> D", replacement: "C --> Z" }]);
  let resolve;
  h.openEditor = () => new Promise((done) => { resolve = done; });
  const pending = h.receive(m);
  h.change(before + "%% new typing\n");
  resolve(h.editor); await pending;
  assert.equal(h.editCalls.length, 0);
  assert.equal(h.doc.text, before + "%% new typing\n");
  assert.equal(h.messages.at(-1).ok, false);
});

test("staging copies engine receipt scalars rather than retaining mutable engine results", async (t) => {
  const h = await controllerHarness(t); h.open();
  const input = h.root.querySelectorAll("textarea")[0]; input.value = "A --> C";
  let response;
  const original = h.engine.applyParseLensEdit;
  h.engine.applyParseLensEdit = (...args) => { response = original(...args); return response; };
  h.button("Stage fragment").click();
  response.result.replacement = "MUTATED";
  response.result.replacedRange.startByte = 0;
  h.button("Apply staged edits").click();
  const sent = h.messages.at(-1).edits[0];
  assert.equal(sent.result.replacement, "A --> C");
  assert.equal(sent.result.replacedRange.startByte, Buffer.byteLength("flowchart LR\n"));
});
