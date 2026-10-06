import { checkRenderId, prepareSvg } from "./mermaid-svg.mjs";

const MAX_SOURCE_BYTES = 2 * 1024 * 1024;
const encoder = new TextEncoder();
const activeNodes = new WeakMap();
let nextDocumentId = 0;
const MAX_DOCUMENT_DIAGRAMS = 64;
const MAX_DOCUMENT_BYTES = 16 * 1024 * 1024;
const TYPES = Object.freeze({ Flowchart: "flowchart-v2", Sequence: "sequence", State: "stateDiagram",
  Gantt: "gantt", Class: "class", Er: "er", Mindmap: "mindmap", Pie: "pie", GitGraph: "gitGraph",
  Journey: "journey", Requirement: "requirement", Timeline: "timeline", QuadrantChart: "quadrantChart",
  Sankey: "sankey", XyChart: "xychart", BlockBeta: "block", PacketBeta: "packet", ArchitectureBeta: "architecture",
  C4Context: "c4", C4Container: "c4", C4Component: "c4", C4Dynamic: "c4", C4Deployment: "c4",
  Kanban: "kanban", Treemap: "treemap", Radar: "radar", Info: "info",
  Ishikawa: "ishikawa", TreeView: "treeView" });

export class MermaidError extends Error {
  constructor(message, code, diagnostics = []) {
    super(message);
    this.name = "MermaidError";
    this.code = code;
    this.diagnostics = diagnostics;
  }
}

function sourceText(source) {
  if (typeof source !== "string" || source.length > MAX_SOURCE_BYTES || encoder.encode(source).length > MAX_SOURCE_BYTES) {
    throw new MermaidError("Mermaid source must be text within the 2 MiB limit.", "source-limit");
  }
  if (!source.trim()) throw new MermaidError("Mermaid source is empty.", "parse");
  for (const character of source) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw new MermaidError("Mermaid source contains an unpaired surrogate.", "source-encoding");
  }
  return source;
}

// Capture caller-owned config once, without allowing JSON serialization to hide unsupported
// values, invoke getters/toJSON, or retain mutable nested objects across an asynchronous load.
function copyConfig(value, depth = 0, ancestors = new Set()) {
  if (depth > 24) throw new TypeError("Configuration nesting exceeds 24 levels.");
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (!value || typeof value !== "object" || ancestors.has(value)
    || (!Array.isArray(value) && ![Object.prototype, null].includes(Object.getPrototypeOf(value)))) {
    throw new TypeError("Configuration must contain only finite JSON values without cycles.");
  }
  ancestors.add(value);
  const result = Array.isArray(value) ? [] : Object.create(null);
  for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(value))) {
    if (!descriptor.enumerable) continue;
    if (descriptor.get || descriptor.set || ["__proto__", "prototype", "constructor"].includes(key)) {
      throw new TypeError("Configuration contains an accessor or unsafe key.");
    }
    result[key] = copyConfig(descriptor.value, depth + 1, ancestors);
  }
  ancestors.delete(value);
  return Object.freeze(result);
}

function configuration(config = {}) {
  if (!config || typeof config !== "object" || Array.isArray(config)) throw new TypeError("initialize expects a configuration object.");
  const value = copyConfig(config);
  if (value.startOnLoad !== undefined && typeof value.startOnLoad !== "boolean") throw new TypeError("startOnLoad must be a boolean.");
  const json = JSON.stringify(value);
  if (encoder.encode(json).length > 256 * 1024) throw new TypeError("Configuration exceeds the 256 KiB limit.");
  return Object.freeze({ value, json });
}

/** Mermaid-shaped browser API over the native engine; not a second Mermaid implementation.
 * loadEngine returns the wasm-bindgen module (optionally already initialized). This factory
 * allows applications to own loading/caching and gives independent widgets isolated config.
 */
export function createMermaid({ loadEngine, document = globalThis.document, wasmInput, autoStart = false,
  reportError = (error) => globalThis.console?.error(error),
} = {}) {
  if (typeof loadEngine !== "function") throw new TypeError("createMermaid requires an engine module loader.");
  let site = configuration();
  let loading;
  let disposed = false;
  let startupTimer;
  let startupFinished = false;
  const owner = {};
  const window = document?.defaultView;
  const checkedConfigs = new WeakMap();
  function assertLive() {
    if (disposed) throw new MermaidError("This Mermaid instance has been disposed.", "disposed");
  }
  function engine() {
    assertLive();
    if (!loading) loading = Promise.resolve().then(loadEngine).then(async (module) => {
      for (const name of ["parse", "renderSvg", "validateConfig"]) {
        if (typeof module?.[name] !== "function") throw new MermaidError(`Engine has no ${name} export. Rebuild the WASM package.`, "engine-capability");
      }
      if (typeof module.default === "function") {
        await (wasmInput === undefined ? module.default() : module.default({ module_or_path: wasmInput }));
      }
      assertLive();
      return module;
    }).catch((error) => { loading = undefined; throw error; });
    return loading;
  }
  async function validate(module, snapshot) {
    let pending = checkedConfigs.get(snapshot);
    if (!pending) {
      pending = Promise.resolve().then(() => module.validateConfig(snapshot.json)).then((raw) => {
        const report = typeof raw === "string" ? JSON.parse(raw) : raw;
        if (!report || !Array.isArray(report.errors)) throw new MermaidError("Engine returned an invalid configuration report.", "engine-contract");
        if (report.errors.length) throw new MermaidError(report.errors.map((error) => `${error.field}: ${error.message}`).join("\n"), "config", report.errors);
      });
      checkedConfigs.set(snapshot, pending);
    }
    await pending;
    assertLive();
  }
  async function inspect(source, snapshot) {
    sourceText(source);
    const module = await engine();
    await validate(module, snapshot);
    const parsed = await module.parse(source);
    assertLive();
    const type = parsed?.ir?.diagram_type;
    const diagramType = Object.hasOwn(TYPES, type) ? TYPES[type] : undefined;
    const diagnostics = parsed?.ir?.diagnostics;
    if (!Array.isArray(diagnostics) || !Array.isArray(parsed?.warnings)) {
      throw new MermaidError("Engine returned an invalid parse report.", "engine-contract");
    }
    const errors = diagnostics.filter((item) => String(item?.severity).toLowerCase() === "error");
    if (!diagramType || errors.length) {
      throw new MermaidError(errors.map((error) => error.message).join("\n") || "The engine could not recognize this diagram.", "parse", diagnostics);
    }
    return { module, diagramType, diagnostics, warnings: parsed.warnings };
  }
  function notify(error) {
    if (typeof api.parseError === "function") {
      try { api.parseError(error, { diagnostics: error?.diagnostics || [] }); }
      catch (hookError) { report(hookError); }
    }
  }
  async function renderSnapshot(id, source, snapshot, ownerDocument = document) {
    checkRenderId(id);
    const parsed = await inspect(source, snapshot);
    const svg = await parsed.module.renderSvg(source, snapshot.value);
    assertLive();
    const output = prepareSvg(svg, id, ownerDocument);
    return { ...output, diagramType: parsed.diagramType, diagnostics: parsed.diagnostics, warnings: parsed.warnings };
  }
  function report(error) {
    // A host error reporter is observational; it must not turn a suppressed failure into an
    // unhandled rejection or prevent processing the remaining independent diagrams.
    try { reportError(error); } catch { /* The original failure remains available to parseError. */ }
  }
  function selectedSources(options) {
    assertLive();
    if (!options || typeof options !== "object" || Array.isArray(options)) throw new TypeError("run expects an options object.");
    if (options.postRenderCallback !== undefined && typeof options.postRenderCallback !== "function") {
      throw new TypeError("postRenderCallback must be a function.");
    }
    let collection = options.nodes;
    if (collection === undefined) {
      if (!document?.querySelectorAll) throw new Error("run requires a browser Document or explicit nodes.");
      const selector = options.querySelector ?? ".mermaid";
      if (typeof selector !== "string" || !selector || selector.length > 4096) throw new TypeError("Invalid Mermaid query selector.");
      collection = document.querySelectorAll(selector);
    }
    const length = collection?.length;
    if (!Number.isSafeInteger(length) || length < 0 || length > MAX_DOCUMENT_DIAGRAMS) {
      throw new RangeError("run supports at most 64 diagram elements per call.");
    }
    const nodes = [...new Set(Array.from({ length }, (_, index) => collection[index]))];
    for (const node of nodes) {
      if (!node || node.nodeType !== 1 || node.namespaceURI !== "http://www.w3.org/1999/xhtml"
        || !node.ownerDocument?.defaultView || typeof node.replaceChildren !== "function") {
        throw new TypeError("run nodes must be HTML elements with an owning browser Document.");
      }
      if (nodes.some((other) => other !== node && node.contains(other))) {
        throw new Error("Mermaid target elements must not contain one another.");
      }
    }
    let totalBytes = 0;
    // Snapshot ALL source nodes before the first await. A slow first render cannot authorize
    // overwriting later elements which the application edits while the engine is loading.
    return nodes.filter((node) => node.getAttribute("data-processed") !== "true").map((node) => {
      const source = node.textContent || "";
      totalBytes += encoder.encode(source).length;
      if (totalBytes > MAX_DOCUMENT_BYTES) throw new RangeError("Mermaid document sources exceed the 16 MiB limit.");
      return { node, source, markup: node.innerHTML, children: [...node.childNodes],
        document: node.ownerDocument, root: node.getRootNode(), connected: node.isConnected };
    });
  }
  function unchanged(item) {
    const { node } = item;
    return node.ownerDocument === item.document && node.getRootNode() === item.root
      && node.isConnected === item.connected && node.textContent === item.source
      && node.innerHTML === item.markup && node.childNodes.length === item.children.length
      && item.children.every((child, index) => node.childNodes[index] === child);
  }
  async function renderNode(item, snapshot) {
    assertLive();
    const { node } = item;
    // Another run may have completed this node since our selection snapshot. Never reparse
    // its SVG as Mermaid source. Removing data-processed is the caller's explicit retry/edit.
    if (node.getAttribute("data-processed") === "true") return undefined;
    const existing = activeNodes.get(node);
    if (existing) {
      if (existing.owner !== owner || existing.snapshot !== snapshot || existing.markup !== item.markup
        || existing.source !== item.source) throw new MermaidError("A different render already owns this element.", "element-busy");
      await existing.promise;
      return undefined; // The owning run performs the callback exactly once.
    }
    const job = { owner, snapshot, markup: item.markup, source: item.source };
    job.promise = Promise.resolve().then(async () => {
      if (!unchanged(item)) throw new MermaidError("Diagram source changed before rendering; it was not replaced.", "stale-source");
      let id;
      do { id = `fm-mermaid-${++nextDocumentId}`; }
      while (item.document.getElementById(id) || item.root.getElementById?.(id));
      const result = await renderSnapshot(id, item.source, snapshot, item.document);
      assertLive();
      if (!unchanged(item) || node.getAttribute("data-processed") === "true") {
        throw new MermaidError("Diagram source changed while rendering; it was not replaced.", "stale-source");
      }
      if (item.document.getElementById(id) || item.root.getElementById?.(id)) {
        throw new MermaidError("The generated SVG ID was claimed while rendering. Retry this element.", "id-conflict");
      }
      // Nothing changes in the host DOM until parsing, rendering, SVG validation, and the
      // source-generation checks all succeed. Failed nodes retain their original source.
      node.replaceChildren(result.element);
      node.setAttribute("data-processed", "true");
      return id;
    });
    activeNodes.set(node, job);
    try { return await job.promise; }
    finally { if (activeNodes.get(node) === job) activeNodes.delete(node); }
  }
  async function run(options = {}) {
    const snapshot = site;
    const suppress = options?.suppressErrors === true;
    const callback = options?.postRenderCallback;
    let items;
    try { items = selectedSources(options); }
    catch (error) {
      if (suppress) { report(error); return; }
      notify(error); throw error;
    }
    const failures = [];
    for (const item of items) {
      try {
        const id = await renderNode(item, snapshot);
        if (id && callback) await callback(id);
      } catch (error) {
        if (suppress) report(error);
        else { notify(error); failures.push(error); }
      }
    }
    if (failures.length) throw new AggregateError(failures, `${failures.length} Mermaid diagram(s) could not be processed.`);
  }
  function clearStartup() {
    document?.removeEventListener?.("DOMContentLoaded", scheduleStartup);
    if (startupTimer !== undefined) window?.clearTimeout(startupTimer);
    startupTimer = undefined;
  }
  function scheduleStartup() {
    clearStartup();
    if (disposed || startupFinished || !window) return;
    // A macrotask lets the importing module's synchronous initialize({startOnLoad:false})
    // run first, including dynamically imported modules after DOMContentLoaded has fired.
    startupTimer = window.setTimeout(() => {
      startupTimer = undefined;
      void api.contentLoaded().catch(report);
    }, 0);
  }
  const api = {
    initialize(config) { assertLive(); site = configuration(config); },
    async parse(source, options = {}) {
      try {
        const { diagramType, diagnostics, warnings } = await inspect(source, site);
        return { diagramType, diagnostics, warnings };
      } catch (error) {
        if (options.suppressErrors === true) return false;
        notify(error); throw error;
      }
    },
    async render(id, source, container) {
      try {
        if (container !== undefined && (!container?.ownerDocument || container.nodeType !== 1)) {
          throw new TypeError("render's optional container must be a DOM element, not a callback.");
        }
        const { element, ...result } = await renderSnapshot(id, source, site, container?.ownerDocument || document);
        return result;
      } catch (error) { notify(error); throw error; }
    },
    run,
    async contentLoaded() {
      if (disposed || startupFinished) return;
      startupFinished = true;
      clearStartup();
      if (site.value.startOnLoad !== false) await run();
    },
    parseError: undefined,
    dispose() { disposed = true; clearStartup(); },
  };
  if (autoStart && document && window) {
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", scheduleStartup, { once: true });
    else scheduleStartup();
  }
  return api;
}
