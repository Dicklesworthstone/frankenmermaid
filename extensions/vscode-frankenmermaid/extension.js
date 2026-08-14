'use strict';

const crypto = require('node:crypto');
const vscode = require('vscode');
const { buildPreviewHtml, DebouncedRenderScheduler, isMermaidDocument } = require('./preview-contract.cjs');

const panels = new Map();
const DEFAULT_PREVIEW_DEBOUNCE_MS = 75;

function previewResources(context, panel) {
  const packageRoot = vscode.Uri.joinPath(
    context.extensionUri,
    'node_modules',
    '@frankenmermaid',
    'core',
  );
  const mediaRoot = vscode.Uri.joinPath(context.extensionUri, 'media');
  return {
    localResourceRoots: [packageRoot, mediaRoot],
    html: buildPreviewHtml({
      cspSource: panel.webview.cspSource,
      nonce: crypto.randomBytes(16).toString('base64'),
      scriptUri: panel.webview.asWebviewUri(vscode.Uri.joinPath(mediaRoot, 'preview.js')),
      wasmModuleUri: panel.webview.asWebviewUri(
        vscode.Uri.joinPath(packageRoot, 'frankenmermaid.js'),
      ),
      wasmBinaryUri: panel.webview.asWebviewUri(
        vscode.Uri.joinPath(packageRoot, 'frankenmermaid_bg.wasm'),
      ),
    }),
  };
}

function postRender(entry) {
  if (!entry.ready) {
    return;
  }
  void entry.panel.webview.postMessage({
    type: 'render',
    source: entry.document.getText(),
    title: vscode.workspace.asRelativePath(entry.document.uri, false),
  });
}

function scheduleRender(entry) {
  entry.scheduler.schedule(() => postRender(entry));
}

function previewDebounceMs() {
  return vscode.workspace
    .getConfiguration('frankenmermaid')
    .get('previewDebounceMs', DEFAULT_PREVIEW_DEBOUNCE_MS);
}

function showPreview(context, document) {
  if (!isMermaidDocument(document)) {
    void vscode.window.showWarningMessage('FrankenMermaid previews .mmd and Mermaid-language documents.');
    return;
  }

  const key = document.uri.toString();
  let entry = panels.get(key);
  if (!entry) {
    const panel = vscode.window.createWebviewPanel(
      'frankenmermaid.preview',
      `FrankenMermaid: ${document.fileName.split(/[\\/]/u).pop()}`,
      vscode.ViewColumn.Beside,
      { enableScripts: true },
    );
    const resources = previewResources(context, panel);
    panel.webview.options = {
      enableScripts: true,
      localResourceRoots: resources.localResourceRoots,
    };
    panel.webview.html = resources.html;
    entry = {
      document,
      panel,
      ready: false,
      scheduler: new DebouncedRenderScheduler(previewDebounceMs()),
    };
    panel.onDidDispose(() => {
      entry.scheduler.dispose();
      panels.delete(key);
    }, undefined, context.subscriptions);
    panel.webview.onDidReceiveMessage((message) => {
      if (message?.type === 'ready') {
        entry.ready = true;
        postRender(entry);
      }
    }, undefined, context.subscriptions);
    panels.set(key, entry);
  } else {
    entry.document = document;
  }

  entry.panel.reveal(vscode.ViewColumn.Beside, true);
  postRender(entry);
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('frankenmermaid.showPreview', () => {
      const editor = vscode.window.activeTextEditor;
      if (editor) {
        showPreview(context, editor.document);
      }
    }),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.contentChanges.length === 0) {
        return;
      }
      const entry = panels.get(event.document.uri.toString());
      if (entry) {
        entry.document = event.document;
        scheduleRender(entry);
      }
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      panels.get(document.uri.toString())?.panel.dispose();
    }),
  );
}

function deactivate() {
  for (const entry of panels.values()) {
    entry.panel.dispose();
  }
  panels.clear();
}

module.exports = { activate, deactivate };
