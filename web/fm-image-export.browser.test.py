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

    def test_png_pixels_preserve_negative_viewbox_origin_and_transparency(self):
        result = self.page.evaluate("""async () => {
          const artifact = await exports.pngArtifact('<svg xmlns="http://www.w3.org/2000/svg" viewBox="-5 -7 10 10">' +
            '<rect x="-5" y="-7" width="5" height="5" fill="red"/></svg>', {scale: 2, filename: 'negative.mmd'});
          const bitmap = await createImageBitmap(artifact.blob);
          const canvas = document.createElement('canvas'); canvas.width = bitmap.width; canvas.height = bitmap.height;
          const ctx = canvas.getContext('2d'); ctx.drawImage(bitmap, 0, 0); bitmap.close();
          return { width: artifact.width, height: artifact.height, filename: artifact.filename, mime: artifact.blob.type,
            red: [...ctx.getImageData(2, 2, 1, 1).data], empty: [...ctx.getImageData(18, 18, 1, 1).data] };
        }""")
        self.assertEqual((result['width'], result['height']), (20, 20))
        self.assertEqual(result['filename'], 'negative.png')
        self.assertEqual(result['mime'], 'image/png')
        self.assertEqual(result['red'], [255, 0, 0, 255])
        self.assertEqual(result['empty'], [0, 0, 0, 0])

    def test_png_backgrounds_and_explicit_dimensions_without_viewbox(self):
        result = self.page.evaluate("""async () => {
          const pixels = [];
          for (const background of ['white', 'dark']) {
            const artifact = await exports.pngArtifact('<svg xmlns="http://www.w3.org/2000/svg" width="12px" height="8"/>',
              { scale: 3, background });
            const bitmap = await createImageBitmap(artifact.blob);
            const canvas = document.createElement('canvas'); canvas.width = bitmap.width; canvas.height = bitmap.height;
            const ctx = canvas.getContext('2d'); ctx.drawImage(bitmap, 0, 0); bitmap.close();
            pixels.push({ width: artifact.width, height: artifact.height, rgba: [...ctx.getImageData(0, 0, 1, 1).data] });
          }
          return pixels;
        }""")
        self.assertEqual(result, [{'width': 36, 'height': 24, 'rgba': [255, 255, 255, 255]},
                                  {'width': 36, 'height': 24, 'rgba': [17, 24, 39, 255]}])

    def test_png_oversize_and_external_assets_fail_before_image_allocation(self):
        result = self.page.evaluate("""async () => {
          const errors = []; let allocations = 0;
          const host = { DOMParser, XMLSerializer, URL: { createObjectURL() { allocations++; throw new Error('allocated'); } } };
          for (const svg of [
            '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10000 10000"/>',
            '<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%"/>',
            '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><image href="https://example.invalid/remote.png"/></svg>',
          ]) {
            try { await exports.pngArtifact(svg, {}, host); }
            catch (error) { errors.push(error.message); }
          }
          return { errors, allocations };
        }""")
        self.assertEqual(result['allocations'], 0)
        self.assertIn('allocation limit', result['errors'][0])
        self.assertIn('positive finite', result['errors'][1])
        self.assertIn('external image', result['errors'][2])

    def test_png_responsive_styles_do_not_clip_the_export(self):
        result = self.page.evaluate("""async () => {
          const artifact = await exports.pngArtifact('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10" ' +
            'style="width:1px;height:1px;max-width:1px;max-height:1px"><rect width="20" height="10" fill="blue"/></svg>', {scale: 4});
          const bitmap = await createImageBitmap(artifact.blob);
          const canvas = document.createElement('canvas'); canvas.width = bitmap.width; canvas.height = bitmap.height;
          const ctx = canvas.getContext('2d'); ctx.drawImage(bitmap, 0, 0); bitmap.close();
          return { width: artifact.width, height: artifact.height, corner: [...ctx.getImageData(79, 39, 1, 1).data] };
        }""")
        self.assertEqual(result, {'width': 80, 'height': 40, 'corner': [0, 0, 255, 255]})

    def test_cancelling_png_encoding_releases_canvas_urls_and_ignores_late_callbacks(self):
        self.page.evaluate("""() => {
          window.createdUrls = []; window.revokedUrls = [];
          const create = URL.createObjectURL.bind(URL), revoke = URL.revokeObjectURL.bind(URL);
          URL.createObjectURL = value => { const url = create(value); createdUrls.push(url); return url; };
          URL.revokeObjectURL = url => { revokedUrls.push(url); revoke(url); };
          HTMLCanvasElement.prototype.toBlob = function(callback) { window.encodingCanvas = this; window.lateCallback = callback; };
          window.abortPng = new AbortController();
          window.pendingPng = exports.pngArtifact('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>',
            {}, window, abortPng.signal).then(() => 'unexpected success', error => error.name);
        }""")
        self.page.wait_for_function('typeof lateCallback === "function"')
        self.page.evaluate('abortPng.abort()')
        self.assertEqual(self.page.evaluate('pendingPng'), 'AbortError')
        self.assertEqual(self.page.evaluate('[encodingCanvas.width, encodingCanvas.height]'), [0, 0])
        self.assertEqual(self.page.evaluate('createdUrls'), self.page.evaluate('revokedUrls'))
        self.assertEqual(self.page.evaluate('revokedUrls.length'), 1)
        self.page.evaluate('lateCallback(new Blob(["late"], {type: "image/png"}))')
        self.assertEqual(self.page.evaluate('revokedUrls.length'), 1)

    def test_null_png_encoding_and_decode_timeouts_report_failure(self):
        error = self.page.evaluate("""async () => {
          HTMLCanvasElement.prototype.toBlob = callback => callback(null);
          try { await exports.pngArtifact('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>'); }
          catch (error) { return error.message; }
        }""")
        self.assertIn('could not encode PNG', error)
        result = self.page.evaluate("""async () => {
          let revoked = 0;
          const host = { DOMParser, XMLSerializer, Blob, document,
            Image: class { removeAttribute() {} },
            URL: { createObjectURL: URL.createObjectURL.bind(URL), revokeObjectURL(url) { revoked++; URL.revokeObjectURL(url); } },
            setTimeout: setTimeout.bind(window), clearTimeout: clearTimeout.bind(window) };
          try { await exports.pngArtifact('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>', {timeoutMs: 5}, host); }
          catch (error) { return { error: error.message, revoked }; }
        }""")
        self.assertIn('decoding timed out', result['error'])
        self.assertEqual(result['revoked'], 1)

    def test_real_png_download_has_requested_resolution_and_preserves_source(self):
        import struct
        self.mount(injected_save=False)
        source = self.page.locator('#source').input_value()
        self.page.locator('#image-export-scale').select_option('3')
        self.page.locator('#image-export-background').select_option('white')
        with self.page.expect_download() as received:
            self.page.locator('#image-export-png').click()
        download = received.value
        self.assertEqual(download.suggested_filename, 'diagram.png')
        content = Path(download.path()).read_bytes()
        self.assertEqual(content[:8], b'\x89PNG\r\n\x1a\n')
        self.assertEqual(struct.unpack('>II', content[16:24]), (300, 150))
        self.assertIn('300 × 150 pixels', self.page.locator('#image-export-status').inner_text())
        self.assertEqual(self.page.locator('#source').input_value(), source)


    def mount_playground_with_transport_fixtures(self):
        """Execute the actual page/module with explicit renderer and document transport fixtures."""
        def module_url(code):
            return 'data:text/javascript;base64,' + base64.b64encode(code.encode()).decode()

        source_backend = module_url("""
          export function createSourceEditorBackend() {
            window.backendCreations++;
            return { target: 'playground export fixture',
              renderSource: async source => {
                window.imageSources.push(source);
                if (window.holdImageRender) return new Promise(resolve => { window.finishImageRender = resolve; });
                const escaped = source.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
                return { svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><text>' + escaped + '</text></svg>' };
              }, cancel() { window.imageCancellations++; }, dispose() { window.imageDisposals++; },
            };
          }
          export function mountSourceEditor() { return { render() {}, dispose() {} }; }
        """)
        document_backend = module_url("""
          export function mountDocumentWorkspace(options) {
            window.documentCallbacks = options;
            return { sourceChanged() {}, dispose() {}, saveArtifact() {} };
          }
        """)
        wasm = module_url('export default async function() {}')
        html = MODULE.with_name('playground.html').read_text()
        html = html.replace('import.meta.url', '"https://example.invalid/web/playground.html"')
        html = html.replace('import("./fm-image-export.js")', f'import("{self.module_url}")')
        html = html.replace('import("./fm-source-editor.js")', f'import("{source_backend}")')
        html = html.replace('import("./fm-document.js")', f'import("{document_backend}")')
        html = html.replace('import("../pkg/frankenmermaid.js")', f'import("{wasm}")')
        self.page.evaluate("""() => {
          window.backendCreations = 0; window.imageSources = []; window.imageCancellations = 0; window.imageDisposals = 0;
          window.Worker = class {
            postMessage(message) {
              if (message.kind === 'init') queueMicrotask(() => this.onmessage?.({data: {
                kind: 'ready', requested: 'svgInWorker', target: 'svgInWorker' }}));
              if (message.kind === 'render') queueMicrotask(() => this.onmessage?.({data: {
                kind: 'completed', requestId: message.requestId,
                svg: '<svg xmlns="http://www.w3.org/2000/svg" id="normal-preview"/>', }}));
            }
            terminate() {}
          };
        }""")
        self.page.set_content(html)
        self.page.wait_for_selector('#image-export-svg')
        self.page.wait_for_selector('#normal-preview', state='attached')

    def test_actual_playground_lazily_mounts_export_and_downloads_current_source(self):
        self.mount_playground_with_transport_fixtures()
        self.assertEqual(self.page.evaluate('backendCreations'), 0)
        source = 'sequenceDiagram\n A->>B: actual page source'
        self.page.locator('#src').fill(source)
        with self.page.expect_download() as received:
            self.page.locator('#image-export-svg').click()
        text = Path(received.value.path()).read_text()
        self.assertIn('actual page source', text)
        self.assertEqual(self.page.evaluate('imageSources'), [source])
        self.assertEqual(self.page.evaluate('backendCreations'), 1)
        self.assertEqual(self.page.locator('#normal-preview').count(), 1)
        self.assertEqual(self.page.locator('#src').input_value(), source)
        self.page.evaluate('dispatchEvent(new PageTransitionEvent("pagehide", {persisted: false}))')
        self.assertEqual(self.page.evaluate('imageDisposals'), 1)
        self.assertTrue(self.page.locator('#image-export-svg').is_disabled())

    def test_actual_playground_document_callbacks_invalidate_equal_source_exports(self):
        self.mount_playground_with_transport_fixtures()
        downloads = []
        self.page.on('download', lambda download: downloads.append(download))
        self.page.evaluate('window.holdImageRender = true')
        self.page.locator('#image-export-svg').click()
        self.page.wait_for_function('typeof finishImageRender === "function"')
        self.page.evaluate("""() => {
          documentCallbacks.onDocumentChange(); documentCallbacks.onChange();
          finishImageRender({svg: '<svg xmlns="http://www.w3.org/2000/svg"/>'});
        }""")
        self.page.wait_for_function('document.querySelector("#image-export-status").textContent.includes("source changed")')
        self.assertEqual(downloads, [])
        self.assertFalse(self.page.locator('#image-export-svg').is_disabled())
        self.assertGreater(self.page.evaluate('imageCancellations'), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
