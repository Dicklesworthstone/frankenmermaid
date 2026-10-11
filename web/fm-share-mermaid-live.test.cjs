"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { deflateSync, deflateRawSync, gzipSync } = require("node:zlib");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-share.js"))).toString("base64")}`);
// Independent producer: Mermaid Live's serde.ts uses UTF-8 JSON, zlib-wrapped
// DEFLATE, and unpadded base64url (or plain UTF-8 base64 for its legacy format).
const live = (state, format = "pako", encoding = "base64url") => {
  const bytes = Buffer.from(JSON.stringify(state));
  return `#${format}:${(format === "pako" ? deflateSync(bytes) : bytes).toString(encoding)}`;
};

for (const format of ["pako", "base64"]) {
  test(`${format} imports exact source, including empty documents and Unicode`, async () => {
    const { decodeShareHash } = await loaded;
    for (const code of ["", "flowchart LR\n A --> B\n", "\uFEFF%% 😀 世界\r\nA-->B\r", "unfinished [", "x\u0001\t"] ) {
      const result = await decodeShareHash(live({ code }, format));
      assert.deepEqual(result, { source: code, name: "diagram.mmd" });
      assert.ok(Object.isFrozen(result));
    }
  });
  test(`${format} drops foreign config, filenames, prototype keys and external resources`, async () => {
    const { decodeShareHash } = await loaded;
    const state = JSON.parse('{"code":"flowchart LR\\nA-->B","mermaid":{"securityLevel":"loose"},"name":"../private","source":"wrong","baseline":"secret","__proto__":{"polluted":true},"url":"https://example.invalid/private","editorMode":"config"}');
    assert.deepEqual(await decodeShareHash(live(state, format)), { source: state.code, name: "diagram.mmd" });
    assert.equal({}.polluted, undefined);
  });
  test(`${format} accepts canonical padded standard base64 from other producers`, async () => {
    const { decodeShareHash } = await loaded;
    for (const code of ["a", "ab", "abc", "世界😀", "\uFFFF\uFFFE"]) {
      assert.equal((await decodeShareHash(live({ code }, format, "base64"))).source, code);
    }
  });
  test(`${format} requires a JSON object with string code`, async () => {
    const { decodeShareHash } = await loaded;
    for (const state of [null, [], "text", 7, {}, { code: null }, { code: [] }, { code: 1 }, { source: "not-code" }]) {
      await assert.rejects(decodeShareHash(live(state, format)), /expected code text/);
    }
    for (const code of ["\ud800", "\udfff", "a\0b"]) {
      await assert.rejects(decodeShareHash(live({ code }, format)));
    }
  });
}

test("pako requires zlib-wrapped DEFLATE, not raw DEFLATE or gzip", async () => {
  const { decodeShareHash } = await loaded;
  const bytes = Buffer.from(JSON.stringify({ code: "flowchart TD\nA-->B" }));
  for (const compress of [deflateRawSync, gzipSync]) {
    await assert.rejects(decodeShareHash(`#pako:${compress(bytes).toString("base64url")}`));
  }
  const good = live({ code: "valid" });
  await assert.rejects(decodeShareHash(good.slice(0, -8)));
  const corrupted = deflateSync(bytes);
  corrupted[corrupted.length - 1] ^= 1;
  await assert.rejects(decodeShareHash(`#pako:${corrupted.toString("base64url")}`));
});

test("invalid UTF-8 and malformed JSON are rejected without replacement characters", async () => {
  const { decodeShareHash } = await loaded;
  for (const bytes of [Buffer.from([0xc3, 0x28]), Buffer.from('{"code":'), Buffer.from("null")]) {
    await assert.rejects(decodeShareHash(`#base64:${bytes.toString("base64url")}`));
    await assert.rejects(decodeShareHash(`#pako:${deflateSync(bytes).toString("base64url")}`));
  }
});

test("malformed encodings do not become silently ignored page anchors", async () => {
  const { decodeShareHash } = await loaded;
  for (const prefix of ["#pako:", "#base64:"]) {
    for (const suffix of ["", "A", "e31", "e30==", "e30===", "e30=\n", " e30", "%41", "e30:extra", "e=30"]) {
      await assert.rejects(decodeShareHash(prefix + suffix), undefined, prefix + suffix);
    }
  }
  for (const hash of ["", "#out", "#pako-not-source", "#base64", "#e30"]) {
    assert.equal(await decodeShareHash(hash), null);
  }
});

test("missing compression support leaves legacy base64 import usable", async () => {
  const { decodeShareHash } = await loaded;
  await assert.rejects(decodeShareHash(live({ code: "A" }), { Decompression: null }), /deflate stream support/);
  assert.equal((await decodeShareHash(live({ code: "A" }, "base64"), { Decompression: null })).source, "A");
});

test("native link encoding and strict validation remain unchanged", async () => {
  const { encodeShareHash, decodeShareHash } = await loaded;
  const document = { source: "\uFEFF世界\r\n", name: "mine.mmd" };
  for (const Compression of [null, globalThis.CompressionStream]) {
    assert.deepEqual(await decodeShareHash(await encodeShareHash(document, { Compression })), document);
  }
  for (const hash of ["#fm:", "#fm:v2:raw:e30", "#fm:v1:raw:e30=", "#fm:v1:raw:e31", "#fm:v1:gzip:AA"]) {
    await assert.rejects(decodeShareHash(hash));
  }
});

test("encoded and UTF-8 source budgets apply to imported links", async () => {
  const { decodeShareHash, MAX_SHARE_HASH_CHARS, MAX_SHARE_SOURCE_BYTES } = await loaded;
  for (const prefix of ["#pako:", "#base64:"]) {
    await assert.rejects(decodeShareHash(prefix + "a".repeat(MAX_SHARE_HASH_CHARS)), /encoded size limit/);
  }
  const code = "😀".repeat(MAX_SHARE_SOURCE_BYTES / 4);
  assert.equal((await decodeShareHash(live({ code }))).source, code);
  await assert.rejects(decodeShareHash(live({ code: code + "a" })), /too large/);
});

test("DEFLATE bombs and oversized foreign settings are bounded before JSON parsing", { timeout: 5000 }, async () => {
  const { decodeShareHash, MAX_SHARE_SOURCE_BYTES, MAX_SHARE_HASH_CHARS } = await loaded;
  const hash = live({ code: "A", mermaid: "x".repeat(MAX_SHARE_SOURCE_BYTES * 7) });
  assert.ok(hash.length < MAX_SHARE_HASH_CHARS);
  await assert.rejects(decodeShareHash(hash), /decoded size limit/);
});

class ElementFixture extends EventTarget {
  constructor(document) { super(); this.ownerDocument = document; this.children = []; this.value = ""; this.textContent = ""; }
  append(...children) { this.children.push(...children); }
  setAttribute(name, value) { this[name] = String(value); }
}
async function controlsFixture(t, hash, beforePublish = async () => {}) {
  const { mountShareControls } = await loaded;
  const host = new EventTarget();
  host.location = new URL(`https://example.test/playground.html${hash}`);
  const document = { defaultView: host, createElement: () => new ElementFixture(document) };
  const panelEl = new ElementFixture(document);
  let revision = 0, source = "original", requests = 0, publications = 0;
  const workspace = {
    sourceSnapshot: () => ({ revision, source, name: "original.mmd" }),
    async openSharedSource(decode, { isCurrent }) {
      requests += 1;
      const captured = revision;
      const result = await decode();
      await beforePublish();
      if (!isCurrent() || revision !== captured) return false;
      source = result.source;
      revision += 1;
      publications += 1;
      return true;
    },
  };
  const controls = mountShareControls({ panelEl, workspace });
  t.after(() => controls.dispose());
  return { controls, host, workspace, element: id => panelEl.children.find(element => element.id === id),
    counts: () => ({ requests, publications }) };
}

for (const format of ["pako", "base64"]) {
  test(`${format} opens through normal workspace transactions and explains discarded settings`, async t => {
    const f = await controlsFixture(t, live({ code: "imported" }, format));
    assert.equal(await f.controls.ready, true);
    assert.equal(f.workspace.sourceSnapshot().source, "imported");
    assert.match(f.element("share-navigation-status").textContent, /settings were not applied/);
    f.host.dispatchEvent(new Event("hashchange"));
    f.host.dispatchEvent(new Event("popstate"));
    assert.deepEqual(f.counts(), { requests: 1, publications: 1 });
    f.host.location.hash = "#ordinary";
    f.host.dispatchEvent(new Event("hashchange"));
    assert.equal(f.workspace.sourceSnapshot().source, "imported");
    assert.match(f.element("share-navigation-status").textContent, /cleared.*retained/);
  });
}

test("navigation and disposal prevent a decoded Mermaid Live document from publishing late", async t => {
  for (const stop of [f => f.controls.dispose(), f => {
    f.host.location.hash = "#ordinary";
    f.host.dispatchEvent(new Event("hashchange"));
  }]) {
    let release, entered;
    const gate = new Promise(resolve => { release = resolve; });
    const decoding = new Promise(resolve => { entered = resolve; });
    const f = await controlsFixture(t, live({ code: "stale" }), async () => { entered(); await gate; });
    await decoding;
    stop(f);
    release();
    assert.equal(await f.controls.ready, false);
    assert.equal(f.workspace.sourceSnapshot().source, "original");
    assert.equal(f.counts().publications, 0);
  }
});
