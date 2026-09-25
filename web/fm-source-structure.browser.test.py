"""Real Chromium DOM + worker-message tests for structural source authoring.

The actual editor runs unchanged. The worker host uses a classic-worker wrapper with
fixture module loading, because this null-origin test page cannot start Blob module
workers in the available Chromium. Real worker execution/message ports are exercised;
production module-worker startup, Rust rendering and builds are NOT verified.
The WASM boundary supplies explicit source/snapshot/transaction examples.
Run: python web/fm-source-structure.browser.test.py
CHROMIUM_PATH may select a local browser; otherwise use Chromium on PATH or Playwright's.
"""
import json
import os
from pathlib import Path
import shutil
import unittest

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
ORIGINAL = "\ufeff%% café 🐉\nflowchart TD\n  a[A] --> b[B]\n  z[Z]\n"
TEXT = "c[世界]"
SPAN = "a[A] --> b[B]"
RENAMED = ORIGINAL.replace(SPAN, "a[Changed] --> b[B]")


def fixture(text=TEXT, render_failure=False, corrupt=False):
    """Supply fixed contract examples, not a JavaScript reimplementation of ParseLens."""
    line_start = ORIGINAL.index("  a[A]")
    line_end = ORIGINAL.index("\n", line_start) + 1
    inserted = ORIGINAL[:line_end] + "  " + text + "\n" + ORIGINAL[line_end:]
    deleted = ORIGINAL[:line_start] + ORIGINAL[line_end:]

    def snapshot(source):
        bindings = []
        specs = [("a", SPAN), ("b", SPAN), ("c", "  " + text), ("z", "z[Z]")]
        if source == RENAMED:
            specs = [("a", "a[Changed] --> b[B]"), ("b", "a[Changed] --> b[B]"), ("z", "z[Z]")]
        for name, span in specs:
            if span not in source:
                continue
            start = source.index(span)
            end = start + len(span)
            bindings.append({"elementId": f"fm-node-{name}-{len(bindings)}", "sourceId": name,
                             "kind": "node", "snippet": span, "textRange": {
                                 "startByte": len(source[:start].encode()), "endByte": len(source[:end].encode())}})
        return {"bindings": bindings, "parsed": {"warnings": []}}

    sources = {}
    for source in [ORIGINAL, inserted, deleted, RENAMED]:
        snap = snapshot(source)
        svg = '<svg xmlns="http://www.w3.org/2000/svg" width="420" height="80">'
        for index, binding in enumerate(snap["bindings"]):
            svg += (f'<g id="{binding["elementId"]}"><rect x="{index * 90}" y="10" width="80" height="40"/>'
                    f'<text x="{index * 90 + 4}" y="35" fill="white">{binding["sourceId"]}</text></g>')
        sources[source] = {"snapshot": snap, "svg": svg + "</svg>"}

    def response(start, end, replacement):
        updated = ORIGINAL[:start] + replacement + ORIGINAL[end:]
        return {"result": {"elementId": "fm-node-a-0", "replacedRange": {
            "startByte": len(ORIGINAL[:start].encode()), "endByte": len(ORIGINAL[:end].encode())},
            "previousSnippet": ORIGINAL[start:end], "replacement": replacement, "updatedSource": updated},
            "snapshot": sources[updated]["snapshot"]}

    insertion = response(line_end, line_end, "  " + text + "\n")
    deletion = response(line_start, line_end, "")
    deletion["snapshot"] = json.loads(json.dumps(deletion["snapshot"]))
    deletion["snapshot"]["parsed"]["warnings"] = ["fixture: other references may remain"]
    start = ORIGINAL.index(SPAN)
    replacement = response(start, start + len(SPAN), "a[Changed] --> b[B]")
    if corrupt:
        insertion["result"]["updatedSource"] += "unrelated change"
    return {"original": ORIGINAL, "inserted": inserted, "deleted": deleted, "renamed": RENAMED,
            "text": text, "sources": sources, "insert": insertion, "delete": deletion,
            "edit": replacement, "renderFailure": render_failure}


MOUNT = """async ({editorCode, workerCode, data, fallback}) => {
  const blobUrl = code => URL.createObjectURL(new Blob([code], {type:'text/javascript'}));
  const moduleCode = `const f = ${JSON.stringify(data)};
    export default function() {}
    export function chooseCanvasTarget() { return JSON.stringify({target:'svgInWorker'}); }
    export function parseLens(input) {
      if (!f.sources[input]) throw Error('fixture source absent');
      return structuredClone(f.sources[input].snapshot);
    }
    export function renderSvg(input) {
      if (!f.sources[input] || (f.renderFailure && input !== f.original)) throw Error('fixture render unavailable');
      return f.sources[input].svg;
    }
    function edit(input, id, operation) {
      if (input !== f.original || id !== 'fm-node-a-0') throw Error('fixture edit target absent');
      return structuredClone(f[operation]);
    }
    export function applyParseLensDelete(input,id) { return edit(input,id,'delete'); }
    export function applyParseLensInsertLineAfter(input,id,text) {
      if (text !== f.text) throw Error('fixture insertion text mismatch');
      return edit(input,id,'insert');
    }
    export function applyParseLensEdit(input,id,text) {
      if (text !== 'a[Changed] --> b[B]') throw Error('fixture replacement mismatch');
      return edit(input,id,'edit');
    }`;
  // Replace only module transport, not the worker's operation dispatch or scheduling. Keep
  // this limitation explicit: successful tests must not claim a production module-worker boot.
  const fixtureHost = moduleCode.replace('export default function() {}', 'function fixtureInit() {}')
    .replaceAll('export function ', 'function ') + `
      globalThis.__fixtureImport = async () => ({default:fixtureInit, chooseCanvasTarget,
        parseLens, renderSvg, applyParseLensDelete, applyParseLensInsertLineAfter, applyParseLensEdit});
    ` + workerCode.replaceAll('import(', 'globalThis.__fixtureImport(');
  window.urls = [blobUrl(editorCode), blobUrl(fixtureHost), blobUrl(moduleCode)];
  window.held = []; window.hold = false; window.workers = []; window.changes = [];
  // Real workers and message ports, with an optional UI-delivery barrier for races.
  class GatedWorker {
    constructor(url, options) {
      if (options.type !== 'module') throw Error('client must request a production module worker');
      this.native = new Worker(url, {type:'classic'}); this.terminated = false; window.workers.push(this);
      this.native.onmessage = event => {
        const deliver = () => this.onmessage?.(event);
        if (window.hold && ['sourceInserted','sourceDeleted'].includes(event.data.kind)) window.held.push(deliver);
        else deliver();
      };
      this.native.onerror = event => this.onerror?.(event);
      this.native.onmessageerror = event => this.onmessageerror?.(event);
    }
    postMessage(message) { this.native.postMessage(message); }
    terminate() { this.terminated = true; this.native.terminate(); }
  }
  const src = document.getElementById('src');
  src.value = data.original;
  src.addEventListener('input', () => { window.rendered = window.editor.render(src.value); });
  const {mountSourceEditor} = await import(window.urls[0]);
  window.editor = mountSourceEditor({sourceEl:src, outEl:document.getElementById('out'),
    panelEl:document.getElementById('panel'), loadModule:() => import(window.urls[2]),
    workerOptions:{WorkerClass:fallback ? null : GatedWorker, workerUrl:window.urls[1], moduleUrl:window.urls[2]},
    onChange:() => { window.changes.push(src.value); window.rendered = window.editor.render(src.value); },
  });
  await window.editor.render(src.value);
}"""


class StructuralBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get("CHROMIUM_PATH") or shutil.which("chromium")
        cls.browser = cls.playwright.chromium.launch(headless=True, executable_path=executable, args=["--no-sandbox"])

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.page = self.browser.new_page()
        self.page.set_default_timeout(5000)
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.set_content('<textarea id="src"></textarea><div id="out"></div><section id="panel"></section>')
        self.data = None

    def tearDown(self):
        self.page.evaluate("() => { window.editor?.dispose(); for (const url of window.urls || []) URL.revokeObjectURL(url); }")
        self.page.close()
        self.assertEqual(self.errors, [])

    def mount(self, **options):
        fallback = options.pop("fallback", False)
        self.data = fixture(**options)
        self.page.evaluate(MOUNT, {"editorCode": (ROOT / "fm-source-editor.js").read_text(),
                                  "workerCode": (ROOT / "fm-render.worker.js").read_text(),
                                  "data": self.data, "fallback": fallback})
        status = self.page.locator("#source-editor-message").inner_text()
        self.assertIn("synchronous" if fallback else "source worker", status)
        if not fallback:
            self.assertNotIn("fallback", status, "fallback must not masquerade as a worker test")
            self.assertFalse(self.page.evaluate("window.workers.some(w => w.terminated)"))
        self.page.locator("#source-editor-elements").select_option("fm-node-a-0")

    def source(self):
        return self.page.locator("#src").input_value()

    def preview(self, kind="insert"):
        if kind == "insert":
            self.page.locator("#source-editor-insert-text").fill(self.data["text"])
        self.page.locator(f"#source-editor-{'insert' if kind == 'insert' else 'delete'}").click()
        self.page.wait_for_function("!document.getElementById('source-editor-confirm').disabled")

    def confirm(self):
        self.page.locator("#source-editor-confirm").click()
        self.page.evaluate("() => window.rendered")

    def test_insert_requires_review_then_updates_source_bindings_and_undo_redo(self):
        self.mount()
        self.preview()
        self.assertEqual(self.source(), ORIGINAL)
        self.assertTrue(self.page.locator("#source-editor-undo").is_disabled())
        self.assertIn('Insert: "  c[世界]\\n"', self.page.locator("#source-editor-structural-preview").inner_text())
        self.confirm()
        self.assertEqual(self.source(), self.data["inserted"])
        self.assertEqual(self.page.locator("#out [data-source-editable]").count(), 4)
        self.assertEqual(self.page.evaluate("window.changes.length"), 1)
        self.page.locator("#source-editor-undo").click()
        self.page.evaluate("() => window.rendered")
        self.assertEqual(self.source(), ORIGINAL)
        self.page.locator("#source-editor-redo").click()
        self.page.evaluate("() => window.rendered")
        self.assertEqual(self.source(), self.data["inserted"])
        self.assertEqual(self.page.evaluate("window.editor.exportSource().text"), self.data["inserted"])

    def test_delete_reviews_shared_statement_and_rebinds_remaining_elements(self):
        self.mount()
        self.preview("delete")
        preview = self.page.locator("#source-editor-structural-preview").inner_text()
        self.assertIn('Remove: "  a[A] --> b[B]\\n"', preview)
        self.assertIn("other references may remain", preview)
        self.assertEqual(self.source(), ORIGINAL)
        self.confirm()
        self.assertEqual(self.source(), self.data["deleted"])
        self.assertEqual(self.page.locator("#out [data-source-editable]").count(), 1)
        self.assertEqual(self.page.locator("#out [data-source-editable]").get_attribute("id"), "fm-node-z-0")

    def test_discard_changes_neither_source_nor_history(self):
        self.mount()
        self.preview("delete")
        self.page.locator("#source-editor-cancel").click()
        self.assertEqual(self.source(), ORIGINAL)
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())
        self.assertTrue(self.page.locator("#source-editor-undo").is_disabled())
        self.assertEqual(self.page.evaluate("window.changes"), [])

    def test_draft_edit_invalidates_preview(self):
        self.mount()
        self.preview()
        self.page.locator("#source-editor-insert-text").fill("different")
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())
        self.assertEqual(self.source(), ORIGINAL)

    def test_programmatic_draft_change_is_rechecked_on_confirmation(self):
        self.mount()
        self.preview()
        self.page.evaluate("document.getElementById('source-editor-insert-text').value = 'different'")
        self.page.locator("#source-editor-confirm").click()
        self.assertEqual(self.source(), ORIGINAL)
        self.assertIn("stale", self.page.locator("#source-editor-message").inner_text())

    def test_source_and_selection_changes_invalidate_prepared_transactions(self):
        self.mount()
        self.preview("delete")
        self.page.locator("#source-editor-elements").select_option("fm-node-z-2")
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())
        self.page.locator("#source-editor-elements").select_option("fm-node-a-0")
        self.preview("delete")
        self.page.locator("#src").fill(RENAMED)
        self.page.evaluate("() => window.rendered")
        self.page.evaluate("document.getElementById('source-editor-confirm').dispatchEvent(new Event('click'))")
        self.assertEqual(self.source(), RENAMED)
        self.assertEqual(self.page.evaluate("window.changes"), [])

    def test_programmatic_source_change_is_rechecked_without_input_event(self):
        self.mount()
        self.preview("delete")
        self.page.evaluate("value => document.getElementById('src').value = value", RENAMED)
        self.page.locator("#source-editor-confirm").click()
        self.assertEqual(self.source(), RENAMED)
        self.assertIn("stale", self.page.locator("#source-editor-message").inner_text())

    def test_delayed_worker_response_cannot_overwrite_new_source(self):
        self.mount()
        self.page.evaluate("window.hold = true")
        self.page.locator("#source-editor-delete").click()
        self.page.wait_for_function("window.held.length === 1")
        self.page.locator("#src").fill(RENAMED)
        self.page.evaluate("() => window.rendered")
        self.page.evaluate("() => window.held.splice(0).forEach(deliver => deliver())")
        self.assertEqual(self.source(), RENAMED)
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())
        self.assertEqual(self.page.evaluate("window.changes"), [])

    def test_delayed_response_rechecks_insertion_draft(self):
        self.mount()
        self.page.evaluate("window.hold = true")
        self.page.locator("#source-editor-insert-text").fill(TEXT)
        self.page.locator("#source-editor-insert").click()
        self.page.wait_for_function("window.held.length === 1")
        self.page.evaluate("document.getElementById('source-editor-insert-text').value = 'changed silently'")
        self.page.evaluate("() => window.held.splice(0).forEach(deliver => deliver())")
        self.page.wait_for_function("document.getElementById('out').getAttribute('aria-busy') === 'false'")
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())
        self.assertEqual(self.source(), ORIGINAL)

    def test_disposal_during_inflight_edit_terminates_worker_and_ignores_late_reply(self):
        self.mount()
        self.page.evaluate("window.hold = true")
        self.page.locator("#source-editor-delete").click()
        self.page.wait_for_function("window.held.length === 1")
        self.page.evaluate("() => { window.editor.dispose(); window.held.splice(0).forEach(deliver => deliver()); }")
        self.assertTrue(self.page.evaluate("window.workers.every(w => w.terminated)"))
        self.assertEqual(self.source(), ORIGINAL)
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())

    def test_missing_worker_uses_explicit_local_structural_api(self):
        self.mount(fallback=True)
        self.preview()
        self.confirm()
        self.assertEqual(self.source(), self.data["inserted"])
        self.assertIn("synchronous", self.page.locator("#source-editor-message").inner_text())
        self.assertEqual(self.page.evaluate("window.workers.length"), 0)

    def test_render_failure_after_commit_does_not_lose_source_or_undo(self):
        self.mount(render_failure=True)
        self.preview()
        self.confirm()
        self.assertEqual(self.source(), self.data["inserted"])
        self.assertIn("fixture render unavailable", self.page.locator("#source-editor-message").inner_text())
        self.assertTrue(self.page.locator("#source-editor-save-svg").is_disabled())
        self.page.locator("#source-editor-undo").click()
        self.page.evaluate("() => window.rendered")
        self.assertEqual(self.source(), ORIGINAL)
        self.assertEqual(self.page.locator("#out [data-source-editable]").count(), 3)

    def test_untrusted_source_in_preview_is_text_not_html(self):
        self.mount(text='<img src=x onerror="window.injected=true">')
        self.preview()
        self.assertEqual(self.page.locator("#source-editor-structural-preview img").count(), 0)
        self.assertFalse(self.page.evaluate("!!window.injected"))
        self.assertEqual(self.source(), ORIGINAL)
        self.confirm()
        self.assertEqual(self.source(), self.data["inserted"])

    def test_corrupt_structural_response_never_becomes_a_confirmable_preview(self):
        self.mount(corrupt=True)
        self.page.locator("#source-editor-insert-text").fill(TEXT)
        self.page.locator("#source-editor-insert").click()
        self.page.wait_for_function("document.getElementById('source-editor-message').textContent.includes('unrelated')")
        self.assertEqual(self.source(), ORIGINAL)
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())
        self.assertTrue(self.page.locator("#source-editor-undo").is_disabled())

    def test_existing_replacement_still_works_and_discards_structural_preview(self):
        self.mount()
        self.preview("delete")
        self.page.locator("#source-editor-snippet").fill("a[Changed] --> b[B]")
        self.page.locator("#source-editor-apply").click()
        self.page.wait_for_function("window.changes.length === 1")
        self.page.evaluate("() => window.rendered")
        self.assertEqual(self.source(), RENAMED)
        self.assertTrue(self.page.locator("#source-editor-confirm").is_hidden())


if __name__ == "__main__":
    unittest.main(verbosity=2)
