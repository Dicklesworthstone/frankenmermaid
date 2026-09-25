"""Graph-deck preview/export integration with the real CLI runtime and template.

Run: python web/fm-deck-editor.browser.test.py
Requires Playwright and Chromium; PLAYWRIGHT_CHROMIUM_EXECUTABLE can override its path.
WASM and asset loading are explicit fixtures. Browser DOM, sandbox/CSP, presentation
playback, navigation and Blob downloads are real. No HTTP server or network is needed.
"""
import json
import os
from pathlib import Path
import shutil
import unittest

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = r'''
function fixtureDeck(source) {
  if (source === 'fail') throw new Error('fixture deck parse failed');
  const bounds = {x:0,y:0,width:500,height:250};
  const geometry = {
    'fm-node-a-0': {x:20,y:40,width:100,height:50},
    'fm-node-b-1': {x:190,y:40,width:100,height:50},
    'fm-node-c-2': {x:360,y:130,width:100,height:50},
  };
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 500 250">' +
    '<g id="fm-edge-0"><path d="M120 65 L190 65" stroke="black" fill="none"/></g>' +
    '<g id="fm-edge-1"><path d="M290 65 L360 155" stroke="black" fill="none"/></g>' +
    Object.entries(geometry).map(([id,r],i) => `<g id="${id}"><rect x="${r.x}" y="${r.y}" width="${r.width}" height="${r.height}" fill="white" stroke="black"/><text x="${r.x+10}" y="${r.y+30}">${['Alpha','Beta','Gamma'][i]}</text></g>`).join('') + '</svg>';
  const manifest = {
    schemaVersion:'1.1.0',generator:'frankenmermaid',diagramType:'Flowchart',title:'Fixture presentation',
    viewBox:bounds,options:{fitMargin:20,zoomMax:2,dimOpacity:0.1,autoAdvanceMs:0},
    slides:[
      {id:'first',title:'First scene',caption:source,bounds,fitMargin:20,zoomMax:2,maxStep:1,
       nodes:[{index:0,sourceId:'A',elementId:'fm-node-a-0',step:0},
              {index:1,sourceId:'B',elementId:'fm-node-b-1',step:1}],
       edges:[{index:0,elementId:'fm-edge-0',step:1,touching:false}],
       steps:[{step:1,elementIds:['fm-node-b-1','fm-edge-0']}]},
      {id:'second',title:'Second scene',caption:'The destination',bounds,fitMargin:20,zoomMax:2,maxStep:0,
       nodes:[{index:2,sourceId:'C',elementId:'fm-node-c-2',step:0}]}
    ],
    overview:{enabled:true,title:'Whole graph',tour:false},
    nodeGeometry:new Map(Object.entries(geometry)),
    nodeSlideIndex:new Map([['fm-node-a-0',['first']],['fm-node-b-1',['first']],['fm-node-c-2',['second']]]),
    edgeEndpoints:new Map([
      ['fm-edge-0',{fromElementId:'fm-node-a-0',toElementId:'fm-node-b-1'}],
      ['fm-edge-1',{fromElementId:'fm-node-b-1',toElementId:'fm-node-c-2'}]
    ]),
  };
  if (source === 'none') return {svg,manifest:null,warnings:[{message:'unsupported family'}]};
  if (source === 'bad-id') manifest.slides[0].nodes[0].elementId = 'missing-node';
  return {svg,manifest,warnings:[{message:'fixture warning retained'}]};
}
'''
APP = '''<!doctype html><meta charset="utf-8"><title>Deck integration</title>
<textarea id="src" style="height:90px;width:90%">initial</textarea>
<button id="open">Open presentation</button><section id="panel" hidden></section>'''
APP_SCRIPT = 'const module = window.deckModule;\n' + FIXTURE + '''
window.fixtureDeck = fixtureDeck;
window.deckCalls = [];
window.saved = [];
window.api = {renderDeck(source) { deckCalls.push(source); return fixtureDeck(source); }};
window.assetLoader = async () => window.testAssets;
window.openEditor = (options = {}) => {
  const sourceEl = document.getElementById('src');
  window.editor = module.mountDeckEditor({sourceEl,panelEl:document.getElementById('panel'),
    loadModule:async()=>api,workerOptions:{WorkerClass:null},
    loadAssets:()=>assetLoader(), ...options });
  return editor.open();
};
document.getElementById('open').addEventListener('click',()=>{ void openEditor(); });
window.appReady = true;
'''


class DeckBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        options = {"headless": True, "args": ["--no-sandbox"]}
        executable = os.environ.get("PLAYWRIGHT_CHROMIUM_EXECUTABLE") or shutil.which("chromium")
        if executable:
            options["executable_path"] = executable
        cls.browser = cls.playwright.chromium.launch(**options)

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.context = self.browser.new_context(reduced_motion="reduce", accept_downloads=True)
        self.addCleanup(self.context.close)
        self.page = self.context.new_page()
        self.page.set_default_timeout(4000)
        self.errors = []
        self.requests = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.on("request", lambda request: self.requests.append(request.url))
        self.page.set_content(APP)
        self.page.evaluate('''async ({sourceEditor, deckEditor, bootstrap, assets}) => {
          const load = text => URL.createObjectURL(new Blob([text], {type:'text/javascript'}));
          const sourceUrl = load(sourceEditor);
          const deckUrl = load(deckEditor.replace('"./fm-source-editor.js"', JSON.stringify(sourceUrl))
            .replaceAll('import.meta.url', '"https://fixture.invalid/web/fm-deck-editor.js"'));
          try { window.sourceEditorModule = await import(sourceUrl); window.deckModule = await import(deckUrl); }
          finally { URL.revokeObjectURL(sourceUrl); URL.revokeObjectURL(deckUrl); }
          window.testAssets = assets;
          const setupUrl = load(bootstrap);
          try { await import(setupUrl); } finally { URL.revokeObjectURL(setupUrl); }
        }''', {"sourceEditor": (ROOT / "web/fm-source-editor.js").read_text(),
               "deckEditor": (ROOT / "web/fm-deck-editor.js").read_text(),
               "bootstrap": APP_SCRIPT,
               "assets": {"runtime": (ROOT / "crates/fm-cli/src/deck_runtime.js").read_text(),
                          "template": (ROOT / "crates/fm-cli/src/deck_template.html").read_text()}})

    def tearDown(self):
        self.assertEqual(self.errors, [], "unexpected browser exceptions")

    def open(self, source="initial"):
        self.page.locator("#src").fill(source)
        self.page.locator("#open").click()
        self.wait_ready()

    def wait_ready(self):
        self.page.wait_for_function("document.querySelector('#deck-editor-save-html')?.disabled === false")

    def frame(self):
        return self.page.frame_locator("#deck-editor-preview iframe")

    def test_real_runtime_reveal_navigation_and_overview(self):
        self.open()
        frame = self.frame()
        self.assertEqual(frame.locator("#deck-title").text_content(), "First scene")
        self.assertEqual(frame.locator("#deck-num").text_content(), "01 / 03 · 0/1")
        self.assertIn("fm-deck-hidden", frame.locator("#fm-node-b-1").get_attribute("class"))
        frame.locator("#deck-next").click()
        frame.locator("#fm-node-b-1:not(.fm-deck-hidden)").wait_for()
        self.assertEqual(frame.locator("#deck-num").text_content(), "01 / 03 · 1/1")
        frame.locator("#deck-next").click()
        self.assertEqual(frame.locator("#deck-title").text_content(), "Second scene")
        frame.locator("#deck-overview-btn").click()
        self.assertEqual(frame.locator("#deck-title").text_content(), "Whole graph")
        self.assertEqual(frame.locator(".fm-deck-dim").count(), 0)
        frame.locator("#deck-stage").focus()
        self.page.keyboard.press("Home")
        self.assertEqual(frame.locator("#deck-title").text_content(), "First scene")
        self.assertIn("fixture warning retained", self.page.locator("#deck-editor-status").text_content())
        self.assertEqual(self.page.evaluate("deckCalls"), ["initial"])
        self.assertEqual(self.page.locator("iframe").get_attribute("sandbox"), "allow-scripts")
        self.assertTrue(self.page.evaluate("document.querySelector('iframe').contentDocument === null"))

    def test_live_refresh_keeps_typing_focus_and_rejects_stale_export(self):
        self.open()
        self.page.locator("#src").fill("a newer source")
        self.assertTrue(self.page.locator("#deck-editor-save-html").is_disabled())
        self.assertEqual(self.page.locator("iframe").count(), 0)
        self.wait_ready()
        self.assertEqual(self.frame().locator("#deck-caption").text_content(), "a newer source")
        self.assertEqual(self.page.evaluate("document.activeElement.id"), "src")
        self.assertEqual(self.page.locator("#src").input_value(), "a newer source")
        self.page.evaluate("src.value = 'programmatic change'")
        failed = self.page.evaluate("() => { try { editor.exportHtml(); return false; } catch { return true; } }")
        self.assertTrue(failed, "unchanged DOM controls must not export a stale artifact")
        self.page.evaluate("editor.sourceChanged()")
        self.wait_ready()
        self.assertEqual(self.frame().locator("#deck-caption").text_content(), "programmatic change")

    def test_real_download_plays_without_network_and_honors_deep_links(self):
        self.open()
        expected = self.page.evaluate("editor.exportHtml().text")
        with self.page.expect_download() as download:
            self.page.locator("#deck-editor-save-html").click()
        self.assertEqual(download.value.suggested_filename, "diagram-deck.html")
        downloaded = Path(download.value.path()).read_text()
        self.assertEqual(downloaded, expected)
        # Load the downloaded bytes as a standalone document, not inside the preview iframe.
        # This proves network-free playback, but deliberately does not claim a file:// test.
        player = self.context.new_page()
        self.addCleanup(player.close)
        errors = []
        requests = []
        player.on("pageerror", lambda error: errors.append(str(error)))
        player.on("request", lambda request: requests.append(request.url))
        player.evaluate("location.hash = '#second'")
        player.set_content(downloaded)
        self.assertEqual(player.locator("#deck-title").text_content(), "Second scene")
        self.assertEqual(player.evaluate("document.activeElement.id"), "deck-stage")
        player.keyboard.press("Home")
        self.assertEqual(player.locator("#deck-title").text_content(), "First scene")
        player.evaluate("location.hash = '#%E0%A4%A'")
        player.set_content(downloaded)
        self.assertEqual(player.locator("#deck-title").text_content(), "First scene")
        self.assertEqual(errors, [])
        self.assertFalse(any(url.startswith("http") for url in requests), requests)
        self.assertEqual(self.page.evaluate("deckCalls.length"), 1, "saving must not recompute a different deck")
        with self.page.expect_download() as manifest_download:
            self.page.locator("#deck-editor-save-manifest").click()
        manifest = json.loads(Path(manifest_download.value.path()).read_text())
        self.assertEqual(len(manifest["nodeGeometry"]), 3)
        self.assertEqual(manifest["edgeEndpoints"]["fm-edge-0"]["toElementId"], "fm-node-b-1")

    def test_input_cannot_escape_script_data_or_execute_svg_handlers(self):
        attack = '</script><script>window.pwned=1</script> {{TITLE}} RUNTIME_JS <img src=x onerror="window.pwned=2">'
        self.page.evaluate("""attack => {
          const base = api.renderDeck;
          api.renderDeck = source => {
            const result = base(source);
            result.manifest.title = attack;
            result.manifest.slides[0].caption = attack;
            result.svg = result.svg.replace('<svg ', '<svg onload="window.pwned=3" ');
            return result;
          };
        }""", attack)
        self.open()
        frame = self.frame()
        self.assertEqual(frame.locator("#deck-caption").text_content(), attack)
        self.assertEqual(frame.locator("#deck-caption img").count(), 0)
        child = next(frame for frame in self.page.frames if frame != self.page.main_frame)
        self.assertIsNone(child.evaluate("window.pwned"))
        self.assertIsNone(self.page.evaluate("window.pwned"))
        self.assertEqual(frame.locator("script").count(), 5, "authored script terminators cannot create extra script elements")
        self.assertFalse(any(url.startswith("http") for url in self.requests))

    def test_engine_failure_null_manifest_and_missing_elements_never_export_a_fake_deck(self):
        self.open()
        for source, reason in [("fail", "fixture deck parse failed"), ("none", "unsupported family"), ("bad-id", "missing SVG element")]:
            self.page.locator("#src").fill(source)
            self.page.wait_for_function("text => document.querySelector('#deck-editor-status').textContent.includes(text)", arg=reason)
            self.assertTrue(self.page.locator("#deck-editor-save-html").is_disabled())
            self.assertTrue(self.page.locator("#deck-editor-save-manifest").is_disabled())
            self.assertEqual(self.page.locator("iframe").count(), 0)
            self.assertEqual(self.page.locator("#src").input_value(), source)
        self.page.locator("#src").fill("recovered")
        self.wait_ready()
        self.assertEqual(self.frame().locator("#deck-caption").text_content(), "recovered")

    def test_disposal_and_close_destroy_preview_and_cancel_asset_waits(self):
        self.page.evaluate("""() => {
          window.releaseAssets = null;
          window.assetLoader = () => new Promise(resolve => { window.releaseAssets = resolve; });
          void openEditor();
        }""")
        self.page.wait_for_function("typeof releaseAssets === 'function'")
        self.page.locator("#deck-editor-close").click()
        self.page.evaluate("() => releaseAssets(testAssets)")
        self.assertTrue(self.page.locator("#panel").is_hidden())
        self.assertEqual(self.page.locator("iframe").count(), 0)
        self.page.evaluate("""async () => {
          assetLoader = async () => testAssets;
          await editor.open();
        }""")
        self.wait_ready()
        self.page.evaluate("editor.dispose()")
        self.assertEqual(self.page.locator("iframe").count(), 0)
        self.assertTrue(self.page.locator("#deck-editor-save-html").is_disabled())
        self.page.locator("#src").fill("after dispose")
        self.page.wait_for_timeout(400)
        self.assertEqual(self.page.locator("iframe").count(), 0)

    def test_preview_handshake_requires_the_exact_frame_and_token(self):
        # Deliberately prevent the canonical runtime from initializing. A forged ready message
        # from the parent must not convert that failure into an exportable presentation.
        self.page.evaluate("""async () => {
          const assets = await assetLoader();
          assetLoader = async () => ({...assets, runtime:'window.FmDeckRuntime = {mount(){return {go(){}, overview(){}}}};'});
          void openEditor({previewTimeoutMs:350});
        }""")
        self.page.locator("iframe").wait_for()
        self.page.evaluate("""() => {
          const nonce = document.querySelector('iframe').srcdoc.match(/token:"([a-f0-9]+)"/)[1];
          window.postMessage({kind:'fm-deck-ready',token:nonce}, '*');
        }""")
        self.page.wait_for_function("document.querySelector('#deck-editor-status').textContent.includes('did not initialize')")
        self.assertTrue(self.page.locator("#deck-editor-save-html").is_disabled())
        self.assertEqual(self.page.locator("iframe").count(), 0)

    def test_save_failures_leave_the_authoritative_source_and_preview_intact(self):
        self.page.evaluate("""() => { void openEditor({saveFile:async () => {throw new Error('fixture disk failure');}}); }""")
        self.wait_ready()
        self.page.locator("#deck-editor-save-html").click()
        self.page.wait_for_function("document.querySelector('#deck-editor-status').textContent.includes('fixture disk failure')")
        self.assertEqual(self.page.locator("#src").input_value(), "initial")
        self.assertEqual(self.frame().locator("#deck-title").text_content(), "First scene")
        self.assertFalse(self.page.locator("#deck-editor-save-html").is_disabled())

    def test_runtime_errors_after_startup_revoke_export_instead_of_saving_a_broken_presentation(self):
        self.open()
        child = next(frame for frame in self.page.frames if frame != self.page.main_frame)
        # Exercise the actual generated runtime error listener, not the parent's handler alone.
        child.evaluate("window.dispatchEvent(new ErrorEvent('error', {message:'fixture animation failure'}))")
        self.page.wait_for_function("document.querySelector('#deck-editor-status').textContent.includes('fixture animation failure')")
        self.assertTrue(self.page.locator("#deck-editor-save-html").is_disabled())
        self.assertEqual(self.page.locator("iframe").count(), 0)
        self.assertEqual(self.page.locator("#src").input_value(), "initial")

    def test_morphing_uses_the_canonical_runtime_and_stops_when_preview_is_closed(self):
        self.page.emulate_media(reduced_motion="no-preference")
        self.open()
        frame = self.frame()
        frame.locator(".fm-deck-morphing").wait_for()
        self.assertEqual(frame.locator(".fm-deck-live path").count(), 2)
        child = next(frame for frame in self.page.frames if frame != self.page.main_frame)
        child.wait_for_function("document.querySelector('#fm-node-a-0').hasAttribute('transform')")
        self.page.locator("#deck-editor-close").click()
        self.page.wait_for_function("document.querySelector('iframe') === null")
        self.assertEqual(len(self.page.frames), 1, "removing the browsing context must stop its animation loop")

    def test_actual_playground_deck_button_and_source_history_stay_synchronized(self):
        # Execute the actual entrypoint, replacing only module transport, WASM, Worker and fetch.
        html = (ROOT / "web/playground.html").read_text()
        shell, remainder = html.split('<script type="module">', 1)
        script = remainder.split('</script>', 1)[0]
        self.page.set_content(shell)
        self.page.evaluate('''async script => {
          window.Worker = undefined;
          window.fetch = async url => {
            const name = String(url);
            if (name.endsWith('fm-deck-runtime.js')) return new Response(testAssets.runtime);
            if (name.endsWith('fm-deck-template.html')) return new Response(testAssets.template);
            throw new Error('Unexpected asset: ' + name);
          };
          const wasm = {
            default() {},
            renderDeck: api.renderDeck,
            parseLens: () => ({bindings:[], parsed:{warnings:[]}}),
            applyParseLensEdit() { throw new Error('No selected element in this fixture'); },
            renderSvg: source => fixtureDeck(source).svg,
            workerHandleMessage: json => {
              const request = JSON.parse(json);
              return JSON.stringify({kind:'completed',requestId:request.requestId,svg:fixtureDeck(request.input).svg});
            },
          };
          window.__import = async url => {
            if (url.endsWith('/frankenmermaid.js')) return wasm;
            if (url.endsWith('/fm-source-editor.js')) return sourceEditorModule;
            if (url.endsWith('/fm-deck-editor.js')) return deckModule;
            throw new Error('Unexpected module: '+url);
          };
          const body = script.replaceAll('import.meta.url','"https://fixture.invalid/web/playground.html"')
            .replaceAll('import(', 'window.__import(');
          const url = URL.createObjectURL(new Blob([body],{type:'text/javascript'}));
          try { await import(url); } finally { URL.revokeObjectURL(url); }
        }''', script)
        original = self.page.locator("#src").input_value()
        self.page.locator("#edit-deck").click()
        self.wait_ready()
        self.assertEqual(self.frame().locator("#deck-caption").text_content(), original)
        self.page.locator("#edit-source").click()
        self.page.locator("#source-editor-save-source").wait_for()
        self.page.locator("#src").fill("changed by typing")
        self.wait_ready()
        self.assertEqual(self.frame().locator("#deck-caption").text_content(), "changed by typing")
        self.page.locator("#source-editor-undo").click()
        self.wait_ready()
        self.assertEqual(self.page.locator("#src").input_value(), original)
        self.assertEqual(self.frame().locator("#deck-caption").text_content(), original)
        self.page.locator("#deck-editor-close").click()
        self.assertTrue(self.page.locator("#deck-editor").is_hidden())
        self.page.locator("#edit-deck").click()
        self.wait_ready()
        self.assertEqual(self.page.locator("#deck-editor h2").count(), 1)
        self.assertGreater(self.page.locator("#out svg g").count(), 0, "normal diagram preview must survive presentation mode")
        self.page.evaluate("window.dispatchEvent(new PageTransitionEvent('pagehide', {persisted:false}))")
        self.assertEqual(self.page.locator("iframe").count(), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
