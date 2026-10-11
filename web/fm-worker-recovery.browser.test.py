#!/usr/bin/env python3
"""Real Chromium Worker termination with a deliberately non-returning WASM function.

Exercises the production source-worker client, backend, and mounted editor. The worker
protocol peer is an explicit fixture, NOT the packaged Rust engine. Unlike an async mock,
its infinite WASM loop cannot read cancellation messages or return to its event loop.
No network resources are loaded. Requires Playwright and an installed Chromium.
"""
import base64
import json
import os
import pathlib
import shutil
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parent
SOURCE = pathlib.Path(os.environ.get("FM_SOURCE_EDITOR_MODULE", ROOT / "fm-source-editor.js")).read_bytes()
MODULE = "data:text/javascript;base64," + base64.b64encode(SOURCE).decode("ascii")

SCRIPT = r"""async (moduleUrl) => {
  const { createSourceEditorBackend, mountSourceEditor } = await import(moduleUrl);
  const assert = (condition, reason) => { if (!condition) throw new Error(reason); };
  const waitFor = async (predicate) => {
    const deadline = performance.now() + 4000;
    while (!predicate()) {
      assert(performance.now() < deadline, 'condition timed out');
      await new Promise(resolve => setTimeout(resolve, 5));
    }
  };
  const outcome = promise => promise.then(value => ({ value }), error => ({ name: error.name, message: error.message }));
  // (module (func (export "hang") (loop (br 0))))
  const bytes = [0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,104,97,110,103,0,0,
    10,9,1,7,0,3,64,12,0,11,11];
  assert(WebAssembly.validate(new Uint8Array(bytes)), 'infinite-loop WASM fixture is invalid');
  const peer = `
    const engine = new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array(${JSON.stringify(bytes)})));
    onmessage = ({ data: m }) => {
      if (m.kind === 'init') {
        postMessage({ kind: 'ready', sourceEditing: true, sourceDeletion: true,
          sourceInsertion: true, sourceBatchEditing: true, deckRendering: true });
        return;
      }
      if (m.kind === 'cancel') { postMessage({ kind: 'noReply', requestId: m.requestId }); return; }
      if (m.input === 'hang') {
        postMessage({ kind: 'fixtureEntered' });
        engine.exports.hang();
        throw new Error('infinite loop unexpectedly returned');
      }
      if (m.kind === 'sourceRender') {
        postMessage({ kind: 'sourceRendered', requestId: m.requestId,
          svg: '<svg xmlns="http://www.w3.org/2000/svg"><g id="fm-node-a-0"><text>' + m.input + '</text></g></svg>',
          snapshot: { bindings: [{ elementId: 'fm-node-a-0', sourceId: 'A', kind: 'node', snippet: m.input,
            textRange: { startByte: 0, endByte: new TextEncoder().encode(m.input).length } }], parsed: { warnings: [] } } });
      }
    };
  `;
  const workerUrl = 'data:text/javascript;base64,' + btoa(peer);
  const reports = [];
  function fixture(requestTimeoutMs = 600) {
    const workers = [];
    let fallbackCalls = 0;
    class ObservedWorker extends Worker {
      constructor(url, options) {
        super(url, options);
        this.entered = false;
        this.retired = false;
        workers.push(this);
        this.addEventListener('message', ({ data }) => {
          if (data.kind === 'fixtureEntered') this.entered = true;
        });
      }
      terminate() { this.retired = true; super.terminate(); }
    }
    const options = { WorkerClass: ObservedWorker, workerUrl, moduleUrl: 'unused-fixture-engine',
      initTimeoutMs: 3000, requestTimeoutMs, cancelGraceMs: 40 };
    const loadModule = async () => {
      fallbackCalls += 1;
      throw new Error('dispatched WASM work must never fall back to the main thread');
    };
    return { workers, options, loadModule, get fallbackCalls() { return fallbackCalls; } };
  }
  let heartbeat = 0;
  const timer = setInterval(() => { heartbeat += 1; }, 5);
  try {
    {
      const f = fixture();
      const backend = createSourceEditorBackend({ ...f.options, loadModule: f.loadModule });
      try {
        const result = outcome(backend.renderSource('hang'));
        await waitFor(() => f.workers[0].entered);
        const before = heartbeat;
        const failure = await result;
        assert(failure.name === 'SourceWorkerInterrupted', 'execution timeout did not stay isolated');
        assert(heartbeat > before + 5, 'browser UI stopped ticking during synchronous WASM');
        assert(f.workers[0].retired && f.fallbackCalls === 0, 'timed-out work was retried on the UI');
        const corrected = await backend.renderSource('corrected');
        assert(corrected.svg.includes('corrected') && f.workers.length === 2, 'fresh worker did not recover');
        reports.push({ scenario: 'execution_timeout_and_corrected_source', passed: true });
      } finally { backend.dispose(); }
      assert(f.workers.every(worker => worker.retired), 'timeout scenario leaked a worker');
    }
    {
      const f = fixture(3000);
      const backend = createSourceEditorBackend({ ...f.options, loadModule: f.loadModule });
      try {
        const first = outcome(backend.renderSource('hang'));
        await waitFor(() => f.workers[0].entered);
        const middle = outcome(backend.renderSource('obsolete'));
        const latest = backend.renderSource('latest');
        assert((await first).name === 'AbortError' && (await middle).name === 'AbortError', 'superseded promises not cancelled');
        assert((await latest).svg.includes('latest'), 'latest explicit source was not rendered');
        assert(f.workers.length === 2 && f.workers[0].retired && f.fallbackCalls === 0, 'busy WASM was not replaced once');
        reports.push({ scenario: 'superseding_active_wasm_renders_latest', passed: true });
      } finally { backend.dispose(); }
    }
    {
      const f = fixture(3000);
      const backend = createSourceEditorBackend({ ...f.options, loadModule: f.loadModule });
      try {
        const first = outcome(backend.renderSource('hang'));
        await waitFor(() => f.workers[0].entered);
        backend.cancel();
        assert((await first).name === 'AbortError', 'explicit cancellation did not settle');
        await waitFor(() => f.workers[0].retired);
        assert(f.workers.length === 1 && f.fallbackCalls === 0, 'cancel without new work started another worker');
        reports.push({ scenario: 'cancel_without_new_request_stops_cpu', passed: true });
      } finally { backend.dispose(); }
    }
    {
      const f = fixture();
      document.body.innerHTML = '<textarea id="source"></textarea><div id="out"></div><section id="panel"></section>';
      const sourceEl = document.getElementById('source'), outEl = document.getElementById('out');
      sourceEl.value = 'original';
      let changes = 0;
      const editor = mountSourceEditor({ sourceEl, outEl, panelEl: document.getElementById('panel'),
        loadModule: f.loadModule, workerOptions: f.options, onChange: () => { changes += 1; } });
      try {
        await editor.render(sourceEl.value);
        assert(editor.exportSvg().text.includes('original'), 'initial preview failed');
        sourceEl.value = 'hang';
        const pending = editor.render(sourceEl.value);
        await waitFor(() => f.workers[0].entered);
        // Actual input remains usable while the other thread is stuck inside WASM.
        document.getElementById('source-editor-search').value = 'UI still responsive';
        await pending;
        assert(sourceEl.value === 'hang' && editor.exportSource().text === 'hang', 'timeout altered or lost source');
        assert(outEl.getAttribute('aria-busy') === 'false', 'timeout left preview permanently busy');
        assert(document.getElementById('source-editor-message').textContent.includes('timed out'), 'timeout not explained');
        assert(document.getElementById('source-editor-save-svg').disabled, 'stale SVG was exportable');
        document.getElementById('source-editor-undo').click();
        assert(sourceEl.value === 'original' && changes === 1, 'undo did not restore pre-timeout source');
        await editor.render(sourceEl.value);
        assert(editor.exportSvg().text.includes('original') && f.workers.length === 2, 'mounted editor failed to recover');
        assert(f.fallbackCalls === 0, 'mounted editor loaded synchronous fallback');
        reports.push({ scenario: 'mounted_editor_preserves_source_undo_and_recovers', passed: true });
      } finally { editor.dispose(); }
      assert(f.workers.every(worker => worker.retired), 'mounted editor leaked a worker');
    }
    return { scenarios: reports, heartbeat_ticks: heartbeat, boundary: 'real Worker and infinite WASM; explicit protocol peer, not packaged Rust' };
  } finally {
    clearInterval(timer);
  }
}"""


def main():
    chromium = os.environ.get("CHROMIUM_PATH") or shutil.which("chromium") or shutil.which("chromium-browser")
    if not chromium:
        raise SystemExit("Set CHROMIUM_PATH to an installed Chromium binary.")
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=chromium, headless=True, args=["--no-sandbox"])
        try:
            page = browser.new_page()
            errors, requests = [], []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("request", lambda request: requests.append(request.url))
            page.route("**/*", lambda route: route.abort())
            result = page.evaluate(SCRIPT, MODULE)
            assert errors == [], errors
            assert requests == [], requests
            print(json.dumps({**result, "page_errors": errors, "network_requests": requests}, indent=2))
        finally:
            browser.close()


if __name__ == "__main__":
    main()
