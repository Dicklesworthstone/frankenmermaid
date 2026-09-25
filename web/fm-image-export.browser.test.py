#!/usr/bin/env python3
"""Real Chromium tests for image serialization, downloads, and export controls.

Run: python web/fm-image-export.browser.test.py
Requires Playwright and Chromium; no WASM build or network service is needed. The rendering
backend is injected so the tests exercise export/browser behavior independently of Rust layout.
"""
import base64
import os
from pathlib import Path
import shutil
import unittest

from playwright.sync_api import sync_playwright

MODULE = Path(__file__).with_name("fm-image-export.js")


class ImageExportBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get("CHROMIUM") or shutil.which("chromium") or shutil.which("chromium-browser")
        cls.browser = cls.playwright.chromium.launch(
            headless=True, args=["--no-sandbox"], **({"executable_path": executable} if executable else {})
        )
        cls.module_url = "data:text/javascript;base64," + base64.b64encode(MODULE.read_bytes()).decode()

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.context = self.browser.new_context(accept_downloads=True)
        self.page = self.context.new_page()
        self.page.set_content('<textarea id="source">flowchart TD\n a[Fresh] --> b</textarea>'
                              '<section id="panel"></section><div id="preview"><svg id="stale-preview"/></div>')
        self.page.evaluate("async url => { window.exports = await import(url); }", self.module_url)

    def tearDown(self):
        self.context.close()

    def mount(self, injected_save=True):
        self.page.evaluate("""injected => {
          window.saved = []; window.rendered = []; window.cancelCount = 0; window.backendDisposed = false;
          window.backend = {
            target: 'test SVG worker',
            renderSource: async source => { rendered.push(source); return { svg:
              '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><text>Fresh ✓</text></svg>' }; },
            cancel: () => { cancelCount++; }, dispose: () => { backendDisposed = true; },
          };
          window.workspace = exports.mountImageExport({ sourceEl: document.querySelector('#source'),
            panelEl: document.querySelector('#panel'), backend,
            ...(injected ? { saveArtifact: artifact => { saved.push(artifact); } } : {}) });
        }""", injected_save)

    def test_svg_is_valid_standalone_unicode_xml(self):
        result = self.page.evaluate("""() => exports.svgArtifact(
          '<svg xmlns="http://www.w3.org/2000/svg" viewBox="-10 -20 300 200"><text>雪 🙂 &amp; &lt;safe&gt;</text></svg>',
          { filename: 'example.mmd' })""")
        self.assertEqual(result["filename"], "example.svg")
        self.assertEqual(result["mime"], "image/svg+xml;charset=utf-8")
        self.assertIn('viewBox="-10 -20 300 200"', result["text"])
        self.assertIn('雪 🙂 &amp; &lt;safe&gt;', result["text"])

    def test_invalid_empty_foreign_namespace_and_doctype_svg_fail(self):
        for svg in ["", "<svg>", "<svg/>", '<html xmlns="http://www.w3.org/1999/xhtml"/>',
                    '<!DOCTYPE svg><svg xmlns="http://www.w3.org/2000/svg"/>']:
            error = self.page.evaluate("""svg => {
              try { exports.svgArtifact(svg); return null; } catch (error) { return error.message; }
            }""", svg)
            self.assertIsNotNone(error, svg)

    def test_export_controls_render_source_not_stale_preview(self):
        self.mount()
        self.page.locator('#image-export-name').fill('architecture.mmd')
        self.page.locator('#image-export-svg').click()
        self.page.wait_for_function('saved.length === 1')
        self.assertEqual(self.page.evaluate('saved[0].filename'), 'architecture.svg')
        self.assertIn('Fresh ✓', self.page.evaluate('saved[0].text'))
        self.assertNotIn('stale-preview', self.page.evaluate('saved[0].text'))
        self.assertEqual(self.page.evaluate('rendered[0]'), self.page.locator('#source').input_value())
        self.assertEqual(self.page.locator('#stale-preview').count(), 1)
        self.assertIn('test SVG worker', self.page.locator('#image-export-status').inner_text())

    def test_typing_invalidates_pending_export_and_restores_controls(self):
        self.mount()
        self.page.evaluate('() => { backend.renderSource = () => new Promise(resolve => { window.finish = resolve; }); }')
        self.page.locator('#image-export-svg').click()
        self.assertTrue(self.page.locator('#image-export-svg').is_disabled())
        self.page.locator('#source').fill('sequenceDiagram\n A->>B: changed')
        self.page.evaluate('finish({ svg: \'<svg xmlns="http://www.w3.org/2000/svg"/>\' })')
        self.page.wait_for_function('document.querySelector("#image-export-cancel").disabled')
        self.assertEqual(self.page.evaluate('saved.length'), 0)
        self.assertFalse(self.page.locator('#image-export-svg').is_disabled())
        self.assertIn('source changed', self.page.locator('#image-export-status').inner_text())

    def test_programmatic_document_change_and_dispose_abort_exports(self):
        self.mount()
        self.page.evaluate('() => { backend.renderSource = () => new Promise(resolve => { window.finish = resolve; }); }')
        self.page.locator('#image-export-svg').click()
        self.page.evaluate("workspace.sourceChanged(); workspace.dispose(); finish({svg: '<svg/>'});")
        self.assertTrue(self.page.evaluate('backendDisposed'))
        self.assertTrue(self.page.locator('#image-export-svg').is_disabled())
        self.assertEqual(self.page.evaluate('saved.length'), 0)

    def test_real_svg_download_does_not_mutate_editable_source(self):
        self.mount(injected_save=False)
        source = self.page.locator('#source').input_value()
        with self.page.expect_download() as received:
            self.page.locator('#image-export-svg').click()
        download = received.value
        self.assertEqual(download.suggested_filename, 'diagram.svg')
        text = Path(download.path()).read_text()
        self.assertIn('xmlns="http://www.w3.org/2000/svg"', text)
        self.assertIn('Fresh ✓', text)
        self.assertEqual(self.page.locator('#source').input_value(), source)

    def test_render_failure_is_visible_without_downloading_old_preview(self):
        self.mount()
        self.page.evaluate('() => { backend.renderSource = async () => { throw new Error("invalid diagram"); }; }')
        self.page.locator('#image-export-svg').click()
        self.page.wait_for_function('document.querySelector("#image-export-status").textContent.includes("invalid diagram")')
        self.assertEqual(self.page.evaluate('saved.length'), 0)
        self.assertEqual(self.page.locator('#stale-preview').count(), 1)
        self.assertFalse(self.page.locator('#image-export-svg').is_disabled())


if __name__ == "__main__":
    unittest.main(verbosity=2)
