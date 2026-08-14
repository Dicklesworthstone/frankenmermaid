'use strict';

const MERMAID_LANGUAGE_IDS = new Set(['mermaid', 'mmd']);
const DEFAULT_PREVIEW_DEBOUNCE_MS = 75;
const MAX_PREVIEW_DEBOUNCE_MS = 1000;

function isMermaidDocument(document) {
  return MERMAID_LANGUAGE_IDS.has(document.languageId) || /\.mmd$/iu.test(document.fileName);
}

function normalizePreviewDebounceMs(value) {
  if (!Number.isInteger(value) || value < 0 || value > MAX_PREVIEW_DEBOUNCE_MS) {
    return DEFAULT_PREVIEW_DEBOUNCE_MS;
  }
  return value;
}

class DebouncedRenderScheduler {
  constructor(delayMs, setTimer = setTimeout, clearTimer = clearTimeout) {
    this.setTimer = setTimer;
    this.clearTimer = clearTimer;
    this.timer = undefined;
    this.setDelayMs(delayMs);
  }

  setDelayMs(delayMs) {
    this.delayMs = normalizePreviewDebounceMs(delayMs);
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

module.exports = {
  buildPreviewHtml,
  DebouncedRenderScheduler,
  isMermaidDocument,
  normalizePreviewDebounceMs,
};
