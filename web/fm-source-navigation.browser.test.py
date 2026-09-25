"""Real Chromium SVG geometry and source-editor integration.

The unchanged production worker is served over loopback HTTP. Only its imported engine
is a fixture returning explicit SVG/source bindings, not an implementation of Mermaid.
Run: python web/fm-source-navigation.browser.test.py
FM_BROWSER_OFFLINE=1 exercises the explicit local fallback instead when loopback HTTP is
blocked by the browser's administrator. That mode does NOT test module-worker startup.
"""
import json
import os
from pathlib import Path
import shutil
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
SOURCE = "%% café 🐉\nflowchart LR\n  a[Login] --> b[Store]\n  c[Cache]\n  orphan[Unrendered]\n"
SPAN = "a[Login] --> b[Store]"

def binding(name, kind, snippet, source=SOURCE):
    start = source.index(snippet)
    return {"elementId": name, "sourceId": name, "kind": kind, "snippet": snippet,
            "textRange": {"startByte": len(source[:start].encode()),
                          "endByte": len(source[:start + len(snippet)].encode())}}

BINDINGS = [binding("a", "node", SPAN), binding("b", "node", SPAN), binding("c", "node", "c[Cache]"),
            binding("edge", "edge", SPAN), binding("cluster", "cluster", SPAN), binding("orphan", "node", "orphan[Unrendered]")]
# Nonzero viewBox, nested transforms, and DOM order deliberately DIFFER from visual order.
SVG = '''<svg xmlns="http://www.w3.org/2000/svg" width="800" height="450" viewBox="-50 -25 800 450">
<g id="cluster"><rect x="-30" y="-10" width="720" height="350" fill="none" stroke="gray"/></g>
<g transform="translate(100 50)">
  <g id="c" transform="translate(200 180)"><rect width="90" height="45"/><text fill="white" y="25">Café cache</text></g>
  <g id="b" transform="translate(200 0) scale(1.2)"><rect width="90" height="45"/><text fill="white" y="25">Store</text></g>
  <g id="a" tabindex="5"><rect width="90" height="45"/><text fill="white" y="25">Sign in</text></g>
  <g id="edge"><path d="M90 22 L200 22" stroke="black"/></g>
</g></svg>'''

ENGINE = f'''
const source = {json.dumps(SOURCE)}, bindings = {json.dumps(BINDINGS)}, svg = {json.dumps(SVG)};
export default function() {{}}
export function chooseCanvasTarget() {{ return JSON.stringify({{target:"svgInWorker"}}); }}
export function parseLens(input) {{
  if (input !== source) throw Error("fixture source absent");
  return {{bindings:structuredClone(bindings),parsed:{{warnings:[]}}}};
}}
export function renderSvg(input) {{ if (input !== source) throw Error("fixture source absent"); return svg; }}
export function applyParseLensEdit() {{ throw Error("navigation must not mutate source"); }}
'''

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.path == "/":
            body = b'<!doctype html><style>#out{width:450px;height:260px;overflow:auto;border:1px solid}#panel{max-height:350px;overflow:auto}#src{width:450px;height:100px}</style><textarea id="src"></textarea><section id="panel"></section><div id="out"></div><button id="after">After diagram</button>'
            mime = "text/html"
        elif self.path == "/engine.js":
            body = ENGINE.encode()
            mime = "text/javascript"
        elif self.path in ["/fm-source-editor.js", "/fm-render.worker.js"]:
            body = (ROOT / self.path[1:]).read_bytes()
            mime = "text/javascript"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", mime + "; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class NavigationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.playwright = sync_playwright().start()
        cls.browser = cls.playwright.chromium.launch(headless=True,
            executable_path=os.environ.get("CHROMIUM_PATH") or shutil.which("chromium"), args=["--no-sandbox"])

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.page = self.browser.new_page()
        self.page.set_default_timeout(5000)
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        offline = os.environ.get("FM_BROWSER_OFFLINE") == "1"
        if offline:
            self.page.set_content('<style>#out{width:450px;height:260px;overflow:auto;border:1px solid}#panel{max-height:350px;overflow:auto}#src{width:450px;height:100px}</style><textarea id="src"></textarea><section id="panel"></section><div id="out"></div><button id="after">After diagram</button>')
        else:
            self.page.goto(f"http://127.0.0.1:{self.server.server_port}/")
        self.page.evaluate('''async ({source, editorCode, engineCode, offline}) => {
          window.urls = [];
          const moduleUrl = code => { const url = URL.createObjectURL(new Blob([code], {type:'text/javascript'})); urls.push(url); return url; };
          const {mountSourceEditor} = await import(offline ? moduleUrl(editorCode) : '/fm-source-editor.js');
          const localEngine = offline ? moduleUrl(engineCode) : null;
          src.value = source;
          window.changes = 0;
          window.editor = mountSourceEditor({sourceEl:src, outEl:out, panelEl:panel,
            loadModule:() => { if (offline) return import(localEngine); throw Error('unexpected synchronous fallback'); },
            workerOptions:offline ? {WorkerClass:null} : {workerUrl:'/fm-render.worker.js', moduleUrl:'/engine.js'},
            onChange:() => { changes++; }});
          await editor.render(source);
        }''', {"source": SOURCE, "editorCode": (ROOT / "fm-source-editor.js").read_text(), "engineCode": ENGINE, "offline": offline})
        target = "main-thread SVG fallback (synchronous)" if offline else "source worker"
        self.assertIn("6 editable source spans — " + target, self.page.locator("#source-editor-message").inner_text())

    def tearDown(self):
        self.page.evaluate("() => { editor.dispose(); urls.forEach(url => URL.revokeObjectURL(url)); }")
        self.page.close()
        self.assertEqual(self.errors, [])

    def selected(self):
        return self.page.locator("#out [data-source-selected]").get_attribute("id")

    def test_search_rendered_label_and_shared_span_identity(self):
        self.page.locator("#source-editor-search").fill("store")
        self.page.locator("#source-editor-search").press("Enter")
        # Source matches a and b (and the edge/cluster); explicit b must not revert to a.
        self.page.locator("#source-editor-elements").select_option("b")
        self.page.wait_for_timeout(20)
        self.assertEqual(self.selected(), "b")
        self.assertEqual(self.page.evaluate("src.value.slice(src.selectionStart,src.selectionEnd)"), SPAN)
        self.assertEqual(self.page.locator("#src").input_value(), SOURCE)
        self.assertEqual(self.page.evaluate("changes"), 0)

    def test_search_enter_and_shift_enter_cycle_without_stealing_focus(self):
        self.page.locator("#source-editor-kind").select_option("node")
        search = self.page.locator("#source-editor-search")
        search.fill("a")
        search.press("Enter")
        self.assertEqual(self.selected(), "a")
        search.press("Enter")
        self.assertEqual(self.selected(), "b")
        search.press("Shift+Enter")
        self.assertEqual(self.selected(), "a")
        self.assertEqual(self.page.evaluate("document.activeElement.id"), "source-editor-search")

    def test_arrows_use_transformed_visual_positions_and_keep_diagram_focus(self):
        self.page.locator("#a").focus()
        self.page.keyboard.press("ArrowRight")
        self.assertEqual(self.selected(), "b")
        self.assertEqual(self.page.evaluate("document.activeElement.id"), "b")
        self.page.keyboard.press("ArrowDown")
        self.assertEqual(self.selected(), "c")
        self.page.keyboard.press("ArrowUp")
        self.assertEqual(self.selected(), "b")
        self.page.keyboard.press("ArrowLeft")
        self.assertEqual(self.selected(), "a")
        self.page.keyboard.press("Enter")
        self.assertEqual(self.page.evaluate("document.activeElement.id"), "src")

    def test_home_end_and_roving_tab_leave_diagram(self):
        self.assertEqual(self.page.locator("#out [tabindex='0']").count(), 1)
        self.page.locator("#a").focus()
        self.page.keyboard.press("End")
        self.assertEqual(self.selected(), "c")
        self.assertEqual(self.page.locator("#out [tabindex='0']").count(), 1)
        self.page.keyboard.press("Tab")
        self.assertEqual(self.page.evaluate("document.activeElement.id"), "after")
        self.page.locator("#c").focus()
        self.page.keyboard.press("Home")
        self.assertEqual(self.selected(), "a")

    def test_no_matches_and_unrendered_bindings_are_honest(self):
        self.page.locator("#source-editor-search").fill("not here [.*]")
        self.assertTrue(self.page.locator("#source-editor-next").is_disabled())
        self.assertEqual(self.page.locator("#out [tabindex='0']").count(), 0)
        self.page.locator("#source-editor-search").fill("orphan")
        self.page.locator("#source-editor-next").click()
        self.assertIn("no addressable SVG", self.page.locator("#source-editor-search-status").inner_text())
        self.assertEqual(self.page.locator("#source-editor-snippet").input_value(), "orphan[Unrendered]")

    def test_literal_unicode_search_and_kind_filter(self):
        self.page.locator("#source-editor-search").fill("CAFE\u0301")
        self.assertIn("1 of 6", self.page.locator("#source-editor-search-status").inner_text())
        self.page.locator("#source-editor-search").press("Enter")
        self.assertEqual(self.selected(), "c")
        self.page.locator("#source-editor-search").fill("")
        self.page.locator("#source-editor-kind").select_option("edge")
        self.page.locator("#source-editor-next").click()
        self.assertEqual(self.selected(), "edge")

    def test_modified_arrows_and_textarea_keys_are_not_hijacked(self):
        self.page.locator("#a").focus()
        self.page.keyboard.press("Control+ArrowRight")
        self.assertEqual(self.page.locator("#out [data-source-selected]").count(), 0)
        self.page.locator("#src").focus()
        self.page.keyboard.press("ArrowRight")
        self.assertEqual(self.page.evaluate("document.activeElement.id"), "src")

    def test_reparse_clears_stale_selection_and_preserves_search(self):
        self.page.locator("#source-editor-search").fill("Sign in")
        self.page.locator("#source-editor-search").press("Enter")
        self.page.evaluate("() => editor.render(src.value)")
        self.assertEqual(self.page.locator("#out [data-source-selected]").count(), 0)
        self.assertIn("1 of 6", self.page.locator("#source-editor-search-status").inner_text())
        self.page.locator("#source-editor-search").press("Enter")
        self.assertEqual(self.selected(), "a")

    def test_failed_render_and_programmatic_changes_disable_navigation(self):
        self.page.evaluate("() => { src.value = 'invalid fixture'; return editor.render(src.value); }")
        self.assertTrue(self.page.locator("#source-editor-search").is_disabled())
        self.assertTrue(self.page.locator("#source-editor-next").is_disabled())
        self.page.locator("#a").focus()
        self.page.keyboard.press("ArrowRight")
        self.assertEqual(self.page.locator("#out [data-source-selected]").count(), 0)

    def test_dispose_restores_tabindex_and_export_is_original_svg(self):
        self.page.locator("#a").focus()
        self.page.keyboard.press("ArrowRight")
        self.assertEqual(self.page.evaluate("editor.exportSvg().text"), SVG)
        self.page.evaluate("editor.dispose()")
        self.assertEqual(self.page.locator("#a").get_attribute("tabindex"), "5")
        self.assertIsNone(self.page.locator("#b").get_attribute("tabindex"))
        self.assertEqual(self.page.locator("#out [data-source-editable]").count(), 0)

if __name__ == "__main__":
    unittest.main(verbosity=2)
