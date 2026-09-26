"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-document.js"))).toString("base64")}`);
const tick = () => new Promise(resolve => setImmediate(resolve));
const gate = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };

// A transactional file fixture. Writes land in a swap buffer until close succeeds, as the
// File System API specifies; external writes and failures can be injected at each boundary.
class FileHandleFixture {
  kind = "file";
  permission = "granted";
  writes = 0; closes = 0; aborts = 0; reads = 0; creates = 0; permissions = 0;
  constructor(source = "original", name = "diagram.mmd") {
    this.name = name;
    this.bytes = new TextEncoder().encode(source);
  }
  get source() { return new TextDecoder("utf-8", { ignoreBOM: true }).decode(this.bytes); }
  external(source) { this.bytes = new TextEncoder().encode(source); }
  async getFile() {
    this.reads++;
    await this.onRead?.(this.reads);
    return new File([this.bytes], this.name, { lastModified: 1000 });
  }
  async requestPermission(options) {
    this.permissions++;
    assert.equal(options.mode, "readwrite");
    await this.onPermission?.();
    return this.permission;
  }
  async createWritable(options) {
    this.creates++;
    assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
    await this.onCreate?.();
    let staged;
    return {
      write: async bytes => { this.writes++; staged = bytes.slice(); await this.onWrite?.(); },
      close: async () => { await this.onClose?.(); this.bytes = staged; this.closes++; },
      abort: async () => { this.aborts++; await this.onAbort?.(); },
    };
  }
}
class ElementFixture extends EventTarget {
  constructor(document) { super(); this.ownerDocument = document; this.children = []; this.value = ""; this.textContent = ""; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  setAttribute(name, value) { this[name] = String(value); }
  click() { if (!this.disabled) this.dispatchEvent(new Event("click")); }
}
class StorageFixture {
  values = new Map();
  get length() { return this.values.size; }
  key(index) { return [...this.values.keys()][index] ?? null; }
  getItem(key) { return this.values.get(key) ?? null; }
  setItem(key, value) { this.values.set(key, value); }
}
async function workspaceFixture(t, { source = "start", handle, maxBytes } = {}) {
  const { mountDocumentWorkspace } = await loaded;
  const host = new EventTarget();
  Object.assign(host, { setTimeout, clearTimeout, crypto: globalThis.crypto, localStorage: new StorageFixture() });
  if (handle) host.showOpenFilePicker = async () => [handle];
  const document = new EventTarget();
  document.defaultView = host;
  document.createElement = () => new ElementFixture(document);
  const sourceEl = document.createElement("textarea"), panel = document.createElement("section");
  sourceEl.value = source;
  const f = { host, document, sourceEl, panel, resets: 0, changes: 0, saved: [], approve: () => true };
  f.workspace = mountDocumentWorkspace({ sourceEl, panelEl: panel, maxBytes,
    confirmReplace: (...args) => f.approve(...args), saveFile: artifact => { f.saved.push(artifact); },
    onDocumentChange() { f.resets++; }, onChange() { f.changes++; } });
  f.byId = id => panel.children.find(element => element.id === id);
  f.edit = text => { sourceEl.value = text; sourceEl.dispatchEvent(new Event("input")); };
  t.after(() => f.workspace.dispose());
  return f;
}

test("file binding preserves BOM, mixed line endings, Unicode and unfinished syntax exactly", async () => {
  const { SourceFileBinding } = await loaded;
  for (const source of ["\uFEFFflowchart TD\r\nA[世界 😀]  \nB[unfinished\r", "", "digraph { a -> b }"]) {
    const h = new FileHandleFixture("before", "世界.mmd");
    const binding = await SourceFileBinding.open(h);
    assert.equal(binding.name, "世界.mmd");
    assert.equal(await binding.save(source), true);
    assert.deepEqual(h.bytes, new TextEncoder().encode(source));
    assert.equal(binding.source, source);
    assert.equal(h.writes, 1); assert.equal(h.closes, 1); assert.equal(h.aborts, 0);
  }
});
test("native open rejects non-source files, malformed encodings, binary data and bad handles", async () => {
  const { SourceFileBinding } = await loaded;
  for (const h of [null, {}, { kind: "directory" }, new FileHandleFixture("text", "x.svg"),
    new FileHandleFixture("A\0B")]) await assert.rejects(SourceFileBinding.open(h));
  const bad = new FileHandleFixture(); bad.bytes = Uint8Array.of(0xc3, 0x28);
  await assert.rejects(SourceFileBinding.open(bad), /UTF-8/);
});
test("save validates UTF-8 source budgets and rejects lossy UTF-16 or binary source before writing", async () => {
  const { SourceFileBinding } = await loaded;
  const h = new FileHandleFixture("old"), binding = await SourceFileBinding.open(h, 8);
  for (const source of ["\ud800", "a\0b", "😀😀x", "abcdefghi", null]) {
    await assert.rejects(binding.save(source));
    assert.equal(h.source, "old"); assert.equal(h.creates, 0);
  }
  assert.equal(await binding.save("😀😀"), true);
  assert.equal(h.bytes.length, 8);
});
test("same-size external changes with unchanged timestamps are conflicts, not permission to overwrite", async () => {
  const { SourceFileBinding } = await loaded;
  const h = new FileHandleFixture("aaaa"), binding = await SourceFileBinding.open(h);
  h.external("bbbb");
  await assert.rejects(binding.save("mine"), /changed on disk/);
  assert.equal(h.source, "bbbb"); assert.equal(binding.source, "aaaa");
  assert.equal(h.creates, 0);
});
test("external writes during writer acquisition and staging abort instead of closing over them", async () => {
  const { SourceFileBinding } = await loaded;
  for (const hook of ["onCreate", "onWrite"]) {
    const h = new FileHandleFixture(), binding = await SourceFileBinding.open(h);
    h[hook] = () => h.external("outside");
    await assert.rejects(binding.save("mine"), /changed on disk/);
    assert.equal(h.source, "outside"); assert.equal(h.closes, 0); assert.equal(h.aborts, 1);
    assert.equal(binding.source, "original");
  }
});
test("permission denial leaves source and disk untouched and does not poison an explicit retry", async () => {
  const { SourceFileBinding } = await loaded;
  const h = new FileHandleFixture(), binding = await SourceFileBinding.open(h);
  h.permission = "denied";
  await assert.rejects(binding.save("mine"), { name: "NotAllowedError" });
  assert.equal(h.creates, 0); assert.equal(binding.source, "original");
  h.permission = "granted";
  assert.equal(await binding.save("mine"), true);
});
test("write/close/abort failures do not mark a file saved or mask the original error", async () => {
  const { SourceFileBinding } = await loaded;
  for (const hook of ["onWrite", "onClose"]) {
    const h = new FileHandleFixture(), binding = await SourceFileBinding.open(h);
    h[hook] = () => { throw new Error("original failure"); };
    h.onAbort = () => { throw new Error("abort failure"); };
    await assert.rejects(binding.save("mine"), /original failure/);
    assert.equal(binding.source, "original"); assert.equal(h.source, "original");
    assert.equal(h.aborts, 1); assert.equal(h.closes, 0);
    h[hook] = null; h.onAbort = null;
    assert.equal(await binding.save("retry"), true);
  }
});
test("document cancellation is checked after permission, writer acquisition and staged writes", async () => {
  const { SourceFileBinding } = await loaded;
  for (const hook of ["onPermission", "onCreate", "onWrite"]) {
    const h = new FileHandleFixture(), binding = await SourceFileBinding.open(h);
    let current = true;
    h[hook] = () => { current = false; };
    await assert.rejects(binding.save("mine", { isCurrent: () => current }), { name: "AbortError" });
    assert.equal(h.source, "original"); assert.equal(h.closes, 0);
    assert.equal(h.aborts, hook === "onPermission" ? 0 : 1);
  }
});
test("concurrent saves do not create last-writer-wins data loss", async () => {
  const { SourceFileBinding } = await loaded;
  const h = new FileHandleFixture(), binding = await SourceFileBinding.open(h), paused = gate();
  h.onWrite = () => paused.promise;
  const first = binding.save("first");
  await tick();
  await assert.rejects(binding.save("second"), /already in progress/);
  assert.equal(h.source, "original"); assert.equal(binding.source, "original");
  paused.resolve(); await first;
  assert.equal(h.source, "first");
});
test("an unchanged save still detects external edits but does not open an unnecessary writer", async () => {
  const { SourceFileBinding } = await loaded;
  const h = new FileHandleFixture(), binding = await SourceFileBinding.open(h);
  assert.equal(await binding.save("original"), true);
  assert.equal(h.creates, 0);
  h.external("external");
  await assert.rejects(binding.save("original"), /changed on disk/);
});
test("reload creates a newly observed binding without changing the old conflict baseline", async () => {
  const { SourceFileBinding } = await loaded;
  const h = new FileHandleFixture(), original = await SourceFileBinding.open(h);
  h.external("new disk contents");
  const reloaded = await original.reload();
  assert.equal(original.source, "original"); assert.equal(reloaded.source, "new disk contents");
  assert.equal(await reloaded.save("edited"), true);
});
test("editable open resets the editor and exposes Save/Reload without requesting write permission", async t => {
  const h = new FileHandleFixture("\uFEFFA\r\n", "source.dot"), f = await workspaceFixture(t, { handle: h });
  assert.equal(await f.workspace.openEditableFile(), true);
  assert.equal(f.sourceEl.value, "A\n");
  assert.equal(f.workspace.sourceSnapshot().source, "\uFEFFA\r\n");
  assert.equal(f.workspace.sourceSnapshot().name, "source.dot");
  assert.equal(f.byId("document-write").hidden, false);
  assert.equal(h.permissions, 0); assert.equal(h.creates, 0);
  assert.equal(f.resets, 1); assert.equal(f.changes, 1);
});
test("cancelled pickers, declined replacements and typing during a picker do not adopt its handle", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  f.host.showOpenFilePicker = async () => { throw new DOMException("cancelled", "AbortError"); };
  assert.equal(await f.workspace.openEditableFile(), false);
  assert.match(f.byId("document-message").textContent, /Open cancelled/);
  f.edit("my work"); f.approve = () => false;
  f.host.showOpenFilePicker = async () => [h];
  assert.equal(await f.workspace.openEditableFile(), false);
  assert.equal(f.sourceEl.value, "my work");
  f.approve = () => true;
  const paused = gate(); f.host.showOpenFilePicker = () => paused.promise;
  const pending = f.workspace.openEditableFile(); f.edit("newer work"); paused.resolve([h]);
  assert.equal(await pending, false);
  assert.equal(await f.workspace.saveToFile(), false);
  assert.equal(h.creates, 0); assert.equal(f.resets, 0);
});
test("file save updates recovery only after close and preserves newer edits made while saving", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("snapshot");
  const paused = gate(); h.onClose = () => paused.promise;
  const pending = f.workspace.saveToFile(); await tick();
  assert.match(f.byId("document-name").textContent, /unsaved/);
  assert.equal(h.source, "original");
  f.edit("newer edit");
  assert.equal(await f.workspace.saveToFile(), false, "no concurrent writer");
  paused.resolve(); assert.equal(await pending, true);
  assert.equal(h.source, "snapshot"); assert.equal(f.sourceEl.value, "newer edit");
  assert.match(f.byId("document-name").textContent, /unsaved/);
  assert.match(f.byId("document-message").textContent, /Newer edits/);
  const records = [...f.host.localStorage.values.values()].map(JSON.parse);
  assert.ok(records.some(record => record.source === "newer edit" && record.baseline === "snapshot"));
});
test("successful save preserves selection and does not reset editor history or render again", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("saved");
  f.sourceEl.selectionStart = 1; f.sourceEl.selectionEnd = 3;
  assert.equal(await f.workspace.saveToFile(), true);
  assert.equal(f.byId("document-name").textContent, "diagram.mmd");
  assert.equal(f.sourceEl.selectionStart, 1); assert.equal(f.sourceEl.selectionEnd, 3);
  assert.equal(f.resets, 1); assert.equal(f.changes, 1);
});
test("downloading a copy does not pretend the bound disk file is saved or reset conflict detection", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("downloaded");
  assert.equal(await f.workspace.saveSource(), true);
  assert.equal(f.saved[0].text, "downloaded"); assert.equal(h.source, "original");
  assert.match(f.byId("document-name").textContent, /unsaved file changes/);
  const unload = new Event("beforeunload", { cancelable: true });
  Object.defineProperty(unload, "returnValue", { value: "", writable: true });
  f.host.dispatchEvent(unload);
  assert.equal(unload.defaultPrevented, true);
  h.external("outside");
  assert.equal(await f.workspace.saveToFile(), false);
  assert.equal(h.source, "outside");
  assert.match(f.byId("document-message").textContent, /changed on disk/);
});
test("reloading external changes is guarded and retains the outgoing edited document in recovery", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("local draft"); h.external("outside");
  f.approve = () => false;
  assert.equal(await f.workspace.reloadFile(), false);
  assert.equal(f.sourceEl.value, "local draft");
  f.approve = () => true;
  assert.equal(await f.workspace.reloadFile(), true);
  assert.equal(f.sourceEl.value, "outside");
  assert.ok([...f.host.localStorage.values.values()].map(JSON.parse).some(record => record.source === "local draft"));
  f.edit("based on reloaded"); assert.equal(await f.workspace.saveToFile(), true);
});
test("ordinary file import and shared-source replacement detach write authority", async t => {
  for (const kind of ["file", "shared"]) {
    const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
    await f.workspace.openEditableFile();
    if (kind === "file") await f.workspace.openFile(new File(["incoming"], "incoming.mmd"));
    else await f.workspace.openSharedSource(async () => ({ source: "incoming", name: "incoming.mmd" }));
    f.edit("unbound edit");
    assert.equal(await f.workspace.saveToFile(), false);
    assert.equal(h.source, "original"); assert.equal(f.byId("document-write").hidden, true);
  }
});
test("changing documents during permission or staging cancels the old write before commit", async t => {
  for (const hook of ["onPermission", "onWrite"]) {
    const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
    await f.workspace.openEditableFile(); f.edit("old document edit");
    const paused = gate(); h[hook] = () => paused.promise;
    const pending = f.workspace.saveToFile(); await tick();
    await f.workspace.openFile(new File(["other document"], "other.mmd"));
    paused.resolve(); assert.equal(await pending, false);
    assert.equal(h.source, "original"); assert.equal(f.sourceEl.value, "other document");
  }
});
test("a close already in progress may finish but cannot mark a replacement document saved", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("old saved snapshot");
  const paused = gate(); h.onClose = () => paused.promise;
  const pending = f.workspace.saveToFile(); await tick();
  await f.workspace.openFile(new File(["other"], "other.mmd")); f.edit("other unsaved");
  paused.resolve(); assert.equal(await pending, false);
  assert.equal(h.source, "old saved snapshot"); assert.equal(f.sourceEl.value, "other unsaved");
  assert.match(f.byId("document-name").textContent, /unexported/);
});
test("file authority never enters recovery records or public source snapshots", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  h.privateAuthority = "never-share-this-handle";
  await f.workspace.openEditableFile(); f.edit("edit"); f.workspace.flushRecovery();
  const snapshot = f.workspace.sourceSnapshot();
  assert.deepEqual(Object.keys(snapshot).sort(), ["name", "revision", "source"]);
  assert.ok(!JSON.stringify([...f.host.localStorage.values]).includes(h.privateAuthority));
});
test("disposal cancels staged writes and does not revive buttons on completion", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("staged");
  const paused = gate(); h.onWrite = () => paused.promise;
  const pending = f.workspace.saveToFile(); await tick(); f.workspace.dispose(); paused.resolve();
  assert.equal(await pending, false); assert.equal(h.source, "original"); assert.equal(h.aborts, 1);
  assert.equal(await f.workspace.openEditableFile(), false);
  assert.equal(await f.workspace.openEditableFile(), false);
  assert.equal(await f.workspace.reloadFile(), false);
  assert.equal(f.byId("document-write").disabled, true);
});
test("unsupported browsers retain import/download without dead native controls", async t => {
  const f = await workspaceFixture(t);
  assert.equal(f.byId("document-open-editable").hidden, true);
  assert.equal(f.byId("document-write").hidden, true);
  assert.equal(await f.workspace.openEditableFile(), false);
  assert.equal(await f.workspace.openFile(new File(["import"], "input.mmd")), true);
  f.edit("download"); assert.equal(await f.workspace.saveSource(), true);
  assert.equal(f.saved[0].text, "download");
});
test("a later copy download cannot override the disk-backed clean-state comparison", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("disk snapshot");
  const paused = gate(); h.onClose = () => paused.promise;
  const pending = f.workspace.saveToFile(); await tick();
  f.edit("download snapshot"); await f.workspace.saveSource(); f.edit("disk snapshot");
  paused.resolve(); assert.equal(await pending, true);
  assert.equal(h.source, "disk snapshot");
  assert.equal(f.byId("document-name").textContent, "diagram.mmd");
  const unload = new Event("beforeunload", { cancelable: true });
  Object.defineProperty(unload, "returnValue", { value: "", writable: true });
  f.host.dispatchEvent(unload); assert.equal(unload.defaultPrevented, false);
});

test("renaming a source document changes share revision without resetting source, identity or baseline", async () => {
  const { SourceDocument } = await loaded;
  const d = new SourceDocument("\uFEFFA\r\n", "old.mmd");
  d.edit("B\n"); const artifact = d.prepareExport(), revision = d.revision;
  assert.equal(d.rename("new.dot"), true);
  assert.equal(d.source, "\uFEFFB\r\n"); assert.equal(d.dirty, true);
  assert.ok(d.revision > revision); assert.equal(d.rename("new.dot"), false);
  assert.equal(d.exported(artifact), true); assert.equal(d.dirty, false);
});
test("Save As creates a file association for an unbound document and refreshes name-bearing consumers", async t => {
  const target = new FileHandleFixture("", "saved.dot"), f = await workspaceFixture(t);
  f.host.showSaveFilePicker = async options => { assert.equal(options.suggestedName, "diagram.mmd"); return target; };
  f.edit("digraph { a -> b }"); const revision = f.workspace.sourceSnapshot().revision;
  f.sourceEl.selectionStart = 2; f.sourceEl.selectionEnd = 6;
  assert.equal(await f.workspace.saveAsFile(), true);
  assert.equal(target.source, "digraph { a -> b }");
  assert.equal(f.workspace.sourceSnapshot().name, "saved.dot");
  assert.ok(f.workspace.sourceSnapshot().revision > revision);
  assert.equal(f.resets, 0); assert.equal(f.changes, 1);
  assert.equal(f.sourceEl.selectionStart, 2); assert.equal(f.sourceEl.selectionEnd, 6);
  f.edit("next saved revision"); assert.equal(await f.workspace.saveToFile(), true);
  assert.equal(target.source, "next saved revision");
});
test("Save As changes the active destination but leaves the old source file untouched", async t => {
  const old = new FileHandleFixture(), target = new FileHandleFixture("", "copy.mmd");
  old.isSameEntry = async () => false;
  const f = await workspaceFixture(t, { handle: old }); await f.workspace.openEditableFile();
  f.edit("copy source"); f.host.showSaveFilePicker = async () => target;
  assert.equal(await f.workspace.saveAsFile(), true);
  assert.equal(old.source, "original"); assert.equal(target.source, "copy source");
  f.edit("copy revision"); assert.equal(await f.workspace.saveToFile(), true);
  assert.equal(old.source, "original"); assert.equal(target.source, "copy revision");
});
test("Save As cancellation preserves the previous filename, dirty state and save destination", async t => {
  const old = new FileHandleFixture(), f = await workspaceFixture(t, { handle: old });
  await f.workspace.openEditableFile(); f.edit("unsaved");
  f.host.showSaveFilePicker = async () => { throw new DOMException("cancel", "AbortError"); };
  assert.equal(await f.workspace.saveAsFile(), false);
  assert.equal(f.workspace.sourceSnapshot().name, "diagram.mmd"); assert.equal(old.creates, 0);
  assert.match(f.byId("document-name").textContent, /unsaved/);
  assert.equal(await f.workspace.saveToFile(), true); assert.equal(old.source, "unsaved");
});
test("a failed Save As never adopts the destination or marks the active draft clean", async t => {
  const old = new FileHandleFixture(), target = new FileHandleFixture("retained", "copy.mmd");
  old.isSameEntry = async () => false;
  target.onClose = () => { throw new Error("disk full"); };
  const f = await workspaceFixture(t, { handle: old }); await f.workspace.openEditableFile();
  f.edit("my source"); f.host.showSaveFilePicker = async () => target;
  assert.equal(await f.workspace.saveAsFile(), false);
  assert.equal(f.workspace.sourceSnapshot().name, "diagram.mmd");
  assert.equal(target.source, "retained"); assert.equal(target.aborts, 1);
  assert.equal(await f.workspace.saveToFile(), true); assert.equal(old.source, "my source");
});
test("Save As cannot bypass an original-file conflict through a different handle for the same file", async t => {
  const h = new FileHandleFixture(), alias = { kind: "file", name: h.name };
  h.isSameEntry = async other => other === alias;
  const f = await workspaceFixture(t, { handle: h }); await f.workspace.openEditableFile();
  h.external("external"); f.edit("local"); f.host.showSaveFilePicker = async () => alias;
  assert.equal(await f.workspace.saveAsFile(), false);
  assert.equal(h.source, "external"); assert.equal(h.creates, 0);
  assert.match(f.byId("document-message").textContent, /changed on disk/);
});
test("a same-handle Save As uses the original conflict baseline too", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); h.external("external"); f.edit("local");
  f.host.showSaveFilePicker = async () => h;
  assert.equal(await f.workspace.saveAsFile(), false); assert.equal(h.source, "external");
});
test("destination identity failures fail closed before creating a writer", async t => {
  const h = new FileHandleFixture(), target = new FileHandleFixture("target", "target.mmd");
  h.isSameEntry = async () => { throw new Error("cannot check identity"); };
  const f = await workspaceFixture(t, { handle: h }); await f.workspace.openEditableFile();
  f.edit("local"); f.host.showSaveFilePicker = async () => target;
  assert.equal(await f.workspace.saveAsFile(), false);
  assert.equal(h.creates, 0); assert.equal(target.creates, 0);
  assert.equal(f.sourceEl.value, "local");
});
test("typing while Save As is choosing a destination saves the requested snapshot, not later edits", async t => {
  const target = new FileHandleFixture("", "new.mmd"), f = await workspaceFixture(t);
  const paused = gate(); f.host.showSaveFilePicker = () => paused.promise;
  f.edit("snapshot"); const pending = f.workspace.saveAsFile(); f.edit("newer");
  paused.resolve(target); assert.equal(await pending, true);
  assert.equal(target.source, "snapshot"); assert.equal(f.sourceEl.value, "newer");
  assert.equal(f.workspace.sourceSnapshot().name, "new.mmd");
  assert.match(f.byId("document-name").textContent, /unsaved/);
  assert.match(f.byId("document-message").textContent, /Newer edits/);
});
test("document replacement while Save As is picking cancels without touching the selected file", async t => {
  const target = new FileHandleFixture("keep", "new.mmd"), f = await workspaceFixture(t);
  const paused = gate(); f.host.showSaveFilePicker = () => paused.promise;
  f.edit("old document"); const pending = f.workspace.saveAsFile();
  await f.workspace.openFile(new File(["replacement"], "replacement.mmd"));
  paused.resolve(target); assert.equal(await pending, false);
  assert.equal(target.creates, 0); assert.equal(f.sourceEl.value, "replacement");
  assert.equal(await f.workspace.saveToFile(), false);
});
test("document replacement after Save As stages bytes aborts the selected file's writer", async t => {
  const target = new FileHandleFixture("keep", "new.mmd"), f = await workspaceFixture(t);
  const paused = gate(); target.onWrite = () => paused.promise; f.host.showSaveFilePicker = async () => target;
  f.edit("old document"); const pending = f.workspace.saveAsFile(); await tick();
  await f.workspace.openFile(new File(["replacement"], "replacement.mmd"));
  paused.resolve(); assert.equal(await pending, false);
  assert.equal(target.source, "keep"); assert.equal(target.aborts, 1);
});
test("concurrent Save As and Save commands cannot race each other's destination", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("snapshot");
  const paused = gate(); let picks = 0;
  f.host.showSaveFilePicker = () => { picks++; return paused.promise; };
  const pending = f.workspace.saveAsFile();
  assert.equal(await f.workspace.saveAsFile(), false);
  assert.equal(await f.workspace.saveToFile(), false);
  assert.equal(picks, 1); paused.resolve(h); assert.equal(await pending, true);
});
test("Save As falls back to source download without native file authority", async t => {
  const f = await workspaceFixture(t); f.edit("fallback");
  assert.equal(f.byId("document-save-as").hidden, true);
  assert.equal(await f.workspace.saveAsFile(), true);
  assert.equal(f.saved[0].text, "fallback"); assert.equal(await f.workspace.saveToFile(), false);
});
function key(target, options = {}) {
  const event = new Event("keydown", { cancelable: true });
  Object.assign(event, { key: "s", ctrlKey: true, ...options });
  target.dispatchEvent(event); return event;
}
test("Save keyboard shortcuts choose native save, Save As or download in the focused workspace", async t => {
  const h = new FileHandleFixture(), f = await workspaceFixture(t, { handle: h });
  await f.workspace.openEditableFile(); f.edit("keyboard save");
  assert.equal(key(f.sourceEl).defaultPrevented, true); await tick();
  assert.equal(h.source, "keyboard save");
  const target = new FileHandleFixture("", "keyboard.mmd"); h.isSameEntry = async () => false;
  f.host.showSaveFilePicker = async () => target;
  f.edit("keyboard copy"); key(f.sourceEl, { shiftKey: true }); await tick();
  assert.equal(target.source, "keyboard copy"); assert.equal(h.source, "keyboard save");
  f.edit("mac save"); key(f.panel, { ctrlKey: false, metaKey: true }); await tick();
  assert.equal(target.source, "mac save");
  const fallback = await workspaceFixture(t); fallback.edit("download"); key(fallback.sourceEl); await tick();
  assert.equal(fallback.saved[0].text, "download");
});
test("keyboard handling respects composition, repeats, other modifiers and unrelated focus", async t => {
  const f = await workspaceFixture(t);
  for (const options of [{ isComposing: true }, { repeat: true }, { altKey: true }, { ctrlKey: false }, { key: "x" }]) {
    assert.equal(key(f.sourceEl, options).defaultPrevented, false);
  }
  const elsewhere = new ElementFixture(f.document); assert.equal(key(elsewhere).defaultPrevented, false);
  await tick(); assert.equal(f.saved.length, 0);
  f.workspace.dispose(); assert.equal(key(f.sourceEl).defaultPrevented, false);
});
test("Save As invalidates older pending opens even when the filename and source stay unchanged", async t => {
  const target = new FileHandleFixture("", "diagram.mmd"), f = await workspaceFixture(t);
  const paused = gate();
  const oldOpen = f.workspace.openFile({ name: "other.mmd", size: 5,
    arrayBuffer: () => paused.promise });
  f.host.showSaveFilePicker = async () => target;
  assert.equal(await f.workspace.saveAsFile(), true);
  paused.resolve(new TextEncoder().encode("other").buffer);
  assert.equal(await oldOpen, false); assert.equal(f.sourceEl.value, "start");
  assert.equal(target.source, "start"); assert.equal(f.resets, 0);
});
test("disposal cancels Save As and prevents late picker completion from reviving controls", async t => {
  const target = new FileHandleFixture("", "new.mmd"), f = await workspaceFixture(t);
  const paused = gate(); f.host.showSaveFilePicker = () => paused.promise;
  const pending = f.workspace.saveAsFile(); f.workspace.dispose(); paused.resolve(target);
  assert.equal(await pending, false); assert.equal(target.creates, 0);
  assert.equal(f.byId("document-save-as").disabled, true);
});
test("preview callback failure cannot turn a successful Save As into a reported save failure", async t => {
  const target = new FileHandleFixture("", "new.mmd"), f = await workspaceFixture(t);
  f.host.showSaveFilePicker = async () => target;
  // Replace the view update with a throwing host boundary by mounting an independent workspace.
  const { mountDocumentWorkspace } = await loaded;
  f.workspace.dispose();
  f.workspace = mountDocumentWorkspace({ sourceEl: f.sourceEl, panelEl: f.panel,
    onChange() { throw new Error("preview unavailable"); }, getStorage: () => f.host.localStorage });
  assert.equal(await f.workspace.saveAsFile(), true); assert.equal(target.source, "start");
  const message = f.panel.children.filter(e => e.id === "document-message").at(-1).textContent;
  assert.match(message, /^Saved new.mmd/); assert.match(message, /Preview update failed/);
});
