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
