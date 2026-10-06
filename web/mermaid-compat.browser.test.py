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

if __name__ == "__main__":
    unittest.main(verbosity=2)
