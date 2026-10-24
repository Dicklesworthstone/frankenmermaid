#!/usr/bin/env python3
"""Chromium configuration-editor tests. Run: python web/fm-config-editor.browser.test.py

Exercises the real module, browser inputs, Blob downloads, file decoding and actual playground
wiring. WASM validation and existing renderer/document transports are explicitly injected;
these tests do not claim Rust semantic validation or native-engine rendering coverage.
"""
import base64
import json
import os
from pathlib import Path
import shutil
import unittest

from playwright.sync_api import sync_playwright

MODULE = Path(__file__).with_name("fm-config-editor.js")
VALIDATOR = """
  window.validatedSources = []; window.moduleLoads = 0; window.sourceEvents = [];
  window.validator = {
    validateInitDirectives(source) {
      validatedSources.push(source);
      if (window.holdValidation) return new Promise(resolve => { window.finishValidation = resolve; });
      return JSON.stringify({schemaVersion:'1.0.0', errors: window.validationErrors || []});
    }
  };
"""


class ConfigurationBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get('CHROMIUM') or shutil.which('chromium') or shutil.which('chromium-browser')
        cls.browser = cls.playwright.chromium.launch(headless=True, args=['--no-sandbox'],
            **({'executable_path': executable} if executable else {}))
        cls.module_url = 'data:text/javascript;base64,' + base64.b64encode(MODULE.read_bytes()).decode()

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.context = self.browser.new_context(accept_downloads=True)
        self.page = self.context.new_page()
        self.page.set_content('<textarea id="source">flowchart TD\n a[Original] --> b</textarea><section id="panel"></section>')
        self.page.evaluate('async url => { window.editorModule = await import(url); }', self.module_url)
        self.page.evaluate('() => {' + VALIDATOR + '}')

    def tearDown(self):
        self.context.close()

    def mount(self, source=None, timeout=15000):
        if source is not None:
            self.page.locator('#source').fill(source)
        self.page.evaluate('''timeout => {
          document.querySelector('#source').addEventListener('input', () => sourceEvents.push(document.querySelector('#source').value));
          window.workspace = editorModule.mountConfigurationEditor({
            sourceEl:document.querySelector('#source'), panelEl:document.querySelector('#panel'),
            loadModule:async () => { moduleLoads++; return validator; }, validationTimeoutMs:timeout });
          workspace.open();
        }''', timeout)

    def field(self, name):
        return self.page.locator('#config-editor-' + name)

    def change_text(self, name, value):
        self.field(name).fill(value)
        self.field(name).press('Tab')

    def validate(self):
        self.field('validate').click()
        self.page.wait_for_function("document.querySelector('#config-editor-cancel').disabled")
        self.assertIn('valid', self.field('status').inner_text())

    def test_opening_is_lazy_and_controls_change_only_the_draft_until_apply(self):
        self.mount()
        original = self.page.locator('#source').input_value()
        self.assertEqual(self.page.evaluate('moduleLoads'), 0)
        self.change_text('theme', 'dark')
        self.change_text('node-spacing', '48')
        self.assertEqual(self.page.locator('#source').input_value(), original)
        self.assertTrue(self.field('apply').is_disabled())
        self.validate()
        self.assertEqual(self.page.evaluate('moduleLoads'), 1)
        self.assertEqual(self.page.locator('#source').input_value(), original)
        self.assertIn('"nodeSpacing": 48', self.field('preview').inner_text())
        self.field('apply').click()
        source = self.page.locator('#source').input_value()
        self.assertTrue(source.endswith(original))
        self.assertIn('"theme": "dark"', source)
        self.assertEqual(self.page.evaluate('sourceEvents'), [source])
        self.assertEqual(self.page.evaluate('validatedSources'), [source])

    def test_existing_payload_changes_preserve_comments_other_fields_and_false(self):
        original = '%% Keep\n%%{init:{"themeVariables":{"custom":true},"flowchart":{"rankDir":"LR","nodeSpacing":0},"sequence":{"mirrorActors":false}}}%%\nflowchart\na-->b'
        self.mount(original)
        self.assertEqual(self.field('mirror-actors').input_value(), 'false')
        self.assertEqual(self.field('node-spacing').input_value(), '0')
        self.field('direction').select_option('bt')
        self.field('sequence-numbers').select_option('true')
        self.validate()
        self.field('apply').click()
        source = self.page.locator('#source').input_value()
        self.assertTrue(source.startswith('%% Keep\n%%{init:'))
        self.assertTrue(source.endswith('\nflowchart\na-->b'))
        draft = json.loads(self.field('draft').input_value())
        self.assertEqual(draft['themeVariables'], {'custom': True})
        self.assertEqual(draft['flowchart'], {'nodeSpacing': 0, 'direction': 'bt'})
        self.assertEqual(draft['sequence'], {'mirrorActors': False, 'showSequenceNumbers': True})

    def test_validation_errors_are_text_not_html_and_never_modify_source(self):
        self.mount()
        original = self.page.locator('#source').input_value()
        self.page.evaluate('''() => { window.validationErrors = [{field:'flowchart.nodeSpacing', value:'-1',
          message:'<img src=x onerror="window.injected=true"> must be nonnegative'}]; }''')
        self.change_text('node-spacing', '-1')
        self.field('validate').click()
        self.page.wait_for_function("document.querySelector('#config-editor-errors').textContent.includes('nonnegative')")
        self.assertEqual(self.page.locator('#source').input_value(), original)
        self.assertTrue(self.field('apply').is_disabled())
        self.assertEqual(self.field('errors').locator('img').count(), 0)
        self.assertFalse(self.page.evaluate('Boolean(window.injected)'))

    def test_typing_during_validation_keeps_draft_but_revokes_apply(self):
        self.mount()
        self.change_text('theme', 'dark')
        draft = self.field('draft').input_value()
        self.page.evaluate('window.holdValidation = true')
        self.field('validate').click()
        self.page.wait_for_function('typeof finishValidation === "function"')
        self.page.locator('#source').fill('flowchart TD\n new-->diagram')
        self.page.evaluate("finishValidation(JSON.stringify({schemaVersion:'1.0.0',errors:[]}))")
        self.assertEqual(self.field('draft').input_value(), draft)
        self.assertTrue(self.field('apply').is_disabled())
        self.assertTrue(self.field('validate').is_disabled())
        self.assertIn('Source changed', self.field('status').inner_text())
        self.field('reload').click()
        self.assertEqual(self.field('draft').input_value(), '{}')
        self.assertFalse(self.field('validate').is_disabled())

    def test_equal_source_document_replacement_revokes_a_prepared_edit(self):
        self.mount()
        self.change_text('theme', 'dark')
        self.validate()
        self.page.evaluate('workspace.sourceChanged()')
        self.assertTrue(self.field('apply').is_disabled())
        self.assertIn('retained', self.field('status').inner_text())
        self.assertFalse(self.page.locator('#source').input_value().startswith('%%{init:'))

    def test_harmless_redraw_does_not_revoke_preparation(self):
        self.mount()
        self.change_text('theme', 'dark')
        self.validate()
        self.page.evaluate('workspace.observeSource()')
        self.assertFalse(self.field('apply').is_disabled())
        self.page.evaluate("document.querySelector('#source').value += '\\n b-->c'; workspace.observeSource()")
        self.assertTrue(self.field('apply').is_disabled())

    def test_changing_draft_during_validation_ignores_late_success(self):
        self.mount()
        self.page.evaluate('window.holdValidation = true')
        self.field('validate').click()
        self.page.wait_for_function('typeof finishValidation === "function"')
        self.field('draft').fill('{"theme":"forest"}')
        self.page.evaluate("finishValidation(JSON.stringify({schemaVersion:'1.0.0',errors:[]}))")
        self.assertTrue(self.field('apply').is_disabled())
        self.assertFalse(self.field('validate').is_disabled())
        self.assertIn('Draft changed', self.field('status').inner_text())

    def test_undo_redo_restore_exact_source_and_external_edit_revokes_undo(self):
        original = '%% Keep\nflowchart LR\n a[雪]-->b'
        self.mount(original)
        self.change_text('theme', 'dark')
        self.validate()
        self.field('apply').click()
        changed = self.page.locator('#source').input_value()
        self.field('undo').click()
        self.assertEqual(self.page.locator('#source').input_value(), original)
        self.assertIn('Redo', self.field('undo').inner_text())
        self.field('undo').click()
        self.assertEqual(self.page.locator('#source').input_value(), changed)
        self.page.locator('#source').fill(changed + '\n b-->c')
        self.assertTrue(self.field('undo').is_disabled())

    def test_multiple_directives_can_be_selected_without_overwriting_each_other(self):
        original = '%%{init:{"theme":"dark"}}%%\n%%{init:{"sequence":{"mirrorActors":false}}}%%\nsequenceDiagram\nA->>B: Hello'
        self.mount(original)
        self.assertEqual(self.field('target').input_value(), '1')
        self.field('target').select_option('0')
        self.change_text('theme', 'forest')
        self.assertTrue(self.field('target').is_disabled())
        self.validate()
        self.field('apply').click()
        source = self.page.locator('#source').input_value()
        self.assertIn('"theme": "forest"', source)
        self.assertTrue(source.endswith(original[original.index('\n'):]))

    def test_json5_is_not_silently_reformatted_and_remains_text_editable(self):
        original = "%%{init:{theme:'dark', /* do not erase */}}%%\nflowchart TD\na-->b"
        self.mount(original)
        self.assertTrue(self.field('theme').is_disabled())
        self.assertIn('JSON5 is retained', self.field('visual-note').inner_text())
        self.field('draft').fill("{theme:'forest', /* do not erase */}")
        self.validate()
        self.field('apply').click()
        self.assertEqual(self.page.locator('#source').input_value(), original.replace("'dark'", "'forest'"))

    def test_real_config_download_is_valid_unicode_json_without_marking_source_saved(self):
        self.mount()
        before = self.page.locator('#source').input_value()
        self.field('draft').fill('{"themeVariables":{"custom":"雪 🙂"}}')
        self.validate()
        with self.page.expect_download() as received:
            self.field('download').click()
        artifact = received.value
        self.assertEqual(artifact.suggested_filename, 'diagram.config.json')
        self.assertEqual(json.loads(Path(artifact.path()).read_text()), {'themeVariables': {'custom': '雪 🙂'}})
        self.assertEqual(self.page.locator('#source').input_value(), before)
        self.assertEqual(self.page.evaluate('sourceEvents'), [])

    def test_file_import_accepts_utf8_bom_and_stays_unapplied_until_validation(self):
        self.mount()
        before = self.page.locator('#source').input_value()
        self.field('file').set_input_files({'name':'settings.json', 'mimeType':'application/json',
            'buffer': b'\xef\xbb\xbf{"theme":"dark","flowchart":{"nodeSpacing":24}}'})
        self.page.wait_for_function("document.querySelector('#config-editor-draft').value.includes('nodeSpacing')")
        self.assertEqual(self.field('theme').input_value(), 'dark')
        self.assertEqual(self.page.locator('#source').input_value(), before)
        self.assertTrue(self.field('apply').is_disabled())
        self.validate()
        self.field('apply').click()
        self.assertIn('"nodeSpacing":24', self.page.locator('#source').input_value())

    def test_invalid_encoding_and_large_imports_leave_existing_draft_intact(self):
        self.mount()
        self.change_text('theme', 'dark')
        draft = self.field('draft').input_value()
        for name, data in [('bad.json', b'\xff\xfe'), ('huge.json', b' ' * (256 * 1024 + 1))]:
            self.field('file').set_input_files({'name':name, 'mimeType':'application/json', 'buffer':data})
            self.page.wait_for_function("document.querySelector('#config-editor-status').textContent.includes('import failed')")
            self.assertEqual(self.field('draft').input_value(), draft)
            self.assertTrue(self.field('apply').is_disabled())
            self.assertFalse(self.field('validate').is_disabled())

    def test_closing_retains_dirty_draft_but_cancels_validation(self):
        self.mount()
        self.change_text('theme', 'dark')
        draft = self.field('draft').input_value()
        self.page.evaluate('window.holdValidation = true')
        self.field('validate').click()
        self.page.wait_for_function('typeof finishValidation === "function"')
        self.field('close').click()
        self.page.evaluate("finishValidation(JSON.stringify({schemaVersion:'1.0.0',errors:[]})); workspace.open()")
        self.assertEqual(self.field('draft').input_value(), draft)
        self.assertTrue(self.field('apply').is_disabled())
        self.assertFalse(self.field('validate').is_disabled())

    def test_timeout_and_disposal_ignore_late_validator_responses(self):
        self.mount(timeout=20)
        self.page.evaluate('window.holdValidation = true')
        self.field('validate').click()
        self.page.wait_for_function("document.querySelector('#config-editor-status').textContent.includes('timed out')")
        self.page.evaluate("finishValidation(JSON.stringify({schemaVersion:'1.0.0',errors:[]}))")
        self.assertTrue(self.field('apply').is_disabled())
        self.assertFalse(self.field('validate').is_disabled())
        self.page.evaluate('workspace.dispose()')
        self.assertTrue(self.field('validate').is_disabled())
        self.assertTrue(self.field('draft').is_disabled())

    def mount_playground(self):
        html = MODULE.with_name('playground.html').read_text()
        modules = {
            '/web/fm-config-editor.js': MODULE.read_text(),
            '/pkg/frankenmermaid.js': '''export default async function() { window.mainLoads++; }
              export function validateInitDirectives(source) { return window.validator.validateInitDirectives(source); }''',
            '/web/fm-source-editor.js': '''
              export function mountSourceEditor() { return {render() {}, dispose() {}}; }
              export function createSourceEditorBackend() { return {renderSource:async source => ({svg:source}), cancel() {}, dispose() {}}; }''',
            '/web/fm-image-export.js': '''export function mountImageExport({sourceEl,panelEl}) {
              const button = document.createElement('button'); button.id = 'export-fixture'; button.textContent='Export fixture';
              button.onclick = () => window.exportedSources.push(sourceEl.value); panelEl.append(button);
              return {sourceChanged() {window.exportInvalidations++;}, dispose() {}};
            }''',
            '/web/fm-document.js': '''export function mountDocumentWorkspace(options) {
              window.documentCallbacks = options;
              return {sourceChanged() { window.recoverySources.push(options.sourceEl.value); }, dispose() {}};
            }''',
            '/web/fm-share.js': '''export function mountShareControls() {
              return {sourceChanged() {window.sharedSources.push(document.querySelector('#src').value);}, dispose() {}};
            }''',
        }
        # Use in-memory modules rather than an HTTP service. Only import locations change;
        # the actual playground's event handlers and rendering/document wiring execute intact.
        html = html.replace('import.meta.url', '"https://example.invalid/web/playground.html"')
        for path, code in modules.items():
            relative = './' + path.removeprefix('/web/') if path.startswith('/web/') else '..' + path
            module_url = 'data:text/javascript;base64,' + base64.b64encode(code.encode()).decode()
            html = html.replace(f'import("{relative}")', f'import("{module_url}")')
        self.page.evaluate('() => {' + VALIDATOR + '''
          window.mainLoads = 0; window.renderedSources = []; window.recoverySources = []; window.sharedSources = [];
          window.exportedSources = []; window.exportInvalidations = 0;
          window.Worker = class {
            postMessage(message) {
              if (message.kind === 'init') queueMicrotask(() => this.onmessage?.({data:{kind:'ready',requested:'svgInWorker',target:'svgInWorker'}}));
              if (message.kind === 'render') {
                renderedSources.push(message.input);
                queueMicrotask(() => this.onmessage?.({data:{kind:'completed',requestId:message.requestId,svg:'<svg id="preview-fixture"/>'}}));
              }
            }
            terminate() {}
          };
        }''')
        self.page.set_content(html)
        self.page.wait_for_selector('#preview-fixture', state='attached')
        self.page.wait_for_selector('#export-fixture')

    def test_actual_playground_routes_applied_settings_through_render_recovery_share_and_export(self):
        self.mount_playground()
        self.assertEqual(self.page.evaluate('mainLoads'), 0)
        self.page.locator('#edit-config').click()
        self.assertEqual(self.page.evaluate('mainLoads'), 0)
        self.change_text('theme', 'dark')
        self.validate()
        self.field('apply').click()
        source = self.page.locator('#src').input_value()
        self.page.locator('#export-fixture').click()
        for name in ['renderedSources', 'recoverySources', 'sharedSources', 'exportedSources']:
            self.assertEqual(self.page.evaluate(f'{name}.at(-1)'), source, name)
        self.assertIn('"theme": "dark"', source)
        self.assertEqual(self.page.evaluate('validatedSources.at(-1)'), source)
        self.assertEqual(self.page.evaluate('mainLoads'), 1)
        self.assertEqual(self.page.locator('#preview-fixture').count(), 1)

    def test_actual_playground_equal_text_document_switch_and_pagehide_revoke_settings(self):
        self.mount_playground()
        self.page.locator('#edit-config').click()
        self.change_text('theme', 'dark')
        self.validate()
        self.page.evaluate('documentCallbacks.onDocumentChange(); documentCallbacks.onChange()')
        self.assertTrue(self.field('apply').is_disabled())
        self.assertIn('Source changed', self.field('status').inner_text())
        self.page.evaluate('dispatchEvent(new PageTransitionEvent("pagehide", {persisted:false}))')
        self.assertTrue(self.field('draft').is_disabled())
        self.assertTrue(self.field('validate').is_disabled())


if __name__ == '__main__':
    unittest.main(verbosity=2)
