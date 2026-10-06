import { checkRenderId, prepareSvg } from "./mermaid-svg.mjs";

const MAX_SOURCE_BYTES = 2 * 1024 * 1024;
const encoder = new TextEncoder();
const TYPES = Object.freeze({ Flowchart: "flowchart-v2", Sequence: "sequence", State: "stateDiagram",
  Gantt: "gantt", Class: "class", Er: "er", Mindmap: "mindmap", Pie: "pie", GitGraph: "gitGraph",
  Journey: "journey", Requirement: "requirement", Timeline: "timeline", QuadrantChart: "quadrantChart",
  Sankey: "sankey", XyChart: "xychart", BlockBeta: "block", PacketBeta: "packet", ArchitectureBeta: "architecture",
  C4Context: "c4", C4Container: "c4", C4Component: "c4", C4Dynamic: "c4", C4Deployment: "c4",
  Kanban: "kanban", Treemap: "treemap", Radar: "radar", Info: "info" });

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
export function createMermaid({ loadEngine, document = globalThis.document, wasmInput,
  reportError = (error) => globalThis.console?.error(error),
} = {}) {
  if (typeof loadEngine !== "function") throw new TypeError("createMermaid requires an engine module loader.");
  let site = configuration();
  let loading;
  let disposed = false;
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
    const diagramType = TYPES[parsed?.ir?.diagram_type];
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
      catch (hookError) { reportError(hookError); }
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
    parseError: undefined,
    dispose() { disposed = true; },
  };
  return api;
}
