"use strict";

const MERMAID_LANGUAGE_IDS = new Set(["mermaid", "mmd"]);
const DEFAULT_PREVIEW_DEBOUNCE_MS = 75;
const MAX_PREVIEW_DEBOUNCE_MS = 1000;
const MAX_PREVIEW_BYTES = 2 * 1024 * 1024;
const MAX_PREVIEW_DIAGRAMS = 64;

function isMermaidDocument(document) {
  return Boolean(document && (MERMAID_LANGUAGE_IDS.has(document.languageId)
    || /\.(?:mmd|mermaid)$/iu.test(document.fileName || "")));
}

function isMarkdownDocument(document) {
  return Boolean(document && (document.languageId === "markdown"
    || /\.(?:md|markdown)$/iu.test(document.fileName || "")));
}

function isPreviewDocument(document) {
  return isMermaidDocument(document) || isMarkdownDocument(document);
}

// A linear scanner for top-level fenced blocks, not a Mermaid parser. Track ALL fences so a
// Mermaid example inside a longer documentation/code fence is never treated as a live diagram.
// Follow CommonMark's fence character/length, info-string, indentation, and unclosed-EOF rules.
// Blockquote/list-container fences are deliberately outside this scanner's contract.
function extractMermaidBlocks(document) {
  const text = document.getText();
  if (Buffer.byteLength(text, "utf8") > MAX_PREVIEW_BYTES) {
    throw new Error("Preview source exceeds the 2 MiB limit.");
  }
  const lines = text.split(/\r\n|\n|\r/u);
  if (isMermaidDocument(document)) {
    return [{ id: 0, source: text, startLine: 0,
      lineMap: lines.map((line, index) => ({ line: index, indent: 0, text: line })) }];
  }
  if (!isMarkdownDocument(document)) return [];
  const blocks = [];
  let fence;
  function finish() {
    if (fence.mermaid) {
      if (blocks.length >= MAX_PREVIEW_DIAGRAMS) {
        throw new Error(`Preview supports at most ${MAX_PREVIEW_DIAGRAMS} diagrams per document.`);
      }
      blocks.push({ id: blocks.length, source: fence.body.join("\n"),
        startLine: fence.startLine, lineMap: fence.lineMap });
    }
    fence = undefined;
  }
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (!fence) {
      const open = /^( {0,3})(`{3,}|~{3,})(.*)$/u.exec(line);
      if (!open || (open[2][0] === "`" && open[3].includes("`"))) continue;
      fence = { marker: open[2][0], length: open[2].length, indent: open[1].length,
        mermaid: /^mermaid(?:[ \t]|$)/iu.test(open[3].trim()),
        startLine: index + 1, body: [], lineMap: [] };
      continue;
    }
    const close = /^ {0,3}(`{3,}|~{3,})[ \t]*$/u.exec(line);
    if (close && close[1][0] === fence.marker && close[1].length >= fence.length) {
      finish();
      continue;
    }
    if (fence.mermaid) {
      let indent = 0;
      while (indent < fence.indent && line[indent] === " ") indent += 1;
      const content = line.slice(indent);
      fence.body.push(content);
      fence.lineMap.push({ line: index, indent, text: content });
    }
  }
  if (fence) finish();
  return blocks;
}

function createRenderSnapshot(document, requestId, title) {
  const blocks = extractMermaidBlocks(document);
  return {
    blocks,
    message: {
      type: "render", requestId, documentVersion: document.version, title,
      diagrams: blocks.map(({ id, source, startLine }) => ({ id, source, startLine })),
    },
  };
}

// Resolve only requested UTF-8 boundaries, not a map entry for every byte/character of a large
// document. Invalid boundaries (including the middle of an emoji) are absent from the result.
function utf8Boundaries(source, requested) {
  const offsets = new Map();
  let bytes = 0;
  let units = 0;
  for (const character of source) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) {
      throw new Error("Source contains an unpaired surrogate; repair it before source navigation.");
    }
    if (requested.has(bytes)) offsets.set(bytes, units);
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    units += character.length;
  }
  if (requested.has(bytes)) offsets.set(bytes, units);
  return offsets;
}

function lineStarts(source) {
  const starts = [0];
  for (const match of source.matchAll(/\r\n|\n|\r/gu)) starts.push(match.index + match[0].length);
  return starts;
}

function documentPosition(block, starts, offset) {
  let low = 0;
  let high = starts.length;
  while (low + 1 < high) {
    const middle = Math.floor((low + high) / 2);
    if (starts[middle] <= offset) low = middle;
    else high = middle;
  }
  const mapped = block.lineMap[low];
  if (!mapped) return undefined;
  const character = offset - starts[low];
  // A CRLF is one document line break, never two selectable characters beyond the line end.
  if (character > mapped.text.length) return undefined;
  return { line: mapped.line, character: mapped.indent + character };
}

function blockRange(block) {
  const first = block.lineMap[0];
  const last = block.lineMap.at(-1);
  return {
    start: { line: first?.line ?? Math.max(0, block.startLine - 1), character: first?.indent ?? 0 },
    end: { line: last?.line ?? Math.max(0, block.startLine - 1), character: last ? last.indent + last.text.length : 0 },
  };
}

function checkedSourceBindings(block, bindings) {
  if (!Array.isArray(bindings) || bindings.length > 20000) throw new Error("Invalid source-binding list.");
  const requested = new Set();
  for (const binding of bindings) {
    if (!binding || typeof binding !== "object") throw new Error("Invalid source binding.");
    if (binding.textRange == null) continue;
    const { startByte, endByte } = binding.textRange;
    if (!Number.isSafeInteger(startByte) || !Number.isSafeInteger(endByte)
      || startByte < 0 || endByte < startByte) throw new Error("Invalid source-binding byte range.");
    requested.add(startByte); requested.add(endByte);
  }
  const offsets = utf8Boundaries(block.source, requested);
  const starts = lineStarts(block.source);
  const checked = new Map();
  for (const binding of bindings) {
    if (binding.textRange == null) continue; // Synthesized elements are not source-editable.
    const start = offsets.get(binding.textRange.startByte);
    const end = offsets.get(binding.textRange.endByte);
    if (typeof binding.elementId !== "string" || !binding.elementId || binding.elementId.length > 4096
      || checked.has(binding.elementId) || start === undefined || end === undefined) {
      throw new Error("Source map has duplicate IDs or invalid UTF-8 boundaries.");
    }
    const snippet = block.source.slice(start, end);
    if (binding.snippet != null && binding.snippet !== snippet) throw new Error("Source map does not match the rendered source.");
    const range = { start: documentPosition(block, starts, start), end: documentPosition(block, starts, end) };
    if (!range.start || !range.end) throw new Error("Source map does not resolve to document positions.");
    checked.set(binding.elementId, { elementId: binding.elementId, range, snippet,
      startByte: binding.textRange.startByte, endByte: binding.textRange.endByte });
  }
  return checked;
}

// Diagnostics select the engine-reported lines. Do not reinterpret parser-specific column units
// as VS Code's UTF-16 columns; exact element selections use the checked byte ranges above.
function diagnosticRange(block, span) {
  const first = span?.start?.line;
  const last = span?.end?.line;
  if (!Number.isSafeInteger(first) || first < 1 || first > block.lineMap.length) return blockRange(block);
  const end = Number.isSafeInteger(last) && last >= first && last <= block.lineMap.length ? last : first;
  const startLine = block.lineMap[first - 1];
  const endLine = block.lineMap[end - 1];
  return { start: { line: startLine.line, character: startLine.indent },
    end: { line: endLine.line, character: endLine.indent + endLine.text.length } };
}

function checkedDiagnostics(block, diagnostics) {
  if (!Array.isArray(diagnostics) || diagnostics.length > 512) throw new Error("Invalid diagnostic list.");
  return diagnostics.map((diagnostic) => {
    if (!diagnostic || typeof diagnostic.message !== "string" || diagnostic.message.length > 16000) {
      throw new Error("Invalid diagnostic message.");
    }
    const severity = ["error", "warning", "info", "hint"].includes(diagnostic.severity) ? diagnostic.severity : "warning";
    const suggestion = typeof diagnostic.suggestion === "string" ? diagnostic.suggestion.slice(0, 16000) : "";
    return { message: diagnostic.message, severity, suggestion, range: diagnosticRange(block, diagnostic.span) };
  });
}

function isCurrentSnapshot(entry, message) {
  return Boolean(message && !entry.disposed && !entry.document.isClosed && entry.snapshot
    && message.requestId === entry.snapshot.message.requestId
    && message.documentVersion === entry.snapshot.message.documentVersion
    && entry.document.version === message.documentVersion);
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
    this.disposed = false;
    this.setDelayMs(delayMs);
  }

  setDelayMs(delayMs) {
    this.delayMs = normalizePreviewDebounceMs(delayMs);
  }

  schedule(render) {
    if (this.disposed) return;
    if (this.timer !== undefined) this.clearTimer(this.timer);
    this.timer = this.setTimer(() => {
      this.timer = undefined;
      if (!this.disposed) render();
    }, this.delayMs);
  }

  dispose() {
    this.disposed = true;
    if (this.timer !== undefined) {
      this.clearTimer(this.timer);
      this.timer = undefined;
    }
  }
}

function escapeAttribute(value) {
  return String(value).replace(/&/gu, "&amp;").replace(/"/gu, "&quot;")
    .replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
}

function buildPreviewHtml({ cspSource, nonce, scriptUri, wasmModuleUri, wasmBinaryUri }) {
  const safeNonce = escapeAttribute(nonce);
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data: blob: ${escapeAttribute(cspSource)}; connect-src ${escapeAttribute(cspSource)}; script-src 'nonce-${safeNonce}' ${escapeAttribute(cspSource)} 'wasm-unsafe-eval'; style-src 'nonce-${safeNonce}'; style-src-attr 'unsafe-inline';">
  <title>FrankenMermaid Preview</title>
  <style nonce="${safeNonce}">
    body { padding: 1rem; color: var(--vscode-editor-foreground); background: var(--vscode-editor-background); font-family: var(--vscode-font-family); }
    section { margin-bottom: 1.5rem; border-bottom: 1px solid var(--vscode-panel-border); padding-bottom: 1rem; }
    h2 { font-size: 1rem; overflow-wrap: anywhere; }
    button { color: var(--vscode-button-foreground); background: var(--vscode-button-background); border: 0; padding: .4rem .7rem; margin: .25rem .5rem .25rem 0; cursor: pointer; }
    button:focus-visible { outline: 2px solid var(--vscode-focusBorder); outline-offset: 2px; }
    .error { color: var(--vscode-errorForeground); white-space: pre-wrap; overflow-wrap: anywhere; }
    #status { margin-bottom: 1rem; }
  </style>
</head>
<body data-wasm-module="${escapeAttribute(wasmModuleUri)}" data-wasm-binary="${escapeAttribute(wasmBinaryUri)}" data-style-nonce="${safeNonce}">
  <div id="status" role="status" aria-live="polite">Loading FrankenMermaid…</div>
  <main id="preview" aria-label="Mermaid diagrams"></main>
  <script nonce="${safeNonce}" type="module" src="${escapeAttribute(scriptUri)}"></script>
</body>
</html>`;
}

module.exports = {
  buildPreviewHtml, createRenderSnapshot, DebouncedRenderScheduler, extractMermaidBlocks,
  isMermaidDocument, isMarkdownDocument, isPreviewDocument, normalizePreviewDebounceMs,
  MAX_PREVIEW_BYTES, MAX_PREVIEW_DIAGRAMS,
  blockRange, checkedSourceBindings, checkedDiagnostics, isCurrentSnapshot,
};
