"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const {
  buildPreviewHtml, createRenderSnapshot, DebouncedRenderScheduler, extractMermaidBlocks,
  isMermaidDocument, isPreviewDocument, normalizePreviewDebounceMs, MAX_PREVIEW_DIAGRAMS,
  blockRange, checkedSourceBindings, checkedDiagnostics, isCurrentSnapshot,
} = require("../preview-contract.cjs");
const { createPreviewController } = require("../media/preview.js");

function documentFor(text, fileName = "/work/readme.md", languageId = "markdown") {
  return { fileName, languageId, version: 3, isClosed: false,
    uri: { path: fileName, toString: () => `file://${fileName}` }, getText: () => text };
}

test("recognizes Mermaid files and Markdown without treating prose as Mermaid", () => {
  assert.equal(isMermaidDocument(documentFor("", "x.txt", "mermaid")), true);
  assert.equal(isMermaidDocument(documentFor("", "x.mmd", "plaintext")), true);
  assert.equal(isMermaidDocument(documentFor("", "x.MERMAID", "plaintext")), true);
  assert.equal(isMermaidDocument(documentFor("")), false);
  assert.equal(isPreviewDocument(documentFor("")), true);
  assert.equal(isPreviewDocument(null), false);
  assert.equal(isPreviewDocument(documentFor("", "x.js", "javascript")), false);
});

test("extracts all fenced diagrams with CRLF and exact indentation/source lines", () => {
  const blocks = extractMermaidBlocks(documentFor(
    "# title\r\n  ```mermaid title\r\n  flowchart LR\r\n   A-->B\r\n  ```\r\nprose\r\n~~~MERMAID\r\npie\r\n~~~\r\n"));
  assert.equal(blocks.length, 2);
  assert.equal(blocks[0].source, "flowchart LR\n A-->B");
  assert.equal(blocks[0].startLine, 2);
  assert.deepEqual(blocks[0].lineMap, [
    { line: 2, indent: 2, text: "flowchart LR" },
    { line: 3, indent: 2, text: " A-->B" },
  ]);
  assert.equal(blocks[1].source, "pie");
  assert.equal(blocks[1].startLine, 7);
});

test("tracks non-Mermaid fences, longer fences and literal nested examples", () => {
  const blocks = extractMermaidBlocks(documentFor(
    "````text\n```mermaid\nnot a diagram\n```\n````\n" +
    "````mermaid\nflowchart LR\n```\n~~~\nA-->B\n````\n"));
  assert.equal(blocks.length, 1);
  assert.equal(blocks[0].source, "flowchart LR\n```\n~~~\nA-->B");
});

test("unclosed fences render through EOF and empty blocks remain visible", () => {
  const blocks = extractMermaidBlocks(documentFor("```mermaid\n```\n~~~mermaid\nflowchart LR\nA-->B"));
  assert.equal(blocks[0].source, "");
  assert.equal(blocks[1].source, "flowchart LR\nA-->B");
  assert.deepEqual(extractMermaidBlocks(documentFor("Just prose.")), []);
});

test("rejects invalid openers, longer language names, and indented code blocks", () => {
  assert.equal(extractMermaidBlocks(documentFor("    ```mermaid\n    A-->B\n    ```")).length, 0);
  assert.equal(extractMermaidBlocks(documentFor("```mermaid-js\nA-->B\n```")).length, 0);
  assert.equal(extractMermaidBlocks(documentFor("```mermaid `bad`\nA-->B")).length, 0);
});

test("whole Mermaid sources preserve bytes while snapshots keep maps host-side", () => {
  const source = "flowchart LR\r\nA[😀]-->B\r\n";
  const doc = documentFor(source, "x.mmd", "plaintext");
  const snapshot = createRenderSnapshot(doc, 9, "x.mmd");
  assert.equal(snapshot.blocks[0].source, source);
  assert.equal(snapshot.message.documentVersion, 3);
  assert.equal(snapshot.message.requestId, 9);
  assert.equal(snapshot.message.diagrams[0].lineMap, undefined);
  assert.equal(snapshot.blocks[0].lineMap[1].text, "A[😀]-->B");
});

test("source and diagram limits fail explicitly rather than silently truncating", () => {
  assert.throws(() => extractMermaidBlocks(documentFor("😀".repeat(530000))), /2 MiB/u);
  assert.equal(extractMermaidBlocks(documentFor("```mermaid\nA\n```\n".repeat(MAX_PREVIEW_DIAGRAMS))).length, MAX_PREVIEW_DIAGRAMS);
  assert.throws(() => extractMermaidBlocks(documentFor("```mermaid\nA\n```\n".repeat(MAX_PREVIEW_DIAGRAMS + 1))), /at most 64/u);
});

test("webview permits WASM and SVG styles without enabling arbitrary script evaluation", () => {
  const html = buildPreviewHtml({ cspSource: "vscode-webview-resource:", nonce: "test-nonce",
    scriptUri: "vscode-webview-resource:/media/preview.js",
    wasmModuleUri: "vscode-webview-resource:/pkg/frankenmermaid.js",
    wasmBinaryUri: 'vscode-webview-resource:/pkg/x.wasm?x="&y=<' });
  assert.match(html, /default-src 'none'/u);
  const scriptPolicy = /script-src ([^;]+)/u.exec(html)[1];
  assert.match(scriptPolicy, /'nonce-test-nonce'/u);
  assert.match(scriptPolicy, /'wasm-unsafe-eval'/u);
  assert.doesNotMatch(scriptPolicy, /'unsafe-eval'|'unsafe-inline'/u);
  assert.match(html, /data-wasm-binary="[^"<>]*&quot;&amp;y=&lt;"/u);
  assert.match(html, /style-src 'nonce-test-nonce'/u);
});

test("scheduler keeps the latest edit, validates delays, and cannot revive after disposal", () => {
  let nextTimer = 0;
  const timers = new Map();
  const cleared = [];
  const scheduler = new DebouncedRenderScheduler(75,
    (callback, delayMs) => { const id = ++nextTimer; timers.set(id, { callback, delayMs }); return id; },
    (id) => cleared.push(id));
  const renders = [];
  scheduler.schedule(() => renders.push("stale"));
  scheduler.schedule(() => renders.push("latest"));
  assert.deepEqual(cleared, [1]);
  assert.equal(timers.get(2).delayMs, 75);
  timers.get(2).callback();
  assert.deepEqual(renders, ["latest"]);
  scheduler.setDelayMs(0);
  scheduler.schedule(() => renders.push("disposed"));
  assert.equal(timers.get(3).delayMs, 0);
  scheduler.dispose();
  timers.get(3).callback();
  scheduler.schedule(() => renders.push("revived"));
  assert.equal(nextTimer, 3);
  assert.deepEqual(renders, ["latest"]);
  for (const value of [-1, 1001, 12.5, NaN]) assert.equal(normalizePreviewDebounceMs(value), 75);
  assert.equal(normalizePreviewDebounceMs(0), 0);
  assert.equal(normalizePreviewDebounceMs(1000), 1000);
});

// A DOM boundary double: records operations on the real controller without replacing any source.
// These tests do NOT claim browser pixel equivalence or execution of the Rust WASM engine.
class Element {
  constructor(tag) { this.localName = tag; this.children = []; this.attrs = new Map(); this.listeners = new Map(); this._text = ""; }
  set textContent(text) { this._text = String(text); this.children = []; }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  get attributes() { return [...this.attrs].map(([name, value]) => ({ name, value })); }
  get id() { return this.getAttribute("id") || ""; }
  setAttribute(name, value) { this.attrs.set(name, String(value)); }
  getAttribute(name) { return this.attrs.get(name) ?? null; }
  removeAttribute(name) { this.attrs.delete(name); }
  append(...nodes) { for (const node of nodes) { node.parent = this; this.children.push(node); } }
  replaceChildren(...nodes) { this.children = []; this._text = ""; this.append(...nodes); }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter((node) => node !== this); }
  addEventListener(type, fn) { this.listeners.set(type, fn); }
  querySelectorAll(selector) {
    return this.children.flatMap((node) => [
      ...(selector === "*" || node.localName === selector || (selector === "[id]" && node.id) ? [node] : []), ...node.querySelectorAll(selector),
    ]);
  }
  attachShadow() { this.shadowRoot = new Element("shadow-root"); return this.shadowRoot; }
}

function previewHarness({ engine, loadEngine, yieldToHost } = {}) {
  const root = new Element("main");
  const status = new Element("div");
  const listeners = new Map();
  const calls = [];
  const messages = [];
  const initializations = [];
  const document = { body: { dataset: { wasmModule: "local-module", wasmBinary: "local-wasm", styleNonce: "nonce" } },
    getElementById: (id) => id === "preview" ? root : status,
    createElement: (tag) => new Element(tag), importNode: (node) => node };
  const window = {
    addEventListener: (type, fn) => listeners.set(type, fn),
    removeEventListener: (type) => listeners.delete(type),
    DOMParser: class {
      parseFromString(svg) {
        const node = new Element(svg === "INVALID" ? "parsererror" : "svg");
        node.namespaceURI = "http://www.w3.org/2000/svg";
        node.setAttribute("onclick", "bad()");
        node.append(new Element("script"), new Element("style"), new Element("defs"));
        const bound = new Element("g"); bound.setAttribute("id", "fm-node-a-0"); node.append(bound);
        return { documentElement: node, querySelector: () => svg === "INVALID" ? node : null };
      }
    },
  };
  const actualEngine = engine || { default: async (arg) => initializations.push(arg),
    renderSvg: (source) => { calls.push(source); if (source === "THROW") throw new Error("broken"); return source === "INVALID" ? source : "<svg/>"; } };
  const controller = createPreviewController({ document, window, vscode: { postMessage: (message) => messages.push(message) },
    loadEngine: loadEngine || (async () => actualEngine), yieldToHost: yieldToHost || (async () => {}) });
  const send = (sources, requestId = 1) => listeners.get("message")({
    origin: "vscode-webview://opaque-origin", source: null,
    data: { type: "render", title: "file.md", requestId, documentVersion: 3,
      diagrams: sources.map((source, id) => ({ id, source, startLine: id * 3 })) },
  });
  return { root, status, listeners, calls, messages, initializations, controller, send };
}

const settle = () => new Promise((resolve) => setImmediate(resolve));

test("real webview controller initializes with an object and handles opaque-origin messages", async () => {
  const h = previewHarness();
  await h.controller.start();
  assert.deepEqual(h.initializations, [{ module_or_path: "local-wasm" }]);
  assert.deepEqual(h.messages, [{ type: "ready" }]);
  h.send(["flowchart LR\nA-->B", "pie\n\"A\": 1"]);
  await settle();
  assert.equal(h.root.children.length, 2);
  assert.equal(h.calls.length, 2);
  assert.equal(h.status.textContent, "2 diagrams.");
  const views = h.root.children.map((card) => card.children[1].shadowRoot);
  assert.notEqual(views[0], views[1]);
  for (const view of views) {
    const svg = view.children[1];
    assert.equal(svg.getAttribute("onclick"), null);
    assert.equal(svg.querySelectorAll("script").length, 0);
    assert.equal(svg.querySelectorAll("style")[0].getAttribute("nonce"), "nonce");
  }
  h.listeners.get("message")({ data: { type: "render", source: 42, title: "invalid" } });
  assert.equal(h.calls.length, 2);
  h.controller.dispose();
});

test("a failed or malformed diagram does not erase valid siblings", async () => {
  const h = previewHarness(); await h.controller.start();
  h.send(["THROW", "valid", "INVALID"]); await settle();
  assert.equal(h.root.children.length, 3);
  assert.match(h.root.children[0].textContent, /broken/u);
  assert.ok(h.root.children[1].children[1].shadowRoot);
  assert.match(h.root.children[2].textContent, /invalid SVG/u);
  assert.match(h.status.textContent, /2 could not render/u);
});

test("a newer edit cancels an in-flight multi-diagram render atomically", async () => {
  const pauses = [];
  const h = previewHarness({ yieldToHost: () => new Promise((resolve) => pauses.push(resolve)) });
  await h.controller.start();
  h.send(["old A", "old B"], 1);
  h.send(["new"], 2);
  assert.deepEqual(h.calls, ["old A", "new"]);
  pauses[1](); await settle();
  assert.equal(h.root.children.length, 1);
  const committed = h.root.children[0];
  pauses[0](); await settle();
  assert.equal(h.root.children[0], committed);
  assert.deepEqual(h.calls, ["old A", "new"]);
  h.send(["out of order"], 1);
  assert.deepEqual(h.calls, ["old A", "new"]);
});

test("empty documents and host errors clear obsolete diagrams", async () => {
  const h = previewHarness(); await h.controller.start();
  h.send(["A"]); await settle();
  h.send([], 2); await settle();
  assert.equal(h.root.children.length, 0);
  assert.match(h.status.textContent, /No Mermaid/u);
  h.listeners.get("message")({ data: { type: "preview-error", requestId: 3, message: "Too large" } });
  assert.equal(h.status.textContent, "Too large");
  h.send(["stale"], 2);
  assert.equal(h.calls.length, 1);
});

test("failed initialization is retryable; disposal suppresses late initialization", async () => {
  let attempts = 0;
  const h = previewHarness({ loadEngine: async () => {
    attempts += 1;
    if (attempts === 1) throw new Error("missing assets");
    return { default: async () => {}, renderSvg: () => "<svg/>" };
  } });
  await h.controller.start();
  assert.match(h.status.textContent, /missing assets/u);
  assert.equal(h.root.children[0].localName, "button");
  await h.controller.start();
  assert.deepEqual(h.messages, [{ type: "ready" }]);
  let resolve;
  const disposed = previewHarness({ loadEngine: () => new Promise((done) => { resolve = done; }) });
  const initializing = disposed.controller.start();
  disposed.controller.dispose();
  resolve({ default: () => assert.fail("must not initialize after disposal") });
  await initializing;
  assert.deepEqual(disposed.messages, []);
  assert.equal(disposed.listeners.size, 0);
});

function extensionHarness({ assetsAvailable = () => true } = {}) {
  const commands = new Map(); const events = new Map(); const created = []; const errors = []; const stats = [];
  const problems = new Map(); const editors = [];
  const uri = (name) => ({ path: name, toString: () => `file://${name}` });
  const vscode = {
    Uri: { joinPath: (base, ...parts) => uri(path.posix.join(base.path, ...parts)) },
    ViewColumn: { Beside: 2, One: 1 },
    Range: class { constructor(line, character, endLine, endCharacter) {
      this.start = { line, character }; this.end = { line: endLine, character: endCharacter };
    } },
    Selection: class { constructor(start, end) { this.start = start; this.end = end; } },
    Diagnostic: class { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
    DiagnosticSeverity: { Error: 0, Warning: 1, Information: 2, Hint: 3 },
    TextEditorRevealType: { InCenterIfOutsideViewport: 2 },
    languages: { createDiagnosticCollection: () => ({
      set: (value, entries) => problems.set(value.toString(), entries),
      delete: (value) => problems.delete(value.toString()), dispose: () => problems.clear(),
    }) },
    window: {
      activeTextEditor: undefined,
      showWarningMessage: (message) => errors.push(message), showErrorMessage: (message) => errors.push(message),
      showTextDocument: async (document, options) => {
        const editor = { document, options, revealed: [], revealRange: (range) => editor.revealed.push(range) };
        editors.push(editor); return editor;
      },
      createWebviewPanel: () => {
        const panel = { messages: [], disposed: false, reveal: () => {},
          onDidDispose: (fn) => { panel.onDispose = fn; },
          dispose: () => { if (!panel.disposed) { panel.disposed = true; panel.onDispose(); } },
          webview: { cspSource: "https://local-resource.invalid", asWebviewUri: (value) => value.toString(),
            postMessage: (message) => panel.messages.push(message),
            onDidReceiveMessage: (fn) => { panel.receive = fn; },
            set html(value) { assert.ok(panel.receive, "ready listener must precede HTML"); panel.html = value; },
          },
        };
        created.push(panel); return panel;
      },
    },
    workspace: {
      fs: { stat: async (value) => { stats.push(value.path); if (!assetsAvailable(value.path)) throw new Error("ENOENT"); return {}; } },
      getConfiguration: () => ({ get: () => 0 }), asRelativePath: (value) => value.path,
      onDidChangeTextDocument: (fn) => events.set("edit", fn),
      onDidChangeConfiguration: (fn) => events.set("config", fn),
      onDidCloseTextDocument: (fn) => events.set("close", fn),
    },
    commands: { registerCommand: (name, fn) => commands.set(name, fn) },
  };
  const module = { exports: {} };
  const filename = path.join(__dirname, "..", "extension.js");
  vm.runInNewContext(fs.readFileSync(filename, "utf8"), { module, require: (name) => name === "vscode" ? vscode
    : name === "./preview-contract.cjs" ? require("../preview-contract.cjs") : require(name), }, { filename });
  const context = { extensionUri: uri("/repo/extensions/vscode-frankenmermaid"), subscriptions: [] };
  module.exports.activate(context);
  const open = async (doc) => { vscode.window.activeTextEditor = { document: doc }; await commands.get("frankenmermaid.showPreview")(); };
  return { commands, events, created, errors, stats, problems, editors, open, vscode, deactivate: module.exports.deactivate };
}

test("extension executes Markdown preview command, waits for ready, and resends latest document", async (t) => {
  const h = extensionHarness(); t.after(h.deactivate);
  const doc = documentFor("# Readme\n```mermaid\nA-->B\n```");
  await h.open(doc);
  assert.equal(h.created.length, 1);
  const panel = h.created[0];
  assert.equal(panel.messages.length, 0);
  panel.receive({ type: "ready" });
  assert.equal(panel.messages[0].diagrams[0].source, "A-->B");
  assert.equal(panel.messages[0].documentVersion, 3);
  doc.version = 4; doc.getText = () => "```mermaid\nB-->C\n```";
  h.events.get("edit")({ document: doc, contentChanges: [{}] });
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.equal(panel.messages.at(-1).diagrams[0].source, "B-->C");
  assert.equal(panel.messages.at(-1).documentVersion, 4);
  panel.receive({ type: "ready" }); // A recreated webview after hide/reveal must catch up.
  assert.equal(panel.messages.at(-1).diagrams[0].source, "B-->C");
  await h.open(doc); assert.equal(h.created.length, 1);
  h.events.get("close")(doc); assert.equal(panel.disposed, true);
});

test("checkout engine fallback is extension-owned, and missing assets fail visibly", async (t) => {
  const h = extensionHarness({ assetsAvailable: (name) => name.startsWith("/repo/pkg/") });
  t.after(h.deactivate);
  await h.open(documentFor("A-->B", "x.mmd", "mermaid"));
  assert.ok(h.stats.includes("/repo/pkg/frankenmermaid_bg.wasm"));
  assert.match(h.created[0].html, /\/repo\/pkg\/frankenmermaid\.js/u);
  const missing = extensionHarness({ assetsAvailable: () => false }); t.after(missing.deactivate);
  await missing.open(documentFor("A-->B", "x.mmd", "mermaid"));
  assert.equal(missing.created[0].disposed, true);
  assert.match(missing.errors[0], /engine assets are missing/u);
});

function bindingFor(source, snippet, elementId = "fm-node-a-0") {
  const offset = source.indexOf(snippet);
  assert.ok(offset >= 0);
  const startByte = Buffer.byteLength(source.slice(0, offset));
  return { elementId, sourceId: "A", snippet,
    textRange: { startByte, endByte: startByte + Buffer.byteLength(snippet) } };
}

test("source bindings map UTF-8 bytes to exact UTF-16 document ranges inside Markdown", () => {
  const block = extractMermaidBlocks(documentFor("# 😀\r\n  ```mermaid\r\n  flowchart LR\r\n  A[😀 café]-->B\r\n  ```"))[0];
  const binding = bindingFor(block.source, "café");
  const selected = checkedSourceBindings(block, [binding]).get(binding.elementId);
  assert.deepEqual(selected.range, { start: { line: 3, character: 7 }, end: { line: 3, character: 11 } });
  assert.equal(selected.snippet, "café");
});

test("multiline source bindings preserve CRLF coordinates and reject broken boundaries", () => {
  const source = "flowchart LR\r\nA[😀]\r\nB[café]\r\n";
  const block = extractMermaidBlocks(documentFor(source, "x.mmd", "mermaid"))[0];
  const raw = bindingFor(source, "A[😀]\r\nB[café]");
  assert.deepEqual(checkedSourceBindings(block, [raw]).get(raw.elementId).range,
    { start: { line: 1, character: 0 }, end: { line: 2, character: 7 } });
  const emoji = bindingFor(source, "😀");
  assert.throws(() => checkedSourceBindings(block, [{ ...emoji,
    textRange: { startByte: emoji.textRange.startByte + 1, endByte: emoji.textRange.endByte } }]), /UTF-8/u);
  assert.throws(() => checkedSourceBindings(block, [{ ...raw, snippet: "changed" }]), /does not match/u);
  assert.throws(() => checkedSourceBindings(block, [raw, raw]), /duplicate/u);
  assert.throws(() => checkedSourceBindings(block, [{ ...raw, textRange: { startByte: -1, endByte: 4 } }]), /byte range/u);
  assert.equal(checkedSourceBindings(block, [{ elementId: "synthetic", textRange: null }]).size, 0);
});

test("diagnostics map engine lines without guessing column units; empty fences stay in bounds", () => {
  const block = extractMermaidBlocks(documentFor("# Readme\n  ```mermaid\n  flowchart LR\n  A[😀]\n  B\n  ```"))[0];
  const issues = checkedDiagnostics(block, [{ severity: "error", message: "broken", suggestion: "fix this",
    span: { start: { line: 2, col: 999 }, end: { line: 3, col: 999 } } }]);
  assert.deepEqual(issues[0].range, { start: { line: 3, character: 2 }, end: { line: 4, character: 3 } });
  assert.equal(issues[0].suggestion, "fix this");
  const empty = extractMermaidBlocks(documentFor("```mermaid"))[0];
  assert.deepEqual(blockRange(empty), { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } });
  assert.throws(() => checkedDiagnostics(block, [{ message: {} }]), /Invalid/u);
});

test("current-snapshot checks reject closed, disposed, and immediately edited documents", () => {
  const document = documentFor("A", "x.mmd", "mermaid");
  const entry = { document, snapshot: createRenderSnapshot(document, 1, "x.mmd") };
  const message = { requestId: 1, documentVersion: 3 };
  assert.equal(isCurrentSnapshot(entry, message), true);
  assert.equal(isCurrentSnapshot(entry, null), false);
  document.version = 4; assert.equal(isCurrentSnapshot(entry, message), false);
  document.version = 3; entry.disposed = true; assert.equal(isCurrentSnapshot(entry, message), false);
  entry.disposed = false; document.isClosed = true; assert.equal(isCurrentSnapshot(entry, message), false);
});

test("production controller exposes engine diagnostics and keyboard source navigation", async () => {
  const source = "flowchart LR\nA[😀]-->B";
  const binding = bindingFor(source, "A[😀]-->B");
  const engine = { default: async () => {}, renderSvg: () => "<svg/>",
    parseLens: () => ({ bindings: [binding], parsed: { warnings: ["recovered"], ir: { diagnostics: [
      { severity: "Error", message: "broken", span: { start: { line: 2 }, end: { line: 2 } } },
    ] } } }) };
  const h = previewHarness({ engine }); await h.controller.start();
  h.send([source]); await settle();
  const report = h.messages.at(-1);
  assert.equal(report.type, "rendered");
  assert.equal(report.reports[0].bindings[0], binding);
  assert.deepEqual(report.reports[0].diagnostics.map((value) => value.severity), ["error", "warning"]);
  const svg = h.root.children[0].children[1].shadowRoot.children[1];
  const node = svg.querySelectorAll("[id]")[0];
  assert.equal(node.getAttribute("role"), "button");
  let prevented = 0;
  node.listeners.get("keydown")({ key: "Enter", preventDefault: () => prevented++, stopPropagation: () => {} });
  assert.equal(prevented, 1);
  assert.deepEqual(h.messages.at(-1), { type: "reveal", requestId: 1, documentVersion: 3, diagramId: 0, elementId: binding.elementId });
  const before = h.messages.length;
  h.send([source], 2);
  node.listeners.get("click")({ preventDefault: () => {}, stopPropagation: () => {} });
  assert.equal(h.messages.length, before, "old DOM must not emit navigation after a new render starts");
  await settle();
});

test("optional inspection failure keeps the diagram and reports the actual failure", async () => {
  const h = previewHarness({ engine: { default: async () => {}, renderSvg: () => "<svg/>",
    parseLens: () => { throw new Error("bad map"); } } });
  await h.controller.start(); h.send(["A"]); await settle();
  assert.ok(h.root.children[0].children[1].shadowRoot);
  assert.match(h.messages.at(-1).reports[0].diagnostics[0].message, /bad map/u);
});

function reportFor(panel, blockSource) {
  const request = panel.messages.at(-1);
  return { type: "rendered", requestId: request.requestId, documentVersion: request.documentVersion,
    reports: [{ id: 0, bindings: [bindingFor(blockSource, "café")], diagnostics: [
      { message: "Recovered invalid syntax", severity: "warning", suggestion: "Check the arrow",
        span: { start: { line: 2 }, end: { line: 2 } } },
    ] }] };
}

test("extension publishes real reports to Problems and navigates only its bound document", async (t) => {
  const h = extensionHarness(); t.after(h.deactivate);
  const doc = documentFor("# readme\n  ```mermaid\n  flowchart LR\n  A[😀 café]-->B\n  ```");
  await h.open(doc); const panel = h.created[0]; await panel.receive({ type: "ready" });
  const report = reportFor(panel, panel.messages.at(-1).diagrams[0].source);
  await panel.receive(report);
  const problems = h.problems.get(doc.uri.toString());
  assert.equal(problems.length, 1); assert.equal(problems[0].severity, 1);
  assert.match(problems[0].message, /Suggestion: Check the arrow/u);
  assert.equal(problems[0].range.start.line, 3);
  const action = { type: "reveal", requestId: report.requestId, documentVersion: 3, diagramId: 0,
    elementId: "fm-node-a-0", uri: "file:///not-allowed.mmd" };
  await panel.receive(action);
  assert.equal(h.editors[0].document, doc);
  assert.deepEqual(h.editors[0].selection.start, { line: 3, character: 7 });
  assert.deepEqual(h.editors[0].selection.end, { line: 3, character: 11 });
  doc.version = 4;
  h.events.get("edit")({ document: doc, contentChanges: [{}] });
  assert.equal(h.problems.size, 0);
  await panel.receive(report); await panel.receive(action);
  assert.equal(h.problems.size, 0); assert.equal(h.editors.length, 1);
});

test("forged ranges disable only source navigation and emit a visible diagnostic", async (t) => {
  const h = extensionHarness(); t.after(h.deactivate);
  const doc = documentFor("flowchart LR\nA[café]", "x.mmd", "mermaid");
  await h.open(doc); const panel = h.created[0]; await panel.receive({ type: "ready" });
  const report = reportFor(panel, doc.getText());
  report.reports[0].bindings[0].textRange.endByte = 9999;
  await panel.receive(report);
  assert.match(h.problems.get(doc.uri.toString()).at(-1).message, /Source navigation disabled/u);
  await panel.receive({ type: "reveal", requestId: report.requestId, documentVersion: 3, diagramId: 0, elementId: "fm-node-a-0" });
  assert.equal(h.editors.length, 0);
});

test("navigation rechecks document revision after asynchronous editor opening", async (t) => {
  const h = extensionHarness(); t.after(h.deactivate);
  const doc = documentFor("A[café]", "x.mmd", "mermaid");
  await h.open(doc); const panel = h.created[0]; await panel.receive({ type: "ready" });
  const report = reportFor(panel, doc.getText()); await panel.receive(report);
  let resolve;
  h.vscode.window.showTextDocument = () => new Promise((done) => { resolve = done; });
  const navigating = panel.receive({ type: "reveal", requestId: report.requestId, documentVersion: 3, diagramId: 0, elementId: "fm-node-a-0" });
  const editor = { document: doc, revealRange: () => assert.fail("stale reveal") };
  doc.version += 1; resolve(editor); await navigating;
  assert.equal(editor.selection, undefined);
});
