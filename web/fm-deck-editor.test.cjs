"use strict";

// Transport/authoring contracts only. WASM is deliberately a fixture, not a second parser.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { pathToFileURL } = require("node:url");
const test = require("node:test");
const editor = import(pathToFileURL(path.join(__dirname, "fm-source-editor.js")));
const delay = () => new Promise((resolve) => setTimeout(resolve, 0));

function result(input = "deck") {
  return { svg: `<svg data-source="${input}"/>`, manifest: { schemaVersion: "1.1.0", slides: [] }, warnings: [] };
}

// Connect the actual worker handler to the actual client. Only Worker transport and engine
// exports are fixtures, so request/response mismatches cannot be hidden by canned replies.
function workerFixture(api = {}) {
  const instances = [];
  const calls = [];
  const module = {
    default() {},
    chooseCanvasTarget: () => JSON.stringify({ target: "svgInWorker" }),
    parseLens: () => ({ bindings: [] }),
    applyParseLensEdit: (source, _id, replacement) => ({
      result: { updatedSource: replacement }, snapshot: { bindings: [] },
    }),
    renderSvg: () => "<svg/>",
    renderDeck: (source, config) => { calls.push({ source, config }); return result(source); },
    ...api,
  };
  class WorkerFixture {
    constructor() {
      instances.push(this);
      const context = {
        setTimeout, clearTimeout, JSON, Number, Promise,
        __import: async () => module,
        self: { postMessage: (data) => queueMicrotask(() => this.onmessage?.({ data })) },
      };
      vm.runInNewContext(fs.readFileSync(path.join(__dirname, "fm-render.worker.js"), "utf8")
        .replaceAll("import(", "__import("), context);
      this.handler = context.self.onmessage;
      this.requests = [];
    }
    postMessage(data) {
      this.requests.push(data);
      this.handler({ data });
    }
    terminate() { this.terminated = true; this.onmessage = null; }
  }
  return { WorkerFixture, instances, calls };
}

async function backend(t, fixture, options = {}) {
  const { createSourceEditorBackend } = await editor;
  const instance = createSourceEditorBackend({
    WorkerClass: fixture.WorkerFixture, workerUrl: "fixture-worker", loadModule: () => {
      throw new Error("Unexpected synchronous fallback");
    }, ...options,
  });
  t.after(() => instance.dispose());
  return instance;
}

test("deck compilation uses one engine invocation and retains the paired SVG, manifest, warnings and config", async (t) => {
  const expected = result("paired");
  expected.warnings = [{ message: "unknown slide member", severity: "warning" }];
  let calls = 0;
  const fixture = workerFixture({
    parseLens() { throw new Error("deck compilation must not parse through the source lens"); },
    renderSvg() { throw new Error("deck compilation must not render an independent layout"); },
    renderDeck(input, config) {
      calls += 1;
      assert.equal(input, "paired");
      assert.deepEqual(JSON.parse(JSON.stringify(config)), { theme: "dark" });
      return expected;
    },
  });
  const api = await backend(t, fixture, { config: { theme: "dark" } });
  assert.deepEqual(await api.renderDeck("paired"), expected);
  assert.equal(calls, 1);
  assert.equal(api.target, "source worker");
});

test("missing or unsupported deck remains null with diagnostics, never an invented slideshow", async (t) => {
  const fixture = workerFixture({ renderDeck: () => ({ svg: "<svg/>", warnings: [{ message: "unsupported family" }] }) });
  const api = await backend(t, fixture);
  assert.deepEqual(await api.renderDeck("pie"), { svg: "<svg/>", manifest: null, warnings: [{ message: "unsupported family" }] });
});

test("obsolete deck compilation is cancelled before cold WASM initialization completes", async (t) => {
  let release;
  const ready = new Promise((resolve) => { release = resolve; });
  const fixture = workerFixture({ default: () => ready });
  const api = await backend(t, fixture);
  const stale = api.renderDeck("old");
  const rejected = assert.rejects(stale, { name: "AbortError" });
  const fresh = api.renderDeck("new");
  release();
  await rejected;
  assert.equal((await fresh).svg, result("new").svg);
  assert.deepEqual(fixture.calls.map((call) => call.source), ["new"]);
});

test("deck rendering and source edits share the existing cancellation boundary", async (t) => {
  const fixture = workerFixture();
  const api = await backend(t, fixture);
  await api.renderDeck("warm");
  const stale = api.renderDeck("obsolete");
  const rejected = assert.rejects(stale, { name: "AbortError" });
  const edited = api.editSource("old", "node", "new");
  await rejected;
  assert.equal((await edited).result.updatedSource, "new");
  assert.deepEqual(fixture.calls.map((call) => call.source), ["warm"]);
});

test("missing renderDeck is an actionable capability error, not a main-thread retry", async (t) => {
  const fixture = workerFixture({ renderDeck: undefined });
  const api = await backend(t, fixture);
  await assert.rejects(api.renderDeck("deck"), /lacks graph-deck rendering/);
  assert.equal(api.target, "source worker");
  assert.equal(fixture.instances[0].terminated, undefined);
  assert.equal((await api.renderSource("ordinary diagram")).svg, "<svg/>");
});

test("engine deck errors leave the worker usable and are not retried on the UI thread", async (t) => {
  const fixture = workerFixture({ renderDeck: (source) => {
    if (source === "bad") throw new Error("invalid deck configuration");
    return result(source);
  } });
  const api = await backend(t, fixture);
  await assert.rejects(api.renderDeck("bad"), /invalid deck configuration/);
  assert.equal((await api.renderDeck("good")).svg, result("good").svg);
  assert.equal(api.target, "source worker");
});

test("invalid deck results fail before crossing the worker response boundary", async (t) => {
  for (const bad of [null, {}, { svg: "<svg/>", warnings: [] , manifest: [] }, { svg: "<svg/>", warnings: "lost" }]) {
    const fixture = workerFixture({ renderDeck: () => bad });
    const api = await backend(t, fixture);
    await assert.rejects(api.renderDeck("deck"), /did not return a graph-deck result/);
    assert.equal(api.target, "source worker");
  }
});

test("explicit synchronous fallback calls renderDeck once and preserves config", async (t) => {
  let calls = 0;
  const api = await backend(t, workerFixture(), { WorkerClass: null, config: { theme: "forest" },
    loadModule: async () => ({ renderDeck(input, config) {
      calls += 1;
      assert.equal(input, "fallback");
      assert.equal(config.theme, "forest");
      return result(input);
    } }),
  });
  assert.deepEqual(await api.renderDeck("fallback"), result("fallback"));
  assert.equal(calls, 1);
  assert.match(api.target, /synchronous/);
});

test("deck fallback rechecks revisions after module loading and reports missing exports", async (t) => {
  let release;
  const ready = new Promise((resolve) => { release = resolve; });
  const calls = [];
  const api = await backend(t, workerFixture(), { WorkerClass: null, loadModule: () => ready });
  const old = api.renderDeck("old");
  const rejected = assert.rejects(old, { name: "AbortError" });
  const fresh = api.renderDeck("fresh");
  release({ renderDeck(input) { calls.push(input); return result(input); } });
  await rejected;
  await fresh;
  assert.deepEqual(calls, ["fresh"]);
  const missing = await backend(t, workerFixture(), { WorkerClass: null, loadModule: () => ({}) });
  await assert.rejects(missing.renderDeck("x"), /lacks graph-deck rendering/);
});

test("closing the editor settles active deck work and rejects future renders", async (t) => {
  const fixture = workerFixture({ default: () => new Promise(() => {}) });
  const api = await backend(t, fixture);
  const pending = api.renderDeck("pending");
  const rejected = assert.rejects(pending, { name: "AbortError" });
  api.dispose();
  await rejected;
  assert.equal(fixture.instances[0].terminated, true);
  await assert.rejects(api.renderDeck("after closing"), { name: "AbortError" });
});
