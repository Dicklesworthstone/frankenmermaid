"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");
const { Worker } = require("node:worker_threads");
const { createRenderSnapshot, checkedSourceBindings, planSourceEdit, planSourceBatch } = require("../preview-contract.cjs");
const { createPreviewController } = require("../media/preview.js");
const { create, attach } = require("../media/engine-worker.js");

// These fixtures pin the text-operation contract in fm-core/src/lens_tests.rs. They are NOT a
// Mermaid parser: ranges and replacement bytes are explicitly supplied, independently of the
// production planner. DOM/VS Code/engine API boundaries below are doubles; worker threads and
// the non-returning WebAssembly cancellation probe execute for real.
const settle = () => new Promise((done) => setImmediate(done));
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
function documentFor(text, markdown = false, eol = text.includes("\r\n") ? 2 : 1) {
  return { text, fileName: markdown ? "/work/readme.md" : "/work/diagram.mmd", languageId: markdown ? "markdown" : "mermaid",
    version: 1, eol, getText() { return this.text; } };
}
function bindingFor(source, snippet, elementId = "fm-node-a-0") {
  const from = source.indexOf(snippet);
  assert.ok(from >= 0, `missing fixture fragment ${snippet}`);
  const startByte = Buffer.byteLength(source.slice(0, from));
  return { elementId, sourceId: "A", snippet, textRange: { startByte, endByte: startByte + Buffer.byteLength(snippet) } };
}
function receipt(source, elementId, start, end, replacement) {
  return { result: { elementId, previousSnippet: source.slice(start, end), replacement,
    replacedRange: { startByte: Buffer.byteLength(source.slice(0, start)), endByte: Buffer.byteLength(source.slice(0, end)) },
    updatedSource: source.slice(0, start) + replacement + source.slice(end) }, snapshot: { bindings: [] } };
}
function fixture(doc, snippet, operation, text, patch, diagramId = 0) {
  const snapshot = createRenderSnapshot(doc, 7, "fixture");
  const source = snapshot.blocks[diagramId].source;
  const binding = bindingFor(source, snippet);
  const reports = new Map([[diagramId, { bindings: checkedSourceBindings(snapshot.blocks[diagramId], [binding]) }]]);
  const specified = patch(source);
  const response = receipt(source, binding.elementId, specified.start, specified.end, specified.replacement);
  const message = { type: "apply-source-edit", requestId: 7, documentVersion: doc.version, editId: 1,
    diagramId, elementId: binding.elementId, operation, replacement: text, result: response.result };
  return { snapshot, reports, message, response };
}
const remove = (text) => (source) => { const start = source.indexOf(text); assert.ok(start >= 0); return { start, end: start + text.length, replacement: "" }; };
const insertBefore = (next, replacement) => (source) => { const start = source.indexOf(next); assert.ok(start >= 0); return { start, end: start, replacement }; };
const append = (replacement) => (source) => ({ start: source.length, end: source.length, replacement });
function planned(doc, snippet, operation, text, patch, diagramId) {
  const f = fixture(doc, snippet, operation, text, patch, diagramId);
  const result = planSourceEdit(doc, f.snapshot, f.reports, f.message);
  // Planning never mutates the real editor document, even on success.
  assert.equal(doc.getText(), result.before);
  return result;
}

test("insert follows the selected line, copies tabs/spaces, and preserves all existing text", () => {
  const doc = documentFor("%% keep 😀\nflowchart LR\n    A-->B\n  C-->D\n");
  const p = planned(doc, "A-->B", "insert-after", "NEW[東京]-->Q", insertBefore("  C", "    NEW[東京]-->Q\n"));
  assert.equal(p.after, "%% keep 😀\nflowchart LR\n    A-->B\n    NEW[東京]-->Q\n  C-->D\n");
  assert.deepEqual(p.range, { start: { line: 3, character: 0 }, end: { line: 3, character: 0 } });
  const crlf = documentFor("flowchart LR\r\n\tA[😀]-->B\r\n\tC-->D\r\n");
  assert.equal(planned(crlf, "A[😀]-->B", "insert-after", "N-->Q", insertBefore("\tC", "\tN-->Q\r\n")).after,
    "flowchart LR\r\n\tA[😀]-->B\r\n\tN-->Q\r\n\tC-->D\r\n");
});

test("insert after a multiline fragment uses its last line and keeps authored continuation indentation", () => {
  const doc = documentFor("flowchart LR\n  A[one\n    two]\nC-->D\n");
  const p = planned(doc, "A[one\n    two]", "insert-after", "N-->Q\n    Q-->R", insertBefore("C-->", "  N-->Q\n    Q-->R\n"));
  assert.equal(p.after, "flowchart LR\n  A[one\n    two]\n  N-->Q\n    Q-->R\nC-->D\n");
});

test("standalone EOF insertion adds the missing separator and preserves native CRLF", () => {
  for (const nl of ["\n", "\r\n"]) {
    const doc = documentFor(`flowchart LR${nl}  A-->B`);
    assert.equal(planned(doc, "A-->B", "insert-after", "N-->Q", append(`${nl}  N-->Q${nl}`)).after,
      `flowchart LR${nl}  A-->B${nl}  N-->Q${nl}`);
  }
});

test("whole-line deletion removes indentation and terminator, including multiline fragments", () => {
  for (const nl of ["\n", "\r\n"]) {
    const doc = documentFor(`flowchart LR${nl}  A-->B${nl}\tC-->D${nl}`);
    const p = planned(doc, "A-->B", "delete", "", remove(`  A-->B${nl}`));
    assert.equal(p.after, `flowchart LR${nl}\tC-->D${nl}`);
    assert.deepEqual(p.range, { start: { line: 1, character: 0 }, end: { line: 2, character: 0 } });
    const eof = documentFor(`flowchart LR${nl}  A-->B`);
    assert.equal(planned(eof, "A-->B", "delete", "", remove("  A-->B")).after, `flowchart LR${nl}`);
  }
  const multi = documentFor("flowchart LR\n  A[one\n    two]\nZ-->Q\n");
  assert.equal(planned(multi, "A[one\n    two]", "delete", "", remove("  A[one\n    two]\n")).after,
    "flowchart LR\nZ-->Q\n");
});

test("deletion never removes other shared-statement content, comments, or Unicode neighbours", () => {
  const doc = documentFor("flowchart LR\n  A[😀]-->B %% keep café\nC-->D\n");
  const p = planned(doc, "A[😀]", "delete", "", remove("A[😀]"));
  assert.equal(p.after, "flowchart LR\n  -->B %% keep café\nC-->D\n");
  assert.equal(p.range.end.character, 7, "UTF-16 end, not the engine's UTF-8 byte count");
  const bad = fixture(doc, "A[😀]", "delete", "", remove("  A[😀]-->B %% keep café\n"));
  assert.throws(() => planSourceEdit(doc, bad.snapshot, bad.reports, bad.message), /structural receipt/u);
});

test("delete whitespace expansion matches Rust Unicode White_Space rather than JS trim", () => {
  const doc = documentFor("flowchart LR\n\u0085A\u2003\nZ\n");
  assert.equal(planned(doc, "A", "delete", "", remove("\u0085A\u2003\n")).after, "flowchart LR\nZ\n");
  const bom = documentFor("flowchart LR\n\ufeffA\nZ\n");
  assert.equal(planned(bom, "A", "delete", "", remove("A")).after, "flowchart LR\n\ufeff\nZ\n");
  const bad = fixture(bom, "A", "delete", "", remove("\ufeffA\n"));
  assert.throws(() => planSourceEdit(bom, bad.snapshot, bad.reports, bad.message), /structural receipt/u);
});

test("Markdown insertion restores only container indentation, with exact sibling/prose preservation", () => {
  const doc = documentFor("# keep 😀\r\n  ```mermaid\r\n  flowchart LR\r\n    A-->B\r\n   C-->D\r\n  ```\r\nuntouched\r\n~~~mermaid\r\npie\r\n~~~", true);
  const p = planned(doc, "A-->B", "insert-after", "N-->Q", insertBefore(" C", "  N-->Q\n"));
  assert.equal(p.after, "# keep 😀\r\n  ```mermaid\r\n  flowchart LR\r\n    A-->B\r\n    N-->Q\r\n   C-->D\r\n  ```\r\nuntouched\r\n~~~mermaid\r\npie\r\n~~~");
  assert.deepEqual(p.range.start, { line: 4, character: 0 });
});

test("Markdown EOF insertion reuses the closing-fence separator without introducing a blank line", () => {
  for (const nl of ["\n", "\r\n"]) {
    const doc = documentFor(`before${nl}  ~~~mermaid${nl}  flowchart LR${nl}    A-->B${nl}  ~~~${nl}after`, true);
    const p = planned(doc, "A-->B", "insert-after", "N-->Q", append("\n  N-->Q\n"));
    assert.equal(p.after, `before${nl}  ~~~mermaid${nl}  flowchart LR${nl}    A-->B${nl}    N-->Q${nl}  ~~~${nl}after`);
    assert.equal(p.normalizeTerminal, true);
  }
});

test("Markdown structural edits preserve already-present blank content lines and their indentation", () => {
  const doc = documentFor("```mermaid\nflowchart LR\nA-->B\n\n```\n", true);
  assert.equal(planned(doc, "A-->B", "insert-after", "N-->Q", append("N-->Q\n")).after,
    "```mermaid\nflowchart LR\nA-->B\nN-->Q\n\n```\n");
  assert.equal(planned(doc, "A-->B", "delete", "", remove("A-->B\n")).after,
    "```mermaid\nflowchart LR\n\n```\n");
  const indented = documentFor("  ```mermaid\n  flowchart LR\n    A-->B\n  \n  ```", true);
  assert.equal(planned(indented, "A-->B", "insert-after", "N", append("  N\n")).after,
    "  ```mermaid\n  flowchart LR\n    A-->B\n    N\n  \n  ```");
});

test("Markdown last-line deletion removes its native separator without consuming the fence", () => {
  const doc = documentFor("before\r\n  ```mermaid\r\n  flowchart LR\r\n    A-->B\r\n  ```\r\nafter", true);
  assert.equal(planned(doc, "A-->B", "delete", "", remove("  A-->B")).after,
    "before\r\n  ```mermaid\r\n  flowchart LR\r\n  ```\r\nafter");
  const only = documentFor("```mermaid\nA-->B\n```", true);
  assert.equal(planned(only, "A-->B", "delete", "", remove("A-->B")).after, "```mermaid\n```");
});

test("unclosed Markdown EOF insertion/deletion preserves exact remaining body bytes", () => {
  const doc = documentFor("prose\n  ```mermaid\n  flowchart LR\n    A-->B", true);
  assert.equal(planned(doc, "A-->B", "insert-after", "N", append("\n  N\n")).after,
    "prose\n  ```mermaid\n  flowchart LR\n    A-->B\n    N\n");
  assert.equal(planned(doc, "A-->B", "delete", "", remove("  A-->B")).after,
    "prose\n  ```mermaid\n  flowchart LR\n");
});

test("native structural positions respect which Markdown diagram owns the selection", () => {
  const doc = documentFor("```mermaid\nA-->B\n```\nkeep\n~~~mermaid\nC-->D\n~~~\n", true);
  assert.equal(planned(doc, "C-->D", "insert-after", "Z", append("\nZ\n"), 1).after,
    "```mermaid\nA-->B\n```\nkeep\n~~~mermaid\nC-->D\nZ\n~~~\n");
});

test("structural receipts cannot widen ranges, change the anchor, or replace other document bytes", () => {
  const doc = documentFor("flowchart LR\nA[😀]-->B\nC-->D\n");
  for (const mutate of [
    (m) => { m.operation = "constructor"; }, (m) => { m.diagramId = 99; },
    (m) => { m.elementId = "missing"; }, (m) => { m.result.elementId = "wrong"; },
    (m) => { m.result.previousSnippet = "A"; }, (m) => { m.result.replacement = "different"; },
    (m) => { m.result.replacedRange.startByte -= 1; }, (m) => { m.result.replacedRange.endByte += 1; },
    (m) => { m.result.updatedSource += "\nforeign"; }, (m) => { m.replacement = "\ud800"; },
    (m) => { m.replacement = "x".repeat(2 * 1024 * 1024 + 1); },
  ]) {
    const f = fixture(doc, "A[😀]-->B", "insert-after", "N", insertBefore("C-->", "N\n"));
    mutate(f.message); assert.throws(() => planSourceEdit(doc, f.snapshot, f.reports, f.message));
  }
  const f = fixture(doc, "A[😀]-->B", "delete", "", remove("A[😀]-->B\n"));
  f.message.replacement = "not deletion";
  assert.throws(() => planSourceEdit(doc, f.snapshot, f.reports, f.message), /Invalid structural/u);
  f.message.replacement = ""; doc.text += "changed";
  assert.throws(() => planSourceEdit(doc, f.snapshot, f.reports, f.message), /Source changed/u);
});

test("fence escapes, native EOL mismatches, and final document size overflow fail before any write", () => {
  const doc = documentFor("```mermaid\nA-->B\n```\nprose", true);
  const text = "```\nforeign prose\n```mermaid\nZ";
  assert.throws(() => planned(doc, "A-->B", "insert-after", text, append(`\n${text}\n`)), /fence/u);
  const wrongEol = documentFor("flowchart LR\r\nA-->B", false, 1);
  assert.throws(() => planned(wrongEol, "A-->B", "insert-after", "N", append("\r\nN\r\n")), /line endings/u);
  const large = documentFor("flowchart LR\nA-->B\n" + "%".repeat(1024 * 1024));
  const more = "x".repeat(1024 * 1024);
  assert.throws(() => planned(large, "A-->B", "insert-after", more, insertBefore("%%%", more + "\n")), /2 MiB/u);
});

test("a structural batch uses the actual expanded deletion range, not the selected snippet", () => {
  const doc = documentFor("flowchart LR\nA-->B\nC-->D\n");
  const f = fixture(doc, "A-->B", "delete", "", remove("A-->B\n"));
  const p = planSourceBatch(doc, f.snapshot, f.reports, { edits: [f.message] });
  assert.equal(p.after, "flowchart LR\nC-->D\n");
  assert.equal(p.edits.length, 1);
  assert.deepEqual(p.edits[0].range, { start: { line: 1, character: 0 }, end: { line: 2, character: 0 } });
});

class Element {
  constructor(tag) { this.localName = tag; this.children = []; this.attrs = new Map(); this.listeners = new Map(); this._text = ""; this._value = ""; }
  set textContent(value) { this._text = String(value); this.children = []; }
  get textContent() { return this._text + this.children.map((node) => node.textContent).join(""); }
  set value(value) { this._value = String(value).replace(/\r\n|\r/gu, "\n"); }
  get value() { return this._value; }
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
  removeEventListener(type, fn) { if (this.listeners.get(type) === fn) this.listeners.delete(type); }
  attachShadow() { this.shadowRoot = new Element("shadow-root"); return this.shadowRoot; }
  querySelectorAll(selector) {
    return this.children.flatMap((node) => [
      ...(selector === "*" || selector === node.localName || (selector === "[id]" && node.id) ? [node] : []),
      ...node.querySelectorAll(selector),
    ]);
  }
  click() { if (!this.disabled && !this.hidden) return this.listeners.get("click")?.({ preventDefault() {}, stopPropagation() {} }); }
}

async function controllerHarness(t, options = {}) {
  const doc = options.doc || documentFor("flowchart LR\nA-->B\nC-->D\n");
  const message = options.message || createRenderSnapshot(doc, 7, "diagram").message;
  const raw = options.raw || bindingFor(message.diagrams[0].source, "A-->B");
  const renderBindings = options.renderBindings || new Map([[0, [raw]]]);
  let nextDiagram = 0;
  const root = new Element("main"), status = new Element("div"), listeners = new Map(), messages = [], calls = [];
  const window = { addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: (name) => listeners.delete(name),
    DOMParser: class { parseFromString() {
      const svg = new Element("svg"); svg.namespaceURI = "http://www.w3.org/2000/svg";
      for (const binding of renderBindings.get(nextDiagram++ % message.diagrams.length) || []) {
        const bound = new Element("g"); bound.setAttribute("id", binding.elementId); svg.append(bound);
      }
      return { documentElement: svg, querySelector: () => null };
    } }, XMLSerializer: class { serializeToString() { return '<svg xmlns="http://www.w3.org/2000/svg"/>'; } } };
  const document = { body: { dataset: { wasmModule: "module", wasmBinary: "binary", engineWorker: "worker", styleNonce: "nonce" } },
    getElementById: (id) => id === "preview" ? root : id === "status" ? status : undefined,
    createElement: (tag) => new Element(tag), importNode: (node) => node };
  const engine = options.engine || { default: async () => {}, renderSvg: async () => "<svg/>",
    parseLens: async () => ({ bindings: [raw], parsed: { warnings: [] } }),
    applyParseLensEdit: async (source, id, text) => { calls.push({ method: "replace", source, id, text });
      return receipt(source, id, source.indexOf("A-->B"), source.indexOf("A-->B") + 5, text); },
    applyParseLensInsertLineAfter: async (source, id, text) => { calls.push({ method: "insert-after", source, id, text });
      const at = source.indexOf("C-->D"); return receipt(source, id, at, at, text + "\n"); },
    applyParseLensDelete: async (source, id) => { calls.push({ method: "delete", source, id });
      const at = source.indexOf("A-->B"); return receipt(source, id, at, at + 6, ""); },
  };
  const controller = createPreviewController({ document, window, vscode: { postMessage: (m) => messages.push(m) },
    loadEngine: async () => engine, yieldToHost: async () => {} });
  t.after(() => controller.dispose());
  await controller.start();
  const send = (m) => listeners.get("message")?.({ data: m });
  send(message); await settle();
  const button = (label, scope = root) => scope.querySelectorAll("button").find((node) => node.textContent === label);
  const open = (operation, bindingIndex = 0, diagramId = 0) => {
    const card = root.children[diagramId];
    card.children[1].shadowRoot.querySelectorAll("[id]")[bindingIndex].click();
    button(operation === "insert-after" ? "Insert after selected source"
      : operation === "delete" ? "Delete selected source fragment" : "Edit selected source fragment", card).click();
  };
  const applyMessages = () => messages.filter((m) => m.type.startsWith("apply-source"));
  return { doc, message, raw, engine, root, status, messages, calls, send, controller, button, open, applyMessages };
}

test("preview insertion calls the structural engine API and preserves its receipt until native acknowledgement", async (t) => {
  const h = await controllerHarness(t); h.open("insert-after");
  const input = h.root.querySelectorAll("textarea")[0];
  assert.equal(input.value, ""); input.value = "N[😀]-->Q";
  assert.notEqual(h.button("Stage fragment").hidden, true);
  await h.button("Insert source").click();
  assert.deepEqual(h.calls, [{ method: "insert-after", source: h.message.diagrams[0].source, id: h.raw.elementId, text: "N[😀]-->Q" }]);
  const request = h.applyMessages()[0];
  assert.equal(request.operation, "insert-after");
  assert.equal(request.result.previousSnippet, "");
  assert.equal(request.result.replacement, "N[😀]-->Q\n");
  assert.equal(input.readOnly, true); assert.equal(h.button("Insert source").disabled, true);
  await h.button("Insert source").click(); assert.equal(h.calls.length, 1);
  h.send({ ...request, type: "source-edit-result", ok: false, message: "Read only" });
  assert.equal(input.value, "N[😀]-->Q"); assert.equal(input.readOnly, false);
  assert.equal(h.button("Insert source").disabled, false);
  await h.button("Insert source").click(); assert.equal(h.calls.length, 2);
});

test("preview deletion is an explicit reviewed action, not an empty replacement or implicit graph cleanup", async (t) => {
  const h = await controllerHarness(t); h.open("delete");
  const input = h.root.querySelectorAll("textarea")[0];
  assert.equal(input.value, "A-->B"); assert.equal(input.readOnly, true);
  assert.match(h.root.textContent, /does not remove references elsewhere/u);
  assert.equal(h.calls.length, 0); assert.equal(h.applyMessages().length, 0);
  h.open("insert-after"); // A pending deletion review cannot disappear behind another edit.
  assert.equal(h.root.querySelectorAll("textarea")[0], input);
  await h.button("Delete source fragment").click();
  assert.deepEqual(h.calls, [{ method: "delete", source: h.message.diagrams[0].source, id: h.raw.elementId }]);
  const request = h.applyMessages()[0];
  assert.equal(request.replacement, ""); assert.equal(request.result.previousSnippet, "A-->B\n");
  h.send({ ...request, type: "source-edit-result", ok: false, message: "Native refusal" });
  assert.equal(input.readOnly, true); assert.equal(h.button("Delete source fragment").disabled, false);
});

test("source invalidation during asynchronous structural computation retains drafts and dispatches no stale write", async (t) => {
  for (const operation of ["insert-after", "delete"]) {
    const h = await controllerHarness(t); const pending = deferred();
    const method = operation === "delete" ? "applyParseLensDelete" : "applyParseLensInsertLineAfter";
    const original = h.engine[method]; let response;
    h.engine[method] = async (...args) => { response = await original(...args); return pending.promise; };
    h.open(operation);
    const input = h.root.querySelectorAll("textarea")[0];
    if (operation === "insert-after") input.value = "N[keep draft]-->Q";
    const applying = h.button(operation === "delete" ? "Delete source fragment" : "Insert source").click();
    await settle();
    h.send({ type: "invalidate", requestId: 8, documentVersion: 2 });
    pending.resolve(response); await applying;
    assert.equal(h.applyMessages().length, 0);
    assert.equal(input.value, operation === "delete" ? "A-->B" : "N[keep draft]-->Q");
    assert.equal(h.button("Discard draft").disabled, false);
    assert.equal(h.button(operation === "delete" ? "Delete source fragment" : "Insert source").disabled, true);
  }
});

test("malformed structural engine results fail visibly without losing the new source", async (t) => {
  const h = await controllerHarness(t); h.open("insert-after");
  h.root.querySelectorAll("textarea")[0].value = "new source";
  h.engine.applyParseLensInsertLineAfter = async () => ({ result: {} });
  await h.button("Insert source").click();
  assert.match(h.root.textContent, /complete source-edit receipt/u);
  assert.equal(h.applyMessages().length, 0); assert.equal(h.button("Insert source").disabled, false);
});

// A genuinely executing non-returning WebAssembly loop, reused only as a cancellation probe.
const spinBytes = Uint8Array.from([0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,
  7,8,1,4,115,112,105,110,0,0,10,9,1,7,0,3,64,12,0,11,11]);
const engineFixture = `
let wasm;
async function __wbg_init({module_or_path}) { wasm = (await WebAssembly.instantiate(module_or_path)).instance; }
function renderSvg(source) { return '<svg>' + source + '</svg>'; }
function parseLens(source) { return {bindings:[],parsed:{warnings:['notice'],ir:{diagnostics:[],nodes:['not transported']}}}; }
function result(method, source, id, replacement) {
  if (id === 'HANG') { self.postMessage({entered:true}); wasm.exports.spin(); }
  return {result:{method,elementId:id,previousSnippet:source,replacement,updatedSource:replacement,
    replacedRange:{startByte:0,endByte:new TextEncoder().encode(source).length}},snapshot:parseLens(replacement)};
}
function applyParseLensEdit(source,id,text) {return result('replace',source,id,text);}
function applyParseLensDelete(source,id) {return result('delete',source,id,'');}
function applyParseLensInsertLineAfter(source,id,text) {return result('insert-after',source,id,text);}
export {__wbg_init as default,renderSvg,parseLens,applyParseLensEdit,applyParseLensDelete,applyParseLensInsertLineAfter};
`;
function workerHarness(t, options = {}) {
  const code = fs.readFileSync(path.join(__dirname, "../media/engine-worker.js"), "utf8");
  const sources = new Map(), workers = [], requests = [], timers = new Map(); let nextTimer = 0, fetches = 0;
  class BlobBoundary { constructor(parts) { this.source = parts.join(""); } }
  class WorkerBoundary {
    constructor(url) {
      this.listeners = new Map(); this.entered = deferred(); this.exited = deferred(); this.terminated = false;
      const prefix = `import {parentPort} from 'node:worker_threads'; globalThis.self = {
        addEventListener:(type,fn)=>parentPort.on(type,data=>fn({data})),postMessage:data=>parentPort.postMessage(data)};\n`;
      this.thread = new Worker(new URL("data:text/javascript;base64," + Buffer.from(prefix + sources.get(url)).toString("base64")));
      this.thread.on("message", (data) => { if (data.entered) this.entered.resolve(); for (const fn of this.listeners.get("message") || []) fn({data}); });
      this.thread.on("error", (error) => { for (const fn of this.listeners.get("error") || []) fn({message:error.message,preventDefault(){}}); });
      this.thread.on("exit", (code) => this.exited.resolve(code)); workers.push(this);
    }
    addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type,new Set()); this.listeners.get(type).add(fn); }
    removeEventListener(type, fn) { this.listeners.get(type)?.delete(fn); }
    postMessage(message, transfer) { requests.push(message); this.thread.postMessage(message, transfer); }
    terminate() { this.terminated = true; void this.thread.terminate(); }
  }
  const api = create({ moduleUrl:"module", binaryUrl:"binary", workerUrl:"worker", WorkerType:WorkerBoundary, BlobType:BlobBoundary,
    urls:{createObjectURL(blob){const url=`blob:${sources.size}`; sources.set(url,blob.source); return url;},revokeObjectURL(){}},
    fetchFile:async (url) => { fetches++; return {ok:true,text:async()=>url==="module"?engineFixture:code,arrayBuffer:async()=>spinBytes.slice().buffer}; },
    ...(options.manualTimers ? {setTimer(fn){const id=++nextTimer;timers.set(id,fn);return id;},clearTimer(id){timers.delete(id);}} : {}),
  });
  t.after(() => api.destroy());
  return { api, workers, requests, timers, fetchCount:()=>fetches };
}

test("actual worker transports structural capabilities and Unicode receipts without a duplicate graph IR", async (t) => {
  const h = workerHarness(t); await h.api.default();
  const inserted = await h.api.applyParseLensInsertLineAfter("café 😀", "node", "N[東京]");
  assert.equal(inserted.result.method, "insert-after"); assert.equal(inserted.result.replacement, "N[東京]");
  assert.equal(inserted.result.replacedRange.endByte, Buffer.byteLength("café 😀"));
  assert.equal(inserted.snapshot.parsed.ir.nodes, undefined);
  const deleted = await h.api.applyParseLensDelete("café 😀", "node");
  assert.equal(deleted.result.method, "delete"); assert.equal(deleted.result.replacement, "");
  assert.equal(h.workers.length, 1); assert.equal(h.fetchCount(), 3);
  const count = h.requests.length;
  await assert.rejects(h.api.applyParseLensDelete("x", ""), /Missing element ID/u);
  await assert.rejects(h.api.applyParseLensInsertLineAfter("x", "node", "😀".repeat(530000)), /size limit/u);
  assert.equal(h.requests.length, count, "invalid edits must not even be sent to the worker");
});

test("delete and insertion can be interrupted while WebAssembly is actually executing and recover from cached bytes", { timeout: 5000 }, async (t) => {
  for (const method of ["applyParseLensDelete", "applyParseLensInsertLineAfter"]) {
    const h = workerHarness(t); await h.api.default();
    const pending = h.api[method]("source", "HANG", "new");
    const rejected = assert.rejects(pending, { name: "AbortError" });
    await h.workers[0].entered.promise;
    h.api.cancelPending(); await rejected; await h.workers[0].exited.promise;
    assert.equal(h.workers[0].terminated, true);
    assert.equal(await h.api.renderSvg("recovered"), "<svg>recovered</svg>");
    assert.equal(h.workers.length, 2); assert.equal(h.fetchCount(), 3);
  }
});

test("structural-operation watchdog and disposal terminate running work rather than only ignoring its result", { timeout: 5000 }, async (t) => {
  const h = workerHarness(t, { manualTimers: true }); await h.api.default();
  const pending = h.api.applyParseLensInsertLineAfter("source", "HANG", "new");
  const rejected = assert.rejects(pending, /time budget/u);
  await h.workers[0].entered.promise; h.timers.values().next().value(); await rejected;
  await h.workers[0].exited.promise; assert.equal(h.timers.size, 0);
  await h.api.renderSvg("restarted");
  const next = h.api.applyParseLensDelete("source", "HANG");
  const cancelled = assert.rejects(next, { name: "AbortError" });
  await h.workers[1].entered.promise; h.api.destroy(); await cancelled;
  await h.workers[1].exited.promise; assert.equal(h.timers.size, 0);
});

test("worker does not advertise absent structural exports and refuses malformed insert payloads", async () => {
  let receive; const replies = [];
  attach({addEventListener(type, fn){receive=fn;},postMessage(value){replies.push(value);}},
    { init:async()=>{}, renderSvg:()=>"<svg/>", applyParseLensInsertLineAfter:()=>assert.fail("invalid payload reached engine") });
  await receive({data:{id:1,method:"init",bytes:spinBytes.slice().buffer}});
  assert.equal(replies[0].value.includes("applyParseLensDelete"), false);
  await receive({data:{id:2,method:"applyParseLensDelete",source:"x",elementId:"node"}});
  await receive({data:{id:3,method:"applyParseLensInsertLineAfter",source:"x",elementId:"node",replacement:{}}});
  assert.deepEqual(replies.slice(1).map((r)=>r.ok),[false,false]);
});

async function hostHarness(t, doc = documentFor("flowchart LR\nA-->B\nC-->D\n")) {
  const events = new Map(), messages = [], errors = [], transactions = [];
  const uri = (name) => ({ path:name, scheme:"file", toString:()=>`file://${name}` });
  doc.uri = uri(doc.fileName); doc.isClosed = false;
  const h = { doc, messages, errors, transactions, refused:false };
  const disposable = { dispose(){} };
  const offset = (text, point) => {
    const starts = [0]; for (const match of text.matchAll(/\r\n|\n|\r/gu)) starts.push(match.index + match[0].length);
    return starts[point.line] + point.character;
  };
  class Range { constructor(sl, sc, el, ec) { this.start={line:sl,character:sc};this.end={line:el,character:ec}; } }
  const editor = { document:doc, viewColumn:1, selection:new Range(0,0,0,0), revealRange(){},
    async edit(build, options) {
      const changes = []; build({replace:(range,text)=>changes.push({range,text})});
      transactions.push({changes,options});
      if (h.refused) return false;
      const before = doc.text;
      const edits = changes.map((c)=>({...c,from:offset(before,c.range.start),to:offset(before,c.range.end)})).sort((a,b)=>b.from-a.from);
      for (const edit of edits) doc.text = doc.text.slice(0,edit.from) + edit.text + doc.text.slice(edit.to);
      const after = doc.text;
      h.undo = () => { doc.text=before;doc.version++; };
      h.redo = () => { doc.text=after;doc.version++; };
      doc.version++; events.get("change")({document:doc,contentChanges:changes}); return true;
    },
  };
  let receive, command, dispose;
  const panel = { reveal(){},onDidDispose(fn){dispose=fn;return disposable;},dispose(){dispose?.();},
    webview:{cspSource:"vscode-webview-resource:",asWebviewUri:(value)=>value.toString(),
      postMessage:(m)=>{messages.push(m);return Promise.resolve(true);},
      onDidReceiveMessage(fn){receive=fn;return disposable;}} };
  const vscode = { Uri:{joinPath:(base,...parts)=>uri(path.posix.join(base.path,...parts))},Range,
    Selection:class {constructor(start,end){this.start=start;this.end=end;}},ViewColumn:{One:1,Beside:2},
    Diagnostic:class {constructor(range,message,severity){Object.assign(this,{range,message,severity});}},
    DiagnosticSeverity:{Error:0,Warning:1,Information:2,Hint:3},TextEditorRevealType:{InCenterIfOutsideViewport:1},
    languages:{createDiagnosticCollection:()=>({set(){},delete(){},dispose(){}})},
    commands:{registerCommand(name,fn){command=fn;return disposable;}},
    window:{activeTextEditor:editor,createWebviewPanel:()=>panel,showTextDocument:async()=>h.openEditor?h.openEditor():editor,
      showWarningMessage:(m)=>errors.push(m),showErrorMessage:(m)=>errors.push(m),onDidChangeTextEditorSelection:()=>disposable},
    workspace:{fs:{stat:async()=>({}),writeFile:()=>assert.fail("source edits must not save files")},asRelativePath:()=>doc.fileName,
      getConfiguration:()=>({get:()=>1000}),onDidChangeTextDocument(fn){events.set("change",fn);return disposable;},
      onDidChangeConfiguration:()=>disposable,onDidCloseTextDocument:()=>disposable},
  };
  const filename = path.join(__dirname,"../extension.js");
  const sandbox = {module:{exports:{}},Buffer,require:(name)=>name==="vscode"?vscode
    :name==="./preview-contract.cjs"?require("../preview-contract.cjs"):require(name)};
  vm.runInNewContext(fs.readFileSync(filename,"utf8"),sandbox,{filename});
  const extension = sandbox.module.exports;
  extension.activate({extensionUri:uri("/repo/extensions/vscode-frankenmermaid"),subscriptions:[]});
  t.after(()=>extension.deactivate());
  await command(); await receive({type:"ready"});
  h.receive = (m)=>receive(m); h.editor=editor;
  h.render = ()=>messages.filter((m)=>m.type==="render").at(-1);
  h.change = (text)=>{doc.text=text;doc.version++;events.get("change")({document:doc,contentChanges:[{}]});};
  return h;
}

test("production controller and native host apply insertion/deletion as a single undoable edit without saving", async (t) => {
  for (const operation of ["insert-after", "delete"]) {
    const before = "# keep\n```mermaid\nflowchart LR\nA-->B\nC-->D\n```\nuntouched";
    const host = await hostHarness(t, documentFor(before, true));
    const ui = await controllerHarness(t, {doc:host.doc,message:host.render()});
    await host.receive(ui.messages.find((m)=>m.type==="rendered"));
    ui.open(operation);
    if (operation==="insert-after") ui.root.querySelectorAll("textarea")[0].value="N[😀]-->Q";
    await ui.button(operation==="insert-after"?"Insert source":"Delete source fragment").click();
    const request = ui.applyMessages()[0]; await host.receive(request);
    const expected = operation==="insert-after" ? before.replace("C-->D","N[😀]-->Q\nC-->D") : before.replace("A-->B\n","");
    assert.equal(host.doc.text,expected); assert.equal(host.transactions.length,1);
    assert.equal(host.transactions[0].changes.length,1);
    assert.equal(host.transactions[0].options.undoStopBefore,true);
    assert.equal(host.transactions[0].options.undoStopAfter,true);
    const acknowledgement = host.messages.find((m)=>m.type==="source-edit-result");
    assert.equal(acknowledgement.ok,true); ui.send(acknowledgement);
    await host.receive(request); assert.equal(host.transactions.length,1,"old receipt cannot be replayed");
    host.undo(); assert.equal(host.doc.text,before); host.redo(); assert.equal(host.doc.text,expected);
    assert.deepEqual(host.errors,[]);
  }
});

test("host refuses a structural write when typing races asynchronous editor opening", async (t) => {
  const host = await hostHarness(t); const ui = await controllerHarness(t,{doc:host.doc,message:host.render()});
  await host.receive(ui.messages.find((m)=>m.type==="rendered"));
  ui.open("delete"); await ui.button("Delete source fragment").click();
  const pending=deferred(); host.openEditor=()=>pending.promise;
  const applying=host.receive(ui.applyMessages()[0]);
  host.change("flowchart LR\nA-->typed\nC-->D\n"); pending.resolve(host.editor); await applying;
  assert.equal(host.transactions.length,0); assert.equal(host.doc.text,"flowchart LR\nA-->typed\nC-->D\n");
  assert.equal(host.messages.find((m)=>m.type==="source-edit-result").ok,false);
});

test("native refusal releases the structural write lock and permits retry without losing source", async (t) => {
  const host=await hostHarness(t); const ui=await controllerHarness(t,{doc:host.doc,message:host.render()});
  await host.receive(ui.messages.find((m)=>m.type==="rendered"));
  ui.open("insert-after"); ui.root.querySelectorAll("textarea")[0].value="N-->Q";
  await ui.button("Insert source").click(); const request=ui.applyMessages()[0];
  const before=host.doc.text;host.refused=true;await host.receive(request);
  assert.equal(host.doc.text,before);assert.equal(host.messages.at(-1).ok,false);
  host.refused=false;await host.receive({...request,editId:request.editId+1});
  assert.equal(host.doc.text,"flowchart LR\nA-->B\nN-->Q\nC-->D\n");
});

test("native host rejects a forged structural receipt without creating any editor transaction", async (t) => {
  const host=await hostHarness(t);const ui=await controllerHarness(t,{doc:host.doc,message:host.render()});
  await host.receive(ui.messages.find((m)=>m.type==="rendered"));
  ui.open("delete");await ui.button("Delete source fragment").click();
  const request=ui.applyMessages()[0];request.result.updatedSource="unrelated whole document";
  await host.receive(request);
  assert.equal(host.transactions.length,0);assert.equal(host.doc.text,"flowchart LR\nA-->B\nC-->D\n");
  assert.equal(host.messages.at(-1).ok,false);assert.match(host.messages.at(-1).message,/structural receipt/u);
});

function batchFixture(doc, changes) {
  const snapshot = createRenderSnapshot(doc, 7, "mixed edits");
  const raw = new Map(snapshot.blocks.map((block) => [block.id, []]));
  const edits = changes.map(({ snippet, operation = "replace", text = "", patch, diagramId = 0, id }, index) => {
    const source = snapshot.blocks[diagramId].source;
    const binding = bindingFor(source, snippet, id || `fm-node-${index}`);
    raw.get(diagramId).push(binding);
    const at = source.indexOf(snippet);
    const specified = patch ? patch(source) : { start: at, end: at + snippet.length, replacement: text };
    const result = receipt(source, binding.elementId, specified.start, specified.end, specified.replacement).result;
    return { diagramId, elementId: binding.elementId, operation, replacement: text, result };
  });
  const reports = new Map(snapshot.blocks.map((block) => [block.id,
    { bindings: checkedSourceBindings(block, raw.get(block.id)), diagnostics: [] }]));
  const message = { type: "apply-source-batch", requestId: 7, documentVersion: doc.version, editId: 1, edits };
  return { snapshot, raw, reports, message };
}
function batchPlanned(doc, changes) {
  const f = batchFixture(doc, changes);
  const result = planSourceBatch(doc, f.snapshot, f.reports, f.message);
  assert.equal(doc.text, result.before);
  return result;
}

test("mixed batches compose original UTF-8 offsets across replacement, insertion and expanded deletion", () => {
  const source = "%% café 😀\nflowchart LR\nA[😀]-->B\nC-->D\nE-->F\nG-->H\n";
  const changes = [
    { snippet: "G-->H", operation: "delete", patch: remove("G-->H\n") },
    { snippet: "C-->D", operation: "insert-after", text: "N[東京]-->Q", patch: insertBefore("E-->", "N[東京]-->Q\n") },
    { snippet: "A[😀]-->B", text: "A[longer]-->B\nB-->Z" },
  ];
  for (const ordering of [changes, [...changes].reverse(), [changes[1], changes[0], changes[2]]]) {
    const p = batchPlanned(documentFor(source), ordering);
    assert.equal(p.after, "%% café 😀\nflowchart LR\nA[longer]-->B\nB-->Z\nC-->D\nN[東京]-->Q\nE-->F\n");
    assert.equal(p.edits.length, 3);
  }
});

test("mixed batches preserve multiple Markdown containers, CRLF, prose, and per-diagram EOF normalization", () => {
  const source = "# keep 😀\r\n  ```mermaid\r\n  flowchart LR\r\n    A-->B\r\n    C-->D\r\n  ```\r\nKeep this prose\r\n~~~mermaid\r\nflowchart LR\r\nE-->F\r\nG-->H\r\n~~~\r\nFooter";
  const p = batchPlanned(documentFor(source, true), [
    { snippet: "C-->D", operation: "insert-after", text: "N[東京]-->Q", patch: append("\n  N[東京]-->Q\n") },
    { snippet: "A-->B", text: "A[changed]-->B" },
    { snippet: "G-->H", diagramId: 1, operation: "delete", patch: remove("G-->H") },
    { snippet: "E-->F", diagramId: 1, text: "E[changed]-->F" },
  ]);
  assert.equal(p.after, "# keep 😀\r\n  ```mermaid\r\n  flowchart LR\r\n    A[changed]-->B\r\n    C-->D\r\n    N[東京]-->Q\r\n  ```\r\nKeep this prose\r\n~~~mermaid\r\nflowchart LR\r\nE[changed]-->F\r\n~~~\r\nFooter");
  assert.equal(p.edits.length, 4);
});

test("adjacent whole-line deletions remove no extra separator and can empty a closed fence", () => {
  const doc = documentFor("# before\n  ```mermaid\n  A-->B\n  C-->D\n  ```\n# after", true);
  assert.equal(batchPlanned(doc, [
    { snippet: "A-->B", operation: "delete", patch: remove("A-->B\n") },
    { snippet: "C-->D", operation: "delete", patch: remove("C-->D") },
  ]).after, "# before\n  ```mermaid\n  ```\n# after");
  const blank = documentFor("```mermaid\nA-->B\nC-->D\n\n```", true);
  assert.equal(batchPlanned(blank, [
    { snippet: "A-->B", operation: "delete", patch: remove("A-->B\n") },
    { snippet: "C-->D", operation: "delete", patch: remove("C-->D\n") },
  ]).after, "```mermaid\n\n```");
});

test("mixed batches retain an unclosed EOF and never shift earlier source ranges", () => {
  const doc = documentFor("before\n  ~~~mermaid\n  flowchart LR\n  A[😀]-->B\n  C-->D", true);
  assert.equal(batchPlanned(doc, [
    { snippet: "A[😀]-->B", text: "A[changed]-->B" },
    { snippet: "C-->D", operation: "insert-after", text: "E-->F", patch: append("\nE-->F\n") },
  ]).after, "before\n  ~~~mermaid\n  flowchart LR\n  A[changed]-->B\n  C-->D\n  E-->F\n");
});

test("actual patch overlap and shared insertion points abort independent of operation order", () => {
  const doc = documentFor("flowchart LR\n  A-->B\n  C-->D\nE-->F\n");
  const cases = [
    [ { snippet: "A-->B", operation: "delete", patch: remove("  A-->B\n") }, { snippet: "B", text: "Z" } ],
    [ { snippet: "A", operation: "insert-after", text: "N", patch: insertBefore("  C", "  N\n") },
      { snippet: "B", operation: "insert-after", text: "Q", patch: insertBefore("  C", "  Q\n") } ],
    [ { snippet: "A-->B", operation: "insert-after", text: "N", patch: insertBefore("  C", "  N\n") },
      { snippet: "C-->D", operation: "delete", patch: remove("  C-->D\n") } ],
  ];
  for (const changes of cases) for (const order of [changes, [...changes].reverse()]) {
    assert.throws(() => batchPlanned(doc, order), /overlap|insertion point/u);
  }
});

test("inserting after an anchor simultaneously changed or deleted is not silently accepted", () => {
  const doc = documentFor("flowchart LR\nA-->B\nC-->D\n");
  for (const edit of [ { snippet: "A-->B", operation: "delete", patch: remove("A-->B\n") },
    { snippet: "B", text: "Z" } ]) {
    const changes = [ { snippet: "A-->B", operation: "insert-after", text: "N", patch: insertBefore("C-->", "N\n") }, edit ];
    for (const order of [changes, [...changes].reverse()]) assert.throws(() => batchPlanned(doc, order), /anchor/u);
  }
});

test("combined fence escapes fail even when each individual receipt is valid", () => {
  const doc = documentFor("```mermaid\nA-->B\n`X``\n```\nprose", true);
  const f = batchFixture(doc, [
    { snippet: "A-->B", operation: "insert-after", text: "N", patch: insertBefore("`X", "N\n") },
    { snippet: "X", operation: "delete", patch: remove("X") },
  ]);
  // This delete alone closes the fence, so it must already fail its individual check.
  assert.throws(() => planSourceEdit(doc, f.snapshot, f.reports, f.message.edits[1]), /fence/u);
  const combined = documentFor("```mermaid\nA-->B\nXY\n```\nprose", true);
  const g = batchFixture(combined, [
    { snippet: "A-->B", operation: "insert-after", text: "N", patch: insertBefore("XY", "N\n") },
    { snippet: "X", text: "``" }, { snippet: "Y", text: "`" },
  ]);
  // Insertion shares X's native start, so put X after a preserved leading space to make every
  // patch disjoint while the two replacements still jointly form a Markdown closer.
  const padded = documentFor("```mermaid\nA-->B\n XY\n```\nprose", true);
  const p = batchFixture(padded, [
    { snippet: "A-->B", operation: "insert-after", text: "N", patch: insertBefore(" XY", "N\n") },
    { snippet: "X", text: "``" }, { snippet: "Y", text: "`" },
  ]);
  for (const edit of p.message.edits) assert.doesNotThrow(() => planSourceEdit(padded, p.snapshot, p.reports, edit));
  assert.throws(() => planSourceBatch(padded, p.snapshot, p.reports, p.message), /Combined edits/u);
  assert.throws(() => planSourceBatch(combined, g.snapshot, g.reports, g.message), /overlap/u);
});

test("mixed batch payload, document size, no-op and malformed-last-receipt checks remain atomic", () => {
  const doc = documentFor("flowchart LR\nA-->B\nC-->D\nE-->F\n");
  const changes = [ { snippet: "A-->B", text: "A-->B" },
    { snippet: "C-->D", operation: "insert-after", text: "N", patch: insertBefore("E-->", "N\n") } ];
  assert.equal(batchPlanned(doc, changes).edits.length, 1);
  const f = batchFixture(doc, changes);
  f.message.edits[1].result.updatedSource += "foreign";
  assert.throws(() => planSourceBatch(doc, f.snapshot, f.reports, f.message), /structural receipt/u);
  f.message.edits[1].result.updatedSource = "x".repeat(16 * 1024 * 1024);
  assert.throws(() => planSourceBatch(doc, f.snapshot, f.reports, f.message), /16 MiB/u);
  const large = "x".repeat(1100000);
  assert.throws(() => batchPlanned(doc, [ { snippet: "A-->B", text: large },
    { snippet: "C-->D", operation: "insert-after", text: large, patch: insertBefore("E-->", large + "\n") } ]), /2 MiB/u);
});

async function batchUiHarness(t, doc, changes, message) {
  const f = batchFixture(doc, changes), calls = [];
  const call = (operation, source, id, text = "") => {
    const edit = f.message.edits.find((entry) => entry.elementId === id);
    assert.ok(edit, "the UI must call an actual selected element");
    assert.equal(operation, edit.operation); assert.equal(text, edit.replacement);
    assert.equal(source, f.snapshot.blocks[edit.diagramId].source, "all operations must address the original render");
    calls.push({ operation, source, id, text });
    return { result: structuredClone(edit.result), snapshot: { bindings: [] } };
  };
  const engine = { default:async()=>{}, renderSvg:async()=>"<svg/>",
    parseLens:async(source)=>({bindings: f.raw.get(f.snapshot.blocks.find((block)=>block.source===source).id),parsed:{warnings:[]}}),
    applyParseLensEdit:async(...args)=>call("replace",...args),
    applyParseLensDelete:async(...args)=>call("delete",...args),
    applyParseLensInsertLineAfter:async(...args)=>call("insert-after",...args),
  };
  const ui = await controllerHarness(t, {doc, message:message || f.snapshot.message, engine,
    raw:f.raw.get(0)[0], renderBindings:f.raw});
  ui.stage = async(index) => {
    const edit = f.message.edits[index], bindings = f.raw.get(edit.diagramId);
    ui.open(edit.operation, bindings.findIndex((binding)=>binding.elementId===edit.elementId), edit.diagramId);
    if (edit.operation!=="delete") ui.root.querySelectorAll("textarea")[0].value=edit.replacement;
    await ui.button("Stage fragment").click();
  };
  return { ...ui, fixture:f, engineCalls:calls };
}

const mixedSource = "flowchart LR\nA-->B\nC-->D\nE-->F\nG-->H\n";
const mixedChanges = () => [
  { snippet:"A-->B",text:"A[東京]-->B" },
  { snippet:"C-->D",operation:"insert-after",text:"N[😀]-->Q",patch:insertBefore("E-->","N[😀]-->Q\n") },
  { snippet:"G-->H",operation:"delete",patch:remove("G-->H\n") },
];
const mixedExpected = "flowchart LR\nA[東京]-->B\nC-->D\nN[😀]-->Q\nE-->F\n";

test("preview stages every operation with exact before/after text and sends just one atomic batch", async(t)=>{
  const h=await batchUiHarness(t,documentFor(mixedSource),mixedChanges());
  await h.stage(2);await h.stage(0);await h.stage(1);
  assert.equal(h.applyMessages().length,0);assert.equal(h.engineCalls.length,3);
  assert.match(h.root.textContent,/Delete —/u);assert.match(h.root.textContent,/Replace —/u);assert.match(h.root.textContent,/Insert after —/u);
  assert.match(h.root.textContent,/Selected source:\nC-->D/u);assert.match(h.root.textContent,/After:\nN\[😀\]-->Q\n/u);
  await h.button("Apply staged edits").click();const request=h.applyMessages()[0];
  assert.equal(request.type,"apply-source-batch");assert.equal(request.edits.length,3);
  assert.deepEqual(request.edits.map((e)=>e.operation||"replace"),["delete","replace","insert-after"]);
  const p=planSourceBatch(h.doc,h.fixture.snapshot,h.fixture.reports,request);assert.equal(p.after,mixedExpected);
  await h.button("Apply staged edits").click();assert.equal(h.applyMessages().length,1);
});

test("staging rejects the real expanded overlap and invalidated anchors without discarding pending source", async(t)=>{
  for (const changes of [
    [ {snippet:"A-->B",operation:"delete",patch:remove("A-->B\n")}, {snippet:"B",text:"Z"} ],
    [ {snippet:"A-->B",operation:"insert-after",text:"N",patch:insertBefore("C-->","N\n")},
      {snippet:"B",text:"Z"} ],
  ]) for(const order of [changes,[...changes].reverse()]) {
    const h=await batchUiHarness(t,documentFor("flowchart LR\nA-->B\nC-->D\n"),order);
    await h.stage(0);await h.stage(1);
    assert.match(h.root.textContent,/overlaps|anchor/u);
    assert.equal(h.root.querySelectorAll("textarea").length,1);
    assert.equal(h.applyMessages().length,0);
    assert.equal(h.root.querySelectorAll("summary").filter((n)=>/— diagram/u.test(n.textContent)).length,1);
  }
});

test("staging removal releases the anchor reservation and refused mixed batches remain editable", async(t)=>{
  const h=await batchUiHarness(t,documentFor(mixedSource),mixedChanges());
  await h.stage(1);await h.button("Remove staged fragment").click();
  assert.equal(h.button("Apply staged edits").disabled,true);
  await h.stage(1);await h.stage(2);await h.button("Apply staged edits").click();
  const request=h.applyMessages()[0];h.send({...request,type:"source-edit-result",ok:false,message:"Read only"});
  assert.match(h.root.textContent,/N\[😀\]-->Q/u);assert.match(h.root.textContent,/G-->H/u);
  assert.equal(h.button("Apply staged edits").disabled,false);
  await h.button("Remove staged fragment").click();await h.button("Apply staged edits").click();
  assert.equal(h.applyMessages()[1].edits.length,1);assert.equal(h.applyMessages()[1].edits[0].operation,"delete");
});

test("a deletion review cannot be bypassed by applying an existing batch, and invalidation keeps staged text", async(t)=>{
  const h=await batchUiHarness(t,documentFor(mixedSource),mixedChanges());
  await h.stage(0);h.open("delete",2);
  await h.button("Apply staged edits").click();assert.match(h.root.textContent,/Stage or discard/u);
  assert.equal(h.applyMessages().length,0);
  await h.button("Stage fragment").click();
  h.send({type:"invalidate",requestId:8,documentVersion:2});
  assert.match(h.root.textContent,/A\[東京\]-->B/u);assert.match(h.root.textContent,/G-->H/u);
  assert.equal(h.button("Apply staged edits").disabled,true);
  await h.button("Apply staged edits").click();assert.equal(h.applyMessages().length,0);
});

test("native host applies a mixed batch as one editor transaction and one complete undo", async(t)=>{
  const host=await hostHarness(t,documentFor(mixedSource));
  const ui=await batchUiHarness(t,host.doc,mixedChanges(),host.render());
  await host.receive(ui.messages.find((m)=>m.type==="rendered"));
  await ui.stage(2);await ui.stage(0);await ui.stage(1);await ui.button("Apply staged edits").click();
  const request=ui.applyMessages()[0];await host.receive(request);
  assert.equal(host.doc.text,mixedExpected);assert.equal(host.transactions.length,1);
  assert.equal(host.transactions[0].changes.length,3);
  assert.equal(host.transactions[0].options.undoStopBefore,true);assert.equal(host.transactions[0].options.undoStopAfter,true);
  assert.equal(host.messages.find((m)=>m.type==="source-edit-result").ok,true);
  await host.receive(request);assert.equal(host.transactions.length,1);
  host.undo();assert.equal(host.doc.text,mixedSource);host.redo();assert.equal(host.doc.text,mixedExpected);
});

test("native mixed batches have no partial effects after a bad last receipt, refusal, or typing race", async(t)=>{
  for(const failure of ["receipt","refused","race"]) {
    const host=await hostHarness(t,documentFor(mixedSource));
    const ui=await batchUiHarness(t,host.doc,mixedChanges(),host.render());
    await host.receive(ui.messages.find((m)=>m.type==="rendered"));
    await ui.stage(0);await ui.stage(1);await ui.stage(2);await ui.button("Apply staged edits").click();
    const request=ui.applyMessages()[0];
    if(failure==="receipt")request.edits[2].result.updatedSource+="forged";
    if(failure==="refused")host.refused=true;
    const opening=deferred();if(failure==="race")host.openEditor=()=>opening.promise;
    const pending=host.receive(request);
    if(failure==="race"){host.change(mixedSource+"%% external typing\n");opening.resolve(host.editor);}
    await pending;
    assert.equal(host.doc.text,mixedSource+(failure==="race"?"%% external typing\n":""));
    assert.equal(host.transactions.length,failure==="refused"?1:0);
    const reply=host.messages.find((m)=>m.type==="source-edit-result");assert.equal(reply.ok,false);ui.send(reply);
    assert.match(ui.root.textContent,/N\[😀\]-->Q/u);assert.match(ui.root.textContent,/G-->H/u);
  }
});
