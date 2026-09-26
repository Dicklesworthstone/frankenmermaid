#!/usr/bin/env python3
"""Exercise production file-backed editing in Chromium.

Run: python web/fm-document-write.browser.test.py
The default uses loopback HTTP and native origin-private FileSystemFileHandles. The picker
is injected, not automated: these tests prove stream/DOM behavior, not OS picker UI or
permissions to user-visible files. No Mermaid/WASM or third-party JS dependencies.
FM_BROWSER_OFFLINE=1 uses set_content/Blob modules with explicit file and storage fixtures
when administrator policy blocks HTTP navigation. That mode proves DOM/controller behavior,
not native file streams; the native-exclusive-lock test is explicitly skipped.
"""
import os
from pathlib import Path
import shutil
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
PAGE = b'<!doctype html><textarea id="source">start</textarea><section id="panel"></section>'


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.path == "/":
            data, mime = PAGE, "text/html"
        elif self.path == "/fm-document.js":
            data, mime = (ROOT / "fm-document.js").read_bytes(), "text/javascript"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", mime + "; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class FileWorkspaceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.offline = os.environ.get("FM_BROWSER_OFFLINE") == "1"
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.playwright = sync_playwright().start()
        cls.browser = cls.playwright.chromium.launch(
            headless=True, executable_path=os.environ.get("CHROMIUM_PATH") or shutil.which("chromium"),
            args=["--no-sandbox"])

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.context = self.browser.new_context()
        self.page = self.context.new_page()
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        if self.offline:
            self.page.set_content(PAGE.decode())
        else:
            self.page.goto(f"http://127.0.0.1:{self.server.server_port}/")
        self.page.evaluate('''async ({offline,code}) => {
          window.moduleUrl=offline?URL.createObjectURL(new Blob([code],{type:'text/javascript'})):null;
          window.module = await import(moduleUrl || '/fm-document.js');
          if (offline) {
            const records=new Map(), files=new Map();
            window.recoveryStorage={get length(){return records.size;},
              key:i=>[...records.keys()][i]??null,getItem:k=>records.get(k)??null,
              setItem:(k,v)=>records.set(k,String(v))};
            window.root={async getFileHandle(name){
              if(!files.has(name))files.set(name,{bytes:new Uint8Array()});
              const record=files.get(name);
              return {kind:'file',name,_record:record,
                getFile:async()=>new File([record.bytes],name),
                isSameEntry:async other=>other._record===record,
                requestPermission:async()=> 'granted',
                createWritable:async()=>{
                  let staged;
                  return {write:async bytes=>{staged=bytes.slice();},
                    close:async()=>{record.bytes=staged;},abort:async()=>{}};
                }};
            }};
          } else {
            window.root = await navigator.storage.getDirectory();
            window.recoveryStorage=localStorage;
          }
          window.handle = await root.getFileHandle('diagram.mmd', {create:true});
          window.writeDisk = async source => {
            const stream=await handle.createWritable();
            await stream.write(new TextEncoder().encode(source)); await stream.close();
          };
          window.readDisk = async () => new TextDecoder('utf-8',{fatal:true,ignoreBOM:true})
            .decode(await (await handle.getFile()).arrayBuffer());
          await writeDisk('original');
          window.approve=() => true; window.resets=0; window.renders=0; window.saved=[];
          window.showOpenFilePicker=async () => [handle];
          window.copyHandle=await root.getFileHandle('copy.mmd',{create:true});
          window.showSaveFilePicker=async () => copyHandle;
          window.workspace=module.mountDocumentWorkspace({
            sourceEl:document.querySelector('#source'),panelEl:document.querySelector('#panel'),
            confirmReplace:(...args)=>approve(...args),saveFile:artifact=>saved.push(artifact),
            getStorage:()=>recoveryStorage,
            onDocumentChange:()=>resets++,onChange:()=>renders++
          });
        }''', {"offline": self.offline, "code": (ROOT / "fm-document.js").read_text()})

    def tearDown(self):
        self.page.evaluate("workspace.dispose(); if(moduleUrl)URL.revokeObjectURL(moduleUrl)")
        self.context.close()
        self.assertEqual(self.errors, [])

    def test_native_streams_save_exact_encoding_and_do_not_reset_editor_selection(self):
        original = "\ufeffflowchart TD\r\n A[世界 😀]  \n B[Beta]\r"
        self.page.evaluate("source => writeDisk(source)", original)
        self.page.locator("#document-open-editable").click()
        self.page.wait_for_function("document.querySelector('#source').value.includes('Beta')")
        self.page.locator("#source").fill(original[1:].replace("\r\n", "\n").replace("\r", "\n").replace("Beta", "unfinished ["))
        self.page.evaluate("source.setSelectionRange(3, 9)")
        self.page.locator("#document-write").click()
        self.page.wait_for_function("document.querySelector('#document-message').textContent.startsWith('Saved ')")
        self.assertEqual(self.page.evaluate("readDisk()"), original.replace("Beta", "unfinished ["))
        self.assertEqual(self.page.evaluate("[source.selectionStart, source.selectionEnd, resets, renders]"), [3, 9, 1, 1])

    def test_real_swap_file_is_not_committed_until_close_and_later_edits_stay_dirty(self):
        self.page.evaluate('''async () => {
          const native=handle.createWritable.bind(handle);
          handle.createWritable=async options=>{
            const writer=await native(options);
            return {write:bytes=>writer.write(bytes),abort:()=>writer.abort(),close:async()=>{
              window.closing=true; await new Promise(r=>{window.release=r}); await writer.close();
            }};
          };
          await workspace.openEditableFile();
        }''')
        self.page.locator("#source").fill("snapshot")
        self.page.evaluate("window.pendingSave=workspace.saveToFile(); void 0")
        self.page.wait_for_function("window.closing")
        self.assertEqual(self.page.evaluate("readDisk()"), "original")
        self.page.locator("#source").fill("newer")
        self.page.evaluate("release()")
        self.assertTrue(self.page.evaluate("pendingSave"))
        self.assertEqual(self.page.evaluate("readDisk()"), "snapshot")
        self.assertEqual(self.page.locator("#source").input_value(), "newer")
        self.assertIn("unsaved", self.page.locator("#document-name").inner_text())

    def test_external_change_blocks_overwrite_and_reload_requires_confirmation(self):
        self.page.evaluate("workspace.openEditableFile()")
        self.page.locator("#source").fill("local edits")
        self.page.evaluate("writeDisk('external edits')")
        self.assertFalse(self.page.evaluate("workspace.saveToFile()"))
        self.assertEqual(self.page.evaluate("readDisk()"), "external edits")
        self.assertIn("changed on disk", self.page.locator("#document-message").inner_text())
        self.page.evaluate("window.approve=()=>false")
        self.assertFalse(self.page.evaluate("workspace.reloadFile()"))
        self.assertEqual(self.page.locator("#source").input_value(), "local edits")
        self.page.evaluate("window.approve=()=>true")
        self.assertTrue(self.page.evaluate("workspace.reloadFile()"))
        self.assertEqual(self.page.locator("#source").input_value(), "external edits")
        self.assertTrue(self.page.evaluate("Array.from({length:recoveryStorage.length},(_,i)=>recoveryStorage.getItem(recoveryStorage.key(i))).some(v=>v.includes('local edits'))"))

    def test_aborting_a_real_staged_write_keeps_disk_and_current_draft(self):
        self.page.evaluate('''async () => {
          const native=handle.createWritable.bind(handle);
          handle.createWritable=async options=>{
            const writer=await native(options);
            return {write:async bytes=>{await writer.write(bytes);throw Error('staging failure');},
              close:()=>writer.close(),abort:()=>writer.abort()};
          };
          await workspace.openEditableFile();
        }''')
        self.page.locator("#source").fill("keep draft")
        self.assertFalse(self.page.evaluate("workspace.saveToFile()"))
        self.assertEqual(self.page.evaluate("readDisk()"), "original")
        self.assertEqual(self.page.locator("#source").input_value(), "keep draft")
        self.assertIn("unsaved", self.page.locator("#document-name").inner_text())

    def test_changing_documents_aborts_a_real_staged_write(self):
        self.page.evaluate('''async () => {
          const native=handle.createWritable.bind(handle);
          handle.createWritable=async options=>{
            const writer=await native(options);
            return {write:async bytes=>{await writer.write(bytes);window.staged=true;
              await new Promise(r=>{window.release=r});},close:()=>writer.close(),abort:()=>writer.abort()};
          };
          await workspace.openEditableFile();
        }''')
        self.page.locator("#source").fill("old edit")
        self.page.evaluate("window.pendingSave=workspace.saveToFile();void 0")
        self.page.wait_for_function("window.staged")
        self.page.evaluate("workspace.openSharedSource(async()=>({source:'new document',name:'new.mmd'}))")
        self.page.evaluate("release()")
        self.assertFalse(self.page.evaluate("pendingSave"))
        self.assertEqual(self.page.evaluate("readDisk()"), "original")
        self.assertEqual(self.page.locator("#source").input_value(), "new document")
        self.assertTrue(self.page.locator("#document-write").is_hidden())

    def test_exclusive_native_writers_reject_competing_bindings(self):
        if self.offline:
            self.skipTest("Offline fixtures do not prove native exclusive-lock semantics")
        result = self.page.evaluate('''async () => {
          const one=await module.SourceFileBinding.open(handle);
          const two=await module.SourceFileBinding.open(handle);
          const native=handle.createWritable.bind(handle);
          let release, started;
          const waiting=new Promise(r=>{started=r});
          handle.createWritable=async options=>{
            const writer=await native(options);
            return {write:async bytes=>{await writer.write(bytes);started();await new Promise(r=>{release=r});},
              close:()=>writer.close(),abort:()=>writer.abort()};
          };
          const saving=one.save('first'); await waiting;
          let error; try {await two.save('second');}catch(e){error=e.name;}
          release(); await saving;
          return {error,source:await readDisk()};
        }''')
        self.assertEqual(result, {"error": "NoModificationAllowedError", "source": "first"})

    def test_download_is_not_an_in_place_save_and_handles_are_never_serialized(self):
        self.page.evaluate("workspace.openEditableFile()")
        self.page.locator("#source").fill("downloaded draft")
        self.assertTrue(self.page.evaluate("workspace.saveSource()"))
        self.assertEqual(self.page.evaluate("readDisk()"), "original")
        self.assertEqual(self.page.evaluate("saved[0].text"), "downloaded draft")
        self.assertIn("unsaved file changes", self.page.locator("#document-name").inner_text())
        self.assertEqual(self.page.evaluate("Object.keys(workspace.sourceSnapshot()).sort()"), ["name", "revision", "source"])

    def test_markup_in_filename_and_source_stays_inert(self):
        text = '<img src=x onerror="window.injected=true">'
        self.page.evaluate("text=>writeDisk(text)", text)
        self.page.evaluate("workspace.openEditableFile()")
        self.assertEqual(self.page.locator("#source").input_value(), text)
        self.assertEqual(self.page.locator("#panel img").count(), 0)
        self.assertTrue(self.page.evaluate("window.injected===undefined"))

    def test_keyboard_save_and_save_as_follow_the_active_destination(self):
        self.page.evaluate("workspace.openEditableFile()")
        self.page.locator("#source").fill("native destination")
        self.page.keyboard.press("Control+s")
        self.page.wait_for_function("document.querySelector('#document-message').textContent.startsWith('Saved ')")
        self.assertEqual(self.page.evaluate("readDisk()"), "native destination")
        self.page.locator("#source").fill("copy destination")
        self.page.keyboard.press("Control+Shift+s")
        self.page.wait_for_function("workspace.sourceSnapshot().name==='copy.mmd'")
        self.assertEqual(self.page.evaluate("readDisk()"), "native destination")
        self.assertEqual(self.page.evaluate("copyHandle.getFile().then(f=>f.text())"), "copy destination")
        self.assertEqual(self.page.evaluate("resets"), 1)
        self.page.locator("#source").fill("later copy")
        self.page.keyboard.press("Control+s")
        self.page.wait_for_function("document.querySelector('#document-name').textContent==='copy.mmd'")
        self.assertEqual(self.page.evaluate("copyHandle.getFile().then(f=>f.text())"), "later copy")

    def test_save_as_keeps_edits_made_during_the_picker(self):
        self.page.locator("#source").fill("snapshot")
        self.page.evaluate("() => { window.showSaveFilePicker=()=>new Promise(r=>{window.choose=r}); }")
        self.page.locator("#document-save-as").click()
        self.page.locator("#source").fill("newer draft")
        self.page.evaluate("choose(copyHandle)")
        self.page.wait_for_function("workspace.sourceSnapshot().name==='copy.mmd'")
        self.assertEqual(self.page.locator("#source").input_value(), "newer draft")
        self.assertEqual(self.page.evaluate("copyHandle.getFile().then(f=>f.text())"), "snapshot")
        self.assertIn("unsaved", self.page.locator("#document-name").inner_text())

    def test_save_as_same_entry_does_not_bypass_an_external_edit(self):
        self.page.evaluate("workspace.openEditableFile()")
        self.page.locator("#source").fill("local")
        self.page.evaluate("writeDisk('external')")
        self.page.evaluate("() => { window.showSaveFilePicker=()=>root.getFileHandle('diagram.mmd'); }")
        self.assertFalse(self.page.evaluate("workspace.saveAsFile()"))
        self.assertEqual(self.page.evaluate("readDisk()"), "external")
        self.assertEqual(self.page.locator("#source").input_value(), "local")


if __name__ == "__main__":
    unittest.main(verbosity=2)
