"""Browser batch-edit coverage using the actual editor AND dedicated render worker.

Only the WASM module is a fixture. Inline data URLs avoid an HTTP server and make the transport
exercise real browser Worker/postMessage/ES-module loading without external requests.
Run: python web/fm-source-batch.browser.test.py
Requires Playwright and Chromium. CHROMIUM_PATH selects an installed browser; optional
FM_BROWSER_ARTIFACT_DIR retains a screenshot. This does not certify Mermaid parsing/rendering.
"""
from __future__ import annotations

import os
from pathlib import Path
import unittest

from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parent
SOURCE = "%% café 🦀\nflowchart TB\nA[one]\nB[two]\n%% preserve\n"

FIXTURE = r'''
export default async function initialize() {}
export function chooseCanvasTarget() { return JSON.stringify({target: "svgInWorker"}); }
export function workerHandleMessage() { return null; }
function bytes(value) { return new TextEncoder().encode(value).length; }
export function parseLens(input) {
  const bindings = [...input.matchAll(/^([A-Z][0-9]*)\[[^\r\n]*\]/gm)].map(match => ({
    elementId: `node-${match[1]}-${input.length}`, sourceId: match[1], kind: "node",
    snippet: match[0], textRange: {startByte: bytes(input.slice(0, match.index)),
      endByte: bytes(input.slice(0, match.index + match[0].length))}
  }));
  if (OPTIONS.overlap && bindings[0]) bindings.push({...bindings[0],
    elementId: `edge-alias-${input.length}`, sourceId: "A-edge", kind: "edge"});
  return {bindings, parsed: {warnings: ["fixture parser warning"]}};
}
export function applyParseLensEdit(input, elementId, replacement) {
  if (replacement.includes("[reject]")) throw new Error("fixture engine rejected edit");
  const binding = parseLens(input).bindings.find(binding => binding.elementId === elementId);
  if (!binding) throw new Error("fixture received an obsolete ID");
  const original = new TextEncoder().encode(input);
  const prefix = new TextDecoder().decode(original.slice(0, binding.textRange.startByte));
  const suffix = new TextDecoder().decode(original.slice(binding.textRange.endByte));
  const updatedSource = prefix + replacement + suffix;
  return {result: {updatedSource, elementId, replacement, previousSnippet: binding.snippet,
    replacedRange: binding.textRange}, snapshot: parseLens(updatedSource)};
}
function escape(value) { return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll('"', "&quot;"); }
export function renderSvg(input) {
  return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 360 120" role="img">' +
    parseLens(input).bindings.filter(binding => binding.kind === "node").map((binding, i) =>
      `<g id="${binding.elementId}" transform="translate(${i * 175},10)"><rect width="160" height="70" fill="none" stroke="black"/>` +
      `<text x="10" y="30">${escape(binding.snippet)}</text></g>`).join("") + '</svg>';
}
'''

MOUNT = r'''async ({editorCode, workerCode, fixtureCode, input, options}) => {
  // Self-contained modules exercise native Worker/import/postMessage without depending on
  // network permissions or a blob URL loader's treatment of an about:blank opaque origin.
  const moduleUrl = code => "data:text/javascript;charset=utf-8," + encodeURIComponent(code);
  const apiUrl = moduleUrl(`const OPTIONS = ${JSON.stringify(options)};\n` + fixtureCode);
  const workerUrl = moduleUrl(workerCode);
  const api = await import(moduleUrl(editorCode));
  const sourceEl = document.getElementById("source"); sourceEl.value = input;
  const probe = window.probe = {sent: [], received: [], held: [], workers: [], changes: [], localLoads: 0};
  window.holdBatch = options.holdBatch || false;
  class ObservedWorker {
    constructor(url, config) {
      this.worker = new Worker(url, config); probe.workers.push(this);
      this.worker.onmessage = ({data}) => {
        probe.received.push(data);
        if (window.holdBatch && data.kind === "sourceBatchEdited") probe.held.push({owner: this, data});
        else this.onmessage?.({data});
      };
      this.worker.onerror = error => this.onerror?.(error);
      this.worker.onmessageerror = error => this.onmessageerror?.(error);
    }
    postMessage(message) { probe.sent.push(structuredClone(message)); this.worker.postMessage(message); }
    terminate() { this.terminated = true; this.worker.terminate(); }
  }
  window.flushBatch = () => {
    window.holdBatch = false;
    for (const {owner, data} of probe.held.splice(0)) owner.onmessage?.({data});
  };
  window.editor = api.mountSourceEditor({sourceEl, outEl: document.getElementById("out"),
    panelEl: document.getElementById("panel"),
    loadModule: async () => { probe.localLoads++; return import(apiUrl); },
    workerOptions: {WorkerClass: options.fallback ? null : ObservedWorker, workerUrl, moduleUrl: apiUrl,
      initTimeoutMs: 5000, requestTimeoutMs: 5000},
    onChange: () => {
      probe.changes.push(sourceEl.value);
      window.renderPromise = window.editor.render(sourceEl.value);
    }
  });
  window.closeEditor = () => window.editor.dispose();
  await window.editor.render(sourceEl.value);
}'''


class BatchBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get("CHROMIUM_PATH")
        if not executable and Path("/usr/bin/chromium").exists():
            executable = "/usr/bin/chromium"
        cls.browser = cls.playwright.chromium.launch(
            headless=True, executable_path=executable, args=["--no-sandbox"]
        )
        print(f"Chromium {cls.browser.version}; actual editor/worker, fixture WASM module", flush=True)

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.page = self.browser.new_page(viewport={"width": 960, "height": 1000})
        self.page.set_default_timeout(4000)
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.set_content('''<!doctype html><meta charset="utf-8"><title>Batch editor test</title>
          <style>body{font:16px system-ui;margin:16px}textarea{display:block;width:90%;min-height:70px}
          #out{max-width:600px}button{margin:4px}pre{font-size:13px}fieldset{max-width:100%}</style>
          <label for="source">Mermaid source</label><textarea id="source"></textarea>
          <div id="out"></div><section id="panel"></section>''')

    def tearDown(self):
        try:
            self.page.evaluate("window.closeEditor?.()")
            self.assertEqual(self.errors, [], "no uncaught browser errors")
        finally:
            self.page.close()

    def mount(self, **options):
        self.page.evaluate(MOUNT, {"editorCode": (ROOT / "fm-source-editor.js").read_text(),
                                  "workerCode": (ROOT / "fm-render.worker.js").read_text(),
                                  "fixtureCode": FIXTURE, "input": SOURCE, "options": options})
        expect(self.page.locator("#source-editor-message")).to_contain_text("editable source spans")

    def source(self):
        return self.page.locator("#source").input_value()

    def choose(self, name):
        value = self.page.locator("#source-editor-elements option").evaluate_all(
            "(options, name) => options.find(option => option.textContent.includes(': ' + name + ' — '))?.value", name)
        self.assertIsNotNone(value, f"binding for {name}")
        self.page.locator("#source-editor-elements").select_option(value)

    def stage(self, name, replacement):
        self.choose(name)
        self.page.locator("#source-editor-snippet").fill(replacement)
        self.page.locator("#source-editor-stage").click()

    def prepare(self):
        self.page.locator("#source-editor-batch-prepare").click()
        expect(self.page.locator("#source-editor-batch-preview")).to_be_visible()
        expect(self.page.locator("#source-editor-batch-apply")).to_be_enabled()

    def apply(self):
        self.page.locator("#source-editor-batch-apply").click()
        self.page.evaluate("window.renderPromise")

    def test_atomic_worker_batch_with_single_undo_redo_and_exact_export(self):
        self.mount()
        self.stage("A", "A[first 😀]")
        self.stage("B", "B[second 日本語]")
        self.assertEqual(self.source(), SOURCE)
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(2)
        self.assertEqual(self.page.evaluate("probe.sent.filter(m => m.kind === 'sourceBatch').length"), 0)
        self.prepare()
        preview = self.page.locator("#source-editor-batch-preview")
        expect(preview).to_contain_text("fixture parser warning")
        expect(preview).to_contain_text('Remove: "A[one]"')
        expect(preview).to_contain_text('Insert: "B[second 日本語]"')
        self.assertEqual(self.source(), SOURCE)
        self.assertEqual(self.page.evaluate("probe.changes"), [])
        self.assertEqual(self.page.evaluate("probe.localLoads"), 0, "the worker did the work")
        self.assertEqual(self.page.evaluate("probe.sent.filter(m => m.kind === 'sourceBatch').length"), 1)
        artifacts = os.environ.get("FM_BROWSER_ARTIFACT_DIR")
        if artifacts:
            Path(artifacts).mkdir(parents=True, exist_ok=True)
            self.page.locator("#source-editor-batch").screenshot(path=str(Path(artifacts) / "batch-preview.png"))
        self.apply()
        expected = SOURCE.replace("A[one]", "A[first 😀]").replace("B[two]", "B[second 日本語]")
        self.assertEqual(self.source(), expected)
        self.assertEqual(self.page.evaluate("probe.changes"), [expected])
        self.assertEqual(self.page.evaluate("editor.exportSource().text"), expected)
        self.assertNotIn("data-source-staged", self.page.evaluate("editor.exportSvg().text"))
        self.page.locator("#source-editor-undo").click(); self.page.evaluate("window.renderPromise")
        self.assertEqual(self.source(), SOURCE)
        self.page.locator("#source-editor-redo").click(); self.page.evaluate("window.renderPromise")
        self.assertEqual(self.source(), expected)

    def test_fallback_has_the_same_preview_apply_and_undo_workflow(self):
        self.mount(fallback=True)
        self.stage("A", "A[fallback]"); self.stage("B", "B[also fallback]")
        self.prepare()
        self.assertEqual(self.source(), SOURCE)
        self.apply()
        self.assertEqual(self.page.evaluate("probe.localLoads"), 1)
        self.assertEqual(self.page.evaluate("probe.workers.length"), 0)
        self.page.locator("#source-editor-undo").click(); self.page.evaluate("window.renderPromise")
        self.assertEqual(self.source(), SOURCE)

    def test_stage_update_unstage_and_noop_do_not_create_duplicate_entries(self):
        self.mount()
        self.stage("A", "A[first]"); self.stage("B", "B[second]")
        self.choose("A")
        expect(self.page.locator("#source-editor-snippet")).to_have_value("A[first]")
        self.page.locator("#source-editor-snippet").fill("A[updated]")
        self.page.locator("#source-editor-stage").click()
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(2)
        self.page.get_by_role("button", name="Unstage B", exact=True).click()
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(1)
        self.page.locator("#source-editor-snippet").fill("A[one]")
        self.page.locator("#source-editor-stage").click()
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(0)
        expect(self.page.locator("#source-editor-apply")).to_be_enabled()
        self.assertEqual(self.source(), SOURCE)

    def test_navigation_and_filters_preserve_staged_replacements(self):
        self.mount()
        self.stage("A", "A[staged]")
        self.page.locator("#source-editor-search").fill("B")
        self.stage("B", "B[staged]")
        self.page.locator("#source-editor-search").fill("no results")
        expect(self.page.locator("#source-editor-elements")).to_be_disabled()
        self.prepare()  # A batch does not require a current selection.
        self.apply()
        self.assertIn("A[staged]", self.source()); self.assertIn("B[staged]", self.source())

    def test_overlapping_statement_aliases_are_rejected_without_losing_valid_drafts(self):
        self.mount(overlap=True)
        self.stage("A", "A[valid]")
        self.stage("A-edge", "A[overlap]")
        expect(self.page.locator("#source-editor-message")).to_contain_text("Overlapping source spans")
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(1)
        expect(self.page.locator("#source-editor-apply")).to_be_disabled()
        expect(self.page.locator("#source-editor-delete")).to_be_disabled()
        self.assertEqual(self.source(), SOURCE)

    def test_discard_preview_retains_staging_and_reprepare_still_works(self):
        self.mount()
        self.stage("A", "A[ready]"); self.stage("B", "B[ready]")
        self.prepare()
        self.page.locator("#source-editor-batch-cancel").click()
        expect(self.page.locator("#source-editor-batch-preview")).to_be_hidden()
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(2)
        self.assertEqual(self.source(), SOURCE)
        self.prepare(); self.apply()
        self.assertIn("A[ready]", self.source())

    def test_clear_pending_batch_does_not_publish_a_delayed_response(self):
        self.mount(holdBatch=True)
        self.stage("A", "A[late]"); self.stage("B", "B[late]")
        self.page.locator("#source-editor-batch-prepare").click()
        self.page.wait_for_function("probe.held.length === 1")
        self.page.locator("#source-editor-batch-clear").click()
        self.page.evaluate("flushBatch()")
        expect(self.page.locator("#source-editor-batch-preview")).to_be_hidden()
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(0)
        expect(self.page.locator("#out")).to_have_attribute("aria-busy", "false")
        expect(self.page.locator("#source-editor-message")).to_contain_text("Staged edits cleared")
        self.assertEqual(self.source(), SOURCE)
        self.assertEqual(self.page.evaluate("probe.changes"), [])

    def test_source_change_cancels_pending_work_but_preserves_drafts_for_copying(self):
        self.mount(holdBatch=True)
        self.stage("A", "A[keep this draft]")
        self.page.locator("#source-editor-batch-prepare").click()
        self.page.wait_for_function("probe.held.length === 1")
        changed = SOURCE.replace("A[one]", "A[typed independently]")
        self.page.locator("#source").fill(changed)
        self.page.evaluate("editor.render(document.getElementById('source').value)")
        self.page.evaluate("flushBatch()")
        self.assertEqual(self.source(), changed)
        expect(self.page.locator("#source-editor-batch-status")).to_contain_text("retained for copying only")
        expect(self.page.locator("#source-editor-batch-list")).to_contain_text("keep this draft")
        expect(self.page.locator("#source-editor-batch-prepare")).to_be_disabled()
        expect(self.page.locator("#source-editor-batch-apply")).to_be_hidden()
        self.page.locator("#source-editor-batch-clear").click()
        self.stage("A", "A[new revision]"); self.prepare(); self.apply()
        self.assertIn("A[new revision]", self.source())

    def test_silent_source_change_is_checked_again_at_confirmation(self):
        self.mount()
        self.stage("A", "A[stale]"); self.prepare()
        self.page.evaluate("document.getElementById('source').value = 'flowchart LR\\nZ[new document]' ")
        self.page.locator("#source-editor-batch-apply").click()
        self.assertEqual(self.source(), "flowchart LR\nZ[new document]")
        expect(self.page.locator("#source-editor-message")).to_contain_text("stale")
        self.assertEqual(self.page.evaluate("probe.changes"), [])

    def test_identical_source_rerender_does_not_revive_an_old_batch(self):
        self.mount()
        self.stage("A", "A[stale]"); self.prepare()
        self.page.evaluate("editor.render(document.getElementById('source').value)")
        expect(self.page.locator("#source-editor-batch-apply")).to_be_hidden()
        expect(self.page.locator("#source-editor-batch-prepare")).to_be_disabled()
        self.assertEqual(self.source(), SOURCE)

    def test_engine_failure_leaves_source_unchanged_and_batch_can_be_corrected(self):
        self.mount()
        self.stage("A", "A[reject]"); self.stage("B", "B[valid]")
        self.page.locator("#source-editor-batch-prepare").click()
        expect(self.page.locator("#source-editor-message")).to_contain_text("fixture engine rejected edit")
        expect(self.page.locator("#source-editor-batch-apply")).to_be_hidden()
        expect(self.page.locator("#source-editor-batch-list > li")).to_have_count(2)
        self.assertEqual(self.source(), SOURCE)
        self.assertEqual(self.page.evaluate("probe.localLoads"), 0)
        self.stage("A", "A[corrected]"); self.prepare(); self.apply()
        self.assertIn("A[corrected]", self.source()); self.assertIn("B[valid]", self.source())

    def test_deletion_and_html_like_text_are_previewed_as_text_not_markup(self):
        self.mount()
        self.stage("A", "")
        replacement = "B[<img src=x onerror=globalThis.pwned=1>]"
        self.stage("B", replacement); self.prepare()
        self.assertEqual(self.page.locator("#source-editor-batch img").count(), 0)
        self.assertIsNone(self.page.evaluate("globalThis.pwned"))
        self.apply()
        self.assertEqual(self.source(), SOURCE.replace("A[one]", "").replace("B[two]", replacement))
        self.assertIsNone(self.page.evaluate("globalThis.pwned"))

    def test_composition_prevents_staging_and_invalidates_a_reviewed_batch(self):
        self.mount()
        self.stage("A", "A[IME]"); self.prepare()
        self.page.locator("#source-editor-snippet").dispatch_event("compositionstart")
        expect(self.page.locator("#source-editor-stage")).to_be_disabled()
        expect(self.page.locator("#source-editor-batch-apply")).to_be_hidden()
        expect(self.page.locator("#source-editor-batch-prepare")).to_be_disabled()
        self.page.locator("#source-editor-snippet").dispatch_event("compositionend")
        self.prepare()
        self.page.locator("#source").dispatch_event("compositionstart")
        expect(self.page.locator("#source-editor-batch-apply")).to_be_hidden()
        self.page.locator("#source").dispatch_event("compositionend")
        self.assertEqual(self.source(), SOURCE)

    def test_disposal_retires_held_work_and_disables_controls(self):
        self.mount(holdBatch=True)
        self.stage("A", "A[late]")
        self.page.locator("#source-editor-batch-prepare").click()
        self.page.wait_for_function("probe.held.length === 1")
        self.page.evaluate("editor.dispose(); flushBatch()")
        expect(self.page.locator("#source-editor-stage")).to_be_disabled()
        expect(self.page.locator("#source-editor-batch-apply")).to_be_hidden()
        self.assertTrue(self.page.evaluate("probe.workers.every(worker => worker.terminated)"))
        self.assertEqual(self.source(), SOURCE)

    def test_narrow_batch_panel_wraps_long_replacements_without_overflow(self):
        self.page.set_viewport_size({"width": 360, "height": 1000})
        self.mount()
        self.stage("A", "A[" + "x" * 1500 + "]")
        self.prepare()
        self.assertTrue(self.page.locator("#source-editor-batch").evaluate(
            "el => el.scrollWidth <= el.clientWidth + 1"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
