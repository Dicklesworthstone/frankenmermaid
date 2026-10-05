const crypto = require("node:crypto");
const vscode = require("vscode");
const {
  buildPreviewHtml, createRenderSnapshot, DebouncedRenderScheduler,
  isPreviewDocument, normalizePreviewDebounceMs,
  blockRange, checkedSourceBindings, checkedDiagnostics, isCurrentSnapshot,
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
      wasmModuleUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(packageRoot, "frankenmermaid.js")),
      wasmBinaryUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(packageRoot, "frankenmermaid_bg.wasm")),
    }),
  };
}

function postRender(entry) {
  if (!entry.ready || entry.disposed || entry.document.isClosed) return;
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

async function receiveMessage(entry, message) {
  if (entry.disposed) return;
  if (message?.type === "ready") { entry.ready = true; postRender(entry); }
  else if (message?.type === "rendered") acceptRenderReport(entry, message);
  else if (message?.type === "reveal") await revealSource(entry, message);
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
        entry.snapshot = undefined;
        entry.reports = undefined;
        diagnostics.delete(entry.document.uri);
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
  );
}

function deactivate() {
  for (const entry of panels.values()) entry.panel.dispose();
  panels.clear();
  diagnostics?.dispose();
  diagnostics = undefined;
}

module.exports = { activate, deactivate };
