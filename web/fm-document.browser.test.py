"""File lifecycle integration with the real DOM, File decoding, and Blob downloads.

Run: python web/fm-document.browser.test.py
Requires Playwright and Chromium. Browser navigation is not needed. The playground
integration uses explicit renderer/source-editor/deck transport fixtures, never fake WASM files.
"""
import os
from pathlib import Path
import re
import shutil
import unittest
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent


class DocumentBrowserTests(unittest.TestCase):
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
        self.context = self.browser.new_context(accept_downloads=True)
        self.addCleanup(self.context.close)
        self.page = self.context.new_page()
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.set_content('<textarea id="source">flowchart TD\n a[Initial]\n</textarea><section id="panel"></section>')
        self.page.evaluate("""async (code) => {
          const url = URL.createObjectURL(new Blob([code], {type:'text/javascript'}));
          try { globalThis.docModule = await import(url); } finally { URL.revokeObjectURL(url); }
          globalThis.sourceEl = document.getElementById('source');
          globalThis.switches = 0; globalThis.renders = []; globalThis.confirmations = [];
          globalThis.confirmResult = true;
          globalThis.options = {
            sourceEl, panelEl: document.getElementById('panel'),
            onChange: () => renders.push(sourceEl.value),
            onDocumentChange: () => switches++,
            confirmReplace: async text => { confirmations.push(text); return confirmResult; }
          };
          globalThis.workspace = docModule.mountDocumentWorkspace(options);
        }""", (ROOT / "web/fm-document.js").read_text())

    def tearDown(self):
        self.assertEqual(self.errors, [])

    def upload(self, data, name="system.mmd"):
        self.page.locator("#document-file").set_input_files({"name": name, "mimeType": "text/plain", "buffer": data})
        self.page.wait_for_function("document.getElementById('document-message').textContent !== 'Reading source…'")

    def test_real_file_open_download_and_original_bytes(self):
        original = '\ufeff%% café 🐉\r\nflowchart TD\r\n A[世界]  \n B[Beta]\r\n'
        self.upload(original.encode(), "architecture.mmd")
        self.assertEqual(self.page.locator("#source").input_value(), original[1:].replace("\r\n", "\n"))
        self.assertEqual(self.page.locator("#document-name").text_content(), "architecture.mmd")
        self.assertEqual(self.page.evaluate("switches"), 1)
        with self.page.expect_download() as pending:
            self.page.locator("#document-save").click()
        download = pending.value
        self.assertEqual(download.suggested_filename, "architecture.mmd")
        self.assertEqual(Path(download.path()).read_bytes(), original.encode())
        self.page.locator("#source").fill(original[1:].replace("\r\n", "\n").replace("Beta", "Edited"))
        with self.page.expect_download() as pending:
            self.page.locator("#document-save").click()
        self.assertEqual(Path(pending.value.path()).read_bytes(), original.replace("Beta", "Edited").encode())

    def test_invalid_encoding_and_unsupported_files_leave_current_source_intact(self):
        self.page.locator("#source").fill("current unsaved source")
        for data, name in [(bytes([0xc3, 0x28]), "bad.mmd"), (b"a\0b", "bad.dot"), (b"<svg/>", "bad.svg")]:
            self.upload(data, name)
            self.assertIn("Open failed", self.page.locator("#document-message").text_content())
            self.assertEqual(self.page.locator("#source").input_value(), "current unsaved source")
        self.assertEqual(self.page.evaluate("switches"), 0)
        self.assertEqual(self.page.evaluate("confirmations.length"), 0)

    def test_dirty_replacement_requires_approval_and_cancel_preserves_source(self):
        self.page.locator("#source").fill("unexported")
        self.page.evaluate("confirmResult = false")
        self.upload(b"digraph {a->b}", "input.dot")
        self.assertEqual(self.page.locator("#source").input_value(), "unexported")
        self.assertEqual(self.page.evaluate("confirmations.length"), 1)
        self.page.evaluate("confirmResult = true")
        self.upload(b"digraph {a->b}", "input.dot")
        self.assertEqual(self.page.locator("#source").input_value(), "digraph {a->b}")
        self.assertEqual(self.page.evaluate("switches"), 1)

    def test_delayed_file_reads_cannot_overwrite_newer_typing(self):
        self.page.evaluate("""() => {
          globalThis.opening = workspace.openFile({name:'late.mmd', size:3,
            arrayBuffer: () => new Promise(resolve => { globalThis.releaseRead = () => resolve(new TextEncoder().encode('old').buffer); })});
        }""")
        self.page.locator("#source").fill("newer typing")
        self.page.evaluate("releaseRead()")
        self.assertFalse(self.page.evaluate("opening"))
        self.assertEqual(self.page.locator("#source").input_value(), "newer typing")
        self.assertIn("source changed", self.page.locator("#document-message").text_content())

    def test_overlapping_reads_and_disposal_cannot_commit_old_files(self):
        self.page.evaluate("""() => {
          globalThis.old = workspace.openFile({name:'old.mmd', size:3,
            arrayBuffer: () => new Promise(resolve => { globalThis.releaseOld = () => resolve(new TextEncoder().encode('old').buffer); })});
        }""")
        self.upload(b"new", "new.mmd")
        self.page.evaluate("releaseOld()")
        self.assertFalse(self.page.evaluate("old"))
        self.assertEqual(self.page.locator("#source").input_value(), "new")
        self.page.evaluate("""() => {
          globalThis.old = workspace.openFile({name:'late.mmd', size:3,
            arrayBuffer: () => new Promise(resolve => { globalThis.releaseOld = () => resolve(new TextEncoder().encode('old').buffer); })});
          workspace.dispose(); releaseOld();
        }""")
        self.assertFalse(self.page.evaluate("old"))
        self.assertEqual(self.page.locator("#source").input_value(), "new")

    def test_typing_during_async_confirmation_invalidates_approval(self):
        self.page.evaluate("""() => {
          workspace.dispose(); panel.replaceChildren();
          workspace = docModule.mountDocumentWorkspace({...options,
            confirmReplace: () => new Promise(resolve => { globalThis.approve = () => resolve(true); })});
          sourceEl.value = 'unsaved'; workspace.sourceChanged();
          globalThis.opening = workspace.openFile(new File(['old'], 'old.mmd'));
        }""")
        self.page.wait_for_function("typeof approve === 'function'")
        self.page.locator("#source").fill("newer")
        self.page.evaluate("approve()")
        self.assertFalse(self.page.evaluate("opening"))
        self.assertEqual(self.page.locator("#source").input_value(), "newer")

    def test_drop_opens_one_file_and_multi_drop_does_not_replace_source(self):
        self.page.evaluate("""() => {
          const transfer = new DataTransfer();
          transfer.items.add(new File(['a[A]'], 'drop.mmd'));
          sourceEl.dispatchEvent(new DragEvent('drop', {dataTransfer: transfer, cancelable:true}));
        }""")
        self.page.wait_for_function("sourceEl.value === 'a[A]'")
        self.page.evaluate("""() => {
          const transfer = new DataTransfer();
          transfer.items.add(new File(['b[B]'], 'one.mmd')); transfer.items.add(new File(['c[C]'], 'two.mmd'));
          const event = new DragEvent('drop', {dataTransfer: transfer, cancelable:true});
          sourceEl.dispatchEvent(event); if (!event.defaultPrevented) throw Error('file navigation not prevented');
        }""")
        self.assertEqual(self.page.locator("#source").input_value(), "a[A]")
        self.assertIn("one source file", self.page.locator("#document-message").text_content())

    def test_failed_save_retains_dirty_state_and_stale_completion_cannot_clear_new_changes(self):
        self.page.evaluate("""() => {
          workspace.dispose(); panel.replaceChildren();
          globalThis.failDownload = true;
          workspace = docModule.mountDocumentWorkspace({...options, saveFile: () => {
            if (failDownload) throw Error('download fixture refused');
            return new Promise(resolve => { globalThis.finishSave = resolve; });
          }});
        }""")
        self.page.locator("#source").fill("edited")
        self.page.locator("#document-save").click()
        self.assertIn("Download failed", self.page.locator("#document-message").text_content())
        self.assertIn("unexported changes", self.page.locator("#document-name").text_content())
        self.page.evaluate("() => { failDownload = false; globalThis.saving = workspace.saveSource(); }")
        self.page.locator("#source").fill("edited again")
        self.page.evaluate("finishSave()")
        self.assertTrue(self.page.evaluate("saving"))
        self.assertIn("unexported changes", self.page.locator("#document-name").text_content())

    def test_new_resets_file_name_and_source_without_needing_a_parser(self):
        self.upload(b"not even valid DOT {", "broken.dot")
        self.page.locator("#document-new").click()
        self.page.wait_for_function("sourceEl.value === 'flowchart TD\\n'")
        self.assertEqual(self.page.locator("#document-name").text_content(), "diagram.mmd")
        self.assertEqual(self.page.evaluate("switches"), 2)

    def test_actual_playground_integrates_programmatic_edits_and_resets_cross_file_history(self):
        self.page.evaluate("workspace.dispose()")
        html = (ROOT / "web/playground.html").read_text()
        script = re.search(r'<script type="module">([\s\S]*?)</script>', html).group(1)
        self.page.set_content(html[:html.index('<script type="module">')])
        self.page.evaluate("""() => {
          globalThis.Worker = undefined; globalThis.confirm = () => true;
          globalThis.editorMounts = 0; globalThis.editorDisposals = 0; globalThis.deckChanges = 0;
          globalThis.renderer = {default: async () => {}, workerHandleMessage: json => {
            const message = JSON.parse(json);
            const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            svg.textContent = message.input;
            return JSON.stringify({kind:'completed', requestId:message.requestId, svg:svg.outerHTML});
          }};
          // Only external engine/editor boundaries are fixtures; document code and playground are real.
          globalThis.__import = async url => {
            if (url === './fm-document.js') return docModule;
            if (url.includes('frankenmermaid.js')) return renderer;
            if (url.includes('fm-deck-editor.js')) return { mountDeckEditor: () => ({
              open: async () => {}, sourceChanged: () => deckChanges++, dispose() {} }) };
            if (url.includes('fm-source-editor.js')) return {mountSourceEditor: options => {
              editorMounts++; globalThis.editorOptions = options;
              return {render: async () => {}, dispose: () => editorDisposals++};
            }};
            throw Error('unexpected module ' + url);
          };
        }""")
        script = script.replace("import.meta.url", '"https://example.invalid/web/playground.html"').replace("import(", "globalThis.__import(")
        self.page.evaluate("code => (0,eval)(code)", script)
        self.page.wait_for_selector("#document-file", state="attached")
        self.page.locator("#edit-source").click()
        self.page.locator("#edit-deck").click()
        self.page.wait_for_function("editorMounts === 1")
        self.page.evaluate("editorOptions.sourceEl.value = 'a[Programmatic]'; editorOptions.onChange()")
        self.assertIn("unexported changes", self.page.locator("#document-name").text_content())
        self.upload(b"b[New file]", "another.mmd")
        self.assertEqual(self.page.evaluate("editorMounts"), 2)
        self.assertEqual(self.page.evaluate("editorDisposals"), 1)
        self.assertGreater(self.page.evaluate("deckChanges"), 0)
        with self.page.expect_download() as pending:
            self.page.evaluate("editorOptions.saveFile({text:'not authoritative',filename:'diagram.mmd',mime:'text/plain'})")
        self.assertEqual(Path(pending.value.path()).read_bytes(), b"b[New file]")
        self.assertEqual(pending.value.suggested_filename, "another.mmd")


if __name__ == "__main__":
    unittest.main(verbosity=2)
