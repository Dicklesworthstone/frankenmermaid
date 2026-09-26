"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { gzipSync } = require("node:zlib");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-share.js"))).toString("base64")}`);
const raw = value => `#fm:v1:raw:${Buffer.from(JSON.stringify(value)).toString("base64url")}`;
const gzip = bytes => `#fm:v1:gzip:${gzipSync(bytes).toString("base64url")}`;

test("source links round-trip Unicode, BOM, mixed endings, DOT, deck frontmatter and invalid syntax", async () => {
  const { encodeShareHash, decodeShareHash } = await loaded;
  const sources = ["", "\uFEFF%% 😀\r\nflowchart TD\r\n A[世界]  \nB[B]\r", "digraph { a -> b }",
    "---\ndeck:\n  title: My talk\n---\nflowchart LR\n A --> B\n", "unfinished [ syntax",
    '</script><img src=x onerror="globalThis.executed=true">', "x\u0001\u0002\t\n"];
  for (const source of sources) {
    for (const Compression of [null, globalThis.CompressionStream]) {
      const document = { source, name: "世界 😀.mmd" };
      const hash = await encodeShareHash(document, { Compression });
      assert.deepEqual(await decodeShareHash(hash), document);
      assert.ok(Object.isFrozen(await decodeShareHash(hash)));
      assert.match(hash, /^#fm:v1:(raw|gzip):[A-Za-z0-9_-]+$/);
    }
  }
});
test("only current source and filename are shared, not recovery or export metadata", async () => {
  const { encodeShareHash, decodeShareHash } = await loaded;
  const snapshot = { source: "current", name: "diagram.mmd", baseline: "private earlier source", revision: 42,
    recoveryKey: "private storage key", html: "<script>evil()</script>" };
  const hash = await encodeShareHash(snapshot, { Compression: null });
  assert.deepEqual(await decodeShareHash(hash), { source: "current", name: "diagram.mmd" });
  assert.equal(hash, raw({ source: "current", name: "diagram.mmd" }));
  assert.equal(snapshot.baseline, "private earlier source");
});
test("compression is optional, selected only when useful, and failures fall back to raw", async () => {
  const { encodeShareHash, decodeShareHash } = await loaded;
  const tiny = { source: "", name: "a" };
  assert.match(await encodeShareHash(tiny), /:raw:/);
  const document = { source: "flowchart LR\n A --> B\n".repeat(500), name: "many.mmd" };
  const hash = await encodeShareHash(document);
  assert.match(hash, /:gzip:/);
  assert.deepEqual(await decodeShareHash(hash), document);
  class Unavailable { constructor() { throw new Error("unsupported"); } }
  assert.equal(await encodeShareHash(tiny, { Compression: Unavailable }), raw(tiny));
  await assert.rejects(decodeShareHash(hash, { Decompression: null }), /cannot decompress/);
});
test("encoding captures a snapshot before asynchronous compression", async () => {
  const { encodeShareHash, decodeShareHash } = await loaded;
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  class Paused {
    constructor() {
      return new TransformStream({ async transform(chunk, controller) { await gate; controller.enqueue(chunk); } });
    }
  }
  const document = { source: "original", name: "original.mmd" };
  const pending = encodeShareHash(document, { Compression: Paused });
  document.source = "new"; document.name = "new.mmd";
  release();
  assert.deepEqual(await decodeShareHash(await pending), { source: "original", name: "original.mmd" });
});
test("source budgets count UTF-8 bytes and allow the exact boundary", async () => {
  const { encodeShareHash, decodeShareHash, MAX_SHARE_SOURCE_BYTES } = await loaded;
  const source = "😀".repeat(MAX_SHARE_SOURCE_BYTES / 4);
  const hash = await encodeShareHash({ source, name: "limit.mmd" });
  assert.equal((await decodeShareHash(hash)).source, source);
  await assert.rejects(encodeShareHash({ source: source + "a", name: "limit.mmd" }), /too large/);
  await assert.rejects(encodeShareHash({ source: "x".repeat(MAX_SHARE_SOURCE_BYTES + 1), name: "limit.mmd" }), /too large/);
});
test("large incompressible links fail without silently truncating source", async () => {
  const { encodeShareHash } = await loaded;
  let seed = 123456789, source = "";
  for (let index = 0; index < 100000; index += 1) {
    seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
    source += String.fromCharCode(33 + (seed >>> 0) % 90);
  }
  await assert.rejects(encodeShareHash({ source, name: "noise.txt" }), /too long/);
  await assert.rejects(encodeShareHash({ source: "a".repeat(50000), name: "raw.mmd" }, { Compression: null }), /too long/);
});
test("unknown versions, malformed base64, truncated streams and excessive hashes are rejected", async () => {
  const { decodeShareHash, MAX_SHARE_HASH_CHARS } = await loaded;
  for (const hash of ["#fm:", "#fm:v2:raw:e30", "#fm:v01:raw:e30", "#fm:v1:other:e30", "#fm:v1:raw:",
    "#fm:v1:raw:A", "#fm:v1:raw:e30=", "#fm:v1:raw:e31", "#fm:v1:raw:%41", "#fm:v1:raw:a+b/", "#fm:v1:gzip:AA"]) {
    await assert.rejects(decodeShareHash(hash), undefined, hash);
  }
  await assert.rejects(decodeShareHash("#fm:" + "x".repeat(MAX_SHARE_HASH_CHARS)), /encoded size limit/);
  const valid = gzip(Buffer.from(JSON.stringify({ source: "test", name: "a.mmd" })));
  await assert.rejects(decodeShareHash(valid.slice(0, -8)));
});
test("decompression bombs are bounded while streaming, before JSON allocation", { timeout: 5000 }, async () => {
  const { decodeShareHash, MAX_SHARE_HASH_CHARS, MAX_SHARE_SOURCE_BYTES } = await loaded;
  const bomb = gzip(Buffer.alloc(MAX_SHARE_SOURCE_BYTES * 7, 65));
  assert.ok(bomb.length < MAX_SHARE_HASH_CHARS);
  await assert.rejects(decodeShareHash(bomb), /decoded size limit/);
});
test("invalid UTF-8 and unpaired UTF-16 never silently become replacement characters", async () => {
  const { encodeShareHash, decodeShareHash } = await loaded;
  await assert.rejects(decodeShareHash("#fm:v1:raw:" + Buffer.from([0xc3, 0x28]).toString("base64url")));
  await assert.rejects(decodeShareHash(gzip(Buffer.from([0xc3, 0x28]))));
  for (const source of ["\ud800", "x\udfff", "a\0b"]) {
    await assert.rejects(encodeShareHash({ source, name: "a.mmd" }));
    await assert.rejects(decodeShareHash(raw({ source, name: "a.mmd" })));
  }
});
test("hostile schemas and filenames cannot enter the source workspace", async () => {
  const { decodeShareHash, encodeShareHash } = await loaded;
  const invalid = [null, [], {}, "string", { source: 1, name: "a" }, { source: "a", name: 1 },
    { source: "a", name: "a", baseline: "b" }, JSON.parse('{"source":"a","name":"a","__proto__":{"polluted":true}}')];
  for (const value of invalid) await assert.rejects(decodeShareHash(raw(value)));
  for (const name of ["", ".", "..", "../a.mmd", "a/b", "a\\b", "a\0", " a", "a ", "a".repeat(241), "\ud800.mmd"]) {
    await assert.rejects(encodeShareHash({ source: "a", name }));
    await assert.rejects(decodeShareHash(raw({ source: "a", name })));
  }
  assert.equal({}.polluted, undefined);
});
test("ordinary anchors are untouched, while non-string hashes are rejected", async () => {
  const { decodeShareHash } = await loaded;
  for (const hash of ["", "#out", "#diagram", "#fm-not-a-share"]) assert.equal(await decodeShareHash(hash), null);
  await assert.rejects(decodeShareHash(null), /must be text/);
});
test("generated URLs keep the hosting prefix but drop query strings and credentials", async () => {
  const { shareUrl } = await loaded;
  const hash = raw({ source: "A", name: "a.mmd" });
  assert.equal(shareUrl("https://name:secret@example.test/project/web/playground.html?token=private#old", hash),
    `https://example.test/project/web/playground.html${hash}`);
  for (const base of ["javascript:alert(1)", "data:text/html,a", "file:///tmp/playground.html"]) {
    assert.throws(() => shareUrl(base, hash), /HTTP/);
  }
  assert.throws(() => shareUrl("https://example.test/", "#out"), /Invalid/);
});

const documentLoaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-document.js"))).toString("base64")}`);
class ElementFixture extends EventTarget {
  constructor(document) { super(); this.ownerDocument = document; this.children = []; this.value = ""; this.textContent = ""; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  setAttribute(name, value) { this[name] = String(value); }
  focus() { this.ownerDocument.activeElement = this; }
  select() { this.selectionStart = 0; this.selectionEnd = this.value.length; }
  click() { if (!this.disabled) this.dispatchEvent(new Event("click")); }
}
class StorageFixture {
  values = new Map();
  get length() { return this.values.size; }
  key(index) { return [...this.values.keys()][index] ?? null; }
  getItem(key) { return this.values.get(key) ?? null; }
  setItem(key, value) { this.values.set(key, value); }
}
async function workspaceFixture(t, { source = "start", hash = "", maxBytes } = {}) {
  const { mountDocumentWorkspace } = await documentLoaded;
  const { mountShareControls } = await loaded;
  const host = new EventTarget();
  Object.assign(host, { setTimeout, clearTimeout, crypto: globalThis.crypto, navigator: {},
    location: new URL("https://example.test/project/web/playground.html?secret=not-for-sharing" + hash),
    localStorage: new StorageFixture() });
  const document = new EventTarget();
  document.defaultView = host;
  document.createElement = () => new ElementFixture(document);
  const sourceEl = document.createElement("textarea"), files = document.createElement("section"), panel = document.createElement("section");
  sourceEl.value = source;
  const fixture = { host, document, sourceEl, files, panel, resets: 0, changes: 0, saved: [], approve: () => true };
  fixture.workspace = mountDocumentWorkspace({ sourceEl, panelEl: files, maxBytes,
    confirmReplace: (...args) => fixture.approve(...args), saveFile: artifact => { fixture.saved.push(artifact); },
    onDocumentChange() { fixture.resets++; }, onChange() { fixture.changes++; fixture.sharing?.sourceChanged(); } });
  fixture.sharing = mountShareControls({ panelEl: panel, workspace: fixture.workspace });
  fixture.byId = id => [...files.children, ...panel.children].find(element => element.id === id);
  fixture.edit = text => {
    sourceEl.value = text;
    sourceEl.dispatchEvent(new Event("input"));
    fixture.sharing.sourceChanged();
  };
  t.after(() => { fixture.sharing.dispose(); fixture.workspace.dispose(); });
  await fixture.sharing.ready;
  return fixture;
}
test("workspace sharing exposes the exact current source but not its dirty baseline", async t => {
  const f = await workspaceFixture(t);
  await f.workspace.openSharedSource(async () => ({ source: "\uFEFFA\r\nB\n", name: "世界.mmd" }));
  f.edit("A\nC\n");
  const snapshot = f.workspace.sourceSnapshot();
  assert.equal(snapshot.source, "\uFEFFA\r\nC\n");
  assert.equal(snapshot.name, "世界.mmd");
  assert.ok(Object.isFrozen(snapshot));
  assert.deepEqual(Object.keys(snapshot).sort(), ["name", "revision", "source"]);
  assert.equal(await f.sharing.createLink(), true);
  assert.match(f.byId("document-name").textContent, /unexported/);
  const url = new URL(f.byId("share-url").value);
  assert.equal(url.search, "");
  assert.equal(f.host.location.hash, "", "creating a link does not rewrite browser history");
  const { decodeShareHash } = await loaded;
  assert.deepEqual(await decodeShareHash(url.hash), { source: snapshot.source, name: snapshot.name });
  assert.equal(f.saved.length, 0);
});
test("initial shared links enter the normal document reset/render/recovery boundary", async t => {
  const source = "\uFEFFdigraph{a->b}\r\n";
  const f = await workspaceFixture(t, { hash: raw({ source, name: "graph.dot" }) });
  assert.equal(f.workspace.sourceSnapshot().source, source);
  assert.equal(f.sourceEl.value, "digraph{a->b}\n");
  assert.equal(f.resets, 1); assert.equal(f.changes, 1);
  assert.match(f.byId("share-status").textContent, /Shared source opened/);
  assert.equal(await f.workspace.saveSource(), true);
  assert.equal(f.saved[0].text, source);
  assert.equal(f.saved[0].filename, "graph.dot");
});
test("corrupt initial links and ordinary anchors never replace the current source", async t => {
  for (const hash of ["#fm:v3:raw:e30", "#fm:v1:raw:!!!!", "#out"]) {
    const f = await workspaceFixture(t, { hash });
    assert.equal(f.workspace.sourceSnapshot().source, "start");
    assert.equal(f.resets, 0);
    assert.match(f.byId("share-status").textContent, hash === "#out" ? /Links contain/ : /retained/);
  }
});
test("edits invalidate generated links, including programmatic edits without input events", async t => {
  const f = await workspaceFixture(t);
  assert.equal(await f.sharing.createLink(), true);
  f.edit("changed");
  assert.equal(f.byId("share-copy").disabled, true);
  assert.equal(f.byId("share-url").value, "");
  assert.equal(await f.sharing.createLink(), true);
  f.sourceEl.value = "programmatic change";
  assert.equal(await f.sharing.copyLink(), false);
  assert.equal(f.byId("share-url").hidden, true);
});
test("a source edit during asynchronous link generation discards the stale result", async t => {
  const f = await workspaceFixture(t);
  const original = globalThis.CompressionStream;
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  globalThis.CompressionStream = class {
    constructor() { return new TransformStream({ async transform(chunk, controller) { await gate; controller.enqueue(chunk); } }); }
  };
  t.after(() => { globalThis.CompressionStream = original; });
  const pending = f.sharing.createLink();
  f.edit("edited before encode completed");
  release();
  assert.equal(await pending, false);
  assert.equal(f.byId("share-url").value, "");
  assert.equal(f.byId("share-create").disabled, false);
});
test("clipboard success and denial both preserve source and its export baseline", async t => {
  const f = await workspaceFixture(t);
  f.edit("unexported");
  await f.sharing.createLink();
  let copied;
  f.host.navigator.clipboard = { async writeText(text) { copied = text; } };
  assert.equal(await f.sharing.copyLink(), true);
  assert.equal(copied, f.byId("share-url").value);
  f.host.navigator.clipboard = { async writeText() { throw new Error("denied"); } };
  assert.equal(await f.sharing.copyLink(), false);
  assert.equal(f.document.activeElement, f.byId("share-url"));
  assert.equal(f.byId("share-url").selectionEnd, copied.length);
  assert.match(f.byId("share-status").textContent, /copy it manually/);
  assert.match(f.byId("document-name").textContent, /unexported/);
});
test("dirty replacement confirmation can be declined without losing either current source or recovery", async t => {
  const f = await workspaceFixture(t);
  f.edit("keep this");
  f.workspace.flushRecovery();
  const records = [...f.host.localStorage.values];
  f.approve = () => false;
  const opened = await f.workspace.openSharedSource(async () => ({ source: "incoming", name: "new.mmd" }));
  assert.equal(opened, false);
  assert.equal(f.workspace.sourceSnapshot().source, "keep this");
  assert.deepEqual([...f.host.localStorage.values], records);
  assert.equal(f.resets, 0);
});
test("source edits while a shared document is decoding supersede the pending open", async t => {
  const f = await workspaceFixture(t);
  let release;
  const incoming = new Promise(resolve => { release = resolve; });
  const pending = f.workspace.openSharedSource(() => incoming);
  f.edit("typed during read");
  release({ source: "obsolete", name: "old.mmd" });
  assert.equal(await pending, false);
  assert.equal(f.workspace.sourceSnapshot().source, "typed during read");
  assert.equal(f.resets, 0);
});
test("navigation guards are rechecked after an asynchronous unsaved-change confirmation", async t => {
  const f = await workspaceFixture(t);
  f.edit("unsaved");
  let approve, current = true;
  f.approve = () => new Promise(resolve => { approve = resolve; });
  const pending = f.workspace.openSharedSource(async () => ({ source: "incoming", name: "new.mmd" }), { isCurrent: () => current });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(typeof approve, "function");
  current = false;
  approve(true);
  assert.equal(await pending, false);
  assert.equal(f.workspace.sourceSnapshot().source, "unsaved");
  assert.match(f.byId("document-message").textContent, /no longer current/);
});
test("successful replacement forks recovery and newer opens supersede older reads", async t => {
  const f = await workspaceFixture(t);
  f.edit("outgoing unsaved");
  let release;
  const old = f.workspace.openSharedSource(() => new Promise(resolve => { release = resolve; }));
  assert.equal(await f.workspace.openSharedSource(async () => ({ source: "newest", name: "newest.mmd" })), true);
  release({ source: "obsolete", name: "old.mmd" });
  assert.equal(await old, false);
  assert.equal(f.workspace.sourceSnapshot().source, "newest");
  const records = [...f.host.localStorage.values.values()].map(JSON.parse);
  assert.ok(records.some(record => record.source === "outgoing unsaved"));
  assert.ok(records.some(record => record.source === "newest"));
  assert.equal(f.resets, 1); assert.equal(f.changes, 1);
});
test("public shared-source imports still enforce workspace text and size limits", async t => {
  const f = await workspaceFixture(t, { maxBytes: 8 });
  for (const incoming of [null, { source: "bad\0", name: "a.mmd" }, { source: "\ud800", name: "a.mmd" },
    { source: "😀😀a", name: "a.mmd" }, { source: "a", name: null }]) {
    assert.equal(await f.workspace.openSharedSource(async () => incoming), false);
    assert.equal(f.workspace.sourceSnapshot().source, "start");
  }
  assert.equal(f.resets, 0);
});
test("disposal cancels pending imports and prevents zombie controls from reading a closed workspace", async t => {
  const f = await workspaceFixture(t);
  let release;
  const pending = f.workspace.openSharedSource(() => new Promise(resolve => { release = resolve; }));
  f.sharing.dispose(); f.workspace.dispose();
  release({ source: "late", name: "late.mmd" });
  assert.equal(await pending, false);
  assert.equal(await f.sharing.createLink(), false);
  assert.equal(await f.sharing.copyLink(), false);
  assert.equal(f.byId("share-create").disabled, true);
  assert.equal(f.byId("share-copy").disabled, true);
  assert.throws(() => f.workspace.sourceSnapshot(), /closed/);
  assert.equal(f.resets, 0);
});
