'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const { pathToFileURL } = require('node:url');
const vm = require('node:vm');

function loadWorker(diagram) {
  const workerPath = path.join(__dirname, 'fm-render.worker.js');
  const source = fs.readFileSync(workerPath, 'utf8').replace(
    'let wasm = null;',
    'let wasm = globalThis.__wasm;',
  );
  const timers = [];
  const messages = [];
  const context = {
    JSON,
    Number,
    Promise,
    setTimeout: (callback) => {
      timers.push(callback);
      return timers.length;
    },
    self: {
      postMessage: (message) => messages.push(message),
    },
    __wasm: {
      chooseCanvasTarget: () => JSON.stringify({ target: 'offscreenInWorker' }),
      Diagram: {
        fromOffscreenCanvas: () => diagram,
      },
    },
  };
  context.globalThis = context;
  vm.runInNewContext(source, context, { filename: workerPath });
  return { messages, onMessage: context.self.onmessage, timers };
}

async function flushMicrotasks() {
  await Promise.resolve();
  await Promise.resolve();
}

async function initializeOffscreenWorker(worker) {
  await worker.onMessage({
    data: {
      kind: 'init',
      capabilities: { canvasTransferred: true, offscreenCanvas: true, worker: true },
      canvas: {},
      moduleUrl: 'unused-when-wasm-is-ready',
    },
  });
}

function plain(value) {
  return JSON.parse(JSON.stringify(value));
}

test('playground resolves the worker module URL to the shipped root package', () => {
  const playgroundPath = path.join(__dirname, 'playground.html');
  const source = fs.readFileSync(playgroundPath, 'utf8');
  const expectedPackage = path.join(__dirname, '..', 'pkg', 'frankenmermaid.js');

  assert.match(source, /new URL\("\.\.\/pkg\/frankenmermaid\.js", import\.meta\.url\)/);
  assert.equal(fs.existsSync(expectedPackage), true, 'the playground must target a shipped package');

  const resolved = new URL('../pkg/frankenmermaid.js', pathToFileURL(playgroundPath));
  assert.equal(path.normalize(resolved.pathname), expectedPackage);
});

test('offscreen worker skips a queued render superseded before synchronous canvas drawing', async () => {
  const inputs = [];
  const worker = loadWorker({
    render: (input) => {
      inputs.push(input);
      return { rendered: input };
    },
  });
  await initializeOffscreenWorker(worker);

  const first = worker.onMessage({ data: { kind: 'render', requestId: 1, input: 'stale' } });
  await flushMicrotasks();
  const second = worker.onMessage({ data: { kind: 'render', requestId: 2, input: 'fresh' } });
  await flushMicrotasks();

  worker.timers.shift()();
  await flushMicrotasks();
  worker.timers.shift()();
  await Promise.all([first, second]);

  assert.deepEqual(inputs, ['fresh']);
  assert.deepEqual(plain(worker.messages.at(-2)), { kind: 'noReply', requestId: 1 });
  assert.deepEqual(plain(worker.messages.at(-1)), {
    kind: 'completed',
    requestId: 2,
    target: 'offscreenInWorker',
    stats: { rendered: 'fresh' },
  });
});

test('offscreen worker cancels a queued render before it reaches the canvas', async () => {
  const inputs = [];
  const worker = loadWorker({
    render: (input) => {
      inputs.push(input);
      return { rendered: input };
    },
  });
  await initializeOffscreenWorker(worker);

  const render = worker.onMessage({ data: { kind: 'render', requestId: 3, input: 'cancelled' } });
  await flushMicrotasks();
  await worker.onMessage({ data: { kind: 'cancel', requestId: 3 } });
  worker.timers.shift()();
  await render;

  assert.deepEqual(inputs, []);
  assert.deepEqual(plain(worker.messages.at(-1)), { kind: 'noReply', requestId: 3 });
});
