#!/usr/bin/env python3
"""Public native-deck API: paired geometry, scoped SVG, real workers and canonical playback.

The engine is an explicit boundary fixture. Tests execute the production adapter, worker
entry and checked-in CLI runtime; they do not execute the packaged Rust WASM renderer.
Run: python web/mermaid-deck.browser.test.py (Playwright + Chromium required).
"""
import json
import os
from pathlib import Path
import shutil
import unittest
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = (ROOT / "crates/fm-cli/tests/fixtures/deck_runtime_fixture.html").read_text()
DATA = FIXTURE.split('<script>\n"use strict";', 1)[1].split('var deck = window.FmDeckRuntime.mount(', 1)[0]
SETUP = r"""async ({svg, compat, fixture, runtime}) => {
  window.urls = [];
  // about:blank is an opaque origin: use host-owned data modules, as the existing worker
  // suite does, so worker submodule imports do not depend on opaque-origin blob access.
  const moduleUrl = window.moduleUrl = text => {
    const bytes = new TextEncoder().encode(text); let binary='';
    for(const byte of bytes) binary+=String.fromCharCode(byte);
    const url='data:text/javascript;base64,'+btoa(binary);urls.push(url);return url;
  };
  const svgUrl = moduleUrl(svg);
  const compatUrl = moduleUrl(compat.replace('"./mermaid-svg.mjs"', JSON.stringify(svgUrl)));
  Object.assign(window, await import(svgUrl), await import(compatUrl));
  // The unmodified existing CLI fixture supplies the same graph used by runtime tests.
  (0,eval)(fixture); (0,eval)(runtime);
  window.rawDeck = () => ({svg:SVG, manifest:structuredClone(MANIFEST), warnings:['native warning']});
  window.check = (value, reason) => { if (!value) throw new Error(reason); };
  window.waitFor = async predicate => {
    for (let i=0;i<500;i++) {if(predicate()) return; await new Promise(resolve=>setTimeout(resolve,10));}
    throw new Error('condition did not complete');
  };
  window.calls = [];
  window.engine = {
    parse: () => { calls.push('parse'); throw new Error('a deck must not be independently parsed'); },
    renderSvg: () => { calls.push('renderSvg'); throw new Error('a deck must not be independently rendered'); },
    validateConfig: json => { calls.push(['config',JSON.parse(json)]); return {errors:[]}; },
    renderDeck: (source, config) => { calls.push(['deck',source,config]); return rawDeck(); }
  };
  window.direct = () => createMermaid({loadEngine:async()=>engine, document});
  window.players = []; window.apis = [];
  window.stage = () => {
    const element = document.createElement('div'); element.tabIndex=0;
    element.style.cssText='position:relative;width:900px;height:600px;overflow:hidden';
    document.body.append(element); return element;
  };
  // Native loop cannot yield to cancellation messages. Only transport of its entry marker
  // is intercepted by the test; the production worker scheduler/protocol is unmodified.
  const RealWorker = Worker;
  window.entered = 0; window.terminations = 0;
  window.Worker = class extends RealWorker {
    constructor(...args) {
      super(...args);
      this.addEventListener('message', event => {
        if(event.data?.fixture === 'entered') {entered++; event.stopImmediatePropagation();}
      });
    }
    terminate() {terminations++; super.terminate();}
  };
  window.workerApi = (extra='', timeout=1500) => {
    const source = `
      const deck = ${JSON.stringify(rawDeck())};
      const loop = new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array(
        [0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,104,97,110,103,0,0,10,9,1,7,0,3,64,12,0,11,11])));
      export function validateConfig() { return {errors:[]}; }
      export function parse() { return {ir:{diagram_type:'Flowchart',diagnostics:[]},warnings:[]}; }
      export function renderSvg() { throw new Error('independent render forbidden'); }
      export function renderDeck(source, config) {
        if(source === 'hang') {postMessage({fixture:'entered'}); loop.exports.hang();}
        if(source === 'trap') throw new WebAssembly.RuntimeError('fixture trap');
        if(source === 'oversized') return {...deck,warnings:['x'.repeat(17*1024*1024)]};
        const output=structuredClone(deck); output.manifest.title=source;
        output.warnings.push(config.theme || 'default');
        ${extra}
        return output;
      }`;
    const url=moduleUrl(source);urls.push(url);
    const api=createWorkerMermaid({engineUrl:url,document,requestTimeoutMs:timeout});apis.push(api);return api;
  };
}"""


class DeckTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pw = sync_playwright().start()
        cls.browser = cls.pw.chromium.launch(executable_path=os.environ.get("CHROMIUM_PATH") or shutil.which("chromium"), args=["--no-sandbox"])

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.pw.stop()

    def setUp(self):
        self.page = self.browser.new_page(reduced_motion="reduce")
        self.page.set_default_timeout(8000)
        self.errors = [];
        self.requests = [];
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.on("request", lambda request: self.requests.append(request.url))
        self.page.set_content('<!doctype html><html><body></body></html>')
        self.page.evaluate(SETUP, {
            "svg": (ROOT / "web/mermaid-svg.mjs").read_text(),
            "compat": (ROOT / "web/mermaid-compat.mjs").read_text(),
            "fixture": DATA,
            "runtime": (ROOT / "crates/fm-cli/src/deck_runtime.js").read_text(),
        })

    def tearDown(self):
        try:
            self.page.evaluate("() => {players.forEach(p=>p.destroy());apis.forEach(a=>a.dispose());urls.forEach(u=>URL.revokeObjectURL(u));}")
            self.assertEqual(self.errors, [])
            self.assertEqual(self.requests, [])
        finally:
            self.page.close()

    def test_single_native_call_keeps_source_config_geometry_and_warnings(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);api.initialize({theme:'dark'});
          const host=stage();host.textContent='retained';
          const raw=rawDeck(), before=JSON.stringify(raw);
          engine.renderDeck=(source,config)=>{calls.push(['deck',source,config]);return raw;};
          const source='%% café 😀\r\nflowchart LR\r\nA-->B';
          const out=await api.renderDeck('deckone',source,host);
          check(host.textContent==='retained','renderDeck mutated the host');
          check(!('element' in out),'private DOM escaped');
          check(calls.length===2 && calls[1][0]==='deck','more than config + one native call');
          check(calls[1][1]===source && calls[1][2].theme==='dark','input/config changed');
          check(JSON.stringify(raw)===before,'engine result mutated');
          check(JSON.stringify(out.manifest.viewBox)===JSON.stringify(raw.manifest.viewBox),'viewBox changed');
          check(JSON.stringify(out.manifest.slides[0].bounds)===JSON.stringify(raw.manifest.slides[0].bounds),'camera changed');
          check(out.manifest.slides[0].id==='intro' && out.manifest.slides[0].nodes[0].sourceId==='a','source/slide IDs changed');
          check(out.warnings[0]==='native warning','native warning lost');
          check(Object.isFrozen(out.manifest.nodeGeometry) && Object.isFrozen(out.manifest.slides[0].nodes[0]),'manifest not immutable');
          check(Array.isArray(out.presentationWarnings),'presentation warnings missing');
        }""")

    def test_ordinary_render_run_and_registered_callbacks_remain_usable(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);api.initialize({securityLevel:'loose',startOnLoad:false});
          engine.parse=()=>({ir:{diagram_type:'Flowchart',diagnostics:[]},warnings:['parse warning']});
          engine.renderSvg=()=>rawDeck().svg.replace('data-id="a"','data-id="a" data-callback="pick"');
          const selected=[];api.registerCallback('pick',id=>selected.push(id));
          const ordinary=await api.render('ordinary','source');const host=stage();host.innerHTML=ordinary.svg;
          ordinary.bindFunctions(host);host.querySelector('[data-callback]').dispatchEvent(new MouseEvent('click',{button:0,bubbles:true}));
          check(selected.join(',')==='a' && ordinary.warnings[0]==='parse warning','ordinary rendering changed');
          check(host.querySelector('[data-id="a"]').id==='ordinary--6','ordinary ID mapping changed');
          const pre=document.createElement('pre');pre.textContent='flowchart LR\nA-->B';document.body.append(pre);
          await api.run({nodes:[pre]});check(pre.dataset.processed==='true' && pre.querySelector('svg'),'run failed');
          check((await api.renderDeck('paired','source')).manifest.slides.length===2,'deck failed after run');
        }""")

    def test_all_join_keys_and_native_runtime_reveals_survive_namespacing(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);const output=await api.renderDeck('joined','source');
          const m=output.manifest;
          const host=stage(), title=document.createElement('p');document.body.append(title);
          const player=FmDeckRuntime.mount({stage:host,svg:output.svg,manifest:m,ui:{title}});players.push(player);
          const root=host.querySelector('svg'), find=id=>root.querySelector(`[id="${id}"]`);
          for(const slide of m.slides) {
            for(const member of [...slide.nodes,...(slide.edges||[]),...(slide.clusters||[])])check(find(member.elementId),'member missing');
            for(const step of slide.steps||[])for(const id of step.elementIds)check(find(id),'step missing');
          }
          for(const id of Object.keys(m.nodeGeometry))check(find(id),'geometry missing');
          for(const [id,ends] of Object.entries(m.edgeEndpoints))check(find(id)&&find(ends.fromElementId)&&find(ends.toElementId),'edge mapping missing');
          for(const id of Object.keys(m.nodeSlideIndex))check(find(id),'owner missing');
          const b=find(m.slides[0].nodes[1].elementId);
          check(b.classList.contains('fm-deck-hidden'),'first reveal not hidden');
          player.next();await waitFor(()=>!b.classList.contains('fm-deck-hidden'));
          player.next();check(title.textContent==='The core cluster','canonical navigation failed');
          player.overview();check(title.textContent==='One graph','overview failed');
        }""")

    def test_two_decks_keep_markers_themes_followers_and_motion_separate(self):
        self.page.emulate_media(reduced_motion="no-preference")
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);
          engine.renderDeck=(source,config)=>{
            const raw=rawDeck();raw.svg=raw.svg.replace('</svg>',
              `<style>.testshape{fill:${config.theme==='dark'?'blue':'red'}}</style><rect class="testshape"/></svg>`);
            return raw;
          };
          api.initialize({theme:'dark'});const one=await api.renderDeck('first','source');
          api.initialize({theme:'neutral'});const two=await api.renderDeck('first-extra','source');
          for(const out of [one,two]) {
            const host=stage();players.push(FmDeckRuntime.mount({stage:host,svg:out.svg,manifest:out.manifest}));
          }
          await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
          const all=[...document.querySelectorAll('[id]')].map(e=>e.id);check(all.length===new Set(all).size,'ID collision');
          check(getComputedStyle(document.querySelector('#first .testshape')).fill==='rgb(0, 0, 255)','first theme lost');
          check(getComputedStyle(document.querySelector('#first-extra .testshape')).fill==='rgb(255, 0, 0)','second theme lost');
          for(const out of [one,two]) {
            const id=out.manifest.slides[1].nodes[1].elementId;
            const node=document.getElementById(id),mirror=document.getElementById(id+'-mirror-header');
            check(node && mirror && node.hasAttribute('transform'),'native node prefix no longer recognized');
            check(node.getAttribute('transform')===mirror.getAttribute('transform'),'follower motion lost');
            check(node.classList.contains('fm-deck-dim')===mirror.classList.contains('fm-deck-dim'),'follower membership lost');
          }
        }""")

    def test_map_backed_native_records_and_legacy_schema(self):
        self.page.evaluate(r"""async () => {
          const api=workerApi(`for(const key of ['nodeGeometry','edgeEndpoints','nodeSlideIndex'])
            output.manifest[key]=new Map(Object.entries(output.manifest[key]));`);
          const out=await api.renderDeck('maps','source');
          check(Object.keys(out.manifest.nodeGeometry).length===4,'Map geometry disappeared');
          check(Object.keys(out.manifest.edgeEndpoints).length===5,'Map edges disappeared');
          const legacy=rawDeck();legacy.manifest.schemaVersion='1.0.0';
          delete legacy.manifest.nodeGeometry;delete legacy.manifest.edgeEndpoints;legacy.manifest.overview=null;
          const result=prepareDeck(legacy,'legacy',document);
          check(result.manifest.schemaVersion==='1.0.0' && result.manifest.overview===null,'legacy static deck changed');
        }""")

    def test_namespace_families_do_not_alias_adversarial_render_ids(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);engine.renderDeck=()=>{
            const out=rawDeck();out.svg=out.svg.replace('</svg>',
              '<defs><marker id="plainmarker"/></defs><g id="fm-node-0"/></svg>');return out;};
          for(const id of ['one','fm-node-deck-3_one','fm-deck-3_one']) {
            const out=await api.renderDeck(id,'source');const host=stage();host.innerHTML=out.svg;
          }
          const ids=[...document.querySelectorAll('[id]')].map(node=>node.id);
          check(ids.length===new Set(ids).size,'generic IDs collided with runtime-recognized IDs');
        }""")

    def test_null_manifest_is_explicit_and_svg_still_obeys_the_resource_policy(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);
          engine.renderDeck=()=>({...rawDeck(),manifest:null,warnings:['unsupported family']});
          const result=await api.renderDeck('static','source');
          check(result.manifest===null && result.svg.startsWith('<svg'),'null manifest was fabricated or lost');
          check(result.warnings[0]==='unsupported family','explanation lost');
          const diagnostic={severity:'Warning',message:'deck ignored',span:{start:{line:2,col:1,byte:8}},notes:['unsupported family']};
          engine.renderDeck=()=>({svg:rawDeck().svg,warnings:[diagnostic]});
          const absent=await api.renderDeck('absent','source');
          check(absent.manifest===null && JSON.stringify(absent.warnings[0])===JSON.stringify(diagnostic),'native None/Diagnostic contract lost');
          engine.renderDeck=()=>({svg:'<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>',manifest:null,warnings:[]});
          let failed=false;try{await api.renderDeck('unsafe','source');}catch{failed=true;}check(failed,'active SVG accepted');
        }""")

    def test_malformed_pair_membership_geometry_versions_and_indices_never_publish(self):
        self.page.evaluate(r"""() => {
          const changes=[m=>m.schemaVersion='2.0.0',m=>m.schemaVersion='1.2.0',m=>m.viewBox.width+=1,
            m=>m.options.zoomMax=0,m=>m.options.autoAdvanceMs=2147483648,m=>m.slides.push(m.slides[0]),
            m=>m.slides[0].nodes[0].elementId='absent',m=>m.slides[0].nodes[0].step=2,
            m=>m.slides[0].steps[0].elementIds=[],m=>m.slides[0].steps[0].elementIds.push('fm-node-b-1'),
            m=>m.nodeGeometry['fm-node-a-0'].width=-1,m=>delete m.nodeGeometry['fm-node-a-0'],
            m=>m.edgeEndpoints['fm-edge-0'].fromElementId='missing',m=>m.nodeSlideIndex['fm-node-a-0']=['core'],
            m=>delete m.nodeSlideIndex['fm-node-a-0'],m=>m.overview.tour='false',
            m=>m.slides[0].edges[0].touching='false'];
          for(const change of changes){const raw=rawDeck();change(raw.manifest);
            let failed=false;try{prepareDeck(raw,'invalid',document);}catch{failed=true;}check(failed,String(change));}
          check(!document.querySelector('svg'),'failed validation mutated the host');
        }""")

    def test_snapshot_rejects_lossy_cyclic_accessor_sparse_and_oversized_data(self):
        self.page.evaluate(r"""() => {
          const changes=[r=>r.manifest.loop=r.manifest,r=>r.manifest.options.zoomMax=Infinity,
            r=>Object.defineProperty(r,'svg',{enumerable:true,get(){throw new Error('GETTER RAN');}}),
            r=>r.warnings=[,],r=>r.warnings=[1],r=>r.manifest=false,
            r=>r.manifest.nodeGeometry=new Map([[7,{}]]),r=>r.warnings=['\ud800'],
            r=>r.warnings=['x'.repeat(17*1024*1024)]];
          for(const change of changes){const raw=rawDeck();change(raw);let error;
            try{snapshotDeckResult(raw);}catch(e){error=e;}check(error && error.message!=='GETTER RAN','lossy/accessor input accepted');}
          const raw=rawDeck();raw.manifest.extra=JSON.parse('{"__proto__":{"polluted":true}}');
          const copied=snapshotDeckResult(raw);check({}.polluted===undefined,'prototype mutated');
          check(Object.hasOwn(copied.manifest.extra,'__proto__'),'inert data lost');
        }""")

    def test_configuration_and_callback_authority_are_captured_before_waiting(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);let release;engine.renderDeck=async(source,config)=>{
            await new Promise(resolve=>release=resolve);const raw=rawDeck();raw.manifest.title=config.theme;
            raw.svg=raw.svg.replace('data-id="a"','data-id="a" data-callback="pick"');return raw;};
          const selected=[];api.initialize({theme:'dark',securityLevel:'loose'});api.registerCallback('pick',id=>selected.push(id));
          const pending=api.renderDeck('callbacks','source');await waitFor(()=>release);
          api.initialize({theme:'neutral',securityLevel:'loose'});release();const result=await pending;
          check(result.manifest.title==='dark','in-flight configuration changed');
          const host=stage();host.innerHTML=result.svg;const unbind=result.bindFunctions(host);
          const node=host.querySelector('[data-callback]');node.dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));
          check(selected.join(',')==='a','authored callback identity lost');
          api.initialize({securityLevel:'strict'});node.dispatchEvent(new MouseEvent('click',{bubbles:true,button:0}));
          check(selected.length===1,'tightened policy did not revoke deck binding');unbind();
        }""")

    def test_cancel_pending_and_disposal_revoke_delayed_native_results(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);let release;engine.renderDeck=()=>new Promise(r=>release=r);
          const pending=api.renderDeck('cancelled','source').catch(e=>e);await waitFor(()=>release);
          api.cancelPending();const error=await pending;check(error.code==='cancelled','pending deck not stopped');
          release(rawDeck());engine.renderDeck=()=>rawDeck();
          check((await api.renderDeck('retry','source')).manifest.slides.length===2,'retry failed');
          api.dispose();const disposed=await api.renderDeck('disposed','source').catch(e=>e);
          check(disposed.code==='disposed','disposed API rendered');
        }""")

    def test_real_worker_timeout_retires_wasm_and_independent_work_recovers(self):
        self.page.evaluate(r"""async () => {
          const api=workerApi('',400);api.initialize({theme:'dark'});
          const bad=api.renderDeck('timeout','hang').catch(e=>e);await waitFor(()=>entered);
          const good=api.renderDeck('recovery','corrected');
          const error=await bad;check(error.code==='worker-timeout','non-yielding deck did not time out');
          const result=await good;check(result.manifest.title==='corrected' && result.warnings.includes('dark'),'independent deck/config did not recover');
          check(terminations>=1,'stuck worker was not terminated');
        }""")

    def test_real_worker_explicit_stop_and_trap_allow_immediate_deck_retry(self):
        self.page.evaluate(r"""async () => {
          const api=workerApi();const old=api.renderDeck('stop','hang').catch(e=>e);
          await waitFor(()=>entered);api.cancelPending();check((await old).code==='cancelled','active deck not cancelled');
          check((await api.renderDeck('retried','fresh')).manifest.title==='fresh','cancelled worker reused');
          const before=terminations;const trap=await api.renderDeck('trapped','trap').catch(e=>e);
          check(trap.code==='worker-error' && terminations>before,'trapped WASM retained');
          check((await api.renderDeck('aftertrap','next')).manifest.title==='next','trap recovery failed');
        }""")

    def test_missing_optional_export_does_not_disable_ordinary_worker_parsing(self):
        self.page.evaluate(r"""async () => {
          const url=moduleUrl(`
            export function parse(){return {ir:{diagram_type:'Flowchart',diagnostics:[]},warnings:[]};}
            export function renderSvg(){return '<svg/>';}
            export function validateConfig(){return {errors:[]};}`);
          const api=createWorkerMermaid({engineUrl:url,document});apis.push(api);
          const error=await api.renderDeck('missing','source').catch(e=>e);
          check(error.code==='engine-capability','missing deck export not explicit');
          check((await api.parse('flowchart LR\nA-->B')).diagramType==='flowchart-v2','optional export broke parsing');
        }""")

    def test_worker_bounds_deck_replies_without_poisoning_the_instance(self):
        self.page.evaluate(r"""async () => {
          const api=workerApi();let error=await api.renderDeck('large','oversized').catch(e=>e);
          check(/16 MiB/.test(error.message),'oversized native reply was accepted');
          check((await api.renderDeck('small','source')).manifest.slides.length===2,'limit failure poisoned the worker');
        }""")

    def test_worker_preserves_structured_native_diagnostics_and_absent_manifest(self):
        self.page.evaluate(r"""async () => {
          const api=workerApi(`output.warnings=[{severity:'Warning',message:'deck ignored',
            span:{start:{line:3,col:2,byte:12}},notes:['retained']}];delete output.manifest;`);
          const result=await api.renderDeck('missingdeck','source');
          check(result.manifest===null,'native absent manifest did not normalize');
          check(result.warnings[0].message==='deck ignored' && result.warnings[0].span.start.byte===12 &&
            result.warnings[0].notes[0]==='retained','structured diagnostics changed during transport');
        }""")

    def test_preaborted_deck_signal_never_loads_or_reports_a_syntax_error(self):
        self.page.evaluate(r"""async () => {
          const controller=new AbortController();controller.abort();let loads=0,reports=0;
          const api=createMermaid({loadEngine:async()=>{loads++;return engine;},document});apis.push(api);
          api.parseError=()=>reports++;
          const error=await api.renderDeck('aborted','source',undefined,{signal:controller.signal}).catch(e=>e);
          check(error.name==='AbortError' && error.code==='cancelled' && loads===0 && reports===0,'pre-abort entered pipeline');
          const invalid=await api.renderDeck('invalid','source',undefined,{signal:{}}).catch(e=>e);
          check(invalid instanceof TypeError && loads===0,'invalid signal entered pipeline');
        }""")

    def test_individual_deck_abort_terminates_active_wasm_but_keeps_siblings(self):
        self.page.evaluate(r"""async () => {
          const api=workerApi(),controller=new AbortController();
          const old=api.renderDeck('oneoperation','hang',undefined,{signal:controller.signal}).catch(e=>e);
          await waitFor(()=>entered);const sibling=api.renderDeck('sibling','retained');
          controller.abort();check((await old).code==='cancelled','individual abort did not settle');
          check((await sibling).manifest.title==='retained' && terminations===1,'independent queued deck was lost');
          check((await api.renderDeck('again','next')).manifest.title==='next','instance cannot be reused');
        }""")

    def test_aborted_deck_does_not_poison_shared_configuration_validation(self):
        self.page.evaluate(r"""async () => {
          const api=direct();apis.push(api);const controller=new AbortController();let release,validations=0;
          engine.validateConfig=()=>{validations++;return new Promise(resolve=>release=()=>resolve({errors:[]}));};
          const first=api.renderDeck('firstvalidation','first',undefined,{signal:controller.signal}).catch(e=>e);
          const second=api.renderDeck('secondvalidation','second');await waitFor(()=>release);
          controller.abort();check((await first).code==='cancelled','validation abort did not settle');release();
          check((await second).manifest.slides.length===2 && validations===1,'shared validation was poisoned');
          check(calls.filter(c=>c[0]==='deck').length===1 && calls[0][1]==='second','aborted source reached native render');
        }""")


if __name__ == "__main__":
    unittest.main(verbosity=2)
