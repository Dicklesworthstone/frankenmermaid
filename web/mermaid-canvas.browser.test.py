#!/usr/bin/env python3
"""Qualify Diagram selection using the actual compiled engine, not a native-boundary double.

Run the canonical build-wasm.sh first. This test fails, rather than skips or substitutes a
fixture, when the package or required exports are missing. It observes actual Canvas2D draw
coordinates and pixels, then drives the production Diagram and bindCanvasSelection APIs.
Only the loopback test server may receive network requests. No external assets are needed.
"""
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import threading
import unittest
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
REQUIRED = [
    ROOT / 'pkg/frankenmermaid.js', ROOT / 'pkg/frankenmermaid_bg.wasm',
    ROOT / 'web/mermaid.mjs', ROOT / 'web/mermaid-compat.mjs', ROOT / 'web/mermaid-svg.mjs',
]
SETUP = r'''async () => {
  const engine = await import('/pkg/frankenmermaid.js');
  await engine.default();
  const {bindCanvasSelection} = await import('/web/mermaid.mjs');
  const required = ['redraw','invalidateFrame','selectionState','selectNode','selectAt',
    'navigateSelection','clearSelection','beginSelectionGesture','previewSelectionBox',
    'previewSelectionLasso','commitSelectionGesture','cancelSelectionGesture','hitTestInteraction'];
  for (const name of required) if (typeof engine.Diagram?.prototype[name] !== 'function') {
    throw new Error(`Fresh WASM package required: Diagram.${name} is missing. Run build-wasm.sh.`);
  }
  window.engine = engine;
  window.bindCanvasSelection = bindCanvasSelection;
  window.check = (ok, why) => {if (!ok) throw new Error(why)};
  window.same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
  window.throws = fn => {let failed=false;try{fn()}catch{failed=true}check(failed,'expected rejection')};
  window.source = 'flowchart LR\nA[Alpha] --> B[Beta]\nclick A "https://example.com" "Alpha tip"\n';
  window.draws = [];
  window.controllers = [];
  window.diagrams = [];
  window.errors = [];
  window.addEventListener('unhandledrejection', e => errors.push(String(e.reason)));
  const record = prototype => {
    const original = prototype.fillText;
    prototype.fillText = function(text,x,y,...rest) {
      const p = this.getTransform().transformPoint({x,y});
      draws.push({canvas:this.canvas, text, x:p.x, y:p.y});
      return original.call(this,text,x,y,...rest);
    };
  };
  record(CanvasRenderingContext2D.prototype);
  if (typeof OffscreenCanvasRenderingContext2D !== 'undefined') record(OffscreenCanvasRenderingContext2D.prototype);
  window.make = (config={}, offscreen=false) => {
    const canvas = offscreen ? new OffscreenCanvas(600,300) : document.createElement('canvas');
    canvas.width=600;canvas.height=300;
    if (!offscreen) {
      canvas.style.cssText='width:300px;height:150px;border:7px solid black;padding:5px;box-sizing:content-box';
      document.body.append(canvas);
    }
    const diagram = offscreen ? engine.Diagram.fromOffscreenCanvas(canvas,config) : new engine.Diagram(canvas,config);
    diagrams.push(diagram);
    return {canvas,diagram};
  };
  window.label = (canvas,text) => {
    const found=draws.findLast(item=>item.canvas===canvas && item.text===text);
    check(found,'actual renderer never drew '+text); return found;
  };
  window.ids = diagram => diagram.selectionState().selectedNodeIds;
  window.pixels = canvas => Array.from(canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data);
  window.start = (diagram,mode='replace') => {
    const token=diagram.beginSelectionGesture(mode,'intersects');
    check(typeof token==='string' && /^[1-9][0-9]{0,19}$/.test(token),'token is not lossless decimal');return token;
  };
  window.previewAll = (diagram,token) => diagram.previewSelectionBox(token,-1,-1,601,301);
  window.bind = (canvas,diagram,tool='box') => {
    const controller=bindCanvasSelection({canvas,diagram,tool,onError:e=>errors.push(String(e))});
    controllers.push(controller);controller.refresh();return controller;
  };
}'''


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/__canvas_selection_test__':
            body = b'<!doctype html><html><body style="margin:20px"></body></html>'
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            super().do_GET()

    def log_message(self, *_args):
        pass


class CanvasEngineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        missing = [str(path) for path in REQUIRED if not path.is_file()]
        if missing:
            raise RuntimeError('Build the real WASM package before testing; missing: ' + ', '.join(missing))
        cls.server = http.server.ThreadingHTTPServer(
            ('127.0.0.1', 0), functools.partial(QuietHandler, directory=str(ROOT)))
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.addClassCleanup(cls.server.server_close)
        cls.addClassCleanup(cls.server.shutdown)
        cls.playwright = sync_playwright().start()
        cls.addClassCleanup(cls.playwright.stop)
        executable = os.environ.get('CHROMIUM_PATH') or shutil.which('chromium') or shutil.which('chromium-browser')
        cls.browser = cls.playwright.chromium.launch(executable_path=executable, args=['--no-sandbox'])
        cls.addClassCleanup(cls.browser.close)
        cls.origin = 'http://127.0.0.1:' + str(cls.server.server_port)

    def setUp(self):
        self.page = self.browser.new_page()
        self.addCleanup(self.page.close)
        self.page_errors = []
        self.external_requests = []
        self.page.on('pageerror', lambda error: self.page_errors.append(str(error)))
        def route(request):
            url = request.request.url
            if url.startswith(self.origin + '/'):
                request.continue_()
            else:
                self.external_requests.append(url)
                request.abort()
        self.page.route('**/*', route)
        # A blank same-origin page, not the showcase with its unrelated startup code.
        self.page.goto(self.origin + '/__canvas_selection_test__')
        self.page.evaluate(SETUP)

    def tearDown(self):
        if not hasattr(self, 'page'):
            return
        self.page.evaluate('''() => {
          (window.controllers||[]).forEach(c=>c.dispose());
          (window.diagrams||[]).forEach(d=>{d.destroy();d.free()});
        }''')
        self.assertEqual(self.page_errors, [])
        self.assertEqual(self.external_requests, [])
        self.assertEqual(self.page.evaluate('window.errors||[]'), [])

    def test_cold_and_destroyed_lifetimes(self):
        self.page.evaluate('''() => {
          const {diagram}=make();
          check(!diagram.selectionState().frameAvailable && ids(diagram).length===0,'cold state');
          check(diagram.redraw()===false && diagram.beginSelectionGesture()==null,'invented frame');
          throws(()=>diagram.selectAt(1,1,'typo'));
          throws(()=>diagram.navigateSelection('diagonal'));
          throws(()=>diagram.commitSelectionGesture('01'));
          diagram.destroy();diagram.destroy();
          for(const call of [()=>diagram.redraw(),()=>diagram.selectionState(),()=>diagram.selectNode('A'),
            ()=>diagram.selectAt(1,1),()=>diagram.clearSelection(),()=>diagram.invalidateFrame(),
            ()=>diagram.beginSelectionGesture(),()=>diagram.hitTestInteraction(1,1)]) throws(call);
        }''')

    def test_picking_tracks_actual_drawn_coordinates_under_fit_and_padding(self):
        self.page.evaluate('''() => {
          for(const autoFit of [true,false]) {
            const {canvas,diagram}=make({canvas:{autoFit,padding:17}});
            diagram.render(source);
            const p=label(canvas,'Alpha');
            check(diagram.hitTestNode(p.x,p.y)==='A','draw/pick viewport mismatch');
            const hit=diagram.hitTestInteraction(p.x,p.y);
            check(hit.nodeId==='A' && hit.tooltip==='Alpha tip' && hit.href==='https://example.com','metadata lost');
            check(diagram.selectAt(p.x,p.y,'replace') && same(ids(diagram),['A']),'pointer selection');
            for(const [x,y] of [[NaN,p.y],[-1,p.y],[canvas.width,p.y]]) {
              check(!diagram.selectAt(x,y,'replace') && same(ids(diagram),['A']),'invalid point stole selection');
            }
          }
        }''')

    def test_selection_redraw_changes_pixels_and_clear_restores_them(self):
        self.page.evaluate('''() => {
          const {canvas,diagram}=make();diagram.render(source);
          const original=pixels(canvas);
          diagram.selectNode('A');diagram.redraw();
          check(!same(original,pixels(canvas)),'selection ring did not draw');
          diagram.navigateSelection('right',true);diagram.redraw();
          check(same(ids(diagram),['A','B']) && diagram.selectionState().primaryNodeId==='B','keyboard extension');
          diagram.clearSelection();diagram.redraw();
          check(same(original,pixels(canvas)),'clearing selection changed base rendering');
        }''')

    def test_transactional_toggle_lasso_and_retired_tokens(self):
        self.page.evaluate('''() => {
          const {diagram}=make();diagram.render(source);diagram.selectNode('A');
          const token=start(diagram,'toggle');
          check(previewAll(diagram,token) && same(ids(diagram),['B']),'toggle baseline');
          check(!previewAll(diagram,token) && same(ids(diagram),['B']),'toggle accumulated');
          diagram.redraw();check(diagram.selectionState().activeGesture===token,'redraw revoked live gesture');
          check(diagram.cancelSelectionGesture(token) && same(ids(diagram),['A']),'rollback');
          const next=start(diagram,'add');
          const lasso=new Float64Array([-1,-1,601,-1,601,301,-1,301]);
          check(diagram.previewSelectionLasso(next,lasso) && same(ids(diagram),['A','B']),'lasso');
          check(!diagram.commitSelectionGesture(token) && diagram.selectionState().activeGesture===next,'stale token');
          const before=diagram.selectionState();
          for(const invalid of [new Float64Array(5),new Float64Array(8194),new Float64Array([0,0,1,1,NaN,2])]) {
            throws(()=>diagram.previewSelectionLasso(next,invalid));
            check(same(before,diagram.selectionState()),'invalid lasso mutated state');
          }
          check(diagram.commitSelectionGesture(next) && !diagram.cancelSelectionGesture(next),'commit did not retire');
        }''')

    def test_resize_theme_and_external_reset_rollback_provisional_selection(self):
        self.page.evaluate('''() => {
          for(const change of ['resize','theme','reset']) {
            const {canvas,diagram}=make();diagram.render(source);diagram.selectNode('A');
            const token=start(diagram);previewAll(diagram,token);check(ids(diagram).length===2,'preview control');
            if(change==='resize') canvas.width=1200;
            if(change==='theme') diagram.setTheme('dark');
            if(change==='reset') {canvas.width=canvas.width;diagram.invalidateFrame();}
            const state=diagram.selectionState();
            check(!state.frameAvailable && state.activeGesture==null && same(ids(diagram),['A']),'stale preview/frame');
            check(!diagram.commitSelectionGesture(token),'stale commit accepted');
            check(diagram.redraw(),'retained redraw unavailable');
            const p=label(canvas,'Alpha');check(diagram.hitTestNode(p.x,p.y)==='A','fresh viewport mismatch');
            canvas.width=0;check(!diagram.redraw() && !diagram.selectionState().frameAvailable,'zero size');
            canvas.width=600;check(diagram.redraw() && same(ids(diagram),['A']),'zero-size recovery');
          }
        }''')

    def test_source_replacement_retains_committed_not_preview_ids(self):
        self.page.evaluate('''() => {
          const {diagram}=make();diagram.render(source);diagram.selectNode('A');
          const token=start(diagram);previewAll(diagram,token);
          diagram.render('flowchart LR\\nB[Beta]');
          check(ids(diagram).length===0 && diagram.selectionState().activeGesture==null,'preview became committed');
          check(!diagram.commitSelectionGesture(token),'source replacement reused token');
          diagram.selectNode('B');diagram.render('flowchart LR\\nC[Gamma]-->B[Beta]');
          check(same(ids(diagram),['B']),'stable author ID lost during reindex');
        }''')

    def test_html_controller_maps_css_border_padding_and_keyboard_to_native(self):
        self.page.evaluate('''() => {
          const {canvas,diagram}=make();diagram.render(source);bind(canvas,diagram);
          const point=label(canvas,'Alpha'), rect=canvas.getBoundingClientRect();
          window.target={x:rect.left+12+point.x/2,y:rect.top+12+point.y/2};
          window.activeDiagram=diagram;
        }''')
        point = self.page.evaluate('target')
        self.page.mouse.click(point['x'], point['y'])
        self.assertEqual(self.page.evaluate('ids(activeDiagram)'), ['A'])
        self.page.keyboard.press('Shift+ArrowRight')
        self.assertEqual(self.page.evaluate('ids(activeDiagram)'), ['A', 'B'])
        self.page.keyboard.press('Escape')
        self.assertEqual(self.page.evaluate('ids(activeDiagram)'), [])

    def test_html_box_and_lasso_commit_and_escape_use_native_transactions(self):
        for tool in ['box', 'lasso']:
            with self.subTest(tool=tool):
                bounds = self.page.evaluate('''tool => {
                  const {canvas,diagram}=make();diagram.render(source);
                  diagram.selectNode('B');diagram.redraw();
                  const controller=bind(canvas,diagram,tool);
                  window.activeDiagram=diagram;window.activeController=controller;
                  const rect=canvas.getBoundingClientRect();
                  // Independent conversion for the fixture's 2x backing store and 12px
                  // border/padding. Stay inside its surface when requesting pointer capture.
                  return {left:rect.left+13,top:rect.top+13,
                    right:rect.left+311,bottom:rect.top+161};
                }''', tool)
                # Two drags traverse the full diagram; the first rolls back while captured,
                # the second commits on pointer-up. Real mouse events exercise the controller
                # and compiled session together, including the frozen preview baseline.
                for cancel in [True, False]:
                    self.page.mouse.move(bounds['left'], bounds['top'])
                    self.page.mouse.down()
                    if tool == 'lasso':
                        self.page.mouse.move(bounds['right'], bounds['top'], steps=3)
                        self.page.mouse.move(bounds['right'], bounds['bottom'], steps=3)
                        self.page.mouse.move(bounds['left'], bounds['bottom'], steps=3)
                    else:
                        self.page.mouse.move(bounds['right'], bounds['bottom'], steps=5)
                    self.page.wait_for_function('''() =>
                      activeDiagram.selectionState().activeGesture != null &&
                      same(ids(activeDiagram),['A','B'])''')
                    if cancel:
                        self.page.keyboard.press('Escape')
                        self.assertEqual(self.page.evaluate('ids(activeDiagram)'), ['B'])
                    self.page.mouse.up()
                    self.assertIsNone(self.page.evaluate(
                        'activeDiagram.selectionState().activeGesture ?? null'))
                    self.assertEqual(self.page.evaluate('ids(activeDiagram)'),
                                     ['B'] if cancel else ['A', 'B'])
                self.page.evaluate('activeController.dispose()')

    def test_lasso_array_length_is_admitted_once_before_scalar_reads(self):
        self.page.evaluate('''() => {
          const {diagram}=make();diagram.render(source);diagram.selectNode('A');
          const token=start(diagram,'add');
          const coordinates=new Float64Array([-1,-1,601,-1,601,301,-1,301]);
          let lengthReads=0;
          Object.defineProperty(coordinates,'length',{get(){
            if(++lengthReads>1) throw new Error('lasso length read after admission');
            return 8;
          }});
          check(diagram.previewSelectionLasso(token,coordinates),'lasso did not select B');
          check(lengthReads===1 && same(ids(diagram),['A','B']),'unbounded or repeated copy');
          check(diagram.commitSelectionGesture(token),'lasso transaction lost');
        }''')

    def test_offscreen_diagram_reads_live_dimensions_and_retains_selection(self):
        self.page.evaluate('''() => {
          check(typeof OffscreenCanvas==='function','OffscreenCanvas is required by this scenario');
          const {canvas,diagram}=make({},true);diagram.render(source);
          let p=label(canvas,'Alpha');check(diagram.selectAt(p.x,p.y),'offscreen pointer');
          canvas.width=1200;canvas.height=600;
          check(!diagram.selectionState().frameAvailable && diagram.hitTestNode(p.x,p.y)==null,'offscreen resize stale');
          check(diagram.redraw(),'offscreen redraw');p=label(canvas,'Alpha');
          check(diagram.hitTestNode(p.x,p.y)==='A' && same(ids(diagram),['A']),'offscreen current draw');
          const token=start(diagram);previewAll(diagram,token);diagram.invalidateFrame();
          check(!diagram.commitSelectionGesture(token) && same(ids(diagram),['A']),'offscreen rollback');
        }''')


if __name__ == '__main__':
    unittest.main(verbosity=2)
