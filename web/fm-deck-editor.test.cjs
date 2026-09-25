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

const deckEditor = import(pathToFileURL(path.join(__dirname, "fm-deck-editor.js")));
const assets = {
  runtime: fs.readFileSync(path.join(__dirname, "../crates/fm-cli/src/deck_runtime.js"), "utf8"),
  template: fs.readFileSync(path.join(__dirname, "../crates/fm-cli/src/deck_template.html"), "utf8"),
};
const nonce = "ab".repeat(24);
function validDeck() {
  const bounds = { x: 0, y: 0, width: 400, height: 200 };
  return { svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200"><g id="fm-node-a-0"/></svg>', warnings: [], manifest: {
    schemaVersion: "1.1.0", generator: "frankenmermaid", diagramType: "Flowchart", title: "Example",
    viewBox: bounds, options: { fitMargin: 20, zoomMax: 2, dimOpacity: 0.1, autoAdvanceMs: 0 },
    slides: [{ id: "first", title: "First", bounds, fitMargin: 20, zoomMax: 2, maxStep: 0,
      nodes: [{ index: 0, elementId: "fm-node-a-0", sourceId: "A", step: 0 }] }],
    overview: { enabled: true, title: "Overview", tour: false },
  } };
}

test("portable deck preserves Map-backed WASM records, including prototype-shaped keys", async () => {
  const { checkedDeck, deckJson } = await deckEditor;
  const deck = validDeck();
  deck.manifest.nodeGeometry = new Map([["fm-node-a-0", deck.manifest.viewBox]]);
  deck.manifest.nodeSlideIndex = new Map([["fm-node-a-0", ["first"]], ["__proto__", ["first"]]]);
  const normalized = checkedDeck(deck);
  assert.deepEqual(normalized.manifest.nodeGeometry["fm-node-a-0"], deck.manifest.viewBox);
  assert.deepEqual(normalized.manifest.nodeSlideIndex.__proto__, ["first"]);
  assert.equal(Object.getPrototypeOf(normalized.manifest.nodeSlideIndex), Object.prototype);
  assert.throws(() => deckJson(new Map([[1, "invalid"]])), /keys must be strings/);
});

test("deck validation rejects empty slides, duplicate IDs, unsupported versions and invalid geometry", async () => {
  const { checkedDeck } = await deckEditor;
  for (const mutate of [
    (d) => { d.manifest = null; },
    (d) => { d.manifest.schemaVersion = "2.0.0"; },
    (d) => { d.manifest.slides = []; },
    (d) => { d.manifest.slides.push(d.manifest.slides[0]); },
    (d) => { d.manifest.viewBox.width = 0; },
    (d) => { d.manifest.viewBox.x = NaN; },
    (d) => { d.manifest.options.zoomMax = Infinity; },
    (d) => { d.manifest.slides[0].nodes[0].step = 1; },
    (d) => { d.manifest.slides[0].steps = [{ step: -1, elementIds: ["fm-node-a-0"] }]; },
  ]) {
    const deck = validDeck();
    mutate(deck);
    assert.throws(() => checkedDeck(deck));
  }
});

test("HTML export embeds the exact canonical runtime and paired SVG without recursive template expansion", async () => {
  const { buildDeckHtml } = await deckEditor;
  const deck = validDeck();
  const hostile = '</script><script>globalThis.pwned=1</script> {{BG}} RUNTIME_JS \u2028 \u2029 & <b>literal</b>';
  deck.manifest.title = hostile;
  deck.manifest.slides[0].caption = hostile;
  deck.svg = deck.svg.replace("</svg>", `<text>${hostile}</text></svg>`);
  const built = buildDeckHtml(deck, assets, { nonce });
  assert.ok(built.html.includes(assets.runtime), "do not ship a second browser presentation runtime");
  const payload = built.html.match(/<script type="application\/json" id="deck-manifest">([\s\S]*?)<\/script>/)[1];
  assert.equal(JSON.parse(payload).title, hostile);
  assert.ok(!payload.includes("<"));
  assert.ok(!payload.includes("\u2028"));
  const encodedSvg = built.html.match(/\n    svg: (.*),\n/)[1];
  assert.equal(JSON.parse(encodedSvg), deck.svg);
  assert.equal((built.html.match(/<script nonce=/g) || []).length, 4);
  assert.ok(built.html.includes(`script-src 'nonce-${nonce}'`));
  assert.ok(!built.html.includes("<script>globalThis.pwned"));
  assert.match(built.html, /&lt;\/script&gt;/);
  assert.ok(built.html.includes("{{BG}} RUNTIME_JS"), "authored tokens must remain literal");
});

test("HTML export uses fresh nonces but is deterministic with an explicit nonce", async () => {
  const { buildDeckHtml } = await deckEditor;
  const deck = validDeck();
  assert.equal(buildDeckHtml(deck, assets, { nonce }).html, buildDeckHtml(deck, assets, { nonce }).html);
  assert.notEqual(buildDeckHtml(deck, assets).nonce, buildDeckHtml(deck, assets).nonce);
  for (const options of [{ nonce: "unsafe\"" }, { background: 'red; background:url(https://evil.invalid)' }, { foreground: "red" }]) {
    assert.throws(() => buildDeckHtml(deck, assets, options));
  }
});

test("missing template sentinels and an unsafe runtime cannot produce an export", async () => {
  const { buildDeckHtml } = await deckEditor;
  for (const broken of [
    { ...assets, runtime: "<html>fallback page</html>" },
    { ...assets, runtime: `${assets.runtime}\n// </script>` },
    { ...assets, template: assets.template.replace("{{TITLE}}", "") },
    { ...assets, template: assets.template + "RUNTIME_JS" },
  ]) assert.throws(() => buildDeckHtml(validDeck(), broken, { nonce }));
});

test("asset loading shares initialization and falls back from HTML route shells to tracked sources", async () => {
  const { createDeckAssetLoader } = await deckEditor;
  const requests = [];
  const load = createDeckAssetLoader(async (url) => {
    requests.push(url.pathname);
    return { ok: true, text: async () => url.pathname.includes("/crates/")
      ? (url.pathname.endsWith(".js") ? assets.runtime : assets.template) : "<html>fallback shell</html>" };
  });
  const [one, two] = await Promise.all([load(), load()]);
  assert.equal(one, two);
  assert.deepEqual(one, assets);
  assert.equal(requests.length, 4);
  await load();
  assert.equal(requests.length, 4);
});

test("failed asset downloads remain retryable instead of caching a broken export", async () => {
  const { createDeckAssetLoader } = await deckEditor;
  let available = false;
  const load = createDeckAssetLoader(async (url) => {
    if (!available) throw new Error("offline");
    return { ok: true, text: async () => url.pathname.endsWith(".js") ? assets.runtime : assets.template };
  });
  await assert.rejects(load(), /assets unavailable/);
  available = true;
  assert.deepEqual(await load(), assets);
});
