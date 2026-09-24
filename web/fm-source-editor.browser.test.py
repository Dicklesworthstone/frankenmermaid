"""In-memory DOM integration regressions; the WASM boundary is explicitly mocked.

Run: python web/fm-source-editor.browser.test.py
Requires Playwright and Chromium (PLAYWRIGHT_CHROMIUM_EXECUTABLE may override its path).
Tests mountSourceEditor with real browser events, not worker startup or real WASM.
No navigation, network, Rust compilation, or fake files under pkg/ are used.
"""
import os
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
          document.querySelector('#edit-source').addEventListener('click', () => {
            const sourceEl = document.querySelector('#src');
            globalThis.editor = editorModule.mountSourceEditor({ sourceEl,
              outEl: document.querySelector('#out'), panelEl: document.querySelector('#source-editor'),
              loadModule: async () => fixtureApi, onChange: () => editor.render(sourceEl.value) });
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
        }""")


if __name__ == "__main__":
    unittest.main(verbosity=2)
