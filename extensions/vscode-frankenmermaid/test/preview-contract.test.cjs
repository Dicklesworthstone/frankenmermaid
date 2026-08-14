'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const {
  buildPreviewHtml,
  DebouncedRenderScheduler,
  isMermaidDocument,
} = require('../preview-contract.cjs');

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

test('render scheduler keeps only the latest edit and cancels disposal work', () => {
  let nextTimer = 0;
  const timers = new Map();
  const cleared = [];
  const scheduler = new DebouncedRenderScheduler(
    75,
    (callback, delayMs) => {
      const timer = ++nextTimer;
      timers.set(timer, { callback, delayMs });
      return timer;
    },
    (timer) => cleared.push(timer),
  );
  const renders = [];

  scheduler.schedule(() => renders.push('stale'));
  scheduler.schedule(() => renders.push('latest'));

  assert.deepEqual(cleared, [1]);
  assert.equal(timers.get(2).delayMs, 75);
  timers.get(2).callback();
  assert.deepEqual(renders, ['latest']);

  scheduler.schedule(() => renders.push('disposed'));
  scheduler.dispose();
  assert.deepEqual(cleared, [1, 3]);
});
