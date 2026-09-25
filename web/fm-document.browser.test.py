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

    def mount_recovery_fixture(self):
        # Navigation/localStorage access is denied on this harness's opaque about:blank origin.
        # This fixture models only the Storage boundary; production serialization, scheduling,
        # restoration, source complements, UI and downloads all run unchanged.
        self.page.evaluate("""() => {
          workspace.dispose(); panel.replaceChildren();
          globalThis.recoveryValues = new Map(); globalThis.rejectStorage = false;
          globalThis.storage = {
            get length() { return recoveryValues.size; },
            key: index => [...recoveryValues.keys()][index] ?? null,
            getItem: key => recoveryValues.get(key) ?? null,
            setItem: (key, value) => {
              if (rejectStorage) throw Error('QuotaExceededError fixture');
              recoveryValues.set(key, String(value));
            }
          };
          globalThis.recoveryOptions = {...options, getStorage: () => storage};
          workspace = docModule.mountDocumentWorkspace(recoveryOptions);
          globalThis.remount = () => {
            workspace.dispose(); panel.replaceChildren();
            sourceEl.value = 'flowchart TD\\n a[Initial]\\n';
            workspace = docModule.mountDocumentWorkspace(recoveryOptions);
          };
        }""")

    def test_pagehide_checkpoints_exact_source_and_recovery_restores_dirty_baseline(self):
        self.mount_recovery_fixture()
        raw = '\ufeffflowchart TD\r\n A[One]\n B[Two]\r\n'
        self.upload(raw.encode(), "recover.mmd")
        self.page.locator("#source").fill(raw[1:].replace("\r\n", "\n").replace("One", "Broken["))
        self.page.evaluate("dispatchEvent(new PageTransitionEvent('pagehide', {persisted:true}))")
        self.assertEqual(self.page.evaluate("JSON.parse([...recoveryValues.values()][0]).source"), raw.replace("One", "Broken["))
        self.page.evaluate("remount()")
        self.assertIn("Initial", self.page.locator("#source").input_value(), "recovery must not overwrite current source automatically")
        key = self.page.evaluate("[...recoveryValues.keys()][0]")
        self.page.locator("#document-recovery").select_option(key)
        self.page.locator("#document-restore").click()
        self.page.wait_for_function("sourceEl.value.includes('Broken[')")
        self.assertIn("unexported changes", self.page.locator("#document-name").text_content())
        self.assertEqual(self.page.evaluate("recoveryValues.size"), 2, "restoration must fork, not adopt the old writer key")
        with self.page.expect_download() as pending:
            self.page.locator("#document-save").click()
        self.assertEqual(Path(pending.value.path()).read_bytes(), raw.replace("One", "Broken[").encode())

    def test_automatic_checkpoint_keeps_latest_text_without_a_parse(self):
        self.mount_recovery_fixture()
        self.page.locator("#source").fill("first")
        self.page.locator("#source").fill("unfinished a[世界")
        self.page.wait_for_function("[...recoveryValues.values()].some(value => JSON.parse(value).source === 'unfinished a[世界')")
        self.assertIn("saved in this browser", self.page.locator("#document-recovery-status").text_content())
        self.assertEqual(self.page.evaluate("renders.length"), 0, "recovery must not depend on or invoke rendering")

    def test_storage_failure_preserves_previous_checkpoint_and_retry_saves_latest(self):
        self.mount_recovery_fixture()
        self.page.locator("#source").fill("last stored")
        self.assertTrue(self.page.evaluate("workspace.flushRecovery()"))
        self.page.evaluate("rejectStorage = true")
        self.page.locator("#source").fill("not yet stored")
        self.assertFalse(self.page.evaluate("workspace.flushRecovery()"))
        self.assertEqual(self.page.evaluate("JSON.parse([...recoveryValues.values()][0]).source"), "last stored")
        self.assertIn("Recovery save failed", self.page.locator("#document-recovery-status").text_content())
        self.assertEqual(self.page.locator("#source").input_value(), "not yet stored")
        self.page.evaluate("rejectStorage = false")
        self.page.locator("#document-retry-recovery").click()
        self.assertEqual(self.page.evaluate("JSON.parse([...recoveryValues.values()][0]).source"), "not yet stored")

    def test_opening_another_file_retains_outgoing_recovery_and_download_does_not_switch(self):
        self.mount_recovery_fixture()
        self.page.locator("#source").fill("unsaved outgoing")
        self.upload(b"incoming", "next.mmd")
        self.assertEqual(self.page.evaluate("recoveryValues.size"), 2)
        key = self.page.evaluate("[...recoveryValues].find(([,value]) => JSON.parse(value).source === 'unsaved outgoing')[0]")
        self.page.locator("#source").fill("current unexported")
        self.page.locator("#document-recovery").select_option(key)
        with self.page.expect_download() as pending:
            self.page.locator("#document-save-recovery").click()
        self.assertEqual(Path(pending.value.path()).read_bytes(), b"unsaved outgoing")
        self.assertEqual(self.page.locator("#source").input_value(), "current unexported")
        self.assertIn("unexported changes", self.page.locator("#document-name").text_content())

    def test_damaged_recovery_and_cross_tab_storage_events_do_not_replace_current_source(self):
        self.mount_recovery_fixture()
        self.page.locator("#source").fill("safe copy")
        self.page.evaluate("workspace.flushRecovery(); remount()")
        key = self.page.evaluate("[...recoveryValues.keys()][0]")
        self.page.locator("#document-recovery").select_option(key)
        self.page.evaluate("key => recoveryValues.set(key, 'not-json')", key)
        self.page.locator("#document-restore").click()
        self.page.wait_for_function("document.querySelector('#document-message').textContent.includes('Open failed')")
        self.assertIn("Initial", self.page.locator("#source").input_value())
        self.page.evaluate("key => dispatchEvent(new StorageEvent('storage', {key}))", key)
        self.assertIn("unreadable", self.page.locator("#document-recovery-status").text_content())
        self.assertEqual(self.page.evaluate("key => recoveryValues.get(key)", key), "not-json")
        self.assertTrue(self.page.locator("#document-restore").is_disabled())

    def test_two_independent_editors_write_separate_drafts_and_never_apply_storage_events(self):
        self.mount_recovery_fixture()
        self.page.locator("#source").fill("editor one")
        self.page.evaluate("""() => {
          workspace.flushRecovery();
          globalThis.secondSource = document.createElement('textarea');
          globalThis.secondPanel = document.createElement('section'); document.body.append(secondSource, secondPanel);
          globalThis.second = docModule.mountDocumentWorkspace({...recoveryOptions,
            sourceEl:secondSource, panelEl:secondPanel, onChange() {}, onDocumentChange() {}});
          secondSource.value = 'editor two'; second.sourceChanged(); second.flushRecovery();
          dispatchEvent(new StorageEvent('storage', {key:[...recoveryValues.keys()][1]}));
        }""")
        self.assertEqual(self.page.evaluate("recoveryValues.size"), 2)
        self.assertEqual(self.page.locator("#source").input_value(), "editor one")
        self.assertEqual(self.page.evaluate("secondSource.value"), "editor two")
        self.assertEqual(sorted(self.page.evaluate("[...recoveryValues.values()].map(value => JSON.parse(value).source)")), ["editor one", "editor two"])
        self.page.evaluate("second.dispose()")

    def test_edits_before_module_mount_and_programmatic_changes_are_recoverable(self):
        self.mount_recovery_fixture()
        self.page.evaluate("""() => {
          workspace.dispose(); panel.replaceChildren();
          sourceEl.value = 'typed before file module loaded';
          workspace = docModule.mountDocumentWorkspace({...recoveryOptions, initialSource:'original sample'});
          workspace.flushRecovery();
          sourceEl.value = 'changed by ParseLens without input event'; workspace.sourceChanged(); workspace.dispose();
        }""")
        record = self.page.evaluate("JSON.parse([...recoveryValues.values()][0])")
        self.assertEqual(record["source"], "changed by ParseLens without input event")
        self.assertEqual(record["baseline"], "original sample")

    def test_storage_denial_never_disables_source_downloads(self):
        self.page.evaluate("""() => {
          workspace.dispose(); panel.replaceChildren();
          workspace = docModule.mountDocumentWorkspace({...options,
            getStorage: () => {throw Error('SecurityError fixture');}});
        }""")
        self.page.locator("#source").fill("keep me despite storage denial")
        self.assertFalse(self.page.evaluate("workspace.flushRecovery()"))
        with self.page.expect_download() as pending:
            self.page.locator("#document-save").click()
        self.assertEqual(Path(pending.value.path()).read_bytes(), b"keep me despite storage denial")

    def test_preview_callback_failure_does_not_report_a_successful_open_as_failed(self):
        self.page.evaluate("""() => {
          workspace.dispose(); panel.replaceChildren();
          workspace = docModule.mountDocumentWorkspace({...options,
            onDocumentChange: () => {throw Error('editor reset fixture failed');}});
        }""")
        self.upload(b"flowchart TD\n b[Retained]\n", "retained.mmd")
        self.assertEqual(self.page.locator("#source").input_value(), "flowchart TD\n b[Retained]\n")
        self.assertEqual(self.page.evaluate("renders.at(-1)"), "flowchart TD\n b[Retained]\n")
        self.assertIn("Opened retained.mmd", self.page.locator("#document-message").text_content())
        self.assertIn("Preview update failed", self.page.locator("#document-message").text_content())
        with self.page.expect_download() as pending:
            self.page.locator("#document-save").click()
        self.assertEqual(Path(pending.value.path()).read_bytes(), b"flowchart TD\n b[Retained]\n")


if __name__ == "__main__":
    unittest.main(verbosity=2)
