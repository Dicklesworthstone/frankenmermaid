const crypto = require("node:crypto");
const vscode = require("vscode");
const {
  buildPreviewHtml, createRenderSnapshot, DebouncedRenderScheduler,
  isPreviewDocument, normalizePreviewDebounceMs,
} = require("./preview-contract.cjs");

const panels = new Map();

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
    scheduler: new DebouncedRenderScheduler(previewDebounceMs()) };
  panels.set(key, entry);
  panel.onDidDispose(() => {
    entry.disposed = true;
    entry.scheduler.dispose();
    panels.delete(key);
  }, undefined, context.subscriptions);
  // Register before assigning HTML: a fast cached WASM init must not race past our ready listener.
  panel.webview.onDidReceiveMessage((message) => {
    if (message?.type === "ready" && !entry.disposed) {
      entry.ready = true;
      postRender(entry);
    }
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
}

module.exports = { activate, deactivate };
