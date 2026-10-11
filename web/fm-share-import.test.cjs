"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { deflateSync } = require("node:zlib");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-share.js"))).toString("base64")}`);
const live = code => `#pako:${deflateSync(Buffer.from(JSON.stringify({ code, mermaid: { securityLevel: "loose" } }))).toString("base64url")}`;
const tick = () => new Promise(resolve => setImmediate(resolve));
function gate() { let release; const promise = new Promise(resolve => { release = resolve; }); return { promise, release }; }

test("pasted full URLs and fragments import without visiting their hosts", async () => {
  const { decodeShareLink, encodeShareHash } = await loaded;
  const document = { source: "世界😀\r\n", name: "local.mmd" };
  for (const hash of [live(document.source), await encodeShareHash(document, { Compression: null })]) {
    for (const input of [hash, `https://mermaid.live/edit${hash}`, `http://example.invalid/self-hosted/view?token=unused${hash}`, `  \nhttps://example.invalid/${hash}\r\n`]) {
      assert.equal((await decodeShareLink(input)).source, document.source);
    }
  }
  assert.deepEqual(await decodeShareLink(await encodeShareHash(document)), document);
});

test("unsupported schemes, credentials, missing fragments and internal whitespace fail explicitly", async () => {
  const { decodeShareLink } = await loaded;
  const hash = live("A");
  for (const input of [null, 7, "", " ", "#out", "/edit" + hash, "//mermaid.live/edit" + hash,
    "javascript:alert(1)" + hash, "data:text/plain,a" + hash, "file:///tmp/a" + hash,
    "https://user:secret@example.test/" + hash, "https://example.test/?code=A", "https://example.test/#out",
    "https://exam\nple.test/" + hash, hash + " more", hash.slice(0, 10) + "\t" + hash.slice(10)]) {
    await assert.rejects(decodeShareLink(input), undefined, String(input));
  }
  await assert.rejects(decodeShareLink("https://user:super-private@example.test/" + hash), error => !error.message.includes("super-private"));
});

test("pasted input is bounded before URL parsing and fragment decoding", async () => {
  const { decodeShareLink, MAX_SHARE_HASH_CHARS } = await loaded;
  await assert.rejects(decodeShareLink(" ".repeat(MAX_SHARE_HASH_CHARS + 4097)), /input size limit/);
  await assert.rejects(decodeShareLink("https://example.test/#pako:" + "a".repeat(MAX_SHARE_HASH_CHARS)), /encoded size limit/);
});

class ElementFixture extends EventTarget {
  constructor(document) { super(); this.ownerDocument = document; this.children = []; this.value = ""; this.textContent = ""; }
  append(...children) { this.children.push(...children); }
  setAttribute(name, value) { this[name] = String(value); }
}
async function fixture(t, { hash = "", beforePublish = async () => {}, approve = () => true } = {}) {
  const { mountShareControls } = await loaded;
  const host = new EventTarget();
  host.location = new URL(`https://example.test/playground.html${hash}`);
  const document = { defaultView: host, createElement: () => new ElementFixture(document) };
  const panelEl = new ElementFixture(document);
  let source = "local edits", revision = 1, calls = 0;
  const workspace = {
    sourceSnapshot: () => ({ source, revision, name: "local.mmd" }),
    async openSharedSource(decode, { isCurrent }) {
      calls += 1;
      const captured = revision;
      const incoming = await decode();
      await beforePublish(incoming.source);
      if (!isCurrent() || captured !== revision || !approve()) return false;
      source = incoming.source;
      revision += 1;
      return true;
    },
  };
  const controls = mountShareControls({ panelEl, workspace });
  t.after(() => controls.dispose());
  const element = id => panelEl.children.find(value => value.id === id);
  return { controls, host, element, workspace, calls: () => calls,
    editSource(value) { source = value; revision += 1; controls.sourceChanged(); },
    input(value) {
      element("share-import-url").value = value;
      element("share-import-url").dispatchEvent(new Event("input"));
    },
  };
}

test("the importer retains history, imports through the workspace and reports source-only behavior", async t => {
  const f = await fixture(t);
  await f.controls.ready;
  const href = f.host.location.href;
  assert.equal(f.element("share-import").disabled, true);
  f.input(`https://mermaid.live/edit${live("new source")}`);
  assert.equal(f.element("share-import").disabled, false);
  assert.equal(await f.controls.importLink(), true);
  assert.equal(f.workspace.sourceSnapshot().source, "new source");
  assert.equal(f.host.location.href, href);
  assert.match(f.element("share-import-status").textContent, /settings were not applied/);
  assert.equal(f.element("share-import-label").htmlFor, "share-import-url");
  assert.equal(f.element("share-import-url")["aria-describedby"], "share-import-status");
});

test("declined replacement and malformed links preserve the current document", async t => {
  const f = await fixture(t, { approve: () => false });
  await f.controls.ready;
  for (const input of [live("discarded"), "#pako:bad"]) {
    f.input(input);
    assert.equal(await f.controls.importLink(), false);
    assert.equal(f.workspace.sourceSnapshot().source, "local edits");
    assert.match(f.element("share-import-status").textContent, /retained/);
    assert.equal(f.element("share-import").disabled, false);
  }
});

test("editing the link, including A-B-A, revokes an in-flight import", async t => {
  const paused = gate(), entered = gate();
  const f = await fixture(t, { beforePublish: async () => { entered.release(); await paused.promise; } });
  await f.controls.ready;
  const original = live("stale");
  f.input(original);
  const pending = f.controls.importLink();
  await entered.promise;
  f.input(live("intermediate"));
  f.input(original);
  paused.release();
  assert.equal(await pending, false);
  assert.equal(f.workspace.sourceSnapshot().source, "local edits");
  assert.match(f.element("share-import-status").textContent, /Link changed/);
});

test("a newer pasted import wins over an older paused import", async t => {
  const paused = gate(), entered = gate();
  const f = await fixture(t, { beforePublish: async source => {
    if (source === "old") { entered.release(); await paused.promise; }
  } });
  await f.controls.ready;
  f.input(live("old"));
  const old = f.controls.importLink();
  await entered.promise;
  assert.equal(await f.controls.importLink(), false); // Duplicate submission coalesces.
  f.input(live("new"));
  assert.equal(await f.controls.importLink(), true);
  paused.release();
  assert.equal(await old, false);
  assert.equal(f.workspace.sourceSnapshot().source, "new");
  assert.match(f.element("share-import-status").textContent, /source imported/);
});

test("a pasted import supersedes an older address-bar import", async t => {
  const paused = gate(), entered = gate();
  const f = await fixture(t, { hash: live("address"), beforePublish: async source => {
    if (source === "address") { entered.release(); await paused.promise; }
  } });
  await entered.promise;
  f.input(live("pasted"));
  assert.equal(await f.controls.importLink(), true);
  paused.release();
  assert.equal(await f.controls.ready, false);
  assert.equal(f.workspace.sourceSnapshot().source, "pasted");
  assert.equal(f.element("share-open").disabled, false);
  assert.match(f.element("share-navigation-status").textContent, /previous shared source/);
});

test("navigation, disposal and source edits cannot publish a stale pasted document", async t => {
  for (const cancel of [f => {
    f.host.location.hash = "#other";
    f.host.dispatchEvent(new Event("hashchange"));
  }, f => f.controls.dispose(), f => f.editSource("new typing")]) {
    const paused = gate(), entered = gate();
    const f = await fixture(t, { beforePublish: async () => { entered.release(); await paused.promise; } });
    await f.controls.ready;
    f.input(live("stale"));
    const pending = f.controls.importLink();
    await entered.promise;
    cancel(f);
    paused.release();
    assert.equal(await pending, false);
    assert.notEqual(f.workspace.sourceSnapshot().source, "stale");
  }
});

test("duplicate navigation notifications do not cancel a valid pasted import", async t => {
  const paused = gate(), entered = gate();
  const f = await fixture(t, { beforePublish: async () => { entered.release(); await paused.promise; } });
  await f.controls.ready;
  f.input(live("kept"));
  const pending = f.controls.importLink();
  await entered.promise;
  f.host.dispatchEvent(new Event("hashchange"));
  f.host.dispatchEvent(new Event("popstate"));
  paused.release();
  assert.equal(await pending, true);
  assert.equal(f.workspace.sourceSnapshot().source, "kept");
});

test("queued A-B-A navigation cancels a pending pasted import", async t => {
  const paused = gate(), entered = gate();
  const f = await fixture(t, { beforePublish: async () => { entered.release(); await paused.promise; } });
  await f.controls.ready;
  f.input(live("stale"));
  const pending = f.controls.importLink();
  await entered.promise;
  const obsolete = new Event("hashchange");
  obsolete.newURL = "https://example.test/playground.html#intervening";
  f.host.dispatchEvent(obsolete);
  paused.release();
  assert.equal(await pending, false);
  assert.equal(f.workspace.sourceSnapshot().source, "local edits");
});

test("plain Enter imports, composing/modifier/repeat events do not, and disposal detaches controls", async t => {
  const f = await fixture(t);
  await f.controls.ready;
  f.input(live("keyboard"));
  const input = f.element("share-import-url");
  for (const flags of [{ isComposing: true }, { repeat: true }, { ctrlKey: true }, { metaKey: true }, { altKey: true }, { shiftKey: true }]) {
    const event = new Event("keydown", { cancelable: true });
    Object.assign(event, { key: "Enter", ...flags });
    input.dispatchEvent(event);
    assert.equal(event.defaultPrevented, false);
  }
  assert.equal(f.calls(), 0);
  const event = new Event("keydown", { cancelable: true });
  event.key = "Enter";
  input.dispatchEvent(event);
  assert.equal(event.defaultPrevented, true);
  for (let attempt = 0; attempt < 100 && f.workspace.sourceSnapshot().source !== "keyboard"; attempt += 1) await tick();
  assert.equal(f.workspace.sourceSnapshot().source, "keyboard");
  f.controls.dispose();
  assert.equal(input.disabled, true);
  assert.equal(f.element("share-import").disabled, true);
  assert.equal(await f.controls.importLink(), false);
  input.dispatchEvent(new Event("input"));
  assert.equal(f.element("share-import").disabled, true);
});
