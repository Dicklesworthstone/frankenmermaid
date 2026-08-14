'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const { buildPreviewHtml, isMermaidDocument } = require('../preview-contract.cjs');

test('recognizes Mermaid documents by language or .mmd extension', () => {
  assert.equal(isMermaidDocument({ languageId: 'mermaid', fileName: '/work/diagram.txt' }), true);
  assert.equal(isMermaidDocument({ languageId: 'plaintext', fileName: '/work/diagram.mmd' }), true);
  assert.equal(isMermaidDocument({ languageId: 'plaintext', fileName: '/work/readme.md' }), false);
});

test('webview HTML loads only nonce-authorized local resources', () => {
  const html = buildPreviewHtml({
    cspSource: 'vscode-webview-resource:',
    nonce: 'test-nonce',
    scriptUri: 'vscode-webview-resource:/media/preview.js',
    wasmModuleUri: 'vscode-webview-resource:/pkg/frankenmermaid.js',
    wasmBinaryUri: 'vscode-webview-resource:/pkg/frankenmermaid_bg.wasm',
  });

  assert.match(html, /default-src 'none'/u);
  assert.match(html, /script-src 'nonce-test-nonce' vscode-webview-resource:/u);
  assert.match(html, /data-wasm-module="vscode-webview-resource:\/pkg\/frankenmermaid\.js"/u);
  assert.doesNotMatch(html, /unsafe-inline/u);
});
