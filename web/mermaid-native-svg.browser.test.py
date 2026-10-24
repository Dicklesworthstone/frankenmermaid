#!/usr/bin/env python3
"""Native renderer SVG -> production browser adapter, without a substitute SVG renderer.

Uses unchanged Rust-renderer goldens, plus hostile inputs and independent pixel comparisons.
It does not run the Rust/WASM compiler or engine. Run from any directory with Playwright and
Chromium installed. FM_SVG_MODULE can select the original adapter to reproduce the regressions.
"""
import os
from pathlib import Path
import shutil
import unittest
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
GOLDENS = ROOT / "crates/fm-cli/tests/golden"
NATIVE_FONT = "@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');"
SVG_MODULE = Path(os.environ.get("FM_SVG_MODULE", ROOT / "web/mermaid-svg.mjs"))
MODULE = SVG_MODULE.read_text()


class NativeSvgTests(unittest.TestCase):
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
        self.context = self.browser.new_context(viewport={"width": 800, "height": 600})
        self.requests, self.errors = [], []
        self.context.route("**/*", self.route)
        self.context.on("page", lambda page: page.on("pageerror", lambda error: self.errors.append(str(error))))
        self.page = self.context.new_page()
        self.page.set_content('<!doctype html><html><body><main id="host"></main></body></html>')
        self.page.evaluate("""async source => {
          const url=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));
          try { window.adapter = await import(url); } finally { URL.revokeObjectURL(url); }
          window.check = (value, reason) => { if (!value) throw new Error(reason); };
          window.wrap = body => `<svg xmlns="http://www.w3.org/2000/svg">${body}</svg>`;
        }""", MODULE)

    def route(self, route):
        if route.request.url.startswith(("http:", "https:")):
            self.requests.append(route.request.url)
            route.abort()
        else:
            route.continue_()

    def tearDown(self):
        self.context.close()
        self.assertEqual(self.errors, [], "browser page errors")
        self.assertEqual(self.requests, [], "adapter attempted a network request")

    def test_native_architecture_and_pie_preserve_all_content_and_geometry(self):
        for name, expected_nodes, expected_edges in [("architecture_basic", 3, 2), ("pie_basic", 3, 0)]:
            with self.subTest(name=name):
                svg = (GOLDENS / f"{name}.svg").read_text()
                self.page.evaluate(r"""({svg, name, expectedNodes, expectedEdges}) => {
                  const before = new DOMParser().parseFromString(svg, 'image/svg+xml').documentElement;
                  const result = adapter.prepareSvg(svg, name, document);
                  const after = result.element;
                  const content = root => [root, ...root.querySelectorAll('*')].filter(n=>n.localName!=='style').map(n=>({
                    tag:n.localName,
                    attributes:[...n.attributes].filter(a=>!['id','marker-end','marker-start','href'].includes(a.localName))
                      .map(a=>[a.name,a.value]).sort(),
                    text:[...n.childNodes].filter(n=>n.nodeType===3).map(n=>n.nodeValue)
                  }));
                  check(JSON.stringify(content(before))===JSON.stringify(content(after)), 'native geometry/text/metadata changed');
                  check(after.getAttribute('data-nodes')===String(expectedNodes), 'fixture node count');
                  check(after.getAttribute('data-edges')===String(expectedEdges), 'fixture edge count');
                  check(after.querySelectorAll('path').length>0 && after.querySelectorAll('text').length>=3, 'empty fixture');
                  check(after.querySelectorAll('[id]').length===before.querySelectorAll('[id]').length, 'lost an addressable element');
                  check(result.presentationWarnings.length===1 && /Inter/.test(result.presentationWarnings[0]), 'font fallback not reported');
                  check(!result.svg.includes('@import') && !result.svg.includes('fonts.googleapis.com'), 'font resource survived');
                  document.getElementById('host').replaceChildren(after);
                  for (const path of after.querySelectorAll('[marker-end]')) {
                    const match=path.getAttribute('marker-end').match(/#([^"')]+)/);
                    check(match && after.querySelector(`[id="${match[1]}"]`)?.localName==='marker', 'arrow reference lost');
                    check(getComputedStyle(path).stroke!=='none','edge became invisible');
                  }
                  if (name==='pie_basic') check(after.querySelectorAll('.fm-pie-slice').length===3, 'pie slices lost');
                }""", {"svg": svg, "name": name, "expectedNodes": expected_nodes, "expectedEdges": expected_edges})

    def test_native_pixels_equal_the_independent_local_font_reference(self):
        for name in ["architecture_basic", "pie_basic"]:
            with self.subTest(name=name):
                svg = (GOLDENS / f"{name}.svg").read_text()
                reference = self.context.new_page()
                reference.set_content('<!doctype html><style>html,body{margin:0}</style>')
                # This reference does NOT use the adapter: remove the exact optional font rule
                # independently in the XML DOM, then render the otherwise unchanged native SVG.
                reference.evaluate("""({svg, font}) => {
                  const root=new DOMParser().parseFromString(svg,'image/svg+xml').documentElement;
                  for(const style of root.querySelectorAll('style')) style.textContent=style.textContent.replace(font,'');
                  root.style.width='800px';root.style.height='600px';
                  document.body.append(document.importNode(root,true));
                }""", {"svg": svg, "font": NATIVE_FONT})
                reference.evaluate("document.fonts.ready")
                expected = reference.screenshot(animations="disabled")
                self.page.evaluate("""svg => {
                  const root=adapter.prepareSvg(svg,'pixels',document).element;
                  document.documentElement.style.margin='0';document.body.style.margin='0';
                  root.style.width='800px';root.style.height='600px';
                  document.body.replaceChildren(root);
                }""", svg)
                self.page.evaluate("document.fonts.ready")
                actual = self.page.screenshot(animations="disabled")
                self.assertEqual(actual, expected, f"{name}: native pixel output changed")
                reference.close()

    def test_native_font_omission_is_reported_once_and_only_when_present(self):
        self.page.evaluate(r"""font => {
          font=font.replaceAll('&','&amp;');
          const prepared=adapter.prepareSvg(wrap(`<style>${font}${font}rect{fill:red}</style><rect width="10" height="10"/>`),'font',document);
          check(prepared.presentationWarnings.length===1,'duplicate font warning');
          check(!prepared.svg.includes('@import'),'import survived');
          const local=adapter.prepareSvg(wrap('<style>rect{fill:blue}</style><rect/>'),'local',document);
          check(local.presentationWarnings.length===0,'invented fallback');
          const comment=adapter.prepareSvg(wrap(`<style>/* ${font} */ rect{fill:blue}</style><rect/>`),'comment',document);
          check(comment.presentationWarnings.length===0,'comment treated as import');
          const single=adapter.prepareSvg(wrap(`<style>${font}</style><rect/>`),'single',document);
          check(single.presentationWarnings.length===1,'font-only stylesheet failed');
          const double=adapter.prepareSvg(wrap(`<style>${font.replaceAll("'",'"')}</style><rect/>`),'double',document);
          check(double.presentationWarnings.length===1,'quoted native declaration failed');
        }""", NATIVE_FONT)

    def test_other_external_stylesheets_and_disguised_native_imports_still_fail(self):
        self.page.evaluate(r"""font => {
          const css=[
            '@import "https://example.com/x.css";', '@import url(https://example.com/x.css);',
            font.replace('fonts.googleapis.com','fonts.googleapis.com.evil.test'),
            font.replace('https://','http://'), font.replace('https://','https://name:secret@'),
            font.replace('display=swap','display=swap&extra=1'), font.replace('display=swap','display=swap#extra'),
            font.replace(');',') screen;'), font.replace('400;500','400;501'),
            font.slice(0,-1), font.replace('@import','@im\\port'),
            `@media screen{${font}rect{fill:red}}`, `rect${font}{fill:red}`,
            `rect[data-x=${font}]{fill:red}`, `:is(rect,${font}){fill:red}`,
            `${font}@import "https://example.com/x.css";rect{fill:red}`,
            '@font-face{font-family:Bad;src:url(https://example.com/x.woff)}',
            '@namespace url(https://example.com/ns);', '/* unfinished', 'rect{font-family:"unfinished}'
          ];
          for(const text of css) {
            let failed=false;
            try{adapter.prepareSvg(wrap(`<style>${text.replaceAll('&','&amp;')}</style><rect/>`),'bad',document);}catch{failed=true;}
            check(failed,'unsupported stylesheet admitted: '+text);
          }
          check(!document.querySelector('svg'),'failure inserted SVG');
        }""", NATIVE_FONT)

    def test_native_font_rule_does_not_allow_active_markup_or_other_remote_resources(self):
        self.page.evaluate(r"""font => {
          for(const body of [
            '<script>alert(1)</script>', '<foreignObject/>', '<rect onclick="alert(1)"/>',
            '<a href="javascript:alert(1)"><text>bad</text></a>',
            '<image href="https://example.com/x.png"/>', '<use href="https://example.com/x.svg#node"/>',
            '<rect fill="url(https://example.com/paint)"/>', '<rect style="fill:url(https://example.com/paint)"/>',
            '<style>rect{fill:url(https://example.com/paint)}</style>',
            '<style>rect{background-image:image-set("https://example.com/x" 1x)}</style>',
            '<style>@keyframes bad{to{fill:url(https://example.com/paint)}}</style>',
            '<set attributeName="href" to="javascript:alert(1)"/>',
            '<g id="dup"/><g id="dup"/>', '<rect fill="url(#missing)"/>',
            '<g aria-labelledby="missing"/>'
          ]) {
            let failed=false;
            try{adapter.prepareSvg(wrap(`<style>${font.replaceAll('&','&amp;')}</style>${body}`),'bad',document);}catch{failed=true;}
            check(failed,'unsafe SVG accepted: '+body);
          }
          check(!document.querySelector('svg'),'failure inserted SVG');
        }""", NATIVE_FONT)

    def test_literal_labels_and_metadata_survive_dom_and_serialized_publication(self):
        self.page.evaluate(r"""() => {
          const labels=['Fetch url(https://example.com/diagram)', 'url(#node)',
            'image-set(x) @font-face :root #node', '東京 \\ путь <script> text'];
          for(const text of labels) {
            const input=new DOMParser().parseFromString(wrap('<g id="node"><text/></g>'),'image/svg+xml');
            const node=input.querySelector('g');
            for(const name of ['data-id','data-tooltip','aria-label','aria-description','aria-roledescription','title']) node.setAttribute(name,text);
            node.querySelector('text').textContent=text;
            const output=adapter.prepareSvg(new XMLSerializer().serializeToString(input),'literal',document);
            document.getElementById('host').innerHTML=output.svg;
            for(const root of [output.element,document.querySelector('#host svg')]) {
              const element=root.querySelector('g');
              for(const name of ['data-id','data-tooltip','aria-label','aria-description','aria-roledescription','title']) {
                check(element.getAttribute(name)===text,'literal attribute rewritten: '+name);
              }
              check(element.querySelector('text').textContent===text,'label changed');
            }
          }
        }""")

    def test_callbacks_receive_the_authored_url_like_source_identity(self):
        self.page.evaluate(r"""() => {
          const output=adapter.prepareSvg(wrap('<g id="node" data-id="url(#node)" data-callback="choose" aria-label="url(#node)"><rect x="10" y="10" width="60" height="40"/></g>'),'interactive',document);
          window.selections=[];
          const choose=id=>selections.push(id);
          const bind=adapter.createSvgBinder(output.element,{resolveCallback:name=>name==='choose'?choose:undefined});
          document.getElementById('host').replaceChildren(output.element);
          bind(output.element);
        }""")
        self.page.locator('g[data-callback="choose"]').click()
        self.page.locator('g[data-callback="choose"]').focus()
        self.page.keyboard.press("Enter")
        self.page.keyboard.press("Space")
        self.assertEqual(self.page.evaluate("selections"), ["url(#node)"] * 3)

    def test_css_strings_are_literal_but_real_paint_references_are_rewritten(self):
        self.page.evaluate(r"""() => {
          const literal='url(https://example.com/private) image-set(x) @font-face @namespace :root #paint';
          const output=adapter.prepareSvg(wrap(`<defs><linearGradient id="paint"><stop stop-color="red"/></linearGradient></defs>
            <style>rect{--literal:"${literal}";--local:url('#paint');--word:curl(#paint);fill:var(--local)}</style>
            <rect style='--literal:"${literal}";stroke:url(#paint)' width="40" height="40"/>`),'values',document);
          document.getElementById('host').innerHTML=output.svg;
          const rect=document.querySelector('#host rect'), style=getComputedStyle(rect);
          check(style.getPropertyValue('--literal')==='"'+literal+'"','quoted CSS text changed');
          check(style.getPropertyValue('--word')==='curl(#paint)','identifier substring changed');
          const paint=document.querySelector('#host linearGradient').id;
          check(style.fill.includes('#'+paint) && style.stroke.includes('#'+paint),'real URL not rebound');
          const comment=adapter.prepareSvg(wrap('<style>/* @font-face @namespace */ rect{fill:red}</style><rect/>'),'comment',document);
          check(comment.presentationWarnings.length===0,'comment interpreted as a resource');
        }""")

    def test_hash_and_root_text_in_attribute_selectors_is_not_rewritten(self):
        self.page.evaluate(r"""() => {
          const output=adapter.prepareSvg(wrap('<style>:root{--color:red}[data-id="#node"]{fill:var(--color)}[aria-label=":root"]{stroke:blue}#node{stroke-width:7}</style><rect id="node" data-id="#node" aria-label=":root" width="10" height="10"/>'),'selectors',document);
          document.getElementById('host').innerHTML=output.svg;
          const rect=document.querySelector('#host rect'), style=getComputedStyle(rect);
          check(style.fill==='rgb(255, 0, 0)','source-ID selector lost');
          check(style.stroke==='rgb(0, 0, 255)','literal :root selector lost');
          check(style.strokeWidth==='7px','actual ID selector lost');
          check(rect.getAttribute('data-id')==='#node' && rect.getAttribute('aria-label')===':root','metadata changed');
        }""")

    def test_accessibility_relationships_resolve_inside_each_diagram(self):
        self.page.evaluate(r"""() => {
          const names=['aria-labelledby','aria-describedby','aria-controls','aria-owns',
            'aria-flowto','aria-details','aria-errormessage','aria-activedescendant'];
          const source=wrap(`<g id="node" ${names.map(n=>`${n}="target"`).join(' ')}/><text id="target">Details</text>`);
          for(const id of ['first','second']) {
            const output=adapter.prepareSvg(source,id,document);
            document.getElementById('host').append(output.element);
            for(const name of names) check(output.element.querySelector('g').getAttribute(name)===output.element.querySelector('text').id,'broken '+name);
          }
          for(const name of names) {
            let failed=false;
            try{adapter.prepareSvg(wrap(`<g ${name}="missing"/>`),'missing',document);}catch{failed=true;}
            check(failed,'unresolved '+name+' accepted');
            const empty=adapter.prepareSvg(wrap(`<g ${name}=""/>`),'empty',document);
            check(empty.element.querySelector('g').getAttribute(name)==='', 'empty IDREF changed');
          }
          const ids=[...document.querySelectorAll('[id]')].map(n=>n.id);
          check(new Set(ids).size===ids.length,'relationship scopes collide');
        }""")

    def test_resource_escapes_case_folding_and_processing_instructions_are_rejected(self):
        self.page.evaluate(r"""() => {
          const bodies=[
            '<rect fill="u\\72l(https://example.com/p)"/>', '<rect filter="u\\000072l(https://example.com/p)"/>',
            '<rect cursor="u\\72 l(https://example.com/p),auto"/>',
            '<STYLE>@import "https://example.com/x.css";</STYLE>',
            '<foreignobject><img src="https://example.com/x.png"/></foreignobject>',
            '<sCrIpT href="https://example.com/x.js"/>', '<a HREF="javascript:alert(1)"/>',
            '<rect STYLE="fill:url(https://example.com/p)"/>', '<g ID="upper"/>',
            '<?xml-stylesheet type="text/css" href="https://example.com/x.css"?><rect/>',
            '<rect marker-end="url(https://example.com/p)"/>', '<rect mask="url(#missing)"/>',
            '<rect style="fill:url(#missing)"/>', '<rect fill="url(&quot;#missing&quot;"/>'
          ];
          for(const body of bodies) {
            let failed=false;
            try{adapter.prepareSvg(wrap(body),'bad',document);}catch{failed=true;}
            check(failed,'resource policy bypass: '+body);
          }
          let failed=false;
          try{adapter.prepareSvg('<?xml-stylesheet type="text/css" href="https://example.com/x.css"?>'+wrap('<rect/>'),'bad',document);}catch{failed=true;}
          check(failed,'document-level processing instruction accepted');
          check(!document.querySelector('svg'),'failed result was inserted');
        }""")

    def test_local_styles_and_markers_remain_isolated_with_native_font_fallback(self):
        self.page.evaluate(r"""font => {
          const make=(id,color)=>adapter.prepareSvg(wrap(`<style>${font.replaceAll('&','&amp;')}
            :root{--paint:${color}} rect{fill:var(--paint)} #node{stroke-width:3}
            @keyframes pulse{from{opacity:1}to{opacity:0.5}}
            @media (min-width:1px){rect{animation:pulse 100s linear infinite}}
            @supports (display:block){path{stroke:var(--paint)}}
            </style><defs><marker id="arrow"><path d="M0 0L5 3L0 6Z"/></marker></defs>
            <rect id="node" width="20" height="20"/><path marker-end="url(#arrow)" d="M0 0L50 50"/>`),id,document).element;
          const first=make('first','red'), second=make('second','blue');
          const outside=document.createElementNS('http://www.w3.org/2000/svg','svg');
          outside.innerHTML='<rect id="outside" width="1" height="1"/>';
          document.body.replaceChildren(first,second,outside);
          const a=first.querySelector('rect'),b=second.querySelector('rect');
          check(getComputedStyle(a).fill==='rgb(255, 0, 0)' && getComputedStyle(b).fill==='rgb(0, 0, 255)','themes leaked');
          check(getComputedStyle(document.getElementById('outside')).fill==='rgb(0, 0, 0)','stylesheet escaped');
          check(getComputedStyle(a).strokeWidth==='3px','ID selector lost');
          check(getComputedStyle(a).animationName!==getComputedStyle(b).animationName,'animation collision');
          const ids=[...document.querySelectorAll('[id]')].map(n=>n.id);
          check(new Set(ids).size===ids.length,'duplicate IDs');
          for(const svg of [first,second]) {
            const marker=svg.querySelector('[marker-end]').getAttribute('marker-end');
            check(marker.includes('#'+svg.querySelector('marker').id),'marker escaped its diagram');
          }
        }""", NATIVE_FONT)


if __name__ == "__main__":
    unittest.main(verbosity=2)
