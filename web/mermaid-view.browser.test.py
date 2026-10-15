#!/usr/bin/env python3
"""Managed SVG views using the production public API, SVG policy and real module workers.

Only engine exports are fixtures; hangs execute non-yielding WebAssembly. These tests do
not qualify the compiled Rust parser/layout. Requires Playwright and installed Chromium.
Run: python web/mermaid-view.browser.test.py (CHROMIUM_PATH selects an existing browser).
"""
import json
import os
from pathlib import Path
import shutil
import unittest
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
ENGINE = r'''
if (typeof document !== 'undefined' && !globalThis.directFixture) throw new Error('engine on UI thread');
const spin = new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array([
  0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,115,112,105,110,0,0,
  10,9,1,7,0,3,64,12,0,11,11]))).exports.spin;
export default async function() {}
export function validateConfig() { return {errors:[]}; }
export async function parse(source) {
  if (source === 'hang-parse') spin();
  if (source.startsWith('slow')) await new Promise(r => setTimeout(r, 100));
  return {ir:{diagram_type:'Flowchart',diagnostics:source === 'bad'
    ? [{severity:'Error',message:'fixture syntax error'}] : []},warnings:[source]};
}
const escape = text => String(text).replaceAll('&','&amp;').replaceAll('<','&lt;')
  .replaceAll('"','&quot;').replaceAll('\r','&#13;');
export function renderSvg(source, config = {}) {
  if (source === 'hang-render') spin();
  if (source === 'trap') throw new WebAssembly.RuntimeError('fixture trap');
  if (source === 'unsafe') return '<svg xmlns="http://www.w3.org/2000/svg"><image href="https://example.invalid/image"/></svg>';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100" role="img" data-theme="${escape(config.theme || 'default')}">
    <style>svg{color:navy}.node{fill:currentColor}</style>
    <defs><marker id="arrow"><path d="M0 0 L2 1 L0 2Z"/></marker></defs>
    <g id="node" data-id="A" data-callback="show" class="node"><rect width="100" height="60"/>
      <text x="10" y="30">${escape(source)}</text></g>
    <path d="M100 30 L180 30" marker-end="url(#arrow)"/>
  </svg>`;
}
'''
SETUP = r'''async ({svg, compat, engine}) => {
  const bundle = source => {
    const bytes = new TextEncoder().encode(source); let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return 'data:text/javascript;base64,' + btoa(binary);
  };
  const svgUrl = bundle(svg);
  const compatUrl = bundle(compat.replace('"./mermaid-svg.mjs"', JSON.stringify(svgUrl)));
  window.modules = await import(compatUrl);
  window.engineUrl = bundle(engine);
  const NativeWorker = Worker;
  window.workers = []; window.clients = []; window.rejections = []; window.reports = [];
  window.addEventListener('unhandledrejection', event => rejections.push(String(event.reason)));
  window.Worker = class extends NativeWorker {
    constructor(...args) { super(...args); this.sent=[];this.stops=0;workers.push(this); }
    postMessage(message,...rest) {this.sent.push(message);return super.postMessage(message,...rest);}
    terminate() {this.stops++;return super.terminate();}
  };
  window.check = (ok, reason) => { if (!ok) throw new Error(reason); };
  window.sleep = ms => new Promise(resolve => setTimeout(resolve,ms));
  window.until = async test => {for(let i=0;i<500;i++){if(test())return;await sleep(10)}throw new Error('condition not reached')};
  window.calls = () => workers.flatMap(worker => worker.sent.filter(call => call.kind==='call'));
  window.host = () => {const node=document.createElement('div');document.body.append(node);return node;};
  window.make = (options={}) => {const api=modules.createWorkerMermaid({engineUrl,requestTimeoutMs:1000,
    reportError:error => reports.push(error),...options});clients.push(api);return api;};
  window.rejects = async (promise, code) => {
    const result=await promise.then(()=>null,error=>error);
    check(result && (!code || result.code===code),'expected rejection '+code+', got '+String(result));return result;
  };
}'''

class ViewTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pw = sync_playwright().start()
        cls.browser = cls.pw.chromium.launch(executable_path=os.environ.get('CHROMIUM_PATH') or
            shutil.which('chromium') or shutil.which('google-chrome'), headless=True, args=['--no-sandbox'])

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.pw.stop()

    def setUp(self):
        self.page = self.browser.new_page()
        self.addCleanup(self.page.close)
        self.errors, self.requests = [], []
        self.page.on('pageerror', lambda e: self.errors.append(str(e)))
        self.page.on('request', lambda r: self.requests.append(r.url))
        self.page.set_content('<!doctype html><html><body><input id="editor"></body></html>')
        self.page.evaluate(SETUP, {'svg': (ROOT/'mermaid-svg.mjs').read_text(),
            'compat': Path(os.environ.get('FM_COMPAT_MODULE', ROOT/'mermaid-compat.mjs')).read_text(), 'engine': ENGINE})

    def tearDown(self):
        remaining = self.page.evaluate('''async () => {
          clients.forEach(api=>api.dispose());await sleep(30);
          return {rejections,workers:workers.filter(worker=>worker.stops!==1).length};
        }''')
        self.assertEqual(remaining, {'rejections': [], 'workers': 0})
        self.assertEqual(self.errors, [])
        self.assertEqual([url for url in self.requests if url.startswith('http')], [])

    def test_lazy_claim_validation_and_nested_cross_instance_ownership(self):
        self.page.evaluate('''async () => {
          const api=make(), other=make(), node=host(), inner=document.createElement('div');node.append(inner);
          const before=node.innerHTML, view=api.createView(node,{id:'preview'});
          check(!view.busy && view.source===null && view.element===null && Object.isFrozen(view),'initial state');
          for(const target of [node,inner,document.body]) {
            let error;try{other.createView(target)}catch(e){error=e}check(error?.code==='element-busy','nested owner admitted');
          }
          await rejects(api.run({nodes:[node]}),'element-busy');
          for(const options of [null,[],{id:'invalid id'},{unknown:true}]) {
            let error;try{api.createView(host(),options)}catch(e){error=e}check(error,'invalid view options');
          }
          check(workers.length===0 && node.innerHTML===before && !node.hasAttribute('aria-busy'),'creation did work');
          view.dispose();check(view.disposed,'dispose not observable');other.createView(node).dispose();
        }''')

    def test_success_returns_diagnostics_and_preserves_unicode_and_stable_ids(self):
        self.page.evaluate(r'''async () => {
          const api=make(), node=host(), view=api.createView(node,{id:'stable'});
          const source='%% café 😀\r\nflowchart LR\r\nA["東京"]-->B';
          api.initialize({theme:'dark'});const result=await view.render(source);
          check(view.source===source && node.firstChild===view.element && view.element.id==='stable','publication');
          check(result.svg.includes('stable') && result.warnings[0]===source && !result.bindFunctions,'result contract');
          check(view.element.getAttribute('data-theme')==='dark','config lost');
          check(calls().find(call=>call.method==='parse').args[0]===source,'source rewritten');
          const first=view.element;await view.render('updated');
          check(view.element!==first && view.element.id==='stable' && !view.busy && !node.hasAttribute('aria-busy'),'replacement');
        }''')

    def test_synchronous_burst_coalesces_without_overloading_or_cancelling_siblings(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host()), other=api.createView(host());
          const promises=[];for(let i=0;i<80;i++)promises.push(view.render('revision '+i).catch(error=>error));
          const sibling=other.render('sibling');const results=await Promise.all(promises);await sibling;
          check(results.slice(0,-1).every(error=>error.name==='AbortError'),'obsolete requests not cancelled');
          check(view.source==='revision 79' && other.source==='sibling','wrong final sources');
          check(calls().filter(call=>call.method==='parse').length===2,'obsolete source entered engine');
        }''')

    def test_non_yielding_wasm_is_preempted_without_losing_last_good_or_other_views(self):
        self.page.evaluate('''async () => {
          const api=make({requestTimeoutMs:10000}), node=host(), view=api.createView(node), other=api.createView(host());
          await view.render('good');const good=view.element;
          const old=view.render('hang-render').catch(error=>error);
          await until(()=>calls().some(call=>call.method==='renderSvg' && call.args[0]==='hang-render'));await sleep(80);
          const sibling=other.render('independent');check(view.element===good && node.firstChild===good,'last good vanished');
          const fresh=view.render('corrected');check((await old).name==='AbortError','old execution survived');
          await Promise.all([sibling,fresh]);check(view.source==='corrected' && other.source==='independent','lost sibling');
          check(workers.length===2 && workers[0].stops===1,'worker not replaced');
          check(calls().filter(call=>call.method==='renderSvg' && call.args[0]==='hang-render').length===1,'failed work replayed');
        }''')

    def test_failures_keep_visible_svg_source_and_registered_callbacks(self):
        self.page.evaluate('''async () => {
          const api=make({requestTimeoutMs:500}), view=api.createView(host());let activated=0;
          api.initialize({securityLevel:'loose'});api.registerCallback('show',()=>activated++);
          await view.render('good');const old=view.element;
          for(const source of ['bad','unsafe','trap','hang-render']) {
            await rejects(view.render(source));check(view.element===old && view.source==='good' && !view.busy,'failed render published');
            old.querySelector('rect').dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));
          }
          check(activated===4,'last good interactions lost');await view.render('recovered');check(view.source==='recovered','no recovery');
        }''')

    def test_cancel_external_abort_and_preaborted_invalid_requests(self):
        self.page.evaluate('''async () => {
          const api=make(), node=host(), view=api.createView(node), c=new AbortController();c.abort();
          node.setAttribute('aria-busy','false');const active=view.render('slow valid');
          await rejects(view.render('never',{signal:c.signal}),'cancelled');await rejects(view.render(''));
          check(view.busy,'invalid request cancelled valid work');await active;
          const next=view.render('hang-parse').catch(error=>error);
          await until(()=>calls().some(call=>call.args[0]==='hang-parse'));
          check(view.cancel() && !view.busy && !view.cancel(),'cancel state');check((await next).name==='AbortError','cancel rejection');
          check(node.getAttribute('aria-busy')==='false' && view.source==='slow valid','cancel changed previous result');
          const external=new AbortController(), wait=view.render('slow external',{signal:external.signal}).catch(error=>error);
          external.abort();check((await wait).code==='cancelled','external abort ignored');
          await view.render('after');check(node.getAttribute('aria-busy')==='false','busy attribute not restored');
        }''')

    def test_api_cancel_dispose_and_view_dispose_have_distinct_ownership(self):
        self.page.evaluate('''async () => {
          const api=make(), other=make(), node=host(), view=api.createView(node), sibling=other.createView(host());
          await view.render('good');const old=view.element;
          const wait=view.render('hang-render').catch(error=>error);
          await until(()=>calls().some(call=>call.method==='renderSvg' && call.args[0]==='hang-render'));
          api.cancelPending();check((await wait).name==='AbortError' && !view.disposed,'instance cancel disposed view');
          await view.render('retry');const retained=view.element;view.dispose();
          check(node.firstChild===retained && view.disposed && !view.busy,'dispose removed visible diagram');
          await rejects(view.render('late'),'disposed');const replacement=api.createView(node);await replacement.render('replacement');
          const pending=replacement.render('slow disposed').catch(error=>error);api.dispose();
          check(replacement.disposed && (await pending).code==='cancelled','instance did not dispose view');
          await sibling.render('unaffected');check(sibling.source==='unaffected','other instance lost');
        }''')

    def test_host_mutations_attachment_changes_and_retry(self):
        self.page.evaluate('''async () => {
          const api=make(), node=host(), view=api.createView(node);await view.render('first');
          for(const mutate of [()=>node.innerHTML='<p>host update</p>',()=>node.firstChild.replaceWith(node.firstChild.cloneNode(true)),
            ()=>node.querySelector('text').textContent='host inner update',()=>node.remove()]) {
            await view.render('first');const pending=view.render('slow changed').catch(error=>error);
            mutate();const markup=node.innerHTML;const error=await pending;
            check(error.code==='stale-source' && node.innerHTML===markup,'host update overwritten');document.body.append(node);
          }
          await view.render('retry');check(view.source==='retry','explicit retry blocked');
        }''')

    def test_id_conflicts_including_duplicate_old_ids_are_rejected(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host(),{id:'reserved'});await view.render('first');
          const old=view.element;
          for(const id of [old.id,old.querySelector('[id]').id]) {
            const pending=view.render('slow conflict').catch(error=>error), foreign=host();foreign.id=id;
            check((await pending).code==='id-conflict' && view.element===old,'foreign ID ignored');foreign.remove();
          }
          await view.render('after');check(view.source==='after','ID conflict unrecoverable');
        }''')

    def test_reporter_reentrancy_cannot_publish_or_revoke_last_good_bindings(self):
        self.page.evaluate('''async () => {
          let intervene=()=>{};const api=make({reportError:()=>intervene()}), node=host(), view=api.createView(node);
          api.initialize({securityLevel:'loose'});await view.render('first');const first=view.element;
          intervene=()=>view.cancel();await rejects(view.render('cancel at binding'),'cancelled');
          check(view.element===first && view.source==='first','reentrant cancelled publication');
          intervene=()=>{node.textContent='host changed during binding'};
          await rejects(view.render('host race'),'stale-source');check(node.textContent==='host changed during binding','reporter host update erased');
          intervene=()=>{};await view.render('recovered');check(view.source==='recovered','reporter poisoned view');
        }''')

    def test_old_callbacks_removed_new_callbacks_bound_and_focus_preserved(self):
        self.page.evaluate('''async () => {
          const api=make(), node=host(), view=api.createView(node);window.activations=[];window.view=view;
          api.initialize({securityLevel:'loose'});api.registerCallback('show',id=>activations.push(id));
          await view.render('first');window.previous=view.element;view.element.querySelector('[data-id]').focus();
          await view.render('second');check(document.activeElement===view.element.querySelector('[data-id]'),'focus lost');
          document.getElementById('editor').focus();await view.render('third');check(document.activeElement.id==='editor','editor focus stolen');
          host().append(previous);previous.querySelector('rect').dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));
          check(activations.length===0,'old bound callback survived replacement');previous.remove();
        }''')
        self.page.locator('div > svg [data-id]').click()
        self.page.locator('div > svg [data-id]').focus()
        self.page.keyboard.press('Enter')
        self.page.keyboard.press('Space')
        self.assertEqual(self.page.evaluate('activations'), ['A', 'A', 'A'])
        self.page.evaluate('view.dispose()')
        self.page.locator('div > svg [data-id]').click()
        self.assertEqual(self.page.evaluate('activations'), ['A', 'A', 'A'])

    def test_shadow_root_ownership_and_focus(self):
        self.page.evaluate('''async () => {
          const api=make(), shadowHost=host(), root=shadowHost.attachShadow({mode:'open'}), node=document.createElement('div');root.append(node);
          const view=api.createView(node);api.initialize({securityLevel:'loose'});api.registerCallback('show',()=>{});
          await view.render('first');view.element.querySelector('[data-id]').focus();await view.render('second');
          check(root.activeElement===view.element.querySelector('[data-id]'),'shadow focus lost');
          let error;try{api.createView(shadowHost)}catch(e){error=e}check(error?.code==='element-busy','shadow child could be destroyed');
        }''')

    def test_pending_document_run_cannot_replace_a_newly_owned_descendant(self):
        self.page.evaluate('''async () => {
          const api=make(), parent=host(), child=document.createElement('div');child.textContent='slow document';parent.append(child);
          const pending=api.run({nodes:[parent]}).catch(error=>error);
          await until(()=>calls().some(call=>call.method==='parse'));
          const view=api.createView(child);const error=await pending;
          check(error instanceof AggregateError && error.errors[0].code==='element-busy','document run overwrote view');
          check(child.parentNode===parent && child.textContent==='slow document','source lost');await view.render('owned');
        }''')

    def test_cold_custom_loader_cancellation_discards_obsolete_result(self):
        self.page.evaluate('''async () => {
          globalThis.directFixture=true;
          let release;const gate=new Promise(resolve=>release=resolve), exports=await import(engineUrl);
          // This custom loader never enters either hang fixture; it is explicitly host-owned.
          const api=modules.createMermaid({loadEngine:()=>gate});clients.push(api);
          const view=api.createView(host()), old=view.render('old').catch(error=>error), latest=view.render('latest');
          check((await old).name==='AbortError','custom loader waiter not cancelled');release(exports);await latest;
          check(view.source==='latest','late custom loader revived old source');
        }''')

if __name__ == '__main__':
    unittest.main(verbosity=2)
