#!/usr/bin/env python3
"""Chromium integration tests for the production playground and SVG selection.

Run: python scripts/playground_explorer_test.py
Rendering/authoring transports are fixtures, not a rebuilt Rust/WASM engine.
The page, SVG selection implementation, DOM geometry, pointer capture and input
are real. Modules load from Blobs with only import URLs/import.meta.url substituted;
no network or development server is needed. Set CHROMIUM to override.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SVG = '''<svg xmlns="http://www.w3.org/2000/svg" width="540" height="320" viewBox="0 0 540 320">
<g class="fm-node" data-id="A" id="fm-node-a-0"><rect x="30" y="35" width="100" height="50" fill="#ddd"/><text x="40" y="65">Alpha</text></g>
<a href="#activated"><g class="fm-node" data-id="B" id="fm-node-b-1"><rect x="260" y="35" width="100" height="50" fill="#ddd"/><text x="270" y="65">Beta</text></g></a>
<g class="fm-node" data-id="C" id="fm-node-c-2"><rect x="30" y="175" width="100" height="50" fill="#ddd"/><text x="40" y="205">Gamma</text></g>
</svg>'''
BOOT = '''
window.workers = []; window.workerMessages = []; window.editorDisposed = 0;
class FixtureWorker {
  constructor() { this.messages = []; this.terminated = false; workers.push(this); }
  postMessage(message) {
    this.messages.push(message); workerMessages.push(message);
    if (message.kind === 'init') {
      this.init = message;
      queueMicrotask(() => this.emit({kind:'ready', requested:'auto',
        target: message.capabilities.canvasTransferred ? 'offscreenInWorker' : 'svgInWorker'}));
    }
    if (message.kind === 'render') this.latest = message;
  }
  emit(message) { this.onmessage?.({data:message}); }
  terminate() { this.terminated = true; }
}
window.Worker = FixtureWorker;
window.finish = (svg = window.testSvg, requestId = workers.at(-1).latest.requestId, worker = workers.at(-1)) =>
  worker.emit({kind:'completed',requestId,svg,nodeCount:3,edgeCount:0});
window.typeSource = source => { const el = document.querySelector('#src'); el.value = source; el.dispatchEvent(new Event('input')); };
window.ids = () => [...document.querySelectorAll('#out [data-fm-selection-overlay] [data-node-id]')].map(el => el.dataset.nodeId);
window.equal = (a,b) => { if (JSON.stringify(a) !== JSON.stringify(b)) throw Error(JSON.stringify(a)+' != '+JSON.stringify(b)); };
window.expect = (value, message = 'assertion failed') => { if (!value) throw Error(message); };
window.center = id => {
  const node = [...document.querySelectorAll('#out .fm-node')].find(el => el.dataset.id === id);
  const r = node.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2};
};
window.camera = () => { const r = document.querySelector('#out svg').viewBox.baseVal;
  return {x:r.x,y:r.y,width:r.width,height:r.height}; };
window.near = (a,b,tolerance=.002) => expect(Math.abs(a-b)<=tolerance, a+' not near '+b);
window.at = (x,y) => new DOMPoint(x,y).matrixTransform(document.querySelector('#out svg').getScreenCTM().inverse());
window.frames = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
'''
STUBS = {
    '/web/mermaid-compat.mjs': '''export function createWorkerMermaid() { return {}; }
export function createMermaid() { return {}; } export class MermaidError extends Error {}''',
    '/web/fm-document.js': '''export function mountDocumentWorkspace(options) {
window.openTestDocument = source => { options.sourceEl.value = source; options.onDocumentChange(); options.onChange(); };
return {sourceChanged(){},dispose(){},saveArtifact(){}}; }''',
    '/web/fm-share.js': 'export function mountShareControls(){return {sourceChanged(){},dispose(){}};}',
    '/web/fm-image-export.js': 'export function mountImageExport(){return {sourceChanged(){},dispose(){}};}',
    '/web/fm-config-editor.js': 'export function mountConfigurationEditor(){return {open(){},observeSource(){},sourceChanged(){},dispose(){}};}',
    '/web/fm-deck-editor.js': 'export function mountDeckEditor(){return {open(){},sourceChanged(){},documentChanged(){},dispose(){}};}',
    '/web/fm-source-editor.js': '''export function mountSourceEditor(options) {
options.panelEl.hidden = false;
return {render(source){window.editorSource=source;},dispose(){window.editorDisposed++;}}; }
export function createSourceEditorBackend(){ return {renderSource(){},cancel(){},dispose(){}}; }''',
    '/pkg/frankenmermaid.js': '''export default async function(){}
export function workerHandleMessage(text){const request=JSON.parse(text); return JSON.stringify({kind:'completed',requestId:request.requestId,svg:globalThis.testSvg});}''',
}


def main():
    failures, page_errors = [], []
    checks = 0
    with sync_playwright() as pw:
        browser = pw.chromium.launch(headless=True,
            executable_path=os.environ.get('CHROMIUM') or shutil.which('chromium'), args=['--no-sandbox'])
        page = browser.new_page(viewport={'width': 1440, 'height': 1100})
        page.set_default_timeout(4000)
        page.on('pageerror', lambda error: page_errors.append(str(error)))

        html = (ROOT / 'web/playground.html').read_text()
        markup, separator, script = html.partition('<script type="module">')
        assert separator and script.endswith('</script>\n'), 'Expected the production inline module'
        page.set_content(markup)
        page.evaluate(BOOT + '\nwindow.testSvg=' + json.dumps(SVG) + ';')
        page.evaluate('''async ({source, selection, stubs}) => {
          const urls = {};
          for (const [path, code] of Object.entries(stubs)) {
            const name = path.startsWith('/web/') ? './' + path.slice(5) : '..' + path;
            urls[name] = URL.createObjectURL(new Blob([code], {type:'text/javascript'}));
          }
          const rewrite = code => {
            for (const [name, url] of Object.entries(urls)) code = code.replaceAll(JSON.stringify(name), JSON.stringify(url));
            return code.replaceAll('import.meta.url', JSON.stringify('https://frankenmermaid.invalid/web/playground.html'));
          };
          urls['./mermaid.mjs'] = URL.createObjectURL(new Blob([rewrite(selection)], {type:'text/javascript'}));
          const entry = URL.createObjectURL(new Blob([rewrite(source)], {type:'text/javascript'}));
          await import(entry);
        }''', {'source': script.removesuffix('</script>\n'),
               'selection': (ROOT/'web/mermaid.mjs').read_text(), 'stubs': STUBS})
        page.wait_for_function('workers[0]?.latest')

        def check(name, operation):
            nonlocal checks
            checks += 1
            try:
                operation() if callable(operation) else page.evaluate('() => {' + operation + '}')
                print(json.dumps({'scenario_id': name, 'passed': True}))
            except Exception as error:
                failures.append(name)
                print(json.dumps({'scenario_id': name, 'passed': False, 'error': str(error)}))

        def open_explorer(svg=SVG):
            if page.get_attribute('#explore', 'aria-expanded') == 'true':
                page.click('#explore')
            previous_workers = page.evaluate('workers.length')
            page.click('#explore')
            page.wait_for_function('(count) => workers.length > count && workers.at(-1)?.latest && !workers.at(-1).terminated && !workers.at(-1).init.capabilities.offscreenCanvas', arg=previous_workers)
            page.evaluate('(svg) => finish(svg)', svg)
            page.wait_for_function('!document.querySelector("#explorer-controls").disabled')

        def click_node(id, modifiers=None):
            p = page.evaluate('(id) => center(id)', id)
            for modifier in modifiers or []:
                page.keyboard.down(modifier)
            try:
                page.mouse.click(p['x'], p['y'])
            finally:
                for modifier in modifiers or []:
                    page.keyboard.up(modifier)

        check('explorer-mounts-svg-worker', lambda: (
            open_explorer(), page.evaluate('''expect(workers[0].terminated); expect(!workers.at(-1).init.capabilities.canvasTransferred);
expect(!document.querySelector('#out').inert); expect(document.querySelectorAll('#explorer-results option').length===3);''')))
        check('real-click-selects-without-activation', lambda: (
            click_node('B'), page.evaluate("equal(ids(),['B']); expect(location.hash===''); expect(document.querySelector('#explorer-selection').textContent==='1 nodes selected: B');")))
        check('real-modifier-multiselect', lambda: (
            click_node('A', ['Shift']), page.evaluate("equal(ids(),['A','B'])")))
        check('keyboard-navigation-updates-inspector', lambda: (
            page.keyboard.press('ArrowDown'), page.evaluate("equal(ids(),['C']); expect(document.querySelector('#explorer-selection').textContent.includes('C'))")))
        check('search-label-and-enter-select', lambda: (
            page.fill('#explorer-search', 'bEtA'), page.press('#explorer-search', 'Enter'),
            page.evaluate("equal(ids(),['B']); expect(document.querySelectorAll('#explorer-results option').length===1)")))
        check('source-edit-suspends-old-selection', '''
window.oldRoot=document.querySelector('#out svg'); typeSource('changed source');
expect(document.querySelector('#out').inert); expect(document.querySelector('#explorer-controls').disabled);
equal(ids(),[]); expect(document.querySelector('#explorer-message').textContent.includes('not interactive'));
''')
        check('rerender-retains-authored-id', '''
finish(testSvg.replace('fm-node-b-1','entirely-different-generated-id').replace('Beta','Beta updated'));
equal(ids(),['B']); expect(document.querySelector('#out svg')!==oldRoot);
expect(!oldRoot.querySelector('[data-fm-selection-overlay]')); expect(!oldRoot.hasAttribute('tabindex'));
expect(!document.querySelector('#out').inert);
''')
        check('obsolete-reply-never-replaces-current-preview', '''
const w=workers.at(-1); typeSource('older'); const old=w.latest.requestId;
typeSource('newer'); finish(); const root=document.querySelector('#out svg'); finish('<svg/>',old);
expect(document.querySelector('#out svg')===root); equal(ids(),['B']);
''')
        check('failure-keeps-stale-preview-inert', '''
typeSource('bad input'); workers.at(-1).emit({kind:'failed',requestId:workers.at(-1).latest.requestId,reason:'parse failed'});
expect(document.querySelector('#out').inert); expect(document.querySelector('#explorer-controls').disabled);
expect(document.querySelector('#explorer-message').textContent.includes('failed'));
''')
        check('recovery-restores-selection', "typeSource('repaired'); finish(); equal(ids(),['B']); expect(!document.querySelector('#out').inert);")
        check('new-document-does-not-inherit-equal-ids', "openTestDocument('another file'); finish(); equal(ids(),[]);")
        check('removed-node-is-forgotten', lambda: (
            page.fill('#explorer-search','Alpha'), page.press('#explorer-search','Enter'),
            page.evaluate("typeSource('remove A'); finish(testSvg.replace('data-id=\"A\"','data-id=\"new-node\"')); equal(ids(),[]);")))
        check('empty-chart-is-explicit', '''
typeSource('pie'); finish('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200"><circle cx="80" cy="80" r="30"/></svg>');
expect(document.querySelector('#explorer-message').textContent.includes('no addressable')); equal(ids(),[]);
expect(!document.querySelector('#out').inert);
''')
        check('duplicate-ids-fail-closed', '''
typeSource('duplicate'); finish(testSvg.replace('data-id="C"','data-id="A"'));
expect(document.querySelector('#out').inert); expect(document.querySelector('#explorer-message').textContent.includes('Ambiguous'));
''')
        check('hostile-label-is-data-in-search-ui', '''
typeSource('escaped labels'); finish(testSvg.replace('Alpha','&lt;img src=x onerror=alert(1)&gt;'));
const input=document.querySelector('#explorer-search'); input.value='img'; input.dispatchEvent(new Event('input'));
expect(document.querySelector('#explorer-results').textContent.includes('<img')); expect(!document.querySelector('#explorer img'));
''')
        check('large-search-is-bounded-but-counted', '''
const nodes=Array.from({length:260},(_,i)=>`<g class="fm-node" data-id="N${i}"><rect x="${i}" y="0" width="2" height="2"/></g>`).join('');
typeSource('large'); finish(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300">${nodes}</svg>`);
const input=document.querySelector('#explorer-search'); input.value=''; input.dispatchEvent(new Event('input'));
expect(document.querySelectorAll('#explorer-results option').length===200);
expect(document.querySelector('#explorer-search-count').textContent.includes('260 matching'));
''')
        check('close-unbinds-and-restores-root', lambda: (
            page.click('#explore'), page.evaluate("expect(!document.querySelector('#out').inert); expect(document.querySelector('#explorer').hidden); expect(!document.querySelector('#out svg').hasAttribute('tabindex'));")))
        check('reopen-is-fresh-and-does-not-double-bind', lambda: (
            open_explorer(), click_node('A'), page.evaluate("equal(ids(),['A']); expect(document.querySelectorAll('[data-fm-selection-overlay]').length===1)")))
        check('source-editor-takes-exclusive-ownership', lambda: (
            page.click('#edit-source'), page.wait_for_function('window.editorSource !== undefined'),
            page.evaluate("expect(document.querySelector('#explorer').hidden); expect(!document.querySelector('#out').inert); expect(workers.at(-1).terminated);")))
        check('explorer-can-replace-source-editor', lambda: (
            open_explorer(), page.evaluate("expect(editorDisposed===1); expect(document.querySelector('#source-editor').hidden); expect(!document.querySelector('#edit-source').disabled);")))
        check('zoom-buttons-do-not-reparse-or-change-source', lambda: (
            page.evaluate("window.beforeCamera=camera(); window.messageCount=workerMessages.length; window.beforeSource=document.querySelector('#src').value;"),
            page.click('#explorer-zoom-in'), page.evaluate("near(camera().width,beforeCamera.width/1.25); expect(workerMessages.length===messageCount); expect(document.querySelector('#src').value===beforeSource);"),
            page.click('#explorer-zoom-out'), page.evaluate("near(camera().width,beforeCamera.width);")))

        def wheel_anchor():
            page.click('#explorer-fit')
            p = page.evaluate("center('B')")
            page.evaluate('(p) => { window.anchor=at(p.x,p.y); window.oldWidth=camera().width; }', p)
            page.mouse.move(p['x'], p['y'])
            page.mouse.wheel(0, -140)
            page.wait_for_function('camera().width < oldWidth')
            page.evaluate('(p) => {const now=at(p.x,p.y); near(now.x,anchor.x); near(now.y,anchor.y);}', p)

        check('wheel-zoom-keeps-pointer-anchor', wheel_anchor)
        check('zoom-extremes-are-bounded-and-finite', '''
const root=document.querySelector('#out svg'), r=root.getBoundingClientRect();
for (let i=0;i<30;i++) root.dispatchEvent(new WheelEvent('wheel',{deltaY:-600,clientX:r.x+r.width/2,clientY:r.y+r.height/2,bubbles:true,cancelable:true}));
expect(document.querySelector('#explorer-zoom').textContent==='51200% of fit'); expect(Object.values(camera()).every(Number.isFinite));
for (let i=0;i<30;i++) root.dispatchEvent(new WheelEvent('wheel',{deltaY:600,clientX:r.x+r.width/2,clientY:r.y+r.height/2,bubbles:true,cancelable:true}));
expect(document.querySelector('#explorer-zoom').textContent==='5% of fit'); expect(Object.values(camera()).every(Number.isFinite));
''')
        check('fit-diagram-restores-home', lambda: (
            page.click('#explorer-fit'), page.evaluate("expect(document.querySelector('#explorer-zoom').textContent==='100% of fit');")))
        check('fit-selection-frames-selected-node', lambda: (
            click_node('A'), page.click('#explorer-fit-selection'), page.evaluate('''
equal(ids(),['A']); const view=camera(); expect(view.x<=30 && view.x+view.width>=130);
expect(view.y<=35 && view.y+view.height>=85); expect(view.width<540);
const overlay=document.querySelector('[data-node-id="A"]').getBoundingClientRect();
const node=document.querySelector('.fm-node[data-id="A"]').getBoundingClientRect();
near(overlay.x,node.x); near(overlay.y,node.y); near(overlay.width,node.width);
''')))
        check('fit-empty-selection-is-explicit', lambda: (
            page.click('#explorer-clear'), page.click('#explorer-fit-selection'),
            page.evaluate("expect(document.querySelector('#explorer-message').textContent.includes('Select one or more')); expect(Object.values(camera()).every(Number.isFinite));")))

        def pan_camera(button='middle', dx=40, dy=25):
            page.click('#explorer-fit')
            click_node('A')
            p = page.evaluate("center('B')")
            before = page.evaluate("center('A')")
            page.mouse.move(p['x'], p['y'])
            page.mouse.down(button=button)
            page.mouse.move(p['x']+dx, p['y']+dy)
            page.mouse.up(button=button)
            page.evaluate('([before,dx,dy]) => {const now=center("A"); near(now.x,before.x+dx); near(now.y,before.y+dy); equal(ids(),["A"]);}', [before,dx,dy])

        check('middle-pan-preserves-selection', pan_camera)

        def left_pan():
            page.select_option('#explorer-tool','pan')
            page.click('#explorer-fit')
            before = page.evaluate("center('A')")
            p = page.evaluate("center('B')")
            page.mouse.move(p['x'],p['y'])
            page.mouse.down()
            page.mouse.move(p['x']-45,p['y']+20)
            page.mouse.up()
            page.evaluate('(before) => {const now=center("A"); near(now.x,before.x-45); near(now.y,before.y+20); equal(ids(),["A"]);}', before)

        check('pan-tool-left-drag-is-camera-only', left_pan)
        check('pan-keyboard-moves-without-changing-selection', lambda: (
            page.evaluate("window.oldPoint=center('A'); document.querySelector('#out svg').focus();"),
            page.keyboard.press('ArrowRight'), page.evaluate("near(center('A').x,oldPoint.x-50); equal(ids(),['A']);")))
        check('keyboard-fit-and-zoom', lambda: (
            page.keyboard.press('0'), page.evaluate("expect(document.querySelector('#explorer-zoom').textContent==='100% of fit'); window.oldWidth=camera().width;"),
            page.keyboard.press('+'), page.evaluate("near(camera().width,oldWidth/1.25);"),
            page.keyboard.press('f'), page.evaluate("expect(camera().width<540); equal(ids(),['A']);")))

        def pinch():
            page.click('#explorer-fit')
            r = page.locator('#out svg').bounding_box()
            x, y = r['x']+r['width']/2, r['y']+r['height']/2
            session = page.context.new_cdp_session(page)
            point = lambda id, px: {'id': id, 'x': px, 'y': y, 'radiusX': 1, 'radiusY': 1, 'force': 1}
            page.evaluate('([x,y])=>{window.pinchAnchor=at(x,y);window.pinchWidth=camera().width;}',[x,y])
            session.send('Input.dispatchTouchEvent',{'type':'touchStart','touchPoints':[point(1,x-40)]})
            session.send('Input.dispatchTouchEvent',{'type':'touchStart','touchPoints':[point(1,x-40),point(2,x+40)]})
            session.send('Input.dispatchTouchEvent',{'type':'touchMove','touchPoints':[point(1,x-80),point(2,x+80)]})
            session.send('Input.dispatchTouchEvent',{'type':'touchEnd','touchPoints':[]})
            page.evaluate('([x,y])=>{near(camera().width,pinchWidth/2);const p=at(x,y);near(p.x,pinchAnchor.x);near(p.y,pinchAnchor.y);equal(ids(),["A"]);}',[x,y])
            session.detach()

        check('real-two-touch-pinch-keeps-midpoint-anchor', pinch)
        check('camera-and-selection-survive-source-refresh', '''
window.beforeCamera=camera(); typeSource('same geometry, new source'); finish(testSvg.replace('Alpha','Alpha revised'));
const now=camera(); for(const key of Object.keys(now)) near(now[key],beforeCamera[key]); equal(ids(),['A']);
''')
        check('document-switch-resets-camera', '''
openTestDocument('new document'); finish(); expect(document.querySelector('#explorer-zoom').textContent==='100% of fit'); equal(ids(),[]);
''')

        def box_after_zoom():
            page.select_option('#explorer-tool','box')
            page.click('#explorer-fit')
            page.click('#explorer-zoom-in')
            r = page.locator('#out .fm-node[data-id="C"]').bounding_box()
            page.mouse.move(r['x']-5,r['y']-5)
            page.mouse.down()
            page.mouse.move(r['x']+r['width']+5,r['y']+r['height']+5)
            page.mouse.up()
            page.evaluate("equal(ids(),['C']);")

        check('box-selection-uses-camera-transformed-geometry', box_after_zoom)

        def pan_revoked_by_source():
            page.select_option('#explorer-tool','pan')
            p = page.evaluate("center('C')")
            page.mouse.move(p['x'],p['y'])
            page.mouse.down()
            page.mouse.move(p['x']+15,p['y']+10)
            page.evaluate("typeSource('typing cancels pan'); window.pendingCamera=camera(); expect(document.querySelector('#out').inert);")
            page.mouse.move(p['x']+75,p['y']+50)
            page.mouse.up()
            page.evaluate("for(const key of Object.keys(camera())) near(camera()[key],pendingCamera[key]); finish(); equal(ids(),['C']);")

        check('source-edit-revokes-pan-capture', pan_revoked_by_source)
        check('nonfinite-wheel-input-does-not-corrupt-view', '''
const root=document.querySelector('#out svg'), before=camera();
// Native WheelEventInit rejects Infinity; a forged event exercises the defensive boundary.
const event=new WheelEvent('wheel',{bubbles:true,cancelable:true});
Object.defineProperty(event,'deltaY',{value:Infinity}); root.dispatchEvent(event); equal(camera(),before);
''')

        def resizing():
            page.evaluate("window.preResize=camera();")
            page.set_viewport_size({'width':1200,'height':950})
            page.evaluate('frames()')
            page.evaluate('''const now=camera(); near(now.x+now.width/2,preResize.x+preResize.width/2);
near(now.y+now.height/2,preResize.y+preResize.height/2); equal(ids(),['C']);''')

        check('resize-preserves-camera-center-and-selection', resizing)
        check('closing-restores-original-viewbox', lambda: (
            page.click('#explore'), page.evaluate("expect(document.querySelector('#out svg').getAttribute('viewBox')==='0 0 540 320'); expect(!document.querySelector('#out').hasAttribute('data-exploring'));"),
            open_explorer()))
        check('pagehide-cleans-up-interaction-and-worker', '''
const root=document.querySelector('#out svg'); window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:false}));
expect(workers.at(-1).terminated); expect(!root.querySelector('[data-fm-selection-overlay]'));
expect(!root.hasAttribute('tabindex'));
''')
        browser.close()
    if page_errors:
        failures.append('uncaught-browser-errors')
        print(json.dumps({'page_errors': page_errors}))
    print(json.dumps({'checks': checks, 'failed': failures,
        'playground_sha256': hashlib.sha256((ROOT/'web/playground.html').read_bytes()).hexdigest()}))
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
