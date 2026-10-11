#!/usr/bin/env python3
"""Public Mermaid API isolation using production modules and real Chromium workers.

The engine module is an explicit boundary fixture, NOT the shipped Rust binary. Its hangs
execute an infinite WebAssembly loop; the actual worker endpoint, queue, timeouts, restart,
DOM/CSS validation, callbacks and document publication run without substitutions. Module URL
bindings are bundled into host-owned data URLs; no network server or requests are needed.
Missing browser dependencies are errors, not passes.
"""
import json
import os
from pathlib import Path
import shutil
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
ENGINE = r'''
if (typeof document !== 'undefined') throw new Error('engine ran on UI thread');
const spin = new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array([
  0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,115,112,105,110,0,0,
  10,9,1,7,0,3,64,12,0,11,11
]))).exports.spin;
const mode = globalThis.fixtureMode;
export default async function() { if (mode === 'hang-boot') spin(); }
export function validateConfig(json) {
  if (JSON.parse(json).theme === 'hang-validate') spin();
  return {errors: []};
}
export async function parse(source) {
  if (source === 'hang-parse') spin();
  if (source === 'trap') throw new WebAssembly.RuntimeError('fixture trap');
  if (source === 'crash') {
    setTimeout(() => { throw new Error('fixture worker crash'); }, 0);
    return new Promise(() => {});
  }
  if (source.startsWith('slow')) await new Promise(r => setTimeout(r, 100));
  return { ir: { diagram_type: 'Flowchart', diagnostics: source === 'syntax-error'
    ? [{severity: 'Error', message: 'fixture syntax error', span: {line: 2}}] : [],
    graphNotNeededByBrowser: () => {} }, warnings: [source] };
}
function escape(text) { return String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;'); }
export function renderSvg(source, config = {}) {
  if (source === 'hang-render') spin();
  if (source === 'oversize') return 'x'.repeat(16 * 1024 * 1024 + 1);
  if (source === 'active-markup') return '<svg xmlns="http://www.w3.org/2000/svg"><script>window.executed=true</script></svg>';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100" data-theme="${escape(config.theme || 'default')}">
    <style>svg{color:navy}.node{fill:currentColor}</style>
    <defs><marker id="arrow"><path d="M0 0 L2 1 L0 2Z"/></marker></defs>
    <g id="node" data-id="${escape(source)}" data-callback="show" class="node">
      <rect x="10" y="10" width="100" height="60"/><text x="20" y="40">${escape(source)}</text>
    </g><path d="M110 40 L190 40" marker-end="url(#arrow)"/>
  </svg>`;
}
'''
SETUP = r'''
({svg, compat, entry, engine}) => {
  window.urls = [];
  const bundle = code => {
    const bytes = new TextEncoder().encode(code);
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    const url = 'data:text/javascript;base64,' + btoa(binary);
    urls.push(url); return url;
  };
  const svgUrl = bundle(svg);
  // Resolve only static asset bindings, as a bundler does. Worker scheduling, RPC and
  // lifecycle code is unchanged; the worker loads this same production module.
  if (!compat.includes('"./mermaid-svg.mjs"')) throw new Error('SVG import binding changed');
  window.compatUrl = bundle(compat.replace('"./mermaid-svg.mjs"', JSON.stringify(svgUrl)));
  window.engineUrl = bundle(engine);
  window.hangBootUrl = bundle("globalThis.fixtureMode = 'hang-boot';\n" + engine);
  window.failedEntryUrl = bundle("throw new Error('fixture first import failed')");
  if (!entry.includes('new URL("../pkg/frankenmermaid.js", import.meta.url)')) throw new Error('engine URL binding changed');
  window.entryUrl = bundle(entry.replaceAll('"./mermaid-compat.mjs"', JSON.stringify(compatUrl))
    .replace('new URL("../pkg/frankenmermaid.js", import.meta.url)', JSON.stringify(engineUrl)));
  const NativeWorker = window.Worker;
  window.workers = [];
  window.clients = [];
  window.rejections = [];
  window.addEventListener('unhandledrejection', event => window.rejections.push(String(event.reason)));
  window.Worker = class extends NativeWorker {
    constructor(...args) {
      if (window.failFirstEntry) { args[0] = failedEntryUrl; window.failFirstEntry = false; }
      super(...args); this.sent = []; this.replies = []; this.stops = 0; workers.push(this);
      this.addEventListener('message', ({data}) => this.replies.push(data)); }
    postMessage(message, ...rest) { this.sent.push(message); return super.postMessage(message, ...rest); }
    terminate() { this.stops += 1; return super.terminate(); }
  };
  window.check = (ok, message) => { if (!ok) throw new Error(message); };
  window.sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  window.until = async fn => {
    for (let i = 0; i < 500; ++i) { if (fn()) return; await sleep(10); }
    throw new Error('worker never reached expected state');
  };
  window.calls = () => workers.flatMap(worker => worker.sent.filter(m => m.kind === 'call'));
  window.make = async (options = {}) => {
    const { createWorkerMermaid } = await import(compatUrl);
    const api = createWorkerMermaid({ engineUrl,
      requestTimeoutMs: 1000, reportError: () => {}, ...options });
    clients.push(api); return api;
  };
}
'''
SCENARIOS = [
('default entry is lazy, uses a real worker, and preserves SVG scoping', r'''
  const { default: api } = await import(entryUrl); clients.push(api);
  api.initialize({ startOnLoad: false });
  await sleep(30); await api.run({ nodes: [] }); check(workers.length === 0, 'eager worker');
  const result = await api.render('default-diagram', 'default source');
  check(workers.length === 1, 'default entry did not isolate engine');
  check(result.svg.includes('id="default-diagram"'), 'render ID missing');
  check(result.svg.includes('#default-diagram--'), 'marker/CSS references not scoped');
  check(result.warnings[0] === 'default source', 'parse warnings lost');
  check(calls().map(m => m.method).join(',') === 'validateConfig,parse,renderSvg', 'unexpected engine calls');
'''),
('callbacks remain host-local and configuration snapshots remain independent', r'''
  const api = await make(); let selected = [];
  api.registerCallback('show', id => selected.push(id));
  const config = {theme: 'dark', securityLevel: 'loose'};
  api.initialize(config);
  const first = api.render('first', 'slow first'); config.theme = 'wrong';
  api.initialize({theme: 'neutral', securityLevel: 'loose'});
  const second = api.render('second', 'second');
  const [a, b] = await Promise.all([first, second]);
  check(a.svg.includes('data-theme="dark"') && b.svg.includes('data-theme="neutral"'), 'config snapshot lost');
  const host = document.createElement('div'); host.innerHTML = a.svg; document.body.append(host);
  const unbind = a.bindFunctions(host); const node = host.querySelector('[data-callback]');
  node.dispatchEvent(new MouseEvent('click', {bubbles: true, button: 0}));
  node.dispatchEvent(new KeyboardEvent('keydown', {bubbles: true, key: 'Enter'}));
  check(selected.join(',') === 'slow first,slow first', 'host callbacks not bound');
  unbind(); node.dispatchEvent(new MouseEvent('click', {bubbles: true, button: 0}));
  check(selected.length === 2, 'cleanup retained callback');
  check(!JSON.stringify(workers[0].sent).includes('selected.push'), 'callback code crossed worker boundary');
'''),
('native parse diagnostics and unsafe SVG still fail before publication', r'''
  const api = await make(); const errors = []; api.parseError = error => errors.push(error);
  const parsed = await api.parse('syntax-error', {suppressErrors: true});
  check(parsed === false && errors.length === 0, 'parse suppression changed');
  let syntax; try { await api.render('syntax', 'syntax-error'); } catch (error) { syntax = error; }
  check(syntax.code === 'parse' && syntax.diagnostics[0].span.line === 2, 'diagnostics lost');
  check(!calls().some(m => m.method === 'renderSvg'), 'rendered invalid parse');
  let unsafe; try { await api.render('unsafe', 'active-markup'); } catch (error) { unsafe = error; }
  check(unsafe && !window.executed && !document.querySelector('svg'), 'unsafe SVG was published');
'''),
('infinite WASM parse is terminated while UI ticks and independent queued work recovers', r'''
  const api = await make({requestTimeoutMs: 300}); let ticks = 0;
  const timer = setInterval(() => ticks++, 10);
  const settled = Promise.allSettled([api.parse('hang-parse'), api.parse('good')]);
  await until(() => calls().some(m => m.method === 'parse' && m.args[0] === 'hang-parse'));
  const results = await settled; clearInterval(timer);
  check(results[0].reason.code === 'worker-timeout', 'hung parse did not time out');
  check(results[1].value.warnings[0] === 'good', 'queued diagram lost');
  check(ticks >= 5 && workers[0].stops === 1 && workers.length === 2, 'UI froze or worker not retired');
  check(calls().filter(m => m.args[0] === 'hang-parse').length === 1, 'hung input was replayed');
'''),
('infinite WASM render is never replayed on the UI thread', r'''
  const api = await make({requestTimeoutMs: 300}); let ticks = 0;
  const timer = setInterval(() => ticks++, 10);
  const results = await Promise.allSettled([api.render('bad', 'hang-render'), api.render('good', 'good')]);
  clearInterval(timer);
  check(results[0].reason.code === 'worker-timeout' && results[1].value.svg.includes('good'), 'render recovery failed');
  check(ticks >= 5 && workers.length === 2, 'render blocked UI');
  check(calls().filter(m => m.method === 'renderSvg' && m.args[0] === 'hang-render').length === 1, 'render replayed');
'''),
('bootstrap failure can be retried without changing configuration', r'''
  window.failFirstEntry = true;
  const api = await make();
  const first = await Promise.allSettled([api.parse('first'), api.parse('second')]);
  check(first.every(r => r.reason.code === 'worker-crash') && workers.length === 1, 'bootstrap not shared');
  check((await api.parse('retry')).warnings[0] === 'retry', 'rejected validation cache poisoned retry');
  check(workers.length === 2 && workers[0].stops === 1, 'bootstrap worker not replaced');
'''),
('initialization and validation WASM hangs are also bounded', r'''
  const boot = await make({engineUrl: hangBootUrl, requestTimeoutMs: 300});
  let error; try { await boot.parse('never executes'); } catch (caught) { error = caught; }
  check(error.code === 'worker-timeout' && calls().length === 0, 'bootstrap timeout failed');
  const api = await make({requestTimeoutMs: 300}); api.initialize({theme: 'hang-validate'});
  try { await api.parse('never parses'); } catch (caught) { error = caught; }
  check(error.code === 'worker-timeout' && calls().every(m => m.method === 'validateConfig'), 'validation not isolated');
  api.initialize({theme: 'neutral'});
  check((await api.parse('corrected')).warnings[0] === 'corrected', 'validation restart failed');
'''),
('worker crashes and fatal engine errors retire the instance without losing siblings', r'''
  const api = await make();
  const results = await Promise.allSettled([api.parse('crash'), api.parse('trap'), api.parse('good')]);
  check(results[0].reason.code === 'worker-crash', 'crash not reported');
  check(results[1].reason.code === 'worker-error' && results[2].value.warnings[0] === 'good', 'trap recovery failed');
  check(workers.length === 3 && workers[0].stops === 1 && workers[1].stops === 1, 'broken engines reused');
'''),
('run retains failed sources, renders good siblings, and supports corrected retries', r'''
  const api = await make({requestTimeoutMs: 300});
  const bad = document.createElement('pre'), good = document.createElement('pre');
  bad.textContent = 'hang-render'; good.textContent = 'good'; document.body.append(bad, good);
  let callbacks = 0, error;
  try { await api.run({nodes: [bad, good], postRenderCallback: () => callbacks++}); } catch (caught) { error = caught; }
  check(error instanceof AggregateError && error.errors.length === 1, 'aggregate failed');
  check(bad.textContent === 'hang-render' && !bad.hasAttribute('data-processed'), 'bad source overwritten');
  check(good.querySelector('svg') && good.dataset.processed === 'true' && callbacks === 1, 'sibling/callback lost');
  bad.textContent = 'corrected';
  await api.run({nodes: [bad, good], postRenderCallback: () => callbacks++});
  check(bad.querySelector('svg') && callbacks === 2, 'corrected retry or processed-node skip failed');
'''),
('DOM edits during a worker call still revoke publication authority', r'''
  const api = await make(); const source = document.createElement('pre');
  source.textContent = 'slow original'; document.body.append(source);
  const done = api.run({nodes: [source]}).catch(error => error);
  await until(() => calls().some(m => m.method === 'parse'));
  source.textContent = 'new source';
  const error = await done;
  check(error instanceof AggregateError && error.errors[0].code === 'stale-source', 'stale render accepted');
  check(source.textContent === 'new source' && !source.hasAttribute('data-processed'), 'edited source lost');
'''),
('disposal preempts a running engine and rejects queued operations without publication', r'''
  const api = await make(); const source = document.createElement('pre'); source.textContent = 'hang-render';
  document.body.append(source);
  const running = api.run({nodes: [source]}).catch(error => error);
  await until(() => calls().some(m => m.method === 'renderSvg'));
  const queued = api.parse('queued').catch(error => error);
  api.dispose(); api.dispose();
  const error = await running;
  check(error instanceof AggregateError && error.errors[0].code === 'disposed', 'active run not disposed');
  check((await queued).code === 'disposed' && source.textContent === 'hang-render', 'queue/source not retained');
  check(workers.length === 1 && workers[0].stops === 1, 'dispose leaked or double-terminated worker');
'''),
('independent instances have independent worker lifetimes', r'''
  const one = await make(), two = await make();
  const pending = one.parse('hang-parse').catch(error => error);
  await until(() => calls().some(m => m.args[0] === 'hang-parse'));
  check((await two.parse('other')).warnings[0] === 'other', 'one worker blocked the other');
  one.dispose(); check((await pending).code === 'disposed', 'first instance not disposed');
  check((await two.parse('still live')).warnings[0] === 'still live', 'other instance disposed');
  check(workers.length === 2 && workers.filter(w => w.stops).length === 1, 'worker ownership wrong');
'''),
('oversized engine replies are rejected inside the worker before DOM parsing', r'''
  const api = await make(); let error;
  try { await api.render('large', 'oversize'); } catch (caught) { error = caught; }
  check(error.code === 'worker-output-limit', 'reply budget not enforced by worker');
  check(!workers[0].replies.some(r => typeof r.value === 'string' && r.value.length > 16 * 1024 * 1024), 'oversized reply crossed boundary');
  check((await api.render('good', 'good')).svg.includes('good') && workers.length === 1, 'ordinary error broke worker');
'''),
('explicit stop preempts infinite WASM and queued calls without destroying configuration', r'''
  const api = await make(); api.initialize({theme: 'dark', securityLevel: 'loose'});
  const pending = Promise.allSettled([api.parse('hang-parse'), api.parse('never sent')]);
  await until(() => calls().some(m => m.args[0] === 'hang-parse'));
  api.cancelPending();
  const retry = api.render('retry', 'corrected');
  check((await pending).every(r => r.reason.name === 'AbortError' && r.reason.code === 'cancelled'), 'stop did not cancel both operations');
  check((await retry).svg.includes('data-theme="dark"'), 'configuration lost on stop');
  check(!calls().some(m => m.args[0] === 'never sent'), 'cancelled queue was restarted');
  check(workers.length === 2 && workers[0].stops === 1, 'stop failed to preempt worker');
'''),
('stop during bootstrap does not poison the next worker or cached validation', r'''
  window.failedEntryUrl = 'data:text/javascript,await new Promise(() => {})';
  window.failFirstEntry = true;
  const api = await make();
  const pending = api.parse('never booted').catch(error => error);
  await until(() => workers.length === 1);
  api.cancelPending();
  const retry = api.parse('retry');
  check((await pending).code === 'cancelled', 'bootstrap not cancelled');
  check((await retry).warnings[0] === 'retry', 'bootstrap cancellation poisoned retry');
  check(workers.length === 2 && workers[0].sent.length === 1, 'cancelled bootstrap entered execution');
'''),
('stop cancels coalesced batches and immediate same-element retry does not inherit stale ownership', r'''
  const api = await make(), first = document.createElement('pre'), second = document.createElement('pre');
  first.textContent = 'hang-render'; second.textContent = 'not started'; document.body.append(first, second);
  let callbacks = 0;
  const old = Promise.allSettled([api.run({nodes: [first, second], postRenderCallback: () => callbacks++}),
    api.run({nodes: [first], postRenderCallback: () => callbacks++})]);
  await until(() => calls().some(m => m.method === 'renderSvg'));
  api.cancelPending(); first.textContent = 'corrected';
  const fresh = api.run({nodes: [first], postRenderCallback: () => callbacks++});
  check((await old).every(r => r.reason instanceof AggregateError && r.reason.errors[0].code === 'cancelled'), 'coalesced batch was not cancelled');
  await fresh;
  check(first.querySelector('svg') && callbacks === 1, 'new run inherited cancelled ownership or callback');
  check(second.textContent === 'not started' && !second.hasAttribute('data-processed'), 'stop consumed remaining source');
  check(!calls().some(m => m.args[0] === 'not started'), 'cancelled batch dispatched its sibling');
  await api.run({nodes: [first, second], postRenderCallback: () => callbacks++});
  check(second.querySelector('svg') && callbacks === 2, 'resuming the remaining document failed');
'''),
('stop in a host callback retains published output and prevents the next diagram', r'''
  const api = await make(), first = document.createElement('pre'), second = document.createElement('pre');
  first.textContent = 'published'; second.textContent = 'remaining'; document.body.append(first, second);
  const error = await api.run({nodes: [first, second], postRenderCallback() { api.cancelPending(); }}).catch(error => error);
  check(error instanceof AggregateError && error.errors[0].code === 'cancelled', 'callback stop lost');
  check(first.querySelector('svg') && first.dataset.processed === 'true', 'stop rolled back published content');
  check(second.textContent === 'remaining' && !calls().some(m => m.args[0] === 'remaining'), 'stop started remaining diagram');
  await api.run({nodes: [first, second]});
  check(second.querySelector('svg'), 'later run could not resume');
'''),
('cancellation during binding is checked before replacing the source DOM', r'''
  let api, cancel = true;
  api = await make({reportError() { if (cancel) { cancel = false; api.cancelPending(); } }});
  api.initialize({securityLevel: 'loose'}); // missing callback reporting invokes the host
  const source = document.createElement('pre'); source.textContent = 'retain me'; document.body.append(source);
  const error = await api.run({nodes: [source]}).catch(error => error);
  check(error instanceof AggregateError && error.errors[0].code === 'cancelled', 'binding reporter cancellation ignored');
  check(source.textContent === 'retain me' && !source.hasAttribute('data-processed'), 'cancelled binding published SVG');
  let called = 0; api.registerCallback('show', () => called++);
  await api.run({nodes: [source]});
  source.querySelector('[data-callback]').dispatchEvent(new MouseEvent('click', {bubbles: true, button: 0}));
  check(called === 1, 'retry lost registered callbacks');
'''),
('stopping pending work preserves already-bound callbacks and other instances', r'''
  const api = await make(), other = await make(); let called = 0;
  api.initialize({securityLevel: 'loose'}); api.registerCallback('show', () => called++);
  const result = await api.render('published', 'published');
  const host = document.createElement('div'); host.innerHTML = result.svg; document.body.append(host); result.bindFunctions(host);
  const old = api.parse('hang-parse').catch(error => error), independent = other.parse('slow other');
  await until(() => calls().some(m => m.args[0] === 'hang-parse'));
  api.cancelPending(); api.cancelPending();
  check((await old).code === 'cancelled' && (await independent).warnings[0] === 'slow other', 'cancellation crossed instance boundary');
  host.querySelector('[data-callback]').dispatchEvent(new MouseEvent('click', {bubbles: true, button: 0}));
  check(called === 1, 'stop revoked already-published callbacks');
'''),
('stop settles a never-resolving host callback without resuming the old document batch', r'''
  const api = await make(), first = document.createElement('pre'), second = document.createElement('pre');
  first.textContent = 'published'; second.textContent = 'remaining'; document.body.append(first, second);
  let release;
  const pending = api.run({nodes: [first, second], postRenderCallback: () => new Promise(resolve => { release = resolve; })}).catch(error => error);
  await until(() => release);
  api.cancelPending();
  const error = await pending;
  check(error instanceof AggregateError && error.errors[0].code === 'cancelled', 'callback wait did not settle');
  release(); await sleep(30);
  check(!calls().some(m => m.args[0] === 'remaining') && second.textContent === 'remaining', 'old callback resumed document batch');
  await api.run({nodes: [first, second]});
  check(second.querySelector('svg'), 'fresh document run did not recover');
'''),
]

results = []
with sync_playwright() as p:
        executable = os.environ.get('CHROMIUM_PATH') or shutil.which('chromium') or shutil.which('chromium-browser')
        browser = p.chromium.launch(executable_path=executable, args=['--no-sandbox'])
        for name, code in SCENARIOS:
            page = browser.new_page()
            errors, external = [], []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('request', lambda request: external.append(request.url) if not request.url.startswith(('blob:', 'data:')) else None)
            page.route('http://**/*', lambda route: route.abort())
            page.route('https://**/*', lambda route: route.abort())
            try:
                page.set_content('<!doctype html><html><body></body></html>')
                page.evaluate(SETUP, {'svg': (ROOT / 'mermaid-svg.mjs').read_text(),
                                     'compat': (ROOT / 'mermaid-compat.mjs').read_text(),
                                     'entry': (ROOT / 'mermaid.mjs').read_text(), 'engine': ENGINE})
                # Start asynchronously, then use a bounded Playwright poll. A regression that
                # runs the infinite loop on the UI thread must fail, not hang the test driver.
                page.evaluate("code => { window.finished = false; window.failure = null;"
                              " (new Function('return (async () => {' + code + '})()'))()"
                              ".catch(error => window.failure = String(error.stack || error))"
                              ".finally(() => window.finished = true); }", code)
                page.wait_for_function('window.finished', timeout=12000)
                failure = page.evaluate('window.failure')
                rejections = page.evaluate('window.rejections')
                if failure or errors or external or rejections:
                    raise AssertionError(json.dumps({'failure': failure, 'page_errors': errors, 'external_requests': external, 'unhandled_rejections': rejections}))
                page.evaluate('clients.forEach(api => api.dispose()); urls.forEach(url => URL.revokeObjectURL(url))')
                if not page.evaluate('workers.every(worker => worker.stops === 1)'):
                    raise AssertionError('worker leaked at teardown')
                results.append({'scenario': name, 'pass': True})
                print('PASS', name, flush=True)
            except Exception as error:
                results.append({'scenario': name, 'pass': False, 'error': str(error)})
                print('FAIL', name, str(error), flush=True)
            finally:
                page.close()
        browser.close()
print(json.dumps({'engine': 'explicit fixture with real WASM infinite-loop failure', 'scenarios': results}, indent=2))
raise SystemExit(0 if all(item['pass'] for item in results) else 1)
