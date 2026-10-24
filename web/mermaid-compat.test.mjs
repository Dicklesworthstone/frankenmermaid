import assert from "node:assert/strict";
import test from "node:test";
import { createMermaid, MermaidError } from "./mermaid-compat.mjs";

function fixture(overrides = {}) {
  const calls = [];
  const engine = { default: async (...args) => { calls.push(["init", ...args]); },
    parse: (source) => { calls.push(["parse", source]); return { ir: { diagram_type: "Flowchart", diagnostics: [] }, warnings: [] }; },
    renderSvg: () => "<svg/>", validateConfig: (json) => { calls.push(["config", json]); return '{"errors":[]}'; }, ...overrides };
  let loads = 0;
  const api = createMermaid({ loadEngine: async () => { loads++; return engine; } });
  return { api, engine, calls, loads: () => loads };
}
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };

test("browser adapter is lazy, shares one initialization, and delegates parsing without rewriting", async () => {
  const f = fixture();
  assert.equal(f.loads(), 0);
  f.api.initialize({ theme: "dark", startOnLoad: false });
  assert.equal(f.loads(), 0);
  const text = '%% café 😀\r\nflowchart LR\r\nA["東京"]-->B\r\n';
  const results = await Promise.all([f.api.parse(text), f.api.parse(text)]);
  assert.equal(f.loads(), 1);
  assert.equal(f.calls.filter(([name]) => name === "init").length, 1);
  assert.equal(f.calls.filter(([name]) => name === "config").length, 1);
  assert.equal(f.calls.filter(([name]) => name === "parse").length, 2);
  assert.equal(f.calls.find(([name]) => name === "parse")[1], text);
  assert.equal(results[0].diagramType, "flowchart-v2");
});

test("configuration is a captured deep snapshot and initialize replaces rather than mutates it", async () => {
  const gate = deferred(), f = fixture({ default: () => gate.promise });
  const config = { theme: "dark", flowchart: { rankDir: "LR" } };
  f.api.initialize(config);
  const first = f.api.parse("A-->B");
  config.flowchart.rankDir = "RL";
  f.api.initialize({ theme: "neutral" });
  const second = f.api.parse("C-->D");
  gate.resolve();
  await Promise.all([first, second]);
  assert.deepEqual(f.calls.filter(([name]) => name === "config").map(([, json]) => JSON.parse(json)),
    [{ theme: "dark", flowchart: { rankDir: "LR" } }, { theme: "neutral" }]);
});

test("config and source validation reject lossy, cyclic, executable and oversized input", async () => {
  const f = fixture();
  const cyclic = {}; cyclic.loop = cyclic;
  for (const config of [null, [], cyclic, { n: Infinity }, { theme: undefined }, { f() {} }, { date: new Date() },
    { get theme() { assert.fail("getter invoked"); } }, { startOnLoad: "true" }, JSON.parse('{"__proto__":{"x":1}}')]) {
    assert.throws(() => f.api.initialize(config));
  }
  assert.throws(() => f.api.initialize({ theme: "x".repeat(256 * 1024) }), /256 KiB/u);
  for (const source of ["", "   ", 7, "A[\ud800]", "😀".repeat(530000)]) await assert.rejects(f.api.parse(source));
  assert.equal(f.loads(), 0);
});

test("native configuration errors are preserved and never enter the parser", async () => {
  const errors = [{ field: "flowchart.rankDirection", message: "unknown key", value: "LR" }];
  const f = fixture({ validateConfig: () => JSON.stringify({ errors }) });
  f.api.initialize({ flowchart: { rankDirection: "LR" } });
  await assert.rejects(f.api.parse("A-->B"), (error) => error instanceof MermaidError && error.code === "config"
    && error.diagnostics[0].field === errors[0].field);
  assert.equal(f.calls.filter(([name]) => name === "parse").length, 0);
});

test("native diagnostics and warnings survive and suppressed parse errors do not call parseError", async () => {
  const diagnostics = [{ severity: "Error", message: "missing participant", span: { start: { line: 2 } } }];
  const f = fixture({ parse: () => ({ ir: { diagram_type: "Sequence", diagnostics }, warnings: ["recovery"] }) });
  const caught = [];
  f.api.parseError = (error, hash) => caught.push([error, hash]);
  assert.equal(await f.api.parse("sequenceDiagram", { suppressErrors: true }), false);
  assert.equal(caught.length, 0);
  await assert.rejects(f.api.parse("sequenceDiagram"), /missing participant/u);
  assert.equal(caught.length, 1);
  assert.equal(caught[0][1].diagnostics[0].span.start.line, 2);
  f.engine.parse = () => ({ ir: { diagram_type: "Sequence", diagnostics: [{ severity: "Warning", message: "recovered" }] }, warnings: ["recovered"] });
  const report = await f.api.parse("sequenceDiagram");
  assert.equal(report.diagramType, "sequence");
  assert.deepEqual(report.warnings, ["recovered"]);
  assert.equal(report.diagnostics.length, 1);
});

test("unknown diagram types and malformed engine reports fail rather than looking successful", async () => {
  for (const parsed of [null, {}, { ir: { diagram_type: "Unknown", diagnostics: [] }, warnings: [] },
    { ir: { diagram_type: "Flowchart" }, warnings: [] }]) {
    const f = fixture({ parse: () => parsed });
    await assert.rejects(f.api.parse("unknown"), MermaidError);
  }
  const f = fixture({ validateConfig: () => ({}) });
  await assert.rejects(f.api.parse("A-->B"), /configuration report/u);
});

test("failed initialization is retryable and missing exports are explicit", async () => {
  let attempts = 0;
  const f = fixture({ default: async () => { if (++attempts === 1) throw new Error("missing WASM"); } });
  await assert.rejects(f.api.parse("A-->B"), /missing WASM/u);
  assert.equal((await f.api.parse("A-->B")).diagramType, "flowchart-v2");
  assert.equal(attempts, 2);
  const bad = fixture({ renderSvg: undefined });
  await assert.rejects(bad.api.parse("A-->B"), /no renderSvg export/u);
});

test("dispose invalidates pending initialization and rejects every subsequent operation", async () => {
  const gate = deferred(), f = fixture({ default: () => gate.promise });
  const pending = f.api.parse("A-->B");
  f.api.dispose(); gate.resolve();
  await assert.rejects(pending, /disposed/u);
  await assert.rejects(f.api.render("diagram", "A-->B"), /disposed/u);
  assert.throws(() => f.api.initialize({}), /disposed/u);
});

test("render validates IDs and requires a browser without silently inserting into a caller container", async () => {
  const f = fixture();
  await assert.rejects(f.api.render('bad"id', "A-->B"), /Render ID/u);
  await assert.rejects(f.api.render("good", "A-->B", () => {}), /container/u);
  await assert.rejects(f.api.render("good", "A-->B"), /browser Document/u);
});


test("new native diagram families are accepted and prototype names are not diagram types", async () => {
  for (const [native, expected] of [["Ishikawa", "ishikawa"], ["TreeView", "treeView"]]) {
    const f = fixture({ parse: () => ({ ir: { diagram_type: native, diagnostics: [] }, warnings: [] }) });
    assert.equal((await f.api.parse("source")).diagramType, expected);
  }
  const f = fixture({ parse: () => ({ ir: { diagram_type: "constructor", diagnostics: [] }, warnings: [] }) });
  await assert.rejects(f.api.parse("source"), /recognize this diagram/u);
});
