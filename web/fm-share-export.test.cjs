"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { inflateSync } = require("node:zlib");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-share.js"))).toString("base64")}`);
function stateFromHash(hash) {
  const match = /^#(pako|base64):([A-Za-z0-9_-]+)$/.exec(hash);
  assert.ok(match, "upstream format and URL-safe alphabet");
  const bytes = Buffer.from(match[2], "base64url");
  return JSON.parse((match[1] === "pako" ? inflateSync(bytes) : bytes).toString("utf8"));
}
function gate() { let release; const promise = new Promise(resolve => { release = resolve; }); return { promise, release }; }

test("both export codecs preserve exact source using an independent upstream-format decoder", async () => {
  const { encodeMermaidLiveHash, decodeShareHash } = await loaded;
  for (const source of ["", "\uFEFF%% 😀 世界\r\nA-->B\r\n", "unfinished [", "<script>not executed</script>", "x\u0001\t"]) {
    for (const Compression of [null, globalThis.CompressionStream]) {
      const hash = await encodeMermaidLiveHash(source, { Compression });
      assert.deepEqual(stateFromHash(hash), { code: source, mermaid: "{}", updateDiagram: true, rough: false, editorMode: "code" });
      assert.equal((await decodeShareHash(hash)).source, source);
    }
  }
});

test("compression uses zlib-wrapped DEFLATE, with legacy fallback on unavailable or failed codecs", async () => {
  const { encodeMermaidLiveHash } = await loaded;
  const source = "flowchart LR\nA-->B\n".repeat(100);
  const hash = await encodeMermaidLiveHash(source);
  assert.match(hash, /^#pako:/);
  assert.equal(stateFromHash(hash).code, source);
  class Broken { constructor() { throw new Error("No codec"); } }
  class FailedStream { constructor() { return new TransformStream({ transform() { throw new Error("Stream failed"); } }); } }
  for (const Compression of [null, Broken, FailedStream]) {
    const fallback = await encodeMermaidLiveHash(source, { Compression });
    assert.match(fallback, /^#base64:/);
    assert.equal(stateFromHash(fallback).code, source);
  }
});

test("export accepts text only and rejects invalid Unicode, NUL and excessive source", async () => {
  const { encodeMermaidLiveHash, MAX_SHARE_SOURCE_BYTES } = await loaded;
  for (const source of [null, 1, {}, { source: "a", baseline: "secret" }, "\ud800", "\udfff", "a\0b", "a".repeat(MAX_SHARE_SOURCE_BYTES + 1)]) {
    await assert.rejects(encodeMermaidLiveHash(source));
  }
  const boundary = "😀".repeat(MAX_SHARE_SOURCE_BYTES / 4);
  assert.equal(stateFromHash(await encodeMermaidLiveHash(boundary)).code, boundary);
  await assert.rejects(encodeMermaidLiveHash(boundary + "a"), /too large/);
  await assert.rejects(encodeMermaidLiveHash("a".repeat(50000), { Compression: null }), /too long/);
});

class ElementFixture extends EventTarget {
  constructor(document) { super(); this.ownerDocument = document; this.children = []; this.value = ""; this.textContent = ""; }
  append(...children) { this.children.push(...children); }
  setAttribute(name, value) { this[name] = String(value); }
}
async function fixture(t) {
  const { mountShareControls } = await loaded;
  const host = new EventTarget();
  host.location = new URL("https://user:secret@example.test/project/playground.html?token=private#out");
  const copied = [];
  host.navigator = { clipboard: { async writeText(text) { copied.push(text); } } };
  const document = { defaultView: host, createElement: () => new ElementFixture(document) };
  const panelEl = new ElementFixture(document);
  let source = "flowchart LR\nA-->B", revision = 0;
  const controls = mountShareControls({ panelEl, workspace: {
    sourceSnapshot: () => ({ source, revision, name: "private-filename.mmd", baseline: "private baseline", config: { securityLevel: "loose" } }),
    async openSharedSource() { throw new Error("Export must not replace the document"); },
  } });
  t.after(() => controls.dispose());
  await controls.ready;
  const element = id => panelEl.children.find(value => value.id === id);
  return { controls, host, element, copied,
    destination(value) { element("share-format").value = value; element("share-format").dispatchEvent(new Event("change")); },
    edit(value) { source = value; revision += 1; controls.sourceChanged(); },
  };
}

test("native sharing remains default and preserves the existing filename contract", async t => {
  const { decodeShareHash } = await loaded;
  const f = await fixture(t);
  assert.equal(f.element("share-format").value, "frankenmermaid");
  assert.equal(await f.controls.createLink(), true);
  const url = new URL(f.element("share-url").value);
  assert.equal(url.origin + url.pathname, "https://example.test/project/playground.html");
  assert.equal(url.search + url.username + url.password, "");
  assert.deepEqual(await decodeShareHash(url.hash), { source: "flowchart LR\nA-->B", name: "private-filename.mmd" });
});

test("opt-in Live sharing excludes private metadata and never navigates or replaces the document", async t => {
  const f = await fixture(t), originalUrl = f.host.location.href;
  f.destination("mermaid-live");
  assert.match(f.element("share-format-notice").textContent, /external.*Only source/);
  assert.equal(await f.controls.createLink(), true);
  const url = new URL(f.element("share-url").value);
  assert.equal(url.origin + url.pathname, "https://mermaid.live/edit");
  assert.deepEqual(stateFromHash(url.hash), { code: "flowchart LR\nA-->B", mermaid: "{}", updateDiagram: true, rough: false, editorMode: "code" });
  assert.match(f.element("share-status").textContent, /FrankenMermaid-only syntax may not render/);
  assert.equal(f.host.location.href, originalUrl);
  assert.equal(await f.controls.copyLink(), true);
  assert.deepEqual(f.copied, [url.href]);
});

test("changing destination clears previously generated URLs and disables stale copying", async t => {
  const f = await fixture(t);
  assert.equal(await f.controls.createLink(), true);
  f.destination("mermaid-live");
  assert.equal(f.element("share-url").value, "");
  assert.equal(f.element("share-copy").disabled, true);
  assert.equal(await f.controls.copyLink(), false);
  assert.equal(await f.controls.createLink(), true);
  f.destination("frankenmermaid");
  assert.equal(f.element("share-url").value, "");
  assert.equal(await f.controls.createLink(), true);
  assert.match(f.element("share-url").value, /#fm:/);
});

async function pauseCompression(t) {
  const paused = gate(), entered = gate();
  const original = globalThis.CompressionStream;
  t.after(() => { globalThis.CompressionStream = original; paused.release(); });
  globalThis.CompressionStream = class {
    constructor() {
      return new TransformStream({ async transform(chunk, controller) {
        entered.release(); await paused.promise; controller.enqueue(chunk);
      } });
    }
  };
  return { paused, entered };
}

test("destination A-B-A changes revoke pending compression rather than publishing the old link", async t => {
  const f = await fixture(t);
  const { paused, entered } = await pauseCompression(t);
  const pending = f.controls.createLink();
  await entered.promise;
  f.destination("mermaid-live");
  f.destination("frankenmermaid");
  paused.release();
  assert.equal(await pending, false);
  assert.equal(f.element("share-url").value, "");
  assert.equal(f.element("share-copy").disabled, true);
  assert.equal(f.element("share-create").disabled, false);
});

test("a source edit during Live compression cannot publish a stale source snapshot", async t => {
  const f = await fixture(t);
  f.destination("mermaid-live");
  const { paused, entered } = await pauseCompression(t);
  const pending = f.controls.createLink();
  await entered.promise;
  f.edit("new typing");
  paused.release();
  assert.equal(await pending, false);
  assert.equal(f.element("share-url").value, "");
  assert.match(f.element("share-status").textContent, /Source changed/);
});

test("disposal revokes pending export and disables destination controls", async t => {
  const f = await fixture(t);
  f.destination("mermaid-live");
  const { paused, entered } = await pauseCompression(t);
  const pending = f.controls.createLink();
  await entered.promise;
  f.controls.dispose();
  paused.release();
  assert.equal(await pending, false);
  assert.equal(f.element("share-format").disabled, true);
  assert.equal(f.element("share-url").value, "");
  f.destination("frankenmermaid");
  assert.equal(f.element("share-create").disabled, true);
});
