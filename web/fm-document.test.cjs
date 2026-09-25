"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-document.js"))).toString("base64")}`);
const file = (bytes, name = "flow.mmd") => new File([bytes], name);

test("UTF-8 imports round-trip BOM, astral Unicode, mixed line endings and trailing whitespace", async () => {
  const { SourceDocument, readSourceFile } = await loaded;
  for (const source of ["\uFEFF%% 😀\r\nflowchart TD\r\n a[世界]  \r\nb[B]\n", "", "digraph{a->b}", "a\rb\r"]) {
    const imported = await readSourceFile(file(source));
    assert.equal(imported.source, source);
    const doc = new SourceDocument(imported.source, imported.name);
    doc.edit(doc.view);
    assert.equal(doc.prepareExport().text, source);
    assert.equal(doc.dirty, false);
    assert.ok(!doc.view.includes("\r"));
  }
});
test("opening supports Mermaid and DOT extensions, never HTML/SVG/image import", async () => {
  const { readSourceFile } = await loaded;
  for (const name of ["architecture.MMD", "flow.mermaid", "graph.dot", "graph.GV", "input.txt"]) {
    assert.equal((await readSourceFile(file("", name))).name, name);
  }
  for (const name of ["page.html", "diagram.svg", "photo.png", "x.mmd.exe"]) {
    await assert.rejects(readSourceFile(file("a[A]", name)), /source file/);
  }
});
test("bad encodings, NULs and file-size mismatches fail before a document can be replaced", async () => {
  const { readSourceFile } = await loaded;
  for (const bytes of [Uint8Array.of(0xc3, 0x28), Uint8Array.of(0xff, 0xfe, 65, 0), Uint8Array.of(65, 0, 66)]) {
    await assert.rejects(readSourceFile(file(bytes)), /UTF-8|NUL/);
  }
  await assert.rejects(readSourceFile(file("12345"), 4), /limit/);
  let read = false;
  await assert.rejects(readSourceFile({ name: "a.mmd", size: Infinity, arrayBuffer() { read = true; } }), /limit/);
  assert.equal(read, false);
  await assert.rejects(readSourceFile({ name: "a.mmd", size: 1, arrayBuffer: async () => new ArrayBuffer(2) }), /size changed/);
  await assert.rejects(readSourceFile(file(""), 0), /limit/);
});
test("editing changes only its source span and preserves every untouched line ending", async () => {
  const { SourceDocument } = await loaded;
  const source = "\uFEFF%% title\r\nflowchart TD\r\n A[Alpha]\n B[Beta]\r C[Gamma]  \r\n";
  const doc = new SourceDocument(source);
  doc.edit(doc.view.replace("Beta", "世界 😀"));
  assert.equal(doc.source, source.replace("Beta", "世界 😀"));
  assert.equal(doc.dirty, true);
  doc.edit(doc.view + " D[New]\n");
  assert.ok(doc.source.endsWith(" C[Gamma]  \r\n D[New]\r\n"));
  doc.edit(source.slice(1).replace(/\r\n?/g, "\n"));
  assert.equal(doc.source, source, "returning to baseline restores its exact format complement");
  assert.equal(doc.dirty, false);
});
test("view projection stays exact for thousands of deterministic Unicode edits", async () => {
  const { SourceDocument } = await loaded;
  let seed = 123;
  const next = (max) => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % max; };
  for (const raw of ["\uFEFFa\r\nb\rc\n", "", "😀a\r\né\n"]) {
    const doc = new SourceDocument(raw);
    for (let step = 0; step < 400; step += 1) {
      const chars = [...doc.view];
      const start = next(chars.length + 1), count = next(chars.length - start + 1);
      chars.splice(start, count, ["x", "😀", "\n", "é", "\uFEFF", ""][next(6)]);
      const view = chars.join("");
      doc.edit(view);
      assert.equal(doc.view, view);
      assert.equal(new TextDecoder("utf-8", { ignoreBOM: true }).decode(new TextEncoder().encode(doc.source)), doc.source);
    }
  }
});
test("failed or stale downloads cannot mark newer source or a different file exported", async () => {
  const { SourceDocument } = await loaded;
  const doc = new SourceDocument("A", "one.mmd");
  doc.edit("B");
  const old = doc.prepareExport();
  doc.edit("C");
  assert.equal(doc.exported(old), true);
  assert.equal(doc.dirty, true);
  const newest = doc.prepareExport();
  doc.exported(newest);
  assert.equal(doc.dirty, false);
  assert.equal(doc.exported(old), false, "older completion cannot roll back export baseline");
  doc.open("D", "two.dot");
  doc.edit("E");
  assert.equal(doc.exported(newest), false);
  assert.equal(doc.exported({ ...newest }), false);
  assert.equal(doc.dirty, true);
  assert.equal(doc.prepareExport().filename, "two.dot");
});
test("malformed UTF-16 remains editable but cannot silently change on download", async () => {
  const { SourceDocument } = await loaded;
  const doc = new SourceDocument("a");
  doc.edit("a\ud800");
  assert.equal(doc.source, "a\ud800");
  assert.throws(() => doc.prepareExport(), /surrogate/);
  doc.edit("a😀");
  assert.equal(doc.prepareExport().text, "a😀");
});
test("filename paths/control characters are removed without treating names as markup", async () => {
  const { SourceDocument } = await loaded;
  const doc = new SourceDocument("", "../../graphs\\世界\0.mmd");
  assert.equal(doc.name, "世界.mmd");
  doc.open("", "../..");
  assert.equal(doc.name, "diagram.mmd");
  doc.open("", "<img>.mmd");
  assert.equal(doc.name, "<img>.mmd");
});

class StorageFixture {
  values = new Map();
  failWrites = false;
  get length() { return this.values.size; }
  key(index) { return [...this.values.keys()][index] ?? null; }
  getItem(key) { return this.values.get(key) ?? null; }
  setItem(key, value) {
    if (this.failWrites) throw new Error("QuotaExceededError fixture");
    this.values.set(key, String(value));
  }
}
function repositoryOptions(storage) {
  let count = 0;
  return { getStorage: () => storage, newId: () => String(++count).padStart(32, "0"), now: () => 1000 };
}
test("recovery restores exact source, encoding complement, and dirty baseline", async () => {
  const { SourceDocument } = await loaded;
  const first = new SourceDocument("\uFEFFA\r\nB\n", "flow.mmd");
  first.edit("A\nC\n");
  const snapshot = JSON.parse(JSON.stringify(first.snapshot()));
  const restored = new SourceDocument("other");
  const unrelatedSave = restored.prepareExport();
  restored.restore(snapshot);
  assert.equal(restored.source, "\uFEFFA\r\nC\n");
  assert.equal(restored.view, "A\nC\n");
  assert.equal(restored.dirty, true);
  assert.equal(restored.exported(unrelatedSave), false);
  restored.edit("A\nB\n");
  assert.equal(restored.source, "\uFEFFA\r\nB\n");
  assert.equal(restored.dirty, false);
});
test("separate writers and restored forks cannot overwrite other recovery records", async () => {
  const { DraftRepository, SourceDocument } = await loaded;
  const storage = new StorageFixture(), options = repositoryOptions(storage);
  const one = new DraftRepository(options), two = new DraftRepository(options);
  const key1 = one.createKey(), key2 = two.createKey();
  one.write(key1, new SourceDocument("one").snapshot());
  two.write(key2, new SourceDocument("two").snapshot());
  assert.notEqual(key1, key2);
  assert.throws(() => two.write(key1, new SourceDocument("damaged").snapshot()), /another document/);
  const original = storage.getItem(key1);
  const recovered = new SourceDocument(""); recovered.restore(two.read(key1)); recovered.edit("fork");
  const forkKey = two.createKey(); two.write(forkKey, recovered.snapshot());
  assert.equal(storage.getItem(key1), original);
  assert.equal(one.list().drafts.length, 3);
  assert.equal(two.read(key2).source, "two");
  assert.equal(two.read(forkKey).source, "fork");
});
test("quota and record-size failures retain the last successfully stored source", async () => {
  const { DraftRepository, SourceDocument } = await loaded;
  const storage = new StorageFixture();
  const repo = new DraftRepository({ ...repositoryOptions(storage), maxRecordUnits: 400 });
  const key = repo.createKey(), doc = new SourceDocument("safe");
  repo.write(key, doc.snapshot()); const before = storage.getItem(key);
  doc.edit("x".repeat(500));
  assert.throws(() => repo.write(key, doc.snapshot()), /record limit/);
  assert.equal(storage.getItem(key), before);
  doc.edit("latest"); storage.failWrites = true;
  assert.throws(() => repo.write(key, doc.snapshot()), /QuotaExceeded/);
  assert.equal(storage.getItem(key), before);
  assert.equal(doc.source, "latest");
  storage.failWrites = false; repo.write(key, doc.snapshot());
  assert.equal(repo.read(key).source, "latest");
});
test("damaged and future-version records are retained, while valid copies remain recoverable", async () => {
  const { DraftRepository, SourceDocument } = await loaded;
  const storage = new StorageFixture(), repo = new DraftRepository(repositoryOptions(storage));
  const key = repo.createKey(); repo.write(key, new SourceDocument("good").snapshot());
  const prefix = "fm.playground.recovery.v1.";
  for (const [id, encoded] of [["broken", "not-json"], ["null", "null"],
    ["future", '{"schemaVersion":2}'], ["bad", '{"schemaVersion":1,"updatedAt":1}']]) storage.setItem(prefix + id, encoded);
  storage.setItem("unrelated-app", "keep me");
  const before = [...storage.values];
  const result = repo.list();
  assert.equal(result.drafts.length, 1); assert.equal(result.unreadable.length, 4);
  assert.equal(result.drafts[0].source, "good");
  assert.deepEqual([...storage.values], before, "listing must never repair, clear, or evict user records");
  assert.throws(() => repo.read("unrelated-app"), /Unknown recovery key/);
  assert.throws(() => repo.read(prefix + "missing"), /no longer available/);
});
test("unavailable storage and identity collisions fail without overwriting an existing record", async () => {
  const { DraftRepository, SourceDocument } = await loaded;
  const denied = new DraftRepository({ getStorage: () => { throw new Error("SecurityError fixture"); } });
  assert.throws(() => denied.list(), /SecurityError/);
  const storage = new StorageFixture();
  const options = { getStorage: () => storage, newId: () => "same-id-0000000000000" };
  const first = new DraftRepository(options), second = new DraftRepository(options);
  const key = first.createKey(); first.write(key, new SourceDocument("retained").snapshot());
  assert.throws(() => second.createKey(), /independent recovery copy/);
  assert.equal(first.read(key).source, "retained");
});
test("invalid restoration is atomic and malformed UTF-16 drafts remain repairable", async () => {
  const { DraftRepository, SourceDocument } = await loaded;
  const doc = new SourceDocument("current");
  for (const invalid of [null, {}, { ...doc.snapshot(), hasBom: true }, { ...doc.snapshot(), newline: "x" }]) {
    assert.throws(() => doc.restore(invalid), /Invalid source recovery snapshot/);
    assert.equal(doc.source, "current");
  }
  doc.edit("unfinished \ud800");
  const repo = new DraftRepository(repositoryOptions(new StorageFixture()));
  const key = repo.createKey(); repo.write(key, doc.snapshot());
  const restored = new SourceDocument(""); restored.restore(repo.read(key));
  assert.equal(restored.source, "unfinished \ud800");
  assert.throws(() => restored.prepareExport(), /surrogate/);
  restored.edit("repaired 😀"); assert.equal(restored.prepareExport().text, "repaired 😀");
});
