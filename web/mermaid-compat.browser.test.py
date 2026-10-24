#!/usr/bin/env python3
"""Real Chromium DOM/CSS tests; the Rust engine boundary is an explicit fixture.

Run: python web/mermaid-compat.browser.test.py
Requires Playwright and an installed Chromium (CHROMIUM_PATH may override discovery).
These tests do not claim execution of the distributed Rust WASM binary.
"""
import json
import os
from pathlib import Path
import shutil
import unittest
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
SETUP = r"""async () => {
  const {createMermaid, prepareSvg} = window.compatModules;
  window.createMermaid = createMermaid;
  window.prepareSvg = prepareSvg;
  window.sampleSvg = color => `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
    viewBox="0 0 160 90" role="img" aria-labelledby="title description">
    <title id="title">Café 😀</title><desc id="description">A to B</desc>
    <defs><linearGradient id="base"><stop stop-color="red"/></linearGradient>
    <linearGradient id="gradient" xlink:href="#base"/>
    <marker id="arrow" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="4" markerHeight="4"><path d="M0 0 L10 5 L0 10z"/></marker></defs>
    <style>.fm-node {fill:${color};stroke:url(#gradient)}
      .fm-edge {marker-end:url('#arrow')}
      #node {stroke-width:3}
      @keyframes pulse {from{opacity:1}to{opacity:0.5}}
      @media (min-width: 1px) {.fm-node {animation:pulse 100s linear infinite}}
      @supports (display:block) {text {font-size:14px}}
    </style>
    <rect id="node" class="fm-node" x="10" y="10" width="50" height="30"/>
    <path class="fm-edge" d="M60 25 L100 25" stroke="black"/>
    <a href="https://example.com/docs" target="_blank"><text x="12" y="30">東京</text></a>
    <use href="#node" transform="translate(80 0)"/>
  </svg>`;
  window.engineFixture = (overrides = {}) => ({
    default: async () => {}, validateConfig: () => '{"errors":[]}',
    parse: () => ({ir:{diagram_type:'Flowchart',diagnostics:[]},warnings:[]}),
    renderSvg: (source,config) => sampleSvg(config.theme === 'dark' ? 'blue' : 'red'),
    ...overrides,
  });
  window.check = (condition, message) => { if (!condition) throw new Error(message); };
}"""

class BrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get("CHROMIUM_PATH") or shutil.which("chromium") or shutil.which("google-chrome")
        cls.browser = cls.playwright.chromium.launch(headless=True, executable_path=executable, args=["--no-sandbox"])

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.page = self.browser.new_page()
        self.addCleanup(self.page.close)
        self.page.set_content('<!doctype html><html><body><main></main></body></html>')
        # Load the exact production modules through local blob URLs: no HTTP server or network
        # permission is needed. Only the import specifier changes, not implementation code.
        self.page.evaluate("""async ({svg, compat}) => {
          const svgURL=URL.createObjectURL(new Blob([svg],{type:'text/javascript'}));
          const source=compat.replace('"./mermaid-svg.mjs"',JSON.stringify(svgURL));
          const compatURL=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));
          window.compatModules={...await import(svgURL),...await import(compatURL)};
          URL.revokeObjectURL(svgURL);URL.revokeObjectURL(compatURL);
        }""", {"svg": (ROOT / "web/mermaid-svg.mjs").read_text(),
               "compat": (ROOT / "web/mermaid-compat.mjs").read_text()})
        self.page.evaluate(SETUP)

    def test_render_is_nonmutating_and_preserves_geometry_text_and_accessibility(self):
        self.page.evaluate(r"""async () => {
          const api = createMermaid({loadEngine:async()=>engineFixture(), document});
          const source = 'flowchart LR\r\nA["café 😀"]-->B';
          const host = document.querySelector('main'); host.textContent = 'do not change';
          const result = await api.render('example',source,host);
          check(host.textContent==='do not change','render mutated its container');
          check(result.diagramType==='flowchart-v2','wrong public diagram type');
          host.innerHTML=result.svg;
          const svg=host.querySelector('svg'), rect=svg.querySelector('rect');
          check(svg.id==='example','root ID missing');
          check(svg.getAttribute('viewBox')==='0 0 160 90','viewBox changed');
          check(rect.getAttribute('x')==='10' && rect.getAttribute('width')==='50','geometry changed');
          check(svg.querySelector('text').textContent==='東京','label changed');
          for (const id of svg.getAttribute('aria-labelledby').split(' ')) check(svg.querySelector(`[id="${id}"]`),'missing accessibility target');
          check(svg.querySelector('title').textContent==='Café 😀','title changed');
          check(svg.querySelector('use').getAttribute('href')==='#'+rect.id,'use reference broken');
          check(svg.querySelector('a').getAttribute('rel')==='noopener noreferrer','unsafe new-tab link');
        }""")

    def test_multiple_diagrams_keep_separate_themes_ids_markers_and_animations(self):
        self.page.evaluate(r"""async () => {
          const api=createMermaid({loadEngine:async()=>engineFixture(),document});
          api.initialize({theme:'default'}); const first=await api.render('first','A-->B');
          api.initialize({theme:'dark'}); const second=await api.render('second','A-->B');
          document.body.innerHTML='<div class="fm-node" id="outside">Outside</div>'+first.svg+second.svg;
          const a=document.querySelector('#first rect'),b=document.querySelector('#second rect');
          check(getComputedStyle(a).fill==='rgb(255, 0, 0)','first theme leaked');
          check(getComputedStyle(b).fill==='rgb(0, 0, 255)','second theme leaked');
          check(getComputedStyle(document.querySelector('#outside')).fill==='rgb(0, 0, 0)','SVG stylesheet escaped root');
          check(getComputedStyle(a).animationName!==getComputedStyle(b).animationName,'keyframe names collide');
          check(getComputedStyle(a).strokeWidth==='3px','ID selector lost');
          const ids=[...document.querySelectorAll('[id]')].map(n=>n.id);
          check(new Set(ids).size===ids.length,'duplicate IDs');
          for(const svg of document.querySelectorAll('svg')) {
            for(const node of svg.querySelectorAll('[marker-end],[stroke],.fm-edge')) {
              for(const value of [node.getAttribute('stroke'),getComputedStyle(node).markerEnd]) {
                const match=value?.match(/#([^"')]+)/); if(match)check(svg.querySelector(`[id="${match[1]}"]`),'reference escaped its diagram');
              }
            }
          }
        }""")

    def test_inline_paint_and_animation_references_are_rewritten_once(self):
        self.page.evaluate(r"""() => {
          const svg=sampleSvg('red').replace('class="fm-node"','class="fm-node" style="fill:url(#gradient);animation:pulse 3s"');
          const output=prepareSvg(svg,'inline',document); document.body.append(output.element);
          const node=output.element.querySelector('rect');
          check(node.style.fill.includes('#inline--'),'inline paint not rewritten');
          check(node.style.animationName==='inline--animation-0','inline keyframe not rewritten');
          const noStyles=prepareSvg('<svg xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="g"/></defs><rect style="fill:url(#g)"/></svg>','nostyle',document);
          check(noStyles.svg.includes('nostyle--0'),'inline URL without stylesheet not rewritten');
        }""")

    def test_active_svg_and_remote_resources_are_rejected_before_insertion(self):
        self.page.evaluate(r"""() => {
          const wrap=body=>`<svg xmlns="http://www.w3.org/2000/svg">${body}</svg>`;
          for(const body of ['<script>alert(1)</script>','<foreignObject/>','<rect onclick="alert(1)"/>',
            '<a href="javascript:alert(1)"><text>bad</text></a>','<use href="https://example.com/x.svg#x"/>',
            '<image href="https://example.com/x.png"/>','<rect style="fill:url(https://example.com/x)"/>',
            '<style>@import "https://example.com/x.css";</style>',
            '<rect style="background-image:image-set(\'https://example.com/x\' 1x)"/>',
            '<set attributeName="href" to="javascript:alert(1)"/>']) {
            let failed=false;try{prepareSvg(wrap(body),'bad',document);}catch{failed=true;}
            check(failed,'unsafe SVG accepted: '+body);
          }
          check(!document.querySelector('svg'),'validation mutated DOM');
        }""")

    def test_invalid_documents_duplicate_ids_and_unresolved_references_fail(self):
        self.page.evaluate(r"""() => {
          for(const svg of ['<svg>', '<html/>', '<svg xmlns="wrong"/>',
            '<!DOCTYPE svg><svg xmlns="http://www.w3.org/2000/svg"/>',
            '<svg xmlns="http://www.w3.org/2000/svg"><g id="dup"/><g id="dup"/></svg>',
            '<svg xmlns="http://www.w3.org/2000/svg" aria-labelledby="missing"/>',
            '<svg xmlns="http://www.w3.org/2000/svg"><rect fill="url(#missing)"/></svg>']) {
            let failed=false;try{prepareSvg(svg,'bad',document);}catch{failed=true;}check(failed,'invalid SVG accepted: '+svg);
          }
        }""")

    def test_render_configuration_isolated_between_concurrent_instances(self):
        self.page.evaluate(r"""async () => {
          const engine=engineFixture();let resolve;const gate=new Promise(r=>resolve=r);
          const a=createMermaid({loadEngine:async()=>{await gate;return engine},document});
          const b=createMermaid({loadEngine:async()=>engine,document});
          a.initialize({theme:'dark'});b.initialize({theme:'default'});
          const old=a.render('old','A-->B');a.initialize({theme:'default'});
          const other=b.render('other','C-->D');resolve();
          const results=await Promise.all([old,other]);document.body.innerHTML=results.map(r=>r.svg).join('');
          check(getComputedStyle(document.querySelector('#old rect')).fill==='rgb(0, 0, 255)','queued config changed');
          check(getComputedStyle(document.querySelector('#other rect')).fill==='rgb(255, 0, 0)','config crossed instance boundary');
        }""")

    def test_run_processes_selected_diagrams_once_and_preserves_original_engine_input(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid" id="a"></pre><pre class="mermaid" id="b"></pre>';
          const a=document.getElementById('a'), b=document.getElementById('b');
          const source='%% café 😀\nflowchart LR\nA["東京"]-->B';a.textContent=source;b.textContent='flowchart LR\nC-->D';
          const calls=[],callbacks=[];let loads=0;
          const engine=engineFixture({renderSvg:(s)=>{calls.push(s);return sampleSvg('red')}});
          const api=createMermaid({loadEngine:async()=>{loads++;return engine},document});
          await api.run({postRenderCallback:async(id)=>{await Promise.resolve();callbacks.push(id)}});
          check(calls.length===2 && calls[0]===source,'source was changed or missing');
          check(a.dataset.processed==='true' && b.dataset.processed==='true','completion marker missing');
          check(a.id==='a' && b.id==='b','host identity changed');
          check(callbacks.length===2 && callbacks[0]!==callbacks[1],'callback identities collide');
          for(const id of callbacks)check(document.getElementById(id)?.localName==='svg','callback ran before insertion');
          await api.run();check(calls.length===2 && loads===1,'processed SVG was reparsed');
          b.textContent='flowchart LR\nNew-->Node';b.removeAttribute('data-processed');
          await api.run();check(calls.length===3 && calls[2].includes('New-->Node'),'explicit replacement did not rerender');
        }""")

    def test_explicit_nodes_override_selectors_and_empty_runs_do_not_load_wasm(self):
        self.page.evaluate(r"""async () => {
          let loads=0;const api=createMermaid({loadEngine:async()=>{loads++;return engineFixture()},document});
          await api.run();await api.run({nodes:[],querySelector:'['});check(loads===0,'empty run loads engine');
          document.body.innerHTML='<pre class="mermaid">A-->B</pre><pre class="chosen">C-->D</pre>';
          const chosen=document.querySelector('.chosen');
          await api.run({nodes:[chosen,chosen],querySelector:'['});
          check(chosen.querySelector('svg'),'explicit target not rendered');
          check(!document.querySelector('.mermaid svg'),'selector unexpectedly overrode nodes');
          const detached=document.createElement('pre');detached.textContent='E-->F';
          await api.run({nodes:[detached]});check(detached.querySelector('svg'),'explicit detached host failed');
          await api.run({querySelector:'.mermaid'});check(document.querySelector('.mermaid svg'),'custom selector failed');
        }""")

    def test_failed_diagrams_keep_source_while_siblings_render_and_retry(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid">A-->B</pre><pre class="mermaid"><code>BROKEN</code></pre><pre class="mermaid">C-->D</pre>';
          const bad=document.querySelectorAll('pre')[1],before=bad.innerHTML,errors=[];
          const engine=engineFixture({parse:(source)=>({ir:{diagram_type:'Flowchart',diagnostics:source==='BROKEN'?[{severity:'Error',message:'bad source'}]:[]},warnings:[]})});
          const api=createMermaid({loadEngine:async()=>engine,document});api.parseError=e=>errors.push(e);
          let rejected;try{await api.run()}catch(e){rejected=e}
          check(rejected instanceof AggregateError && rejected.errors.length===1,'missing aggregate failure');
          check(errors.length===1 && errors[0].message==='bad source','native error lost');
          check(document.querySelectorAll('pre svg').length===2,'valid siblings lost');
          check(bad.innerHTML===before && !bad.hasAttribute('data-processed'),'failed source overwritten/marked');
          bad.textContent='Fixed-->Node';await api.run();check(document.querySelectorAll('pre svg').length===3,'retry failed');
        }""")

    def test_suppressed_errors_are_reported_without_hiding_valid_diagrams(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid">BROKEN</pre><pre class="mermaid">A-->B</pre>';
          const reported=[],engine=engineFixture({renderSvg:s=>{if(s==='BROKEN')throw Error('failed render');return sampleSvg('red')}});
          const api=createMermaid({loadEngine:async()=>engine,document,reportError:e=>{reported.push(e);throw Error('logger failed')}});
          api.parseError=()=>{throw Error('suppressed error called parseError')};
          await api.run({suppressErrors:true});
          check(reported.length===1 && reported[0].message==='failed render','suppressed failures not reported');
          check(document.querySelectorAll('svg').length===1,'failure blocked sibling');
          check(document.querySelector('pre').textContent==='BROKEN','bad source not retained');
        }""")

    def test_loading_races_cannot_overwrite_changed_later_or_detached_sources(self):
        self.page.evaluate(r"""async () => {
          for(const change of ['first','later','detached','same-text-new-node']) {
            document.body.innerHTML='<pre class="mermaid">A-->B</pre><pre class="mermaid">C-->D</pre>';
            const [a,b]=document.querySelectorAll('pre');let release;const gate=new Promise(r=>release=r);
            const api=createMermaid({loadEngine:async()=>{await gate;return engineFixture()},document});
            const pending=api.run().catch(e=>e);
            if(change==='first')a.textContent='USER EDIT';
            if(change==='later')b.textContent='USER EDIT';
            if(change==='detached')a.remove();
            if(change==='same-text-new-node')a.replaceChildren(document.createTextNode('A-->B'));
            release();const error=await pending;
            check(error instanceof AggregateError,'race did not reject: '+change);
            const target=change==='later'?b:a;
            check(!target.querySelector('svg') && !target.hasAttribute('data-processed'),'stale source overwritten: '+change);
            check((change==='later'?a:b).querySelector('svg'),'independent sibling suppressed by race');api.dispose();
          }
        }""")

    def test_concurrent_runs_coalesce_shared_nodes_and_callbacks_can_reenter(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid">A-->B</pre><pre class="mermaid">C-->D</pre>';
          let release;const gate=new Promise(r=>release=r);const calls=[],callbacks=[];
          const engine=engineFixture({renderSvg:s=>{calls.push(s);return sampleSvg('red')}});
          const api=createMermaid({loadEngine:async()=>{await gate;return engine},document});
          const first=api.run({postRenderCallback:async id=>{callbacks.push(id);await api.run()}});
          const second=api.run({postRenderCallback:id=>callbacks.push(id)});
          release();await Promise.all([first,second]);
          check(calls.length===2,'overlapping runs rendered nodes twice');
          check(new Set(callbacks).size===callbacks.length,'callback invoked twice per inserted SVG');
          check(document.querySelectorAll('svg').length===2,'reentrant run lost a diagram');
        }""")

    def test_run_snapshots_config_and_callback_and_refuses_competing_ownership(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid">A-->B</pre>';
          let release;const gate=new Promise(r=>release=r),callbacks=[];
          const api=createMermaid({loadEngine:async()=>{await gate;return engineFixture()},document});
          api.initialize({theme:'dark'});const options={postRenderCallback:id=>callbacks.push(id)};
          const first=api.run(options);options.postRenderCallback=()=>{throw Error('callback mutation leaked')};
          api.initialize({theme:'default'});const second=api.run().catch(e=>e);
          release();await first;
          check((await second) instanceof AggregateError,'competing config was silently accepted');
          check(callbacks.length===1,'captured callback not called');
          check(getComputedStyle(document.querySelector('rect')).fill==='rgb(0, 0, 255)','captured config changed');
        }""")

    def test_callback_failures_do_not_rollback_or_block_sibling_diagrams(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid">A-->B</pre><pre class="mermaid">C-->D</pre>';
          let count=0;const api=createMermaid({loadEngine:async()=>engineFixture(),document});
          const failure=await api.run({postRenderCallback:()=>{count++;if(count===1)throw Error('callback failed')}}).catch(e=>e);
          check(failure instanceof AggregateError && failure.errors[0].message==='callback failed','callback failure lost');
          check(document.querySelectorAll('svg').length===2 && count===2,'callback blocked siblings or removed committed diagram');
          await api.run();check(count===2,'processed node invoked callback again');
        }""")

    def test_invalid_targets_and_limits_fail_before_any_dom_write(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<div class="mermaid" id="parent">A-->B<pre class="mermaid" id="child">C-->D</pre></div>';
          let loads=0;const api=createMermaid({loadEngine:async()=>{loads++;return engineFixture()},document});
          const original=document.body.innerHTML,parent=document.getElementById('parent');
          for(const options of [null,{nodes:{}},{nodes:[document]},{querySelector:'['},{postRenderCallback:true},
            {nodes:Array(65).fill(parent)},{}]) {
            let error;try{await api.run(options)}catch(e){error=e}
            check(error,'invalid options accepted');check(document.body.innerHTML===original,'invalid selection caused a partial write');
          }
          check(loads===0,'invalid selection loaded WASM');
          const nodes=Array.from({length:9},()=>{const n=document.createElement('pre');n.textContent='x'.repeat(2*1024*1024);return n});
          let error;try{await api.run({nodes})}catch(e){error=e}
          check(error instanceof RangeError && loads===0,'aggregate source limit not checked before load');
        }""")

    def test_default_startup_is_deferred_configurable_and_idempotent(self):
        self.page.evaluate(r"""async () => {
          const tick=()=>new Promise(r=>setTimeout(r,20));
          document.body.innerHTML='<pre class="mermaid">A-->B</pre>';let loads=0;
          const disabled=createMermaid({autoStart:true,loadEngine:async()=>{loads++;return engineFixture()},document});
          disabled.initialize({startOnLoad:false});await tick();
          check(loads===0 && !document.querySelector('svg'),'synchronous disable lost startup race');
          await disabled.run();check(loads===1 && document.querySelector('svg'),'explicit run was disabled');disabled.dispose();
          document.body.innerHTML='<pre class="mermaid">C-->D</pre>';
          const auto=createMermaid({autoStart:true,loadEngine:async()=>{loads++;return engineFixture()},document});
          auto.initialize({theme:'dark'});await tick();
          check(getComputedStyle(document.querySelector('rect')).fill==='rgb(0, 0, 255)','automatic startup missed config');
          await auto.contentLoaded();check(loads===2,'contentLoaded rendered twice');auto.dispose();
        }""")

    def test_loading_document_startup_and_disposal_clean_up_without_publication(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<pre class="mermaid">A-->B</pre>';let loads=0;
          Object.defineProperty(document,'readyState',{configurable:true,get:()=> 'loading'});
          const api=createMermaid({autoStart:true,loadEngine:async()=>{loads++;return engineFixture()},document});
          await new Promise(r=>setTimeout(r,10));check(loads===0,'rendered before DOMContentLoaded');
          document.dispatchEvent(new Event('DOMContentLoaded'));await new Promise(r=>setTimeout(r,20));
          check(loads===1 && document.querySelector('svg'),'DOM-ready handler failed');api.dispose();
          document.body.innerHTML='<pre class="mermaid">C-->D</pre>';
          const cancelled=createMermaid({autoStart:true,loadEngine:async()=>{loads++;return engineFixture()},document});
          cancelled.dispose();document.dispatchEvent(new Event('DOMContentLoaded'));await new Promise(r=>setTimeout(r,20));
          check(loads===1 && !document.querySelector('svg'),'disposed startup was not cleaned up');
          delete document.readyState;
          let release;const gate=new Promise(r=>release=r);
          const pendingApi=createMermaid({loadEngine:async()=>{await gate;return engineFixture()},document});
          const pending=pendingApi.run().catch(e=>e);pendingApi.dispose();release();
          check((await pending) instanceof AggregateError,'disposed in-flight run succeeded');
          check(!document.querySelector('svg') && document.querySelector('pre').textContent==='C-->D','disposed run overwrote source');
        }""")

    def test_generated_ids_avoid_existing_document_ids(self):
        self.page.evaluate(r"""async () => {
          document.body.innerHTML='<div id="fm-mermaid-1">Unrelated</div><pre class="mermaid">A-->B</pre>';
          const api=createMermaid({loadEngine:async()=>engineFixture(),document});await api.run();
          check(document.getElementById('fm-mermaid-1').textContent==='Unrelated','existing host ID overwritten');
          const ids=[...document.querySelectorAll('[id]')].map(n=>n.id);check(new Set(ids).size===ids.length,'generated ID collision');
        }""")

if __name__ == "__main__":
    unittest.main(verbosity=2)
