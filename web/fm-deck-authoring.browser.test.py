#!/usr/bin/env python3
"""Chromium composition UI tests. Engine results and player startup are explicit fixtures.

Run: python web/fm-deck-authoring.browser.test.py
Uses the production composition/session/HTML packaging code. These tests do not execute
Rust WASM or the canonical presentation runtime; neither boundary is a second diagram parser.
"""
import base64
import json
import os
from pathlib import Path
import shutil
import unittest
from playwright.sync_api import sync_playwright

MODULE = Path(__file__).with_name('fm-deck-editor.js')
SETUP = r'''() => {
  window.baseSource = 'flowchart LR\n a[Alpha] --> b[Beta]\n b --> c[Gamma]';
  document.querySelector('#source').value = baseSource;
  window.svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 360 100">' +
    ['a','b','c'].map((id,i) => `<g id="fm-node-${id}-${i}" data-id="${id}"><title>${id.toUpperCase()}</title>` +
      `<rect x="${i*110}" width="90" height="50" fill="lightblue"/><text x="${i*110+10}" y="30">${id}</text></g>`).join('') + '</svg>';
  window.slide = (id='slide-1', overrides={}) => ({ id, title:id, bounds:{x:0,y:0,width:360,height:100},
    fitMargin:150, zoomMax:1.4, maxStep:0,
    nodes:[{sourceId:'a',elementId:'fm-node-a-0',step:0},{sourceId:'b',elementId:'fm-node-b-1',step:0}], ...overrides });
  window.result = (slides=null) => ({svg, warnings:[], manifest:slides === null ? null : {
    schemaVersion:'1.1.0', viewBox:{x:0,y:0,width:360,height:100},
    options:{fitMargin:150,zoomMax:1.4,dimOpacity:.07,autoAdvanceMs:0}, slides }});
  window.before = result(); window.after = result([slide()]);
  window.calls=[];window.saves=[];window.cancels=0;window.disposals=0;window.creations=0;window.inputs=0;
  document.querySelector('#source').addEventListener('input',()=>inputs++);
  window.makeBackend = () => {
    creations++;
    return { target:'explicit renderer fixture', renderDeck:async source => {
      calls.push(source);
      if(window.holdRender) return new Promise(resolve=>window.finishRender=resolve);
      return source===baseSource ? before : after;
    }, cancel(){cancels++},dispose(){disposals++} };
  };
  window.assets = {
    runtime:'window.FmDeckRuntime={mount:function(o){document.querySelector("#deck-stage").innerHTML="<div class=\\"fm-deck-viewport\\">"+o.svg+"</div>";}};',
    template:'<!doctype html><html><head><meta charset="utf-8"><title>{{TITLE}}</title>' +
      '<style>body{background:{{BG}};color:{{FG}}}</style></head><body><div id="deck-stage"></div>' +
      '<script>RUNTIME_JS</script><script>window.FmDeckRuntime.mount({svg:{{SVG_JS_STRING}},manifest:{{MANIFEST_JSON}}});</script></body></html>'
  };
  window.options = {sourceEl:document.querySelector('#source'),panelEl:document.querySelector('#panel'),
    backend:makeBackend(),loadAssets:async()=>assets,timeoutMs:1500,saveFile:file=>saves.push(file)};
}'''


class DeckAuthoringBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get('CHROMIUM_PATH') or shutil.which('chromium') or shutil.which('google-chrome')
        cls.browser = cls.playwright.chromium.launch(headless=True, args=['--no-sandbox'], executable_path=executable)
        code = MODULE.read_text()
        specifier = 'import { createSourceEditorBackend } from "./fm-source-editor.js";'
        assert code.count(specifier) == 1
        code = code.replace(specifier, 'const createSourceEditorBackend = () => window.makeBackend();')
        cls.module_url = 'data:text/javascript;base64,' + base64.b64encode(code.encode()).decode()

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.context = self.browser.new_context(accept_downloads=True)
        self.page = self.context.new_page()
        self.page.set_content('<textarea id="source"></textarea><section id="panel"></section>')
        self.page.evaluate('async url=>{window.exports=await import(url)}', self.module_url)
        self.page.evaluate(SETUP)

    def tearDown(self):
        self.context.close()

    def mount(self, real_download=False):
        if real_download:
            self.page.evaluate('delete options.saveFile')
        self.page.evaluate('window.composer=exports.mountDeckComposer(options)')

    def add_slide(self, nodes='["a","b"]'):
        self.page.locator('#deck-compose-add').click()
        self.page.locator('#deck-compose-members').fill(nodes)

    def prepare(self):
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('!document.querySelector("#deck-compose-apply").disabled')

    def source_spec(self):
        return self.page.evaluate('JSON.parse(document.querySelector("#source").value.split("%%{deck: ").at(-1).split("}%%")[0])')

    def test_form_is_lazy_and_catalog_comes_from_addressable_engine_ids(self):
        self.mount()
        self.assertEqual(self.page.evaluate('calls.length'), 0)
        self.add_slide('[]')
        self.page.locator('#deck-compose-load-nodes').click()
        self.page.wait_for_selector('#deck-compose-nodes input')
        self.assertEqual(self.page.locator('#deck-compose-nodes input').count(), 3)
        self.page.locator('#deck-compose-nodes input').nth(0).check()
        self.page.locator('#deck-compose-nodes input').nth(2).check()
        self.assertEqual(json.loads(self.page.locator('#deck-compose-members').input_value()), ['a', 'c'])
        self.assertEqual(self.page.evaluate('inputs'), 0)
        self.assertEqual(self.page.locator('#source').input_value(), self.page.evaluate('baseSource'))

    def test_reveal_controls_and_selector_summary_follow_the_current_draft(self):
        self.mount()
        self.add_slide()
        self.assertIn('2 explicit selectors', self.page.locator('#deck-compose-node-count').inner_text())
        self.assertTrue(self.page.locator('#deck-compose-groups').is_hidden())
        self.page.locator('#deck-compose-reveal').select_option('groups')
        self.assertTrue(self.page.locator('#deck-compose-groups').is_visible())
        self.page.locator('#deck-compose-reveal').select_option('auto')
        self.assertTrue(self.page.locator('#deck-compose-groups').is_hidden())

    def test_review_then_apply_publishes_one_input_event_and_exact_source_prefix(self):
        self.mount()
        self.add_slide()
        source = self.page.locator('#source').input_value()
        self.prepare()
        self.assertEqual(self.page.locator('#source').input_value(), source)
        self.assertEqual(self.page.evaluate('inputs'), 0)
        self.assertIn('a (step 0)', self.page.locator('#deck-compose-review').inner_text())
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').get_attribute('sandbox'), 'allow-scripts')
        self.page.locator('#deck-compose-apply').click()
        self.assertTrue(self.page.locator('#source').input_value().startswith(source + '\n'))
        self.assertEqual(self.page.evaluate('inputs'), 1)
        self.assertEqual(self.source_spec()['slides'][0]['nodes'], ['a', 'b'])
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())

    def test_revising_applied_slides_replaces_owned_block_without_duplication(self):
        self.mount()
        self.add_slide()
        self.prepare()
        self.page.locator('#deck-compose-apply').click()
        self.page.locator('#deck-compose-slide-title').fill('Revised title')
        self.page.evaluate('after=result([slide("slide-1",{title:"Revised title"})])')
        self.prepare()
        self.page.locator('#deck-compose-apply').click()
        self.assertEqual(self.page.locator('#source').input_value().count('%%{deck:'), 1)
        self.assertEqual(self.source_spec()['slides'][0]['title'], 'Revised title')
        self.assertEqual(self.page.evaluate('inputs'), 2)
        self.assertEqual(self.page.evaluate('calls[2]'), self.page.evaluate('baseSource'))

    def test_duplicate_reorder_remove_and_caption_changes_are_source_backed(self):
        self.mount()
        self.add_slide()
        self.page.locator('#deck-compose-caption').fill('One\nTwo 雪')
        self.page.locator('#deck-compose-duplicate').click()
        self.assertEqual(self.page.locator('#deck-compose-id').input_value(), 'slide-2')
        self.page.locator('#deck-compose-up').click()
        self.page.locator('#deck-compose-add').click()
        self.page.locator('#deck-compose-remove').click()
        self.page.evaluate('caption=>{after=result([slide("slide-2",{caption}),slide("slide-1",{caption})])}', 'One\nTwo 雪')
        self.prepare()
        self.page.locator('#deck-compose-apply').click()
        spec = self.source_spec()
        self.assertEqual([s['id'] for s in spec['slides']], ['slide-2', 'slide-1'])
        self.assertEqual(spec['slides'][0]['caption'], 'One\nTwo 雪')

    def test_authored_reveal_steps_and_camera_and_edge_settings_are_not_lost(self):
        self.mount()
        self.add_slide()
        self.page.locator('#deck-compose-reveal').select_option('groups')
        self.page.locator('#deck-compose-groups').fill('[["b"]]')
        self.page.locator('#deck-compose-edges').select_option('none')
        self.page.locator('#deck-compose-margin').fill('40')
        self.page.locator('#deck-compose-zoom').fill('2')
        self.page.evaluate('after=result([slide("slide-1",{maxStep:1,nodes:[slide().nodes[0],{...slide().nodes[1],step:1}]})])')
        self.prepare()
        self.assertIn('b (step 1)', self.page.locator('#deck-compose-review').inner_text())
        self.page.locator('#deck-compose-apply').click()
        slide = self.source_spec()['slides'][0]
        self.assertEqual((slide['reveal'], slide['edges'], slide['fitMargin'], slide['zoomMax']), ([['b']], 'none', 40, 2))

    def test_whole_diagram_and_automatic_reveal_leave_resolution_to_the_engine(self):
        self.mount()
        self.add_slide()
        self.page.locator('#deck-compose-all').click()
        self.page.locator('#deck-compose-reveal').select_option('auto')
        self.page.evaluate('after=result([slide("slide-1",{maxStep:3})])')
        self.prepare()
        self.assertIn('3 reveal steps', self.page.locator('#deck-compose-review').inner_text())
        self.page.locator('#deck-compose-apply').click()
        self.assertEqual(self.source_spec()['slides'][0]['nodes'], ['*'])
        self.assertEqual(self.source_spec()['slides'][0]['reveal'], 'auto')

    def test_existing_slide_ids_are_avoided_and_existing_source_is_not_rewritten(self):
        self.page.evaluate('before=result([slide("slide-1")]);after=result([slide("slide-1"),slide("slide-2")])')
        self.mount()
        self.page.locator('#deck-compose-load-nodes').click()
        self.page.wait_for_function('document.querySelector("#deck-compose-catalog-status").textContent.includes("Existing slides: 1")')
        self.add_slide()
        self.assertEqual(self.page.locator('#deck-compose-id').input_value(), 'slide-2')
        self.prepare()
        self.assertIn('1 existing slides retained', self.page.locator('#deck-compose-status').inner_text())
        self.page.locator('#deck-compose-apply').click()
        self.assertEqual(self.source_spec()['slides'][0]['id'], 'slide-2')

    def test_source_change_revokes_prepared_review_but_retains_draft(self):
        self.mount()
        self.add_slide()
        self.prepare()
        self.page.locator('#source').fill('flowchart LR\n new --> graph')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').count(), 0)
        self.assertEqual(self.page.locator('#deck-compose-members').input_value(), '["a","b"]')
        self.assertIn('Draft retained', self.page.locator('#deck-compose-status').inner_text())

    def test_editing_a_reviewed_draft_and_equal_text_document_switch_revoke_apply(self):
        self.mount()
        self.add_slide()
        self.prepare()
        self.page.locator('#deck-compose-caption').fill('Changed')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.page.locator('#deck-compose-caption').fill('')
        self.prepare()
        self.page.evaluate('composer.sourceChanged(true)')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertEqual(self.page.evaluate('inputs'), 0)

    def test_cancel_during_render_ignores_late_result_and_restores_controls(self):
        self.mount()
        self.add_slide()
        self.page.evaluate('window.holdRender=true')
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('typeof finishRender==="function"')
        self.page.locator('#deck-compose-cancel').click()
        self.page.evaluate('finishRender(before)')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertFalse(self.page.locator('#deck-compose-validate').is_disabled())
        self.assertEqual(self.page.evaluate('calls.length'), 1)

    def test_failed_membership_validation_never_installs_a_preview_or_changes_source(self):
        self.mount()
        self.add_slide('["missing"]')
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("missing node")')
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').count(), 0)
        self.assertEqual(self.page.evaluate('inputs'), 0)
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())

    def test_missing_svg_references_and_unsupported_families_block_apply(self):
        self.mount()
        self.add_slide()
        self.page.evaluate('after.svg=svg.replace("fm-node-b-1","absent")')
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("missing SVG")')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.page.evaluate('after=result()')
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("No presentation resolved")')
        self.assertEqual(self.page.evaluate('inputs'), 0)

    def test_player_failure_and_asset_timeout_do_not_authorize_apply(self):
        self.page.evaluate('options.timeoutMs=120')
        self.mount()
        self.add_slide()
        self.page.evaluate('runtime=>{assets.runtime=runtime}', 'window.FmDeckRuntime={mount(){throw Error("player broke")}}')
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("player broke")')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.page.evaluate('composer.dispose();document.querySelector("#panel").replaceChildren(); options.loadAssets=()=>new Promise(()=>{});window.composer=exports.mountDeckComposer(options)')
        self.add_slide()
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("timed out")')
        self.assertFalse(self.page.locator('#deck-compose-validate').is_disabled())
        self.assertEqual(self.page.evaluate('inputs'), 0)

    def test_draft_import_and_real_json_download_do_not_change_source(self):
        self.mount(real_download=True)
        spec = {'title': 'Imported 雪', 'options': {'autoAdvanceMs': 3000}, 'slides': [{'id': 'imported', 'nodes': ['a'], 'reveal': 'auto'}]}
        self.page.locator('#deck-compose-import').set_input_files({'name': 'composition.json', 'mimeType': 'application/json', 'buffer': json.dumps(spec, ensure_ascii=False).encode()})
        self.page.wait_for_function('document.querySelector("#deck-compose-id").value==="imported"')
        with self.page.expect_download() as receive:
            self.page.locator('#deck-compose-download').click()
        downloaded = json.loads(Path(receive.value.path()).read_text())
        self.assertEqual(downloaded['title'], 'Imported 雪')
        self.assertEqual(downloaded['options'], {'autoAdvanceMs': 3000})
        self.assertEqual(downloaded['slides'][0]['nodes'], ['a'])
        self.assertEqual(self.page.evaluate('inputs'), 0)
        self.assertEqual(self.page.evaluate('calls.length'), 0)

    def test_invalid_utf8_import_preserves_the_existing_draft(self):
        self.mount()
        self.add_slide()
        self.page.locator('#deck-compose-import').set_input_files({'name': 'invalid.json', 'mimeType': 'application/json', 'buffer': b'\xff'})
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("unavailable")')
        self.assertEqual(self.page.locator('#deck-compose-id').input_value(), 'slide-1')
        self.assertEqual(self.page.evaluate('inputs'), 0)

    def test_invalid_member_json_does_not_get_discarded_when_switching_slides(self):
        self.mount()
        self.add_slide()
        self.page.locator('#deck-compose-members').fill('["unfinished')
        self.page.locator('#deck-compose-add').click()
        self.assertEqual(self.page.locator('#deck-compose-members').input_value(), '["unfinished')
        self.assertEqual(self.page.locator('#deck-compose-slides option').count(), 1)
        self.assertIn('Draft not changed', self.page.locator('#deck-compose-status').inner_text())

    def test_catalog_is_bounded_searchable_and_never_mounts_label_markup(self):
        self.page.evaluate(r'''before.svg='<svg xmlns="http://www.w3.org/2000/svg">'+Array.from({length:250},(_,i)=>
          `<g id="fm-node-n${i}-${i}" data-id="n${i}"><title>&lt;img src=x onerror=alert(1)&gt; ${i}</title></g>`).join('')+'</svg>' ''')
        self.mount()
        self.add_slide('[]')
        self.page.locator('#deck-compose-load-nodes').click()
        self.page.wait_for_function('document.querySelectorAll("#deck-compose-nodes input").length===200')
        self.assertEqual(self.page.locator('#panel img').count(), 0)
        self.page.locator('#deck-compose-search').fill('n249')
        self.assertEqual(self.page.locator('#deck-compose-nodes input').count(), 1)
        self.page.locator('#deck-compose-nodes input').check()
        self.assertEqual(json.loads(self.page.locator('#deck-compose-members').input_value()), ['n249'])

    def test_legacy_node_variants_are_deduplicated_and_chart_marks_are_not_invented(self):
        catalog = self.page.evaluate(r'''() => exports.deckNodeCatalog('<svg xmlns="http://www.w3.org/2000/svg">' +
          '<g id="fm-node-a-0" data-id="a"/><g id="fm-node-a-0-mirror-header" data-id="a"/>' +
          '<rect id="chart-part" data-id="invented"/></svg>')''')
        self.assertEqual(catalog, [{'id': 'a', 'label': 'a'}])

    def test_close_reopen_and_dispose_preserve_draft_but_cancel_authority(self):
        self.mount()
        self.add_slide()
        self.prepare()
        self.page.evaluate('composer.suspend()')
        self.assertTrue(self.page.locator('#deck-compose-validate').is_disabled())
        self.page.evaluate('composer.resume()')
        self.assertFalse(self.page.locator('#deck-compose-validate').is_disabled())
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertEqual(self.page.locator('#deck-compose-id').input_value(), 'slide-1')
        self.page.evaluate('composer.dispose();composer.dispose()')
        self.assertEqual(self.page.evaluate('disposals'), 1)
        self.assertTrue(self.page.locator('#deck-compose-add').is_disabled())

    def test_actual_deck_panel_lazily_mounts_composer_and_revokes_equal_document_switch(self):
        self.page.evaluate('window.workspace=exports.mountDeckEditor({sourceEl:options.sourceEl,panelEl:options.panelEl,loadModule:async()=>({}),loadAssets:options.loadAssets})')
        initial = self.page.evaluate('creations')
        self.assertEqual(self.page.locator('#deck-compose-add').count(), 0)
        self.page.locator('#deck-editor-composition summary').click()
        self.page.wait_for_selector('#deck-compose-add')
        self.assertEqual(self.page.evaluate('creations'), initial)
        self.add_slide()
        self.prepare()
        self.assertEqual(self.page.evaluate('creations'), initial + 1)
        self.page.evaluate('workspace.documentChanged()')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.page.locator('#deck-editor-close').click()
        self.assertTrue(self.page.locator('#panel').is_hidden())
        self.page.evaluate('workspace.open()')
        self.assertEqual(self.page.locator('#deck-compose-id').input_value(), 'slide-1')
        self.assertFalse(self.page.locator('#deck-compose-add').is_disabled())
        self.page.evaluate('workspace.dispose()')
        self.assertTrue(self.page.locator('#deck-compose-add').is_disabled())

    def test_draft_preview_does_not_replace_committed_presentation_exports_until_apply(self):
        self.page.evaluate('''async () => {
          before=result([slide('existing')]); after=result([slide('existing'),slide()]);
          window.workspace=exports.mountDeckEditor({sourceEl:options.sourceEl,panelEl:options.panelEl,
            loadModule:async()=>({}),loadAssets:options.loadAssets});
          await workspace.open();
        }''')
        original_html = self.page.evaluate('workspace.exportHtml().text')
        self.page.locator('#deck-editor-composition summary').click()
        self.page.wait_for_selector('#deck-compose-add')
        self.add_slide()
        self.prepare()
        self.assertEqual(self.page.locator('#deck-editor-preview iframe').count(), 1)
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').count(), 1)
        self.assertEqual(self.page.evaluate('workspace.exportHtml().text'), original_html)
        self.assertEqual(self.page.evaluate('JSON.parse(workspace.exportManifest().text).slides.map(s=>s.id)'), ['existing'])
        self.page.locator('#deck-compose-apply').click()
        self.page.wait_for_function('document.querySelector("#deck-editor-status").textContent.startsWith("2 slides ready")')
        self.assertEqual(self.page.evaluate('JSON.parse(workspace.exportManifest().text).slides.map(s=>s.id)'), ['existing', 'slide-1'])
        self.assertEqual(self.page.evaluate('inputs'), 1)
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').count(), 0)

    def test_existing_presentation_download_and_close_reopen_still_work(self):
        self.page.evaluate('''async () => {
          before=result([slide('existing')]);
          window.workspace=exports.mountDeckEditor({sourceEl:options.sourceEl,panelEl:options.panelEl,
            loadModule:async()=>({}),loadAssets:options.loadAssets});
          await workspace.open();
        }''')
        with self.page.expect_download() as received:
            self.page.locator('#deck-editor-save-html').click()
        download = received.value
        self.assertEqual(download.suggested_filename, 'diagram-deck.html')
        downloaded = Path(download.path()).read_text()
        self.assertIn('Content-Security-Policy', downloaded)
        self.assertIn('existing', downloaded)
        self.page.locator('#deck-editor-close').click()
        self.assertEqual(self.page.locator('#deck-editor-preview iframe').count(), 0)
        self.assertTrue(self.page.locator('#deck-editor-save-html').is_disabled())
        self.page.evaluate('workspace.open()')
        self.assertFalse(self.page.locator('#deck-editor-save-html').is_disabled())
        self.assertEqual(self.page.locator('#deck-editor-preview iframe').count(), 1)
        self.assertEqual(self.page.evaluate('inputs'), 0)

    def test_typing_before_unchanged_composition_preserves_editing_and_never_reverts_new_diagram_text(self):
        self.mount()
        self.add_slide()
        self.prepare()
        self.page.locator('#deck-compose-apply').click()
        previous = self.page.locator('#source').input_value()
        self.page.locator('#source').fill('%% diagram edited\n' + previous)
        self.assertIn('still editable', self.page.locator('#deck-compose-status').inner_text())
        self.assertTrue(self.page.locator('#deck-compose-undo').is_disabled())
        self.page.evaluate('baseSource="%% diagram edited\\n"+baseSource')
        self.page.locator('#deck-compose-caption').fill('New narration')
        self.page.evaluate('after=result([slide("slide-1",{caption:"New narration"})])')
        self.prepare()
        self.page.locator('#deck-compose-apply').click()
        current = self.page.locator('#source').input_value()
        self.assertTrue(current.startswith('%% diagram edited\n'))
        self.assertEqual(current.count('%%{deck:'), 1)
        self.assertEqual(self.source_spec()['slides'][0]['caption'], 'New narration')

    def test_source_undo_redo_are_exact_and_do_not_discard_the_draft(self):
        self.mount()
        self.add_slide()
        original = self.page.locator('#source').input_value()
        self.prepare()
        self.page.locator('#deck-compose-apply').click()
        applied = self.page.locator('#source').input_value()
        self.page.locator('#deck-compose-undo').click()
        self.assertEqual(self.page.locator('#source').input_value(), original)
        self.assertEqual(self.page.locator('#deck-compose-members').input_value(), '["a","b"]')
        self.page.locator('#deck-compose-redo').click()
        self.assertEqual(self.page.locator('#source').input_value(), applied)
        self.assertEqual(self.page.evaluate('inputs'), 3)
        self.page.evaluate('composer.sourceChanged(true)')
        self.assertTrue(self.page.locator('#deck-compose-undo').is_disabled())

    def test_late_player_failure_revokes_an_already_reviewed_presentation(self):
        self.mount()
        self.add_slide()
        self.prepare()
        frame = self.page.frame_locator('#deck-compose-preview iframe')
        frame.locator('body').evaluate('() => { setTimeout(()=>{throw Error("late playback failure")},0); }')
        self.page.wait_for_function('document.querySelector("#deck-compose-status").textContent.includes("late playback failure")')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').count(), 0)
        self.assertEqual(self.page.evaluate('inputs'), 0)

    def test_source_change_during_asset_loading_ignores_the_late_asset_result(self):
        self.page.evaluate('() => { options.loadAssets=()=>new Promise(resolve=>window.finishAssets=resolve); }')
        self.mount()
        self.add_slide()
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('typeof finishAssets==="function"')
        self.page.locator('#source').fill('changed source')
        self.page.evaluate('finishAssets(assets)')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertEqual(self.page.locator('#deck-compose-preview iframe').count(), 0)

    def test_edit_and_revert_during_render_cannot_resurrect_an_old_review(self):
        self.mount()
        self.add_slide()
        original = self.page.locator('#source').input_value()
        self.page.evaluate('window.holdRender=true')
        self.page.locator('#deck-compose-validate').click()
        self.page.wait_for_function('typeof finishRender==="function"')
        self.page.locator('#source').fill('temporary source')
        self.page.locator('#source').fill(original)
        self.page.evaluate('finishRender(before)')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertEqual(self.page.evaluate('calls.length'), 1)

    def test_actual_playground_publishes_to_all_consumers_and_revokes_document_identity(self):
        def module_url(code):
            return 'data:text/javascript;base64,' + base64.b64encode(code.encode()).decode()

        wrapper = module_url(f'''import {{mountDeckEditor as mount}} from {json.dumps(self.module_url)};
          export function mountDeckEditor(options) {{
            window.pageDeck=mount({{...options,loadAssets:async()=>window.assets}}); return window.pageDeck;
          }}''')
        documents = module_url('''export function mountDocumentWorkspace(options) {
          window.documentCallbacks=options;
          return {sourceChanged(){window.documentUpdates++},dispose(){},saveArtifact(){}};
        }''')
        share = module_url('export function mountShareControls(){return {sourceChanged(){window.shareUpdates++},dispose(){}}}')
        image = module_url('export function mountImageExport(){return {sourceChanged(){window.imageUpdates++},dispose(){}}}')
        source = module_url('export function createSourceEditorBackend(){return window.makeBackend()}')
        html = MODULE.with_name('playground.html').read_text()
        html = html.replace('import.meta.url', '"https://example.invalid/web/playground.html"')
        for name, url in [('fm-deck-editor.js', wrapper), ('fm-document.js', documents),
                          ('fm-share.js', share), ('fm-image-export.js', image), ('fm-source-editor.js', source)]:
            html = html.replace(f'import("./{name}")', f'import({json.dumps(url)})')
        self.page.evaluate('''() => {
          window.documentUpdates=0;window.shareUpdates=0;window.imageUpdates=0;
          window.Worker=class {
            postMessage(msg) {
              if(msg.kind==='init')queueMicrotask(()=>this.onmessage?.({data:{kind:'ready',requested:'svgInWorker',target:'svgInWorker'}}));
              if(msg.kind==='render')queueMicrotask(()=>this.onmessage?.({data:{kind:'completed',requestId:msg.requestId,
                svg:'<svg xmlns="http://www.w3.org/2000/svg" id="normal-preview"/>'}}));
            }
            terminate(){}
          };
        }''')
        self.page.set_content(html)
        self.page.wait_for_function('typeof documentCallbacks==="object"')
        self.page.evaluate('baseSource=document.querySelector("#src").value; document.querySelector("#src").addEventListener("input",()=>inputs++)')
        self.page.locator('#edit-deck').click()
        self.page.locator('#deck-editor-composition summary').click()
        self.page.wait_for_selector('#deck-compose-add')
        self.add_slide()
        self.prepare()
        counts = self.page.evaluate('[documentUpdates,shareUpdates,imageUpdates]')
        self.page.locator('#deck-compose-apply').click()
        self.assertEqual(self.page.evaluate('inputs'), 1)
        self.assertTrue(all(after > before for after, before in zip(self.page.evaluate('[documentUpdates,shareUpdates,imageUpdates]'), counts)))
        self.assertEqual(self.page.locator('#normal-preview').count(), 1)
        self.prepare()
        self.page.evaluate('documentCallbacks.onDocumentChange()')
        self.assertTrue(self.page.locator('#deck-compose-apply').is_disabled())
        self.assertTrue(self.page.locator('#deck-compose-undo').is_disabled())
        self.page.evaluate('dispatchEvent(new PageTransitionEvent("pagehide",{persisted:false}))')
        self.assertTrue(self.page.locator('#deck-compose-add').is_disabled())


if __name__ == '__main__':
    unittest.main(verbosity=2)
