const crypto = require("node:crypto");
const vscode = require("vscode");
const {
  buildPreviewHtml, createRenderSnapshot, DebouncedRenderScheduler,
  isPreviewDocument, normalizePreviewDebounceMs,
  blockRange, checkedSourceBindings, checkedDiagnostics, isCurrentSnapshot,
  sourceSelectionTargets, planSourceEdit, planSourceBatch,
} = require("./preview-contract.cjs");

const panels = new Map();
let diagnostics;

async function previewResources(context, panel) {
  const candidates = [
    vscode.Uri.joinPath(context.extensionUri, "node_modules", "@frankenmermaid", "core"),
    vscode.Uri.joinPath(context.extensionUri, "pkg"),
  ];
  // A checkout can use the repository's exact tracked engine without depending on npm publication.
  // Never load executable code from the user's workspace or a document-supplied path.
  if (/\/extensions\/vscode-frankenmermaid\/?$/u.test(context.extensionUri.path)) {
    candidates.push(vscode.Uri.joinPath(context.extensionUri, "..", "..", "pkg"));
  }
  let packageRoot;
  for (const candidate of candidates) {
    try {
      await vscode.workspace.fs.stat(vscode.Uri.joinPath(candidate, "frankenmermaid.js"));
      await vscode.workspace.fs.stat(vscode.Uri.joinPath(candidate, "frankenmermaid_bg.wasm"));
      packageRoot = candidate;
      break;
    } catch { /* Try the next extension-owned package location. */ }
  }
  if (!packageRoot) throw new Error("FrankenMermaid engine assets are missing. Install @frankenmermaid/core, bundle pkg/, or use the repository checkout with its built pkg/.");
  const mediaRoot = vscode.Uri.joinPath(context.extensionUri, "media");
  return {
    localResourceRoots: [packageRoot, mediaRoot],
    html: buildPreviewHtml({
      cspSource: panel.webview.cspSource,
      nonce: crypto.randomBytes(16).toString("base64"),
      scriptUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(mediaRoot, "preview.js")),
      workerUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(mediaRoot, "engine-worker.js")),
      wasmModuleUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(packageRoot, "frankenmermaid.js")),
      wasmBinaryUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(packageRoot, "frankenmermaid_bg.wasm")),
    }),
  };
}

function postRender(entry) {
  if (!entry.ready || entry.disposed || entry.document.isClosed) return;
  entry.scheduler.cancel();
  const requestId = ++entry.requestId;
  entry.reports = undefined;
  diagnostics?.delete(entry.document.uri);
  try {
    entry.snapshot = createRenderSnapshot(entry.document, requestId,
      vscode.workspace.asRelativePath(entry.document.uri, false));
    void entry.panel.webview.postMessage(entry.snapshot.message);
  } catch (error) {
    entry.snapshot = undefined;
    void entry.panel.webview.postMessage({ type: "preview-error", requestId,
      message: error instanceof Error ? error.message : String(error) });
  }
}

function invalidateRender(entry) {
  entry.snapshot = undefined;
  entry.reports = undefined;
  diagnostics?.delete(entry.document.uri);
  // Source changes must interrupt executing WASM now, not after the render debounce.
  // A fresh host-owned sequence token also fences late renders and queued UI actions.
  if (entry.ready && !entry.disposed && !entry.document.isClosed) {
    void entry.panel.webview.postMessage({ type: "invalidate", requestId: ++entry.requestId,
      documentVersion: entry.document.version });
  }
}

function vscodeRange(range) {
  return new vscode.Range(range.start.line, range.start.character, range.end.line, range.end.character);
}

function acceptRenderReport(entry, message) {
  if (!isCurrentSnapshot(entry, message) || !Array.isArray(message.reports)
    || message.reports.length !== entry.snapshot.blocks.length) return;
  const checked = new Map();
  const problems = [];
  for (const [index, report] of message.reports.entries()) {
    if (report?.id !== index) throw new Error("Preview returned an invalid diagram identity.");
    const block = entry.snapshot.blocks[index];
    let bindings;
    const issues = checkedDiagnostics(block, report.diagnostics);
    try { bindings = checkedSourceBindings(block, report.bindings); }
    catch (error) {
      // Keep the SVG and real diagnostics; make a broken map non-actionable and visible.
      bindings = new Map();
      issues.push({ message: `Source navigation disabled: ${error.message}`, severity: "warning", range: blockRange(block) });
    }
    checked.set(index, { bindings, diagnostics: issues });
    for (const issue of issues) {
      const severity = { error: vscode.DiagnosticSeverity.Error, warning: vscode.DiagnosticSeverity.Warning,
        info: vscode.DiagnosticSeverity.Information, hint: vscode.DiagnosticSeverity.Hint }[issue.severity];
      const problem = new vscode.Diagnostic(vscodeRange(issue.range),
        issue.message + (issue.suggestion ? `\nSuggestion: ${issue.suggestion}` : ""), severity);
      problem.source = "FrankenMermaid";
      problems.push(problem);
    }
  }
  entry.reports = checked;
  diagnostics.set(entry.document.uri, problems);
  publishSourceSelection(entry, vscode.window.activeTextEditor);
}

function publishSourceSelection(entry, editor) {
  if (!editor || editor.document.uri.toString() !== entry.document.uri.toString()
    || !entry.reports || !isCurrentSnapshot(entry, entry.snapshot?.message)) return;
  void entry.panel.webview.postMessage({ type: "select-source",
    requestId: entry.snapshot.message.requestId, documentVersion: entry.document.version,
    targets: sourceSelectionTargets(entry.reports, editor.selection) });
}

async function exportSvg(entry, message) {
  if (!isCurrentSnapshot(entry, message) || !Number.isSafeInteger(message.diagramId)
    || !entry.reports?.has(message.diagramId) || !["save", "copy"].includes(message.action)) return;
  const svg = message.svg;
  if (typeof svg !== "string" || Buffer.byteLength(svg, "utf8") > 16 * 1024 * 1024
    || !/^\s*(?:<\?xml[^>]*\?>\s*)?<svg(?:\s|>)/u.test(svg)) {
    throw new Error("The preview did not return a valid bounded SVG export.");
  }
  if (message.action === "copy") {
    await vscode.env.clipboard.writeText(svg);
  } else {
    if (entry.exporting) return;
    entry.exporting = true;
    try {
      const sourceUri = entry.document.uri;
      const suffix = entry.snapshot.blocks.length > 1 ? `-diagram-${message.diagramId + 1}` : "";
      const defaultUri = sourceUri.scheme === "untitled" ? undefined
        : sourceUri.with({ path: sourceUri.path.replace(/\.[^./]+$/u, "") + suffix + ".svg", query: "", fragment: "" });
      const destination = await vscode.window.showSaveDialog({ defaultUri,
        filters: { "SVG image": ["svg"] }, saveLabel: "Save SVG" });
      if (!destination) return;
      // A delayed save dialog cannot authorize exporting an obsolete document snapshot.
      if (!isCurrentSnapshot(entry, message)) throw new Error("Source changed while choosing a destination. Render again before exporting.");
      if (!/\.svg$/iu.test(destination.path)
        || destination.toString().toLowerCase() === sourceUri.toString().toLowerCase()) {
        throw new Error("Choose a separate .svg file; export cannot overwrite the source document.");
      }
      // Only the native save dialog chooses the destination. Never use a URI from webview data.
      await vscode.workspace.fs.writeFile(destination, Buffer.from(svg, "utf8"));
    } finally { entry.exporting = false; }
  }
  if (isCurrentSnapshot(entry, message)) void entry.panel.webview.postMessage({ type: "export-complete",
    requestId: message.requestId, documentVersion: message.documentVersion, action: message.action });
}

async function revealSource(entry, message) {
  if (!isCurrentSnapshot(entry, message) || !Number.isSafeInteger(message.diagramId)) return;
  const block = entry.snapshot.blocks[message.diagramId];
  const report = entry.reports?.get(message.diagramId);
  if (!block || !report) return;
  let range;
  if (typeof message.elementId === "string") range = report.bindings.get(message.elementId)?.range;
  else if (Number.isSafeInteger(message.diagnosticIndex)) range = report.diagnostics[message.diagnosticIndex]?.range;
  else if (message.elementId === undefined && message.diagnosticIndex === undefined) range = blockRange(block);
  if (!range) return;
  // The bound document is the ONLY navigation target. Never accept a URI/path from webview data.
  const editor = await vscode.window.showTextDocument(entry.document,
    { viewColumn: entry.sourceColumn, preserveFocus: false, preview: false });
  if (!isCurrentSnapshot(entry, message) || editor.document.uri.toString() !== entry.document.uri.toString()) return;
  const target = vscodeRange(range);
  editor.selection = new vscode.Selection(target.start, target.end);
  editor.revealRange(target, vscode.TextEditorRevealType.InCenterIfOutsideViewport);
}

async function applySourceEdit(entry, message) {
  if (!Number.isSafeInteger(message.editId) || message.editId < 1) return;
  const reply = (ok, text) => {
    if (!entry.disposed) void entry.panel.webview.postMessage({ type: "source-edit-result",
      requestId: message.requestId, documentVersion: message.documentVersion,
      editId: message.editId, ok, message: text });
  };
  if (!isCurrentSnapshot(entry, message)) {
    reply(false, "Source changed. Your draft was not applied; select a current element.");
    return;
  }
  if (entry.editing) { reply(false, "Another source edit is being applied."); return; }
  entry.editing = true;
  try {
    const snapshot = entry.snapshot;
    const plan = message.type === "apply-source-batch"
      ? planSourceBatch(entry.document, snapshot, entry.reports, message)
      : planSourceEdit(entry.document, snapshot, entry.reports, message);
    if (!plan.changed) { reply(true, "Source is unchanged."); return; }
    const editor = await vscode.window.showTextDocument(entry.document,
      { viewColumn: entry.sourceColumn, preserveFocus: true, preview: false });
    // Opening an editor is asynchronous. Recheck at the actual commit boundary, then let
    // TextEditor.edit's versioned transaction reject any edit racing inside the VS Code host.
    if (entry.snapshot !== snapshot || !isCurrentSnapshot(entry, message)
      || entry.document.getText() !== plan.before
      || editor.document.uri.toString() !== entry.document.uri.toString()) {
      throw new Error("Source changed while opening the editor. Your draft was not applied.");
    }
    const edits = plan.edits || [{ range: plan.range, newText: plan.newText }];
    const applied = await editor.edit((builder) => {
      for (const edit of edits) builder.replace(vscodeRange(edit.range), edit.newText);
    }, { undoStopBefore: true, undoStopAfter: true });
    if (!applied) throw new Error("VS Code refused the edit (read-only or changed document). Your draft was kept.");
    reply(true, "Source updated. Use the editor's Undo to restore it.");
    // Source-change events normally invalidate first. Do so here as well before another queued
    // webview message can reuse this receipt, including hosts which deliver that event later.
    if (entry.snapshot) invalidateRender(entry);
    entry.scheduler.schedule(() => postRender(entry));
  } catch (error) {
    reply(false, error instanceof Error ? error.message : String(error));
  } finally { entry.editing = false; }
}

async function receiveMessage(entry, message) {
  if (entry.disposed) return;
  if (message?.type === "ready") { entry.ready = true; postRender(entry); }
  else if (message?.type === "retry-render" && entry.ready && !entry.editing
    && Number.isSafeInteger(message.requestId) && message.requestId === entry.requestId) {
    // Re-read the bound document; never replay source text or accept a path from the webview.
    postRender(entry);
  }
  else if (message?.type === "rendered") acceptRenderReport(entry, message);
  else if (message?.type === "reveal") await revealSource(entry, message);
  else if (message?.type === "export-svg") await exportSvg(entry, message);
  else if (message?.type === "apply-source-edit" || message?.type === "apply-source-batch") await applySourceEdit(entry, message);
}

function previewDebounceMs() {
  return normalizePreviewDebounceMs(
    vscode.workspace.getConfiguration("frankenmermaid").get("previewDebounceMs"),
  );
}

async function showPreview(context, document) {
  if (!isPreviewDocument(document)) {
    void vscode.window.showWarningMessage("FrankenMermaid previews Mermaid files and fenced Mermaid diagrams in Markdown.");
    return;
  }
  const key = document.uri.toString();
  let entry = panels.get(key);
  if (entry) {
    entry.document = document;
    entry.panel.reveal(vscode.ViewColumn.Beside, true);
    postRender(entry);
    return;
  }
  const panel = vscode.window.createWebviewPanel("frankenmermaid.preview",
    `FrankenMermaid: ${document.fileName.split(/[\\/]/u).pop()}`, vscode.ViewColumn.Beside,
    { enableScripts: true });
  entry = { document, panel, ready: false, disposed: false, requestId: 0,
    sourceColumn: vscode.window.activeTextEditor?.viewColumn || vscode.ViewColumn.One,
    scheduler: new DebouncedRenderScheduler(previewDebounceMs()) };
  panels.set(key, entry);
  panel.onDidDispose(() => {
    entry.disposed = true;
    entry.scheduler.dispose();
    diagnostics?.delete(entry.document.uri);
    panels.delete(key);
  }, undefined, context.subscriptions);
  // Register before assigning HTML: a fast cached WASM init must not race past our ready listener.
  panel.webview.onDidReceiveMessage((message) => {
    return receiveMessage(entry, message).catch((error) => {
      if (!entry.disposed) void vscode.window.showErrorMessage(`FrankenMermaid: ${error.message}`);
    });
  }, undefined, context.subscriptions);
  try {
    const resources = await previewResources(context, panel);
    if (entry.disposed) return;
    panel.webview.options = { enableScripts: true, localResourceRoots: resources.localResourceRoots };
    panel.webview.html = resources.html;
  } catch (error) {
    if (!entry.disposed) {
      panel.dispose();
      void vscode.window.showErrorMessage(error instanceof Error ? error.message : String(error));
    }
  }
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection("frankenmermaid");
  context.subscriptions.push(diagnostics);
  context.subscriptions.push(
    vscode.commands.registerCommand("frankenmermaid.showPreview", () => {
      const editor = vscode.window.activeTextEditor;
      if (editor) return showPreview(context, editor.document);
    }),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.contentChanges.length === 0) return;
      const entry = panels.get(event.document.uri.toString());
      if (entry) {
        entry.document = event.document;
        // Invalidate source-bound actions immediately, not after the debounce expires.
        invalidateRender(entry);
        entry.scheduler.schedule(() => postRender(entry));
      }
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("frankenmermaid.previewDebounceMs")) {
        const delayMs = previewDebounceMs();
        for (const entry of panels.values()) entry.scheduler.setDelayMs(delayMs);
      }
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      panels.get(document.uri.toString())?.panel.dispose();
    }),
    vscode.window.onDidChangeTextEditorSelection((event) => {
      const entry = panels.get(event.textEditor.document.uri.toString());
      if (entry) publishSourceSelection(entry, event.textEditor);
    }),
  );
}

function deactivate() {
  for (const entry of panels.values()) entry.panel.dispose();
  panels.clear();
  diagnostics?.dispose();
  diagnostics = undefined;
}

module.exports = { activate, deactivate };
