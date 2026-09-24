"""In-memory DOM integration regressions; the WASM boundary is explicitly mocked.

Run: python web/fm-source-editor.browser.test.py
Requires Playwright and Chromium (PLAYWRIGHT_CHROMIUM_EXECUTABLE may override its path).
Tests real DOM events with explicit WASM, download, and worker-transport fixtures.
Real cross-thread execution of the production worker is tested separately by the Node
suite. No external network, Rust compilation, or fake files under pkg/ are used.
"""
import os
import json
from pathlib import Path
import shutil
import unittest

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
# This fixture models the public JSON contract, not the Mermaid grammar or real WASM execution.
WASM_FIXTURE = r'''
const byteLength = (text) => new TextEncoder().encode(text).length;
export default async function initialize() {}
export function parseLens(source) {
  if (source.includes("PARSER_FAIL")) throw new Error("fixture parser failed");
  return { parsed: { warnings: [] }, bindings: [...source.matchAll(/\[[^\]]*\]/gu)].map((match, i) => ({
    elementId: `fm-node-${i}`, kind: "Node", sourceId: `node-${i}`,
    textRange: { startByte: byteLength(source.slice(0, match.index)),
      endByte: byteLength(source.slice(0, match.index + match[0].length)) }, snippet: match[0],
  })) };
}
export function applyParseLensEdit(source, id, replacement) {
  globalThis.mutationCalls = (globalThis.mutationCalls || 0) + 1;
  const { textRange } = parseLens(source).bindings.find((b) => b.elementId === id);
  const bytes = new TextEncoder().encode(source);
  const decode = new TextDecoder();
  const updatedSource = decode.decode(bytes.slice(0, textRange.startByte)) + replacement +
    decode.decode(bytes.slice(textRange.endByte));
  return { result: { updatedSource }, snapshot: parseLens(updatedSource) };
}
export function renderSvg(source) {
  if (source.includes("RENDER_FAIL")) throw new Error("fixture renderer failed");
  globalThis.renderCalls = [...(globalThis.renderCalls || []), source];
  const escape = (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 300">` +
    parseLens(source).bindings.map((binding, i) => `<g id="${binding.elementId}" transform="translate(10 ${i * 70 + 10})"><a href="https://example.invalid/never-navigate"><rect width="270" height="50" fill="white" stroke="black"/><text x="8" y="30">${escape(binding.snippet)}</text></a></g>`).join("") + "</svg>";
}
export function workerHandleMessage(json) {
  const message = JSON.parse(json);
  return JSON.stringify({ kind: "completed", requestId: message.requestId, svg: renderSvg(message.input) });
}
'''


class SourceEditorBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get("PLAYWRIGHT_CHROMIUM_EXECUTABLE") or shutil.which("chromium")
        options = {"headless": True, "args": ["--no-sandbox"]}
        if executable:
            options["executable_path"] = executable
        cls.browser = cls.playwright.chromium.launch(**options)

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.context = self.browser.new_context()
        self.page = self.context.new_page()
        self.addCleanup(self.context.close)
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.set_content('''<textarea id="src"></textarea>
          <button id="edit-source">Edit diagram source</button>
          <section id="source-editor" hidden></section><div id="out" style="width:400px"></div>''')
        self.page.evaluate("""async ({ host, fixture }) => {
          const load = async text => {
            const url = URL.createObjectURL(new Blob([text], { type: 'text/javascript' }));
            try { return await import(url); } finally { URL.revokeObjectURL(url); }
          };
          globalThis.editorModule = await load(host);
          globalThis.fixtureApi = await load(fixture);
          globalThis.savedFiles = [];
          document.querySelector('#edit-source').addEventListener('click', () => {
            const sourceEl = document.querySelector('#src');
            globalThis.editor = editorModule.mountSourceEditor({ sourceEl,
              outEl: document.querySelector('#out'), panelEl: document.querySelector('#source-editor'),
              loadModule: async () => fixtureApi, onChange: () => editor.render(sourceEl.value),
              saveFile: artifact => {
                if (globalThis.failSave) return Promise.reject(new Error('fixture save failed'));
                savedFiles.push(artifact);
              } });
            sourceEl.addEventListener('input', () => editor.render(sourceEl.value));
            editor.render(sourceEl.value);
          });
        }""", {"host": (ROOT / "web/fm-source-editor.js").read_text(), "fixture": WASM_FIXTURE})

    def tearDown(self):
        self.assertEqual(self.errors, [], "unexpected browser exceptions")

    def enter_editor(self, source=None):
        if source is not None:
            self.page.locator("#src").fill(source)
        self.page.locator("#edit-source").click()
        self.page.wait_for_function("document.querySelector('#source-editor-elements')?.disabled === false")

    def test_opt_in_selection_edit_and_rerender(self):
        self.assertTrue(self.page.locator("#source-editor").is_hidden())
        self.enter_editor("%% café 🐉\nflowchart TD\n  a[世界 😀] --> b[Beta]\n")
        self.page.locator("#fm-node-0 rect").click()
        self.assertEqual(self.page.locator("#source-editor-snippet").input_value(), "[世界 😀]")
        self.assertEqual(self.page.evaluate("src.value.slice(src.selectionStart, src.selectionEnd)"), "[世界 😀]")
        self.assertEqual(self.page.url, "about:blank")
        self.page.locator("#source-editor-snippet").fill("[New 🌍]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[New 🌍]'")
        self.assertEqual(self.page.locator("#src").input_value(), "%% café 🐉\nflowchart TD\n  a[New 🌍] --> b[Beta]\n")
        self.assertEqual(self.page.evaluate("mutationCalls"), 1)
        self.assertEqual(self.page.locator("#fm-node-1 text").text_content(), "[Beta]")

    def test_keyboard_and_source_to_diagram_selection(self):
        self.enter_editor("flowchart TD\n a[Alpha] --> b[Beta]\n")
        self.page.locator("#fm-node-1").focus()
        self.page.keyboard.press("Enter")
        self.assertEqual(self.page.locator("#source-editor-snippet").input_value(), "[Beta]")
        self.page.evaluate("src.setSelectionRange(src.value.indexOf('[Alpha]'), src.value.indexOf('[Alpha]') + 7)")
        self.page.wait_for_function("document.querySelector('#source-editor-elements').value === 'fm-node-0'")
        self.assertEqual(self.page.locator("#fm-node-0").get_attribute("data-source-selected"), "true")
        self.page.locator("#source-editor-elements").select_option("fm-node-1")
        self.assertEqual(self.page.locator("#source-editor-snippet").input_value(), "[Beta]")

    def test_stale_preview_cannot_replace_newer_source(self):
        self.enter_editor("flowchart TD\n a[Alpha]\n")
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#source-editor-snippet").fill("[Old draft]")
        # Change the source and click in one JS task, BEFORE a render could invalidate the button.
        self.page.evaluate("src.value = 'flowchart TD\\n b[Newer]'; document.querySelector('#source-editor-apply').click()")
        self.assertEqual(self.page.locator("#src").input_value(), "flowchart TD\n b[Newer]")
        self.assertEqual(self.page.evaluate("globalThis.mutationCalls || 0"), 0)
        self.assertIn("Source changed", self.page.locator("#source-editor-message").text_content())

    def test_failed_render_disables_editing_and_typing_recovers(self):
        self.enter_editor("flowchart TD\n a[Alpha]\n")
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#src").fill("flowchart TD\n a[RENDER_FAIL]\n")
        self.page.wait_for_function("document.querySelector('#source-editor-message').textContent.includes('fixture renderer failed')")
        self.assertTrue(self.page.locator("#source-editor-apply").is_disabled())
        self.assertTrue(self.page.locator("#source-editor-elements").is_disabled())
        self.page.locator("#src").fill("flowchart TD\n a[Recovered]\n")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Recovered]'")
        self.assertFalse(self.page.locator("#source-editor-elements").is_disabled())

    def test_untrusted_snippets_are_text_not_editor_markup(self):
        self.enter_editor("flowchart TD\n a[<img src=x onerror=alert(1)>]\n")
        self.page.locator("#fm-node-0 rect").click()
        self.assertIn("<img", self.page.locator("#source-editor-snippet").input_value())
        self.assertEqual(self.page.locator("#source-editor img").count(), 0)
        self.assertEqual(self.page.locator("#out img").count(), 0)

    def test_overlapping_loads_render_only_the_latest_source_and_dispose_is_final(self):
        self.page.evaluate("""async () => {
          const { mountSourceEditor } = editorModule;
          const api = fixtureApi;
          const sourceEl = document.createElement('textarea');
          const panelEl = document.createElement('section');
          const outEl = document.createElement('div');
          document.body.append(sourceEl, panelEl, outEl);
          let release;
          const ready = new Promise(resolve => { release = resolve; });
          const editor = mountSourceEditor({ sourceEl, panelEl, outEl, loadModule: () => ready, onChange() {} });
          sourceEl.value = 'a[Obsolete]';
          const old = editor.render(sourceEl.value);
          sourceEl.value = 'b[Latest]';
          const fresh = editor.render(sourceEl.value);
          release(api);
          await Promise.all([old, fresh]);
          if (outEl.textContent !== '[Latest]') throw new Error('stale render committed');
          if (globalThis.renderCalls.includes('a[Obsolete]')) throw new Error('stale input rendered');
          sourceEl.value = 'c[Disposed]';
          const pending = editor.render(sourceEl.value);
          editor.dispose();
          await pending;
          if (outEl.textContent !== '[Latest]') throw new Error('render committed after dispose');
          await editor.render(sourceEl.value);
          if (outEl.getAttribute('aria-busy') !== 'false') throw new Error('disposed editor became busy');
        }""")

    def test_applied_edits_and_typing_share_reversible_source_history(self):
        original = "%% comment\nflowchart TD\n a[Alpha]\n"
        self.enter_editor(original)
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#source-editor-snippet").fill("[Changed]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Changed]'")
        self.page.locator("#source-editor-undo").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Alpha]'")
        self.assertEqual(self.page.locator("#src").input_value(), original)
        self.page.locator("#source-editor-redo").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Changed]'")
        self.page.locator("#src").fill("flowchart TD\n a[Typed]")
        self.page.keyboard.press("Control+z")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Changed]'")
        self.assertEqual(self.page.locator("#src").input_value(), original.replace("Alpha", "Changed"))
        self.page.keyboard.press("Control+Shift+z")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Typed]'")

    def test_undo_recovers_from_parser_failure_and_new_edits_clear_redo(self):
        original = "flowchart TD\n a[Good]"
        self.enter_editor(original)
        self.page.locator("#src").fill("PARSER_FAIL")
        self.page.wait_for_function("document.querySelector('#source-editor-message').textContent.includes('fixture parser failed')")
        self.assertFalse(self.page.locator("#source-editor-undo").is_disabled())
        self.page.locator("#source-editor-undo").click()
        self.page.wait_for_function("document.querySelector('#source-editor-elements').disabled === false")
        self.assertEqual(self.page.locator("#src").input_value(), original)
        self.assertFalse(self.page.locator("#source-editor-redo").is_disabled())
        self.page.locator("#src").fill("flowchart TD\n a[Different]")
        self.assertTrue(self.page.locator("#source-editor-redo").is_disabled())

    def test_source_and_svg_export_preserve_engine_bytes_not_selection_markup(self):
        source = "%% 😀\nflowchart TD\n a[Export me]\n"
        self.enter_editor(source)
        self.page.locator("#fm-node-0 rect").click()
        self.assertEqual(self.page.locator("#fm-node-0").get_attribute("data-source-selected"), "true")
        self.page.locator("#source-editor-save-source").click()
        self.page.locator("#source-editor-save-svg").click()
        saved = self.page.evaluate("savedFiles")
        self.assertEqual(saved[0], {"text": source, "filename": "diagram.mmd", "mime": "text/plain;charset=utf-8"})
        self.assertEqual(saved[1]["text"], self.page.evaluate("fixtureApi.renderSvg(src.value)"))
        self.assertEqual(saved[1]["filename"], "diagram.svg")
        self.assertNotIn("data-source-selected", saved[1]["text"])
        self.assertNotIn("data-source-editable", saved[1]["text"])
        self.assertNotIn("tabindex", saved[1]["text"])

    def test_stale_and_failed_previews_cannot_be_exported_as_current_svg(self):
        self.enter_editor("flowchart TD\n a[Good]")
        self.page.evaluate("src.value = 'flowchart TD\\n a[Newer]'; document.querySelector('#source-editor-save-svg').click()")
        self.assertEqual(self.page.evaluate("savedFiles.length"), 0)
        self.assertIn("current source", self.page.locator("#source-editor-message").text_content())
        self.page.locator("#src").fill("PARSER_FAIL")
        self.page.wait_for_function("document.querySelector('#source-editor-message').textContent.includes('fixture parser failed')")
        self.assertTrue(self.page.locator("#source-editor-save-svg").is_disabled())
        self.page.locator("#source-editor-save-source").click()
        self.assertEqual(self.page.evaluate("savedFiles[0].text"), "PARSER_FAIL")

    def test_asynchronous_save_failure_is_reported_without_losing_source(self):
        source = "flowchart TD\n a[Safe]"
        self.enter_editor(source)
        self.page.evaluate("globalThis.failSave = true")
        self.page.locator("#source-editor-save-source").click()
        self.page.wait_for_function("document.querySelector('#source-editor-message').textContent.includes('fixture save failed')")
        self.assertEqual(self.page.locator("#src").input_value(), source)

    def test_draft_undo_remains_native_and_does_not_change_committed_source(self):
        source = "flowchart TD\n a[Original]"
        self.enter_editor(source)
        self.page.locator("#fm-node-0 rect").click()
        draft = self.page.locator("#source-editor-snippet")
        draft.focus()
        self.page.keyboard.press("End")
        self.page.keyboard.type(" draft")
        self.page.keyboard.press("Control+z")
        self.assertEqual(self.page.locator("#src").input_value(), source)
        self.assertEqual(self.page.evaluate("globalThis.mutationCalls || 0"), 0)
        self.assertTrue(self.page.locator("#source-editor-undo").is_disabled())

    def open_worker_editor(self, *, playground=False, blocked=False):
        # Real DOM and production client; the worker transport and WASM are explicit fixtures.
        # Delayed replies are deliberately delivered after cancellation to exercise the UI
        # commit boundary, independently of the worker queue tests in the Node suite.
        self.page.evaluate("globalThis.editor?.dispose()")
        self.page.evaluate(r'''() => {
          globalThis.mainModuleLoads = 0;
          globalThis.savedFiles = [];
          globalThis.delayedKind = null;
          globalThis.transportCalls = [];
          class FixtureWorker {
            constructor() { globalThis.authoringWorker = this; this.terminated = false; }
            terminate() { this.terminated = true; }
            postMessage(message) {
              transportCalls.push(message);
              if (message.kind === 'cancel') return; // A response may already be in transit.
              const delayed = (message.replacement || message.input || '').includes('DELAY_');
              if (delayed) globalThis.delayedKind = message.kind;
              setTimeout(() => {
                if (this.terminated) return;
                let data;
                try {
                  if (message.kind === 'init') {
                    data = {kind: 'ready', target: 'svgInWorker', requested: 'svgInWorker', sourceEditing: true};
                  } else if (message.kind === 'sourceRender') {
                    data = {kind: 'sourceRendered', requestId: message.requestId,
                      snapshot: fixtureApi.parseLens(message.input), svg: fixtureApi.renderSvg(message.input)};
                  } else if (message.kind === 'sourceEdit') {
                    const response = fixtureApi.applyParseLensEdit(message.input, message.elementId, message.replacement);
                    if (message.replacement.includes('CORRUPT')) response.result.updatedSource += 'outside selected span';
                    data = {kind: 'sourceEdited', requestId: message.requestId, response};
                  } else {
                    data = JSON.parse(fixtureApi.workerHandleMessage(JSON.stringify(message)));
                  }
                } catch (error) { data = {kind: 'failed', requestId: message.requestId, reason: error.message}; }
                this.onmessage?.({data});
              }, delayed ? 250 : 0);
            }
          }
          globalThis.FixtureWorker = FixtureWorker;
        }''')
        if playground:
            # Exercise the actual page script. Only browser/WASM transport URLs are changed.
            urls = self.page.evaluate("""({host, fixture}) => {
              const blob = text => URL.createObjectURL(new Blob([text], {type: 'text/javascript'}));
              return {host: blob(host), wasm: blob(fixture)};
            }""", {"host": (ROOT / "web/fm-source-editor.js").read_text(), "fixture": WASM_FIXTURE})
            self.page.evaluate("globalThis.Worker = FixtureWorker; globalThis.OffscreenCanvas = undefined")
            entry = (ROOT / "web/playground.html").read_text()
            entry = entry.replace('import.meta.url', json.dumps("https://example.invalid/web/playground.html"))
            entry = entry.replace('import("../pkg/frankenmermaid.js")', f'import({json.dumps(urls["wasm"])})')
            entry = entry.replace('import("./fm-source-editor.js")', f'import({json.dumps(urls["host"])})')
            entry = entry.replace('workerOptions: { moduleUrl: MODULE_URL }',
                                  'workerOptions: { moduleUrl: MODULE_URL, workerUrl: "https://example.invalid/worker.js" }')
            self.page.set_content(entry)
            self.page.locator("#edit-source").click()
        else:
            self.page.set_content('''<textarea id="src">flowchart TD
 a[Alpha] --> b[Beta]</textarea><section id="source-editor"></section><div id="out" style="width:400px"></div>''')
            self.page.evaluate("""blocked => {
              const sourceEl = document.querySelector('#src');
              globalThis.editor = editorModule.mountSourceEditor({sourceEl,
                outEl: document.querySelector('#out'), panelEl: document.querySelector('#source-editor'),
                workerOptions: {WorkerClass: blocked ? null : FixtureWorker, workerUrl: 'https://example.invalid/worker.js'},
                loadModule: async () => { mainModuleLoads++; return fixtureApi; },
                onChange: () => editor.render(sourceEl.value), saveFile: artifact => savedFiles.push(artifact)});
              sourceEl.addEventListener('input', () => editor.render(sourceEl.value));
              editor.render(sourceEl.value);
            }""", blocked)
        self.page.wait_for_function("document.querySelector('#source-editor-elements')?.disabled === false")

    def test_worker_transport_edit_history_and_export_without_fallback(self):
        self.open_worker_editor()
        original = self.page.locator("#src").input_value()
        self.assertNotIn("fallback", self.page.locator("#source-editor-message").text_content())
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#source-editor-snippet").fill("[雪 🦀]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[雪 🦀]'")
        self.assertEqual(self.page.locator("#src").input_value(), original.replace("[Alpha]", "[雪 🦀]"))
        self.page.locator("#source-editor-save-svg").click()
        self.assertIn("[雪 🦀]", self.page.evaluate("savedFiles[0].text"))
        self.assertNotIn("data-source-editable", self.page.evaluate("savedFiles[0].text"))
        self.page.locator("#source-editor-undo").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Alpha]'")
        self.page.locator("#source-editor-redo").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[雪 🦀]'")
        self.assertEqual(self.page.evaluate("mainModuleLoads"), 0)

    def test_late_worker_preview_cannot_replace_newer_source(self):
        self.open_worker_editor()
        self.page.locator("#src").fill("a[DELAY_RENDER]")
        self.page.wait_for_function("delayedKind === 'sourceRender'")
        self.page.locator("#src").fill("a[Latest]")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Latest]'")
        self.page.wait_for_timeout(300) # The cancelled response is still delivered by the fixture.
        self.assertEqual(self.page.locator("#fm-node-0 text").text_content(), "[Latest]")
        self.assertEqual(self.page.locator("#src").input_value(), "a[Latest]")
        self.assertEqual(self.page.evaluate("mainModuleLoads"), 0)

    def test_late_worker_edit_cannot_overwrite_typing_or_enter_undo_history(self):
        self.open_worker_editor()
        original = self.page.locator("#src").input_value()
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#source-editor-snippet").fill("[DELAY_EDIT]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("delayedKind === 'sourceEdit'")
        self.assertTrue(self.page.locator("#source-editor-apply").is_disabled())
        self.page.locator("#src").fill("a[Typed while editing]")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Typed while editing]'")
        self.page.wait_for_timeout(300)
        self.assertEqual(self.page.locator("#src").input_value(), "a[Typed while editing]")
        self.page.locator("#source-editor-undo").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Alpha]'")
        self.assertEqual(self.page.locator("#src").input_value(), original)

    def test_worker_errors_preserve_source_and_do_not_silently_switch_to_main(self):
        self.open_worker_editor()
        original = self.page.locator("#src").input_value()
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#source-editor-snippet").fill("[CORRUPT]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("document.querySelector('#source-editor-message').textContent.includes('outside')")
        self.assertEqual(self.page.locator("#src").input_value(), original)
        self.page.locator("#src").fill("PARSER_FAIL")
        self.page.wait_for_function("document.querySelector('#source-editor-message').textContent.includes('fixture parser failed')")
        self.assertEqual(self.page.evaluate("mainModuleLoads"), 0)
        self.assertTrue(self.page.locator("#source-editor-save-svg").is_disabled())
        self.page.locator("#source-editor-undo").click()
        self.page.wait_for_function("document.querySelector('#source-editor-elements').disabled === false")
        self.assertEqual(self.page.locator("#src").input_value(), original)

    def test_worker_unavailable_and_runtime_transport_failure_recover_current_source(self):
        self.open_worker_editor(blocked=True)
        self.assertIn("main-thread SVG fallback", self.page.locator("#source-editor-message").text_content())
        self.assertEqual(self.page.evaluate("mainModuleLoads"), 1)
        self.page.locator("#src").fill("a[Still works]")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Still works]'")
        self.assertEqual(self.page.evaluate("mainModuleLoads"), 1)
        self.open_worker_editor()
        self.page.evaluate("authoringWorker.onerror({message: 'simulated transport crash'})")
        self.page.locator("#src").fill("a[Recovered]")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Recovered]'")
        self.assertIn("main-thread SVG fallback", self.page.locator("#source-editor-message").text_content())
        self.assertIn("simulated transport crash", self.page.locator("#source-editor-message").text_content())
        self.assertEqual(self.page.evaluate("mainModuleLoads"), 1)

    def test_actual_playground_uses_worker_authoring_messages(self):
        self.open_worker_editor(playground=True)
        self.assertIn("source worker", self.page.locator("#source-editor-message").text_content())
        self.assertNotIn("fallback", self.page.locator("#source-editor-message").text_content())
        self.assertIn("worker-first", self.page.locator("#status").text_content())
        self.assertGreater(self.page.evaluate("transportCalls.filter(m => m.kind === 'sourceRender').length"), 0)
        self.page.locator("#src").fill("a[From playground]")
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[From playground]'")
        self.page.locator("#fm-node-0 rect").click()
        self.page.locator("#source-editor-snippet").fill("[Edited in playground]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("document.querySelector('#fm-node-0 text')?.textContent === '[Edited in playground]'")
        self.assertEqual(self.page.evaluate("transportCalls.filter(m => m.kind === 'sourceEdit').length"), 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
