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
  window.textarea = (value='initial') => {const node=document.createElement('textarea');node.value=value;document.body.append(node);return node;};
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

    def test_source_connection_initial_render_debounce_and_exact_source(self):
        self.page.evaluate(r'''async () => {
          const api=make(), view=api.createView(host()), src=textarea('initial'), rendered=[];
          const binding=view.connectSource(src,{debounceMs:40,onRender:(result,source)=>rendered.push([result.warnings[0],source])});
          check(Object.isFrozen(binding) && binding.pending && !workers.length,'connection rendered synchronously');
          for(let i=0;i<50;i++){src.value='revision '+i;src.dispatchEvent(new Event('input'));}
          src.value='flowchart LR\nA["東京 café 😀"]-->B';src.dispatchEvent(new Event('input'));
          await until(()=>view.source===src.value && !binding.pending);
          check(rendered.length===1 && rendered[0][0]===src.value && rendered[0][1]===src.value,'source changed or duplicate notification');
          check(calls().filter(call=>call.method==='parse').length===1,'debounce dispatched obsolete text');
        }''')

    def test_source_connection_validation_keeps_existing_binding_and_work(self):
        self.page.evaluate('''async () => {
          const api=make(), node=host(), view=api.createView(node), src=textarea();
          const binding=view.connectSource(src,{debounceMs:30});
          const nested=document.createElement('textarea');node.append(nested);
          for(const target of [document.querySelector('input'),nested,node,null]) {
            let error;try{view.connectSource(target)}catch(e){error=e}check(error,'invalid source admitted');
          }
          for(const options of [null,[],{debounceMs:-1},{debounceMs:1.5},{debounceMs:60001},
            {debounceMs:Infinity},{onRender:7},{onError:false},{renderOnConnect:1},{unused:1}]) {
            let error;try{view.connectSource(src,options)}catch(e){error=e}check(error,'invalid options admitted');
          }
          check(!binding.disconnected && binding.pending,'invalid replacement disconnected editor');
          await until(()=>view.source==='initial');check(src.value==='initial','source overwritten');
        }''')

    def test_source_input_preempts_infinite_wasm_before_debounce_and_preserves_siblings(self):
        self.page.evaluate('''async () => {
          const api=make({requestTimeoutMs:10000}), view=api.createView(host()), sibling=api.createView(host()), src=textarea('good');
          const binding=view.connectSource(src,{debounceMs:300,renderOnConnect:false});await binding.flush();const old=view.element;
          src.value='hang-render';const hung=binding.flush().catch(error=>error);
          await until(()=>calls().some(call=>call.method==='renderSvg' && call.args[0]==='hang-render'));await sleep(60);
          const other=sibling.render('other');src.value='corrected';src.dispatchEvent(new Event('input'));
          check(workers[0].stops===1 && view.element===old,'input did not preempt immediately or removed old diagram');
          check((await hung).name==='AbortError','old source not cancelled');
          await other;check(sibling.source==='other' && !calls().some(call=>call.args[0]==='corrected'),'sibling stalled or debounce bypassed');
          await until(()=>view.source==='corrected');check(src.value==='corrected','editor changed');
        }''')

    def test_source_composition_never_publishes_partial_text_and_flush_refuses_it(self):
        self.page.evaluate('''async () => {
          const api=make({requestTimeoutMs:10000}), view=api.createView(host()), src=textarea('good');
          const binding=view.connectSource(src,{debounceMs:20,renderOnConnect:false});await binding.flush();
          src.value='hang-parse';const old=binding.flush().catch(error=>error);
          await until(()=>calls().some(call=>call.args[0]==='hang-parse'));await sleep(30);
          src.dispatchEvent(new CompositionEvent('compositionstart'));
          check((await old).name==='AbortError','composition failed to stop old work');
          for(const text of ['t','to','とう']){src.value=text;src.dispatchEvent(new InputEvent('input',{isComposing:true}));}
          await sleep(70);await rejects(binding.flush(),'source-composing');
          check(view.source==='good' && !calls().some(call=>['t','to','とう'].includes(call.args[0])),'partial composition rendered');
          src.value='東京';src.dispatchEvent(new CompositionEvent('compositionend'));src.dispatchEvent(new InputEvent('input'));
          await until(()=>view.source==='東京');check(calls().filter(call=>call.method==='parse' && call.args[0]==='東京').length===1,'composition commit duplicated');
        }''')

    def test_source_changes_without_input_or_during_binding_cannot_publish(self):
        self.page.evaluate('''async () => {
          let intervene=()=>{};const api=make({reportError:()=>intervene()}), view=api.createView(host()), src=textarea('good'), errors=[];
          const binding=view.connectSource(src,{renderOnConnect:false,onError:e=>errors.push(e.code)});await binding.flush();const good=view.element;
          src.value='slow old';const pending=binding.flush().catch(error=>error);
          await until(()=>calls().some(call=>call.args[0]==='slow old'));src.value='new without event';
          check((await pending).code==='stale-source' && view.element===good,'programmatic edit lost');
          api.initialize({securityLevel:'loose'});intervene=()=>{src.value='changed by reporter'};
          await rejects(binding.flush(),'stale-source');check(view.element===good && src.value==='changed by reporter','binding race published');
          intervene=()=>{};await binding.flush();check(view.source==='changed by reporter' && errors.length===2,'retry or error hooks wrong');
        }''')

    def test_source_cancel_global_stop_and_manual_render_revoke_queued_refresh(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host()), src=textarea(), binding=view.connectSource(src,{debounceMs:40});
          api.cancelPending();await sleep(70);check(!workers.length && !binding.pending && !binding.disconnected,'global stop resurrected pending refresh');
          src.value='cancelled';src.dispatchEvent(new Event('input'));check(view.cancel(),'view stop did not see timer');
          await sleep(70);check(!workers.length && !binding.pending,'view cancel left debounce');
          src.value='queued';src.dispatchEvent(new Event('input'));await view.render('manual');await sleep(70);
          check(view.source==='manual' && !calls().some(call=>call.args[0]==='queued'),'manual render overwritten by queued source');
          src.value='next input';src.dispatchEvent(new Event('input'));await until(()=>view.source==='next input');
          src.value='connection cancelled';src.dispatchEvent(new Event('input'));check(binding.cancel(),'connection cancel false');
          await sleep(70);check(view.source==='next input','cancelled refresh ran');
        }''')

    def test_source_disconnect_reconnect_and_disposal_release_listeners_and_timers(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host()), first=textarea('first'), second=textarea('second');
          const a=view.connectSource(first,{debounceMs:40}), b=view.connectSource(second,{debounceMs:10});
          check(a.disconnected && !a.pending,'reconnect retained first binding');
          first.value='old listener';first.dispatchEvent(new Event('input'));await until(()=>view.source==='second');
          check(!calls().some(call=>['first','old listener'].includes(call.args[0])),'old listener dispatch');
          b.disconnect();b.disconnect();second.value='after disconnect';second.dispatchEvent(new Event('input'));
          await sleep(60);check(view.source==='second' && !view.disposed,'disconnect disposed view or kept listening');
          await rejects(b.flush(),'disposed');const c=view.connectSource(first,{debounceMs:40});view.dispose();
          await sleep(60);check(c.disconnected && !c.pending && view.element.isConnected,'view dispose leaked connection or removed last diagram');
          const other=api.createView(host()), d=other.connectSource(second,{debounceMs:40});api.dispose();await sleep(60);
          check(d.disconnected && !d.pending && other.disposed,'API dispose leaked source connection');
        }''')

    def test_source_failures_and_async_hook_failures_are_observed_without_losing_last_good(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host()), src=textarea('good'), seen=[];
          const binding=view.connectSource(src,{debounceMs:10,onRender:async (_output,source)=>{seen.push(source);throw new Error('render hook');},
            onError:async (error,source)=>{seen.push(source+':'+error.code);throw new Error('error hook');}});
          await until(()=>view.source==='good' && reports.length===1);const old=view.element;
          src.value='bad';src.dispatchEvent(new Event('input'));await until(()=>reports.length===2 && !binding.pending);
          check(view.element===old && view.source==='good' && src.value==='bad','parse failure destroyed last good or editor');
          check(seen[0]==='good' && seen[1]==='bad:parse','hooks missing source/error');
          binding.disconnect();view.connectSource(src,{debounceMs:10});await until(()=>reports.length===3);
          check(reports[2].code==='parse','automatic failures unreported');
        }''')

    def test_source_callbacks_may_reenter_render_or_disconnect_without_stale_notifications(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host()), src=textarea('first'), notified=[];
          let binding;binding=view.connectSource(src,{debounceMs:10,onRender:(_output,source)=>{
            notified.push(source);if(source==='first'){src.value='second';src.dispatchEvent(new Event('input'));}else binding.disconnect();
          }});
          await until(()=>binding.disconnected);check(view.source==='second' && notified.join(',')==='first,second','reentrant source update lost');
          src.value='third';src.dispatchEvent(new Event('input'));await sleep(40);check(notified.length===2,'disconnected notification');
        }''')

    def test_source_detachment_and_moving_editor_into_view_reject_before_publication(self):
        self.page.evaluate('''async () => {
          const api=make(), node=host(), view=api.createView(node), src=textarea('good');
          const binding=view.connectSource(src,{renderOnConnect:false,onError:()=>{}});await binding.flush();const good=view.element;
          src.value='slow detached';const pending=binding.flush().catch(error=>error);src.remove();
          check((await pending).code==='stale-source' && view.element===good,'detached editor published');
          document.body.append(src);await binding.flush();const last=view.element;
          src.value='slow moved';const moved=binding.flush().catch(error=>error);node.append(src);
          check((await moved).code==='stale-source' && src.parentNode===node && last.parentNode===node,'nested editor erased');
        }''')

    def test_actual_typing_undo_and_focus_remain_owned_by_textarea(self):
        self.page.evaluate('''() => {
          const api=make();window.liveView=api.createView(host());window.liveSource=textarea('');liveSource.id='live-source';
          window.binding=liveView.connectSource(liveSource,{debounceMs:30,renderOnConnect:false});liveSource.focus();
        }''')
        self.page.keyboard.type('flowchart LR\nA-->B')
        self.page.wait_for_function('liveView.source === liveSource.value')
        state = self.page.evaluate('({value:liveSource.value,start:liveSource.selectionStart,end:liveSource.selectionEnd,focus:document.activeElement.id})')
        self.assertEqual(state['value'], 'flowchart LR\nA-->B')
        self.assertEqual(state['start'], len(state['value']))
        self.assertEqual(state['end'], len(state['value']))
        self.assertEqual(state['focus'], 'live-source')
        self.page.keyboard.press('Control+z')
        self.page.wait_for_function('liveSource.value !== "flowchart LR\\nA-->B"')
        undone = self.page.locator('#live-source').input_value()
        self.page.wait_for_function('!binding.pending')
        self.assertEqual(self.page.locator('#live-source').input_value(), undone)
        self.assertEqual(self.page.evaluate('document.activeElement.id'), 'live-source')

    def test_source_flush_bypasses_delay_and_only_latest_success_notifies(self):
        self.page.evaluate('''async () => {
          const api=make(), view=api.createView(host()), src=textarea('slow first'), seen=[];
          const binding=view.connectSource(src,{debounceMs:60000,renderOnConnect:false,onRender:(_result,source)=>seen.push(source)});
          const old=binding.flush().catch(error=>error);await until(()=>calls().some(call=>call.args[0]==='slow first'));
          src.value='latest';src.dispatchEvent(new Event('input'));check(binding.pending,'input did not schedule');
          const result=await binding.flush();check((await old).name==='AbortError','supersession not cancellation');
          check(result.warnings[0]==='latest' && view.source==='latest' && seen.join(',')==='latest' && !binding.pending,'flush or notification mismatch');
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
