'use strict';

const MERMAID_LANGUAGE_IDS = new Set(['mermaid', 'mmd']);

function isMermaidDocument(document) {
  return MERMAID_LANGUAGE_IDS.has(document.languageId) || /\.mmd$/iu.test(document.fileName);
}

class DebouncedRenderScheduler {
  constructor(delayMs, setTimer = setTimeout, clearTimer = clearTimeout) {
    this.delayMs = delayMs;
    this.setTimer = setTimer;
    this.clearTimer = clearTimer;
    this.timer = undefined;
  }

  schedule(render) {
    if (this.timer !== undefined) {
      this.clearTimer(this.timer);
    }
    this.timer = this.setTimer(() => {
      this.timer = undefined;
      render();
    }, this.delayMs);
  }

  dispose() {
    if (this.timer !== undefined) {
      this.clearTimer(this.timer);
      this.timer = undefined;
    }
  }
}

function buildPreviewHtml({ cspSource, nonce, scriptUri, wasmModuleUri, wasmBinaryUri }) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src blob:; connect-src ${cspSource}; script-src 'nonce-${nonce}' ${cspSource};">
  <title>FrankenMermaid Preview</title>
</head>
<body data-wasm-module="${wasmModuleUri}" data-wasm-binary="${wasmBinaryUri}">
  <main id="preview" aria-live="polite">Loading FrankenMermaid…</main>
  <script nonce="${nonce}" type="module" src="${scriptUri}"></script>
</body>
</html>`;
}

module.exports = { buildPreviewHtml, DebouncedRenderScheduler, isMermaidDocument };
