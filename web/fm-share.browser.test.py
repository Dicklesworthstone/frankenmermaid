#!/usr/bin/env python3
"""Real Chromium DOM, streams and history tests, independent of Mermaid/WASM.

Run: python web/fm-share.browser.test.py
FM_BROWSER_OFFLINE=1 uses set_content and Blob modules when browser policy blocks loopback.
Offline mode still tests native hashchange/popstate/back/forward, DOM, and stream codecs;
it does NOT test HTTP module loading, production playground startup, clipboard permission,
or WASM. URL generation is checked by the Node suite; about:blank correctly cannot share.
No browser policy is disabled and no external network requests are needed.
"""
import base64
import gzip
import json
import os
from pathlib import Path
import shutil
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
FIXTURE = '''<!doctype html><textarea id="source">start</textarea>
<section id="files"></section><section id="sharing"></section>'''


def fragment(source, name="shared.mmd"):
    data = json.dumps({"source": source, "name": name}, ensure_ascii=False).encode("utf-8")
    return "#fm:v1:gzip:" + base64.urlsafe_b64encode(gzip.compress(data, mtime=0)).decode().rstrip("=")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.path == "/":
            body, mime = FIXTURE.encode(), "text/html"
        elif self.path in ["/fm-document.js", "/fm-share.js"]:
            body, mime = (ROOT / self.path[1:]).read_bytes(), "text/javascript"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", mime + "; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class SharingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.playwright = sync_playwright().start()
        cls.browser = cls.playwright.chromium.launch(headless=True,
            executable_path=os.environ.get("CHROMIUM_PATH") or shutil.which("chromium"), args=["--no-sandbox"])
        cls.offline = os.environ.get("FM_BROWSER_OFFLINE") == "1"

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.page = self.browser.new_page(accept_downloads=True)
        self.page.set_default_timeout(5000)
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        if self.offline:
            self.page.set_content(FIXTURE)
        else:
            self.page.goto(f"http://127.0.0.1:{self.server.server_port}/")
        self.page.evaluate('''async ({documentCode, shareCode, offline}) => {
          window.urls = [];
          const moduleUrl = code => {
            const url = URL.createObjectURL(new Blob([code], {type:'text/javascript'}));
            urls.push(url); return url;
          };
          window.documentModule = await import(offline ? moduleUrl(documentCode) : '/fm-document.js');
          window.shareModule = await import(offline ? moduleUrl(shareCode) : '/fm-share.js');
          window.boot = async (hash = '') => {
            if (hash) { location.hash = hash; await new Promise(r => setTimeout(r, 0)); }
            const sourceEl = document.querySelector('#source');
            window.approve = () => true;
            window.resets = 0; window.changes = 0; window.saved = [];
            // Explicit storage fixture: about:blank has an opaque origin. The production
            // DraftRepository and replacement/recovery code still run unchanged.
            window.records = new Map();
            const storage = {
              get length(){return records.size;}, key(i){return [...records.keys()][i] ?? null;},
              getItem(k){return records.get(k) ?? null;}, setItem(k,v){records.set(k,v);}
            };
            window.workspace = documentModule.mountDocumentWorkspace({sourceEl,
              panelEl:document.querySelector('#files'), getStorage:() => storage,
              confirmReplace:(...args) => approve(...args), saveFile:artifact => saved.push(artifact),
              onDocumentChange:() => {resets++;},
              onChange:() => {changes++; window.sharing?.sourceChanged();}
            });
            window.sharing = shareModule.mountShareControls({workspace, panelEl:document.querySelector('#sharing')});
            sourceEl.addEventListener('input', () => sharing.sourceChanged());
            await sharing.ready;
          };
        }''', {"documentCode": (ROOT / "fm-document.js").read_text(),
               "shareCode": (ROOT / "fm-share.js").read_text(), "offline": self.offline})

    def tearDown(self):
        self.page.evaluate("() => { window.sharing?.dispose(); window.workspace?.dispose(); urls.forEach(u => URL.revokeObjectURL(u)); }")
        self.page.close()
        self.assertEqual(self.errors, [])

    def boot(self, hash_value=""):
        self.page.evaluate("hash => boot(hash)", hash_value)

    def navigate(self, source, name="shared.mmd"):
        self.page.evaluate("hash => {location.hash = hash;}", fragment(source, name))
        self.page.wait_for_function("expected => workspace.sourceSnapshot().source === expected", arg=source)

    def test_initial_link_preserves_exact_source_filename_and_download_artifact(self):
        source = "\ufeffflowchart TD\r\n A[世界 😀]  \n B[Beta]\r"
        self.boot(fragment(source, "世界.mmd"))
        self.assertEqual(self.page.evaluate("workspace.sourceSnapshot().source"), source)
        self.assertEqual(self.page.locator("#source").input_value(), source[1:].replace("\r\n", "\n").replace("\r", "\n"))
        self.assertEqual(self.page.locator("#document-name").inner_text(), "世界.mmd")
        self.page.locator("#document-save").click()
        self.page.wait_for_function("saved.length === 1")
        self.assertEqual(self.page.evaluate("saved[0].text"), source)
        self.assertEqual(self.page.evaluate("saved[0].filename"), "世界.mmd")
        self.assertEqual(self.page.evaluate("[resets, changes]"), [1, 1])

    def test_real_history_back_and_forward_restores_sources_once_per_traversal(self):
        self.boot()
        self.navigate("first", "first.mmd")
        self.navigate("digraph{a->b}", "second.dot")
        self.page.evaluate("history.back()")
        self.page.wait_for_function("workspace.sourceSnapshot().source === 'first'")
        self.assertEqual(self.page.locator("#document-name").inner_text(), "first.mmd")
        self.page.evaluate("history.forward()")
        self.page.wait_for_function("workspace.sourceSnapshot().source === 'digraph{a->b}'")
        self.page.wait_for_timeout(30)  # Allow the paired popstate/hashchange to both arrive.
        self.assertEqual(self.page.evaluate("[resets, changes]"), [4, 4])

    def test_native_unsaved_confirmation_decline_then_retry_retains_recovery(self):
        self.boot()
        self.page.locator("#source").fill("unsaved text")
        self.page.evaluate("() => {window.approve = message => window.confirm(message);}")
        self.page.once("dialog", lambda dialog: dialog.dismiss())
        self.page.evaluate("hash => {location.hash = hash;}", fragment("incoming"))
        self.page.wait_for_function("document.querySelector('#share-navigation-status').textContent.includes('not opened')")
        self.assertEqual(self.page.locator("#source").input_value(), "unsaved text")
        self.page.once("dialog", lambda dialog: dialog.accept())
        self.page.locator("#share-open").click()
        self.page.wait_for_function("workspace.sourceSnapshot().source === 'incoming'")
        self.assertTrue(self.page.evaluate("[...records.values()].some(v => JSON.parse(v).source === 'unsaved text')"))
        self.assertEqual(self.page.evaluate("resets"), 1)

    def test_delayed_confirmation_cannot_overwrite_a_later_navigation(self):
        self.boot()
        self.page.locator("#source").fill("unsaved")
        self.page.evaluate("() => {window.approvals=[]; window.approve=() => new Promise(r=>approvals.push(r));}")
        self.page.evaluate("hash => {location.hash = hash;}", fragment("old"))
        self.page.wait_for_function("approvals.length === 1")
        self.page.evaluate("hash => {location.hash = hash;}", fragment("latest"))
        self.page.wait_for_function("approvals.length === 2")
        self.page.evaluate("approvals[1](true)")
        self.page.wait_for_function("workspace.sourceSnapshot().source === 'latest'")
        self.page.evaluate("approvals[0](true)")
        self.page.wait_for_timeout(20)
        self.assertEqual(self.page.locator("#source").input_value(), "latest")
        self.assertEqual(self.page.evaluate("resets"), 1)

    def test_typing_during_gzip_decompression_cancels_the_open(self):
        self.boot()
        self.page.evaluate('''() => {
          const Native=DecompressionStream;
          const gate=new Promise(r=>{window.release=r});
          window.DecompressionStream=class {
            constructor(format){const native=new Native(format);return {
              writable:native.writable, readable:native.readable.pipeThrough(new TransformStream({
                async transform(chunk,controller){window.decoding=true;await gate;controller.enqueue(chunk)}
              }))
            }}
          };
        }''')
        self.page.evaluate("hash => {location.hash = hash;}", fragment("incoming\n" * 100))
        self.page.wait_for_function("window.decoding === true")
        self.page.locator("#source").fill("typed while decoding")
        self.page.evaluate("release()")
        self.page.wait_for_function("document.querySelector('#share-navigation-status').textContent.includes('not opened')")
        self.assertEqual(self.page.locator("#source").input_value(), "typed while decoding")
        self.assertEqual(self.page.evaluate("resets"), 0)

    def test_ordinary_anchor_cancels_pending_share_and_keeps_current_source(self):
        self.boot()
        self.page.locator("#source").fill("unsaved")
        self.page.evaluate("() => {window.approve=() => new Promise(r=>{window.confirmPending=r});}")
        self.page.evaluate("hash => {location.hash = hash;}", fragment("incoming"))
        self.page.wait_for_function("typeof confirmPending === 'function'")
        self.page.evaluate("location.hash = 'files'")
        self.page.wait_for_function("document.querySelector('#share-open').hidden")
        self.page.evaluate("confirmPending(true)")
        self.page.wait_for_timeout(20)
        self.assertEqual(self.page.locator("#source").input_value(), "unsaved")
        self.assertEqual(self.page.evaluate("resets"), 0)

    def test_malformed_future_links_leave_source_and_valid_navigation_still_works(self):
        self.boot("#fm:v99:raw:e30")
        self.assertIn("version", self.page.locator("#document-message").inner_text())
        self.assertEqual(self.page.locator("#source").input_value(), "start")
        self.page.evaluate("location.hash = '#fm:v1:raw:!!!!'")
        self.page.wait_for_function("document.querySelector('#document-message').textContent.includes('encoding')")
        self.navigate("valid")
        self.assertEqual(self.page.evaluate("resets"), 1)

    def test_shared_markup_and_filename_are_inert_text(self):
        source = '<img src=x onerror="globalThis.injected=true">\n<script>globalThis.injected=true</script>'
        self.boot(fragment(source, "<img>.mmd"))
        self.assertEqual(self.page.locator("#source").input_value(), source)
        self.assertEqual(self.page.locator("#document-name").inner_text(), "<img>.mmd")
        self.assertEqual(self.page.locator("#files img, #sharing img, #files script, #sharing script").count(), 0)
        self.assertTrue(self.page.evaluate("window.injected === undefined"))

    def test_duplicate_events_preserve_selection_and_edits(self):
        self.boot(fragment("original"))
        self.page.locator("#source").fill("edited source")
        self.page.evaluate("document.querySelector('#source').setSelectionRange(2, 5)")
        self.page.evaluate("dispatchEvent(new PopStateEvent('popstate'));dispatchEvent(new HashChangeEvent('hashchange'))")
        self.page.wait_for_timeout(20)
        self.assertEqual(self.page.evaluate("[source.selectionStart, source.selectionEnd]"), [2, 5])
        self.assertEqual(self.page.locator("#source").input_value(), "edited source")
        self.assertEqual(self.page.evaluate("resets"), 1)
        self.assertIn("original shared source", self.page.locator("#share-navigation-status").inner_text())

    def test_browser_stream_codec_roundtrip_and_decompression_limit(self):
        self.boot()
        result = self.page.evaluate('''async () => {
          const source='😀\\r\\n'.repeat(2000), name='Unicode.mmd';
          const hash=await shareModule.encodeShareHash({source,name});
          const decoded=await shareModule.decodeShareHash(hash);
          return {equal:decoded.source===source&&decoded.name===name,gzip:hash.includes(':gzip:')};
        }''')
        self.assertEqual(result, {"equal": True, "gzip": True})
        bomb = "#fm:v1:gzip:" + base64.urlsafe_b64encode(gzip.compress(b"A" * (256 * 1024 * 7))).decode().rstrip("=")
        reason = self.page.evaluate("async hash => {try{await shareModule.decodeShareHash(hash);return null;}catch(e){return e.message;}}", bomb)
        self.assertIn("decoded size limit", reason)

    def test_generation_requires_http_or_https_and_never_rewrites_history(self):
        self.boot()
        before = self.page.evaluate("location.href")
        result = self.page.evaluate("sharing.createLink()")
        self.assertEqual(self.page.evaluate("location.href"), before)
        if self.offline:
            self.assertFalse(result)
            self.assertIn("HTTP or HTTPS", self.page.locator("#share-status").inner_text())
        else:
            self.assertTrue(result)
            self.assertTrue(self.page.locator("#share-url").input_value().startswith(before + "#fm:v1:"))
            self.page.evaluate("Object.defineProperty(navigator,'clipboard',{value:{writeText:async()=>{throw Error('denied')}}})")
            self.page.locator("#share-copy").click()
            self.page.wait_for_function("document.activeElement.id === 'share-url'")
            self.assertEqual(self.page.evaluate("document.activeElement.selectionStart"), 0)

    def test_disposal_cancels_navigation_and_removes_history_listeners(self):
        self.boot()
        self.page.locator("#source").fill("retained")
        self.page.evaluate("() => {window.approve=() => new Promise(r=>{window.confirmPending=r});}")
        self.page.evaluate("hash => {location.hash = hash;}", fragment("old"))
        self.page.wait_for_function("typeof confirmPending === 'function'")
        self.page.evaluate("sharing.dispose();workspace.dispose();confirmPending(true)")
        self.page.evaluate("hash => {location.hash = hash;}", fragment("new"))
        self.page.wait_for_timeout(20)
        self.assertEqual(self.page.locator("#source").input_value(), "retained")
        self.assertTrue(self.page.locator("#share-open").is_disabled())
        self.assertFalse(self.page.evaluate("sharing.openCurrentLink()"))
        self.assertEqual(self.page.evaluate("resets"), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
