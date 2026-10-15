import { checkCallbackName, checkRenderId, createSvgBinder, prepareSvg } from "./mermaid-svg.mjs";

const MAX_SOURCE_BYTES = 2 * 1024 * 1024;
const encoder = new TextEncoder();
// Private transport hook: AbortSignals stay on the host, never in WASM arguments or postMessage.
const CANCELLABLE_CALL = Symbol("fm-cancellable-engine-call");
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
  Ishikawa: "ishikawa", TreeView: "treeView", Venn: "venn", Wardley: "wardley", EventModeling: "eventmodeling" });

export class MermaidError extends Error {
  constructor(message, code, diagnostics = []) {
    super(message);
    this.name = "MermaidError";
    this.code = code;
    this.diagnostics = diagnostics;
  }
}

function cancelledError() {
  const error = new MermaidError("The Mermaid operation was cancelled.", "cancelled");
  error.name = "AbortError";
  return error;
}

function operationSignal(options) {
  if (!options || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("Mermaid operation options must be an object.");
  }
  const signal = options.signal;
  if (signal == null) return undefined;
  if (typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function"
    || typeof signal.removeEventListener !== "function") throw new TypeError("signal must be an AbortSignal.");
  return signal;
}

function callEngine(module, method, args, signal) {
  return module[CANCELLABLE_CALL]
    ? module[CANCELLABLE_CALL](method, args, signal)
    : module[method](...args);
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
  let checkedConfigs = new WeakMap();
  let operationEpoch = {};
  const pendingWaits = new Set();
  const callbacks = new Map();
  const bindingRoots = new Set();
  const bindingCleanup = new WeakMap();
  let callbackEpoch = 0;
  let callbackRevision = 0;
  const captureInteractions = () => ({ epoch: callbackEpoch, revision: callbackRevision, registered: new Map(callbacks) });
  const allowsCallbacks = (snapshot) => String(snapshot.value.securityLevel || "strict").toLowerCase() === "loose";
  function trackBinding(root, cleanup) {
    // A long-lived docs widget must not retain detached diagrams just so dispose can find them.
    for (const reference of bindingRoots) if (!reference.deref()) bindingRoots.delete(reference);
    const previous = bindingCleanup.get(root);
    if (previous) { previous.cleanup(); bindingRoots.delete(previous.reference); }
    const reference = new WeakRef(root);
    bindingRoots.add(reference);
    bindingCleanup.set(root, { reference, cleanup });
  }
  function clearBindings() {
    for (const reference of bindingRoots) {
      const root = reference.deref();
      if (root) { bindingCleanup.get(root)?.cleanup(); bindingCleanup.delete(root); }
    }
    bindingRoots.clear();
  }
  function assertLive() {
    if (disposed) throw new MermaidError("This Mermaid instance has been disposed.", "disposed");
  }
  function assertCurrent(epoch, signal) {
    assertLive();
    if (epoch !== operationEpoch || signal?.aborted) throw cancelledError();
  }
  // Settle logical operations immediately even when a host loader/callback never resolves.
  // This does not stop host code: checkpoints revoke its authority to continue our pipeline.
  // The worker factory additionally terminates synchronous WASM at the execution boundary.
  function inOperation(promise, epoch, signal) {
    return new Promise((resolve, reject) => {
      let done = false;
      const finish = (success, value) => {
        if (done) return;
        done = true;
        pendingWaits.delete(cancel);
        signal?.removeEventListener("abort", cancel);
        if (success) resolve(value); else reject(value);
      };
      const cancel = () => {
        try { assertCurrent(epoch, signal); } catch (error) { finish(false, error); }
      };
      pendingWaits.add(cancel);
      signal?.addEventListener("abort", cancel, { once: true });
      // Observe rejection even when this operation has already been cancelled.
      Promise.resolve(promise).then(value => {
        try { assertCurrent(epoch, signal); finish(true, value); } catch (error) { finish(false, error); }
      }, error => finish(false, error));
      cancel();
    });
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
  async function validate(module, snapshot, epoch, signal) {
    assertCurrent(epoch, signal);
    let validation = checkedConfigs.get(snapshot);
    if (!validation) {
      validation = { controller: new AbortController(), waiters: 0, settled: false, promise: null };
      validation.promise = Promise.resolve().then(() => {
        assertCurrent(epoch, validation.controller.signal);
        return callEngine(module, "validateConfig", [snapshot.json], validation.controller.signal);
      }).then((raw) => {
        const report = typeof raw === "string" ? JSON.parse(raw) : raw;
        if (!report || !Array.isArray(report.errors)) throw new MermaidError("Engine returned an invalid configuration report.", "engine-contract");
        if (report.errors.length) throw new MermaidError(report.errors.map((error) => `${error.field}: ${error.message}`).join("\n"), "config", report.errors);
      }).then(() => { validation.settled = true; }).catch((error) => {
        validation.settled = true;
        // A failed worker bootstrap/validation must not poison this configuration forever.
        // The next explicit call may start a fresh worker; never replay failed work here.
        if (checkedConfigs.get(snapshot) === validation) checkedConfigs.delete(snapshot);
        throw error;
      });
      checkedConfigs.set(snapshot, validation);
    }
    // Validation is shared, but cancellation is not. Retire its work only after its last
    // waiter leaves; an aborted first caller must not poison another diagram's config.
    validation.waiters += 1;
    try { await inOperation(validation.promise, epoch, signal); }
    finally {
      validation.waiters -= 1;
      if (!validation.waiters && !validation.settled) {
        if (checkedConfigs.get(snapshot) === validation) checkedConfigs.delete(snapshot);
        validation.controller.abort();
      }
    }
    assertCurrent(epoch, signal);
  }
  async function inspect(source, snapshot, epoch, signal) {
    assertCurrent(epoch, signal);
    sourceText(source);
    const module = await inOperation(engine(), epoch, signal);
    assertCurrent(epoch, signal);
    await validate(module, snapshot, epoch, signal);
    assertCurrent(epoch, signal);
    const parsed = await inOperation(callEngine(module, "parse", [source], signal), epoch, signal);
    assertCurrent(epoch, signal);
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
  async function renderSnapshot(id, source, snapshot, ownerDocument, interactions, operation, signal) {
    assertCurrent(operation, signal);
    checkRenderId(id);
    const { epoch, registered } = interactions;
    const allowed = allowsCallbacks(snapshot);
    const parsed = await inOperation(inspect(source, snapshot, operation, signal), operation, signal);
    assertCurrent(operation, signal);
    const svg = await inOperation(callEngine(parsed.module, "renderSvg", [source, snapshot.value], signal), operation, signal);
    assertCurrent(operation, signal);
    const output = prepareSvg(svg, id, ownerDocument);
    const bindFunctions = output.element.querySelector("[data-callback]") ? createSvgBinder(output.element, {
      isLive: () => !disposed,
      resolveCallback: (name) => allowed && epoch === callbackEpoch &&
        callbacks.get(name) === registered.get(name) ? registered.get(name) : undefined,
      reportError: (error) => { if (allowed && epoch === callbackEpoch) report(error); },
      onBind: trackBinding,
    }) : undefined;
    return { ...output, bindFunctions, diagramType: parsed.diagramType, diagnostics: parsed.diagnostics, warnings: parsed.warnings };
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
  async function renderNode(item, snapshot, interactions, epoch) {
    assertCurrent(epoch);
    const { node } = item;
    // Another run may have completed this node since our selection snapshot. Never reparse
    // its SVG as Mermaid source. Removing data-processed is the caller's explicit retry/edit.
    if (node.getAttribute("data-processed") === "true") return undefined;
    let existing = activeNodes.get(node);
    if (existing?.owner === owner && existing.epoch !== epoch) {
      // An immediate retry must not await or inherit a cancelled run's ownership.
      // Its publication checkpoints and identity-checked cleanup remain revoked.
      activeNodes.delete(node);
      existing = undefined;
    }
    if (existing) {
      if (existing.owner !== owner || existing.snapshot !== snapshot || existing.callbackRevision !== interactions.revision || existing.markup !== item.markup
        || existing.source !== item.source) throw new MermaidError("A different render already owns this element.", "element-busy");
      await inOperation(existing.promise, epoch);
      return undefined; // The owning run performs the callback exactly once.
    }
    const job = { owner, snapshot, epoch, callbackRevision: interactions.revision, markup: item.markup, source: item.source };
    job.promise = Promise.resolve().then(async () => {
      assertCurrent(epoch);
      if (!unchanged(item)) throw new MermaidError("Diagram source changed before rendering; it was not replaced.", "stale-source");
      let id;
      do { id = `fm-mermaid-${++nextDocumentId}`; }
      while (item.document.getElementById(id) || item.root.getElementById?.(id));
      const result = await inOperation(renderSnapshot(id, item.source, snapshot, item.document, interactions, epoch), epoch);
      assertCurrent(epoch);
      if (!unchanged(item) || node.getAttribute("data-processed") === "true") {
        throw new MermaidError("Diagram source changed while rendering; it was not replaced.", "stale-source");
      }
      if (item.document.getElementById(id) || item.root.getElementById?.(id)) {
        throw new MermaidError("The generated SVG ID was claimed while rendering. Retry this element.", "id-conflict");
      }
      // Nothing changes in the host DOM until parsing, rendering, SVG validation, and the
      // source-generation checks all succeed. Failed nodes retain their original source.
      const unbind = result.bindFunctions?.(result.element);
      try {
        // A missing-handler reporter is host code and may edit source or dispose this owner.
        assertCurrent(epoch);
        if (!unchanged(item) || node.getAttribute("data-processed") === "true") {
          throw new MermaidError("Diagram source changed during interaction binding; it was not replaced.", "stale-source");
        }
        if (item.document.getElementById(id) || item.root.getElementById?.(id)) {
          throw new MermaidError("The generated SVG ID was claimed during interaction binding. Retry this element.", "id-conflict");
        }
        node.replaceChildren(result.element);
        node.setAttribute("data-processed", "true");
      } catch (error) { unbind?.(); throw error; }
      return id;
    });
    activeNodes.set(node, job);
    try { return await inOperation(job.promise, epoch); }
    finally { if (activeNodes.get(node) === job) activeNodes.delete(node); }
  }
  async function run(options = {}) {
    const epoch = operationEpoch;
    const snapshot = site;
    const interactions = captureInteractions();
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
        assertCurrent(epoch);
        const id = await inOperation(renderNode(item, snapshot, interactions, epoch), epoch);
        assertCurrent(epoch);
        if (id && callback) await inOperation(callback(id), epoch);
        assertCurrent(epoch);
      } catch (error) {
        if (suppress) report(error);
        else { notify(error); failures.push(error); }
        // A stopped document batch may not start its next diagram in a fresh worker.
        if (disposed || epoch !== operationEpoch) break;
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
    initialize(config) {
      assertLive();
      site = configuration(config);
      if (!allowsCallbacks(site)) {
        callbackEpoch += 1;
        clearBindings(); // Tightening policy revokes old diagrams, including pending results.
      }
    },
    registerCallback(name, callback) {
      assertLive();
      checkCallbackName(name);
      if (typeof callback !== "function") throw new TypeError("A diagram callback must be a function.");
      if (!callbacks.has(name) && callbacks.size >= 1024) throw new RangeError("At most 1024 diagram callbacks may be registered per instance.");
      // A fresh identity revokes prior registrations even when the same function is reused.
      const handler = (sourceId) => callback(sourceId);
      callbacks.set(name, handler);
      callbackRevision += 1;
      return () => {
        if (callbacks.get(name) === handler) { callbacks.delete(name); callbackRevision += 1; }
      };
    },
    async parse(source, options = {}) {
      const epoch = operationEpoch;
      let signal;
      try {
        signal = operationSignal(options);
        const { diagramType, diagnostics, warnings } = await inOperation(inspect(source, site, epoch, signal), epoch, signal);
        assertCurrent(epoch, signal);
        return { diagramType, diagnostics, warnings };
      } catch (error) {
        // Explicit caller cancellation is control flow, not a suppressible syntax failure.
        if (signal?.aborted) throw cancelledError();
        if (options?.suppressErrors === true) return false;
        notify(error); throw error;
      }
    },
    async render(id, source, container, options = {}) {
      const epoch = operationEpoch;
      let signal;
      try {
        signal = operationSignal(options);
        assertCurrent(epoch, signal);
        if (container !== undefined && (!container?.ownerDocument || container.nodeType !== 1)) {
          throw new TypeError("render's optional container must be a DOM element, not a callback.");
        }
        const { element, ...result } = await inOperation(renderSnapshot(id, source, site,
          container?.ownerDocument || document, captureInteractions(), epoch, signal), epoch, signal);
        assertCurrent(epoch, signal);
        return result;
      } catch (error) {
        if (signal?.aborted) throw cancelledError();
        notify(error); throw error;
      }
    },
    run,
    async contentLoaded() {
      if (disposed || startupFinished) return;
      startupFinished = true;
      clearStartup();
      if (site.value.startOnLoad !== false) await run();
    },
    parseError: undefined,
    cancelPending() {
      if (disposed) return;
      operationEpoch = {};
      checkedConfigs = new WeakMap();
      for (const cancel of pendingWaits) cancel();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      for (const cancel of pendingWaits) cancel();
      callbacks.clear();
      clearBindings();
      clearStartup();
    },
  };
  if (autoStart && document && window) {
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", scheduleStartup, { once: true });
    else scheduleStartup();
  }
  return api;
}

// This module doubles as the dedicated worker entry. Using an existing shipped module keeps
// source checkouts and sibling-engine packages on the same protocol without another asset.
// Importing it into an ordinary worker has no side effects: only this fragment opts in.
const WORKER_PROTOCOL = "fm-mermaid-engine/1";
const WORKER_FRAGMENT = "#fm-mermaid-engine";
const WORKER_METHODS = new Set(["parse", "renderSvg", "validateConfig"]);
const MAX_QUEUED_REQUESTS = 128;
const MAX_QUEUED_BYTES = 32 * 1024 * 1024;
const MAX_REPLY_BYTES = 16 * 1024 * 1024;

function workerError(message, code = "worker-error") {
  return new MermaidError(message, code);
}

function workerEngine(engineUrl, requestTimeoutMs) {
  let worker = null, ready = false, active = null, disposed = false;
  let bootTimer, requestTimer, idleTimer, sequence = 0, queuedBytes = 0;
  const queue = [];
  function retire() {
    clearTimeout(bootTimer);
    clearTimeout(requestTimer);
    clearTimeout(idleTimer);
    ready = false;
    if (worker) {
      const previous = worker;
      worker = null; // Revoke the generation BEFORE invoking the host's termination hook.
      previous.onmessage = previous.onerror = previous.onmessageerror = null;
      previous.terminate();
    }
  }
  function settle(job, error, value) {
    if (job.settled) return;
    job.settled = true;
    job.signal?.removeEventListener("abort", job.abort);
    queuedBytes -= job.bytes;
    if (error) job.reject(error); else job.resolve(value);
  }
  function fail(error, bootstrap = !ready) {
    const job = active;
    active = null;
    retire();
    if (job) settle(job, error);
    // A bad URL/CSP/import must not cause one failed bootstrap per queued diagram. Future
    // explicit calls may retry. Once execution started, keep independent queued work.
    if (bootstrap) while (queue.length) settle(queue.shift(), error);
    else pump();
  }
  function complete(error, value, fatal = false) {
    clearTimeout(requestTimer);
    const job = active;
    active = null;
    if (fatal) retire(); // A trapped WASM instance is not reused for the next diagram.
    settle(job, error, value);
    pump();
  }
  function start() {
    if (disposed || worker) return;
    try {
      if (typeof globalThis.Worker !== "function") {
        throw workerError("Module workers are unavailable. Serve over HTTP(S) with worker-src permission, or explicitly use createMermaid with a host-owned loader.", "worker-unavailable");
      }
      const url = new URL(import.meta.url);
      url.hash = WORKER_FRAGMENT;
      const current = new globalThis.Worker(url, { type: "module", name: "frankenmermaid-browser" });
      worker = current;
      const isCurrent = () => !disposed && worker === current;
      current.onmessage = ({ data }) => {
        if (!isCurrent()) return;
        if (data?.protocol !== WORKER_PROTOCOL) {
          fail(workerError("Invalid render-worker protocol.", "worker-protocol"));
          return;
        }
        if (!ready) {
          if (data.kind === "ready") {
            clearTimeout(bootTimer);
            ready = true;
            pump();
          } else {
            fail(workerError(data.message || "Render worker could not initialize.", "worker-bootstrap"), true);
          }
          return;
        }
        if (!active || data.requestId !== active.id || !["result", "failed"].includes(data.kind)) {
          fail(workerError("Render worker returned an unexpected response.", "worker-protocol"), false);
          return;
        }
        if (data.kind === "failed") {
          complete(workerError(data.message || "The render engine failed.", data.code || "worker-error"), undefined, data.fatal === true);
        } else complete(null, data.value);
      };
      current.onerror = (event) => {
        if (!isCurrent()) return;
        event.preventDefault();
        fail(workerError(event.message || "Render worker crashed.", "worker-crash"));
      };
      current.onmessageerror = () => {
        if (isCurrent()) fail(workerError("Render worker reply could not be decoded.", "worker-protocol"));
      };
      bootTimer = setTimeout(() => {
        if (isCurrent()) fail(workerError("Render worker initialization timed out; no work was run on the UI thread.", "worker-timeout"), true);
      }, requestTimeoutMs);
      current.postMessage({ protocol: WORKER_PROTOCOL, kind: "init", engineUrl });
    } catch (error) {
      fail(error instanceof MermaidError ? error : workerError(`Cannot start render worker: ${error.message || error}`, "worker-bootstrap"), true);
    }
  }
  function pump() {
    if (disposed || active) return;
    clearTimeout(idleTimer);
    // One signal can cancel several queued jobs in the same event dispatch. Do not start
    // another cancelled job while its abort listener is still waiting to be called.
    while (queue.length && queue[0].signal?.aborted) settle(queue.shift(), cancelledError());
    if (!queue.length) {
      if (worker) idleTimer = setTimeout(retire, 60_000);
      return;
    }
    if (!worker) { start(); return; }
    if (!ready) return;
    active = queue.shift();
    const job = active, current = worker;
    requestTimer = setTimeout(() => {
      if (worker === current && active === job) {
        fail(workerError(`Engine ${job.method} timed out. The worker was terminated; retry with corrected source or a larger requestTimeoutMs.`, "worker-timeout"), false);
      }
    }, requestTimeoutMs);
    try {
      current.postMessage({ protocol: WORKER_PROTOCOL, kind: "call", requestId: job.id, method: job.method, args: job.args });
    } catch (error) {
      fail(workerError(`Cannot send render request: ${error.message || error}`, "worker-protocol"), false);
    }
  }
  function abortJob(job) {
    if (job.settled || !job.signal.aborted) return;
    if (active === job) {
      active = null;
      // postMessage cannot interrupt synchronous WASM. Terminate only its execution
      // generation, never replay it, and keep independent queued work in FIFO order.
      retire();
    } else {
      const index = queue.indexOf(job);
      if (index === -1) return;
      queue.splice(index, 1);
      if (!active && !queue.length) retire();
    }
    settle(job, cancelledError());
    pump();
  }
  function request(method, args, signal) {
    if (disposed) return Promise.reject(workerError("This Mermaid instance has been disposed.", "disposed"));
    if (signal?.aborted) return Promise.reject(cancelledError());
    const bytes = encoder.encode(JSON.stringify(args)).length;
    if (queue.length + (active ? 1 : 0) >= MAX_QUEUED_REQUESTS || queuedBytes + bytes > MAX_QUEUED_BYTES) {
      return Promise.reject(workerError("Render worker queue is full. Await earlier diagrams before submitting more.", "worker-queue-full"));
    }
    if (sequence === Number.MAX_SAFE_INTEGER) return Promise.reject(workerError("Render request IDs exhausted; create a new instance.", "worker-limit"));
    return new Promise((resolve, reject) => {
      const job = { id: ++sequence, method, args, bytes, resolve, reject, signal, settled: false };
      job.abort = () => abortJob(job);
      queuedBytes += bytes;
      queue.push(job);
      signal?.addEventListener("abort", job.abort, { once: true });
      if (signal?.aborted) { abortJob(job); return; }
      pump();
    });
  }
  function cancelPending(error) {
    const job = active;
    active = null;
    retire();
    if (job) settle(job, error);
    while (queue.length) settle(queue.shift(), error);
  }
  return {
    module: Object.freeze({
      [CANCELLABLE_CALL]: request,
      parse: (source) => request("parse", [source]),
      renderSvg: (source, config) => request("renderSvg", [source, config]),
      validateConfig: (json) => request("validateConfig", [json]),
    }),
    cancelPending() {
      if (!disposed) cancelPending(cancelledError());
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      cancelPending(workerError("This Mermaid instance has been disposed.", "disposed"));
    },
  };
}

/** Isolated public API: all WASM calls execute in one lazy, bounded, instance-owned worker.
 * Independent diagrams are FIFO, NOT superseded as editor keystrokes are. A deadline/crash
 * terminates the active engine, never replays its request, and lets queued diagrams recover.
 * SVG validation, DOM publication, and registered callbacks remain on the owning UI thread.
 */
export function createWorkerMermaid({ engineUrl, requestTimeoutMs = 30_000, ...options } = {}) {
  if (options.loadEngine !== undefined || options.wasmInput !== undefined) {
    throw new TypeError("createWorkerMermaid accepts engineUrl, not loadEngine/wasmInput. Use createMermaid for custom main-thread loading.");
  }
  if (!Number.isSafeInteger(requestTimeoutMs) || requestTimeoutMs < 1 || requestTimeoutMs > 2_147_483_647) {
    throw new RangeError("requestTimeoutMs must be a positive 32-bit timer interval.");
  }
  if (!(typeof engineUrl === "string" || engineUrl instanceof URL) || !String(engineUrl)) {
    throw new TypeError("createWorkerMermaid requires an engine module URL.");
  }
  const url = new URL(engineUrl, import.meta.url);
  // Only the host supplies this module URL, never diagram source or an init directive.
  // Blob/data modules also let bundlers and offline hosts supply their own engine assets.
  if (!["http:", "https:", "blob:", "data:"].includes(url.protocol)) throw new TypeError("The worker engine URL must use HTTP(S) or a host-owned blob/data URL.");
  const client = workerEngine(url.href, requestTimeoutMs);
  const api = createMermaid({ ...options, loadEngine: () => client.module });
  const cancelPending = api.cancelPending;
  api.cancelPending = () => { try { cancelPending(); } finally { client.cancelPending(); } };
  const dispose = api.dispose;
  api.dispose = () => { try { dispose(); } finally { client.dispose(); } };
  return api;
}

function serveWorker(scope) {
  let module, initializing = false, busy = false;
  const post = (message) => scope.postMessage({ protocol: WORKER_PROTOCOL, ...message });
  scope.addEventListener("message", async ({ data }) => {
    if (data?.protocol !== WORKER_PROTOCOL) return;
    if (data.kind === "init" && !initializing) {
      initializing = true;
      try {
        const loaded = await import(/* @vite-ignore */ data.engineUrl);
        for (const name of WORKER_METHODS) {
          if (typeof loaded[name] !== "function") throw new Error(`Engine has no ${name} export. Rebuild the WASM package.`);
        }
        if (typeof loaded.default === "function") await loaded.default();
        module = loaded; // Do not publish an uninitialized wasm-bindgen namespace.
        post({ kind: "ready" });
      } catch (error) { post({ kind: "failed", message: String(error?.message || error).slice(0, 8192) }); }
      return;
    }
    const { requestId, method, args } = data;
    try {
      if (!module || busy || data.kind !== "call" || !Number.isSafeInteger(requestId) || requestId < 1 ||
          !WORKER_METHODS.has(method) || !Array.isArray(args) || args.length !== (method === "renderSvg" ? 2 : 1) ||
          typeof args[0] !== "string") throw workerError("Invalid render-worker request.", "worker-protocol");
      busy = true;
      let value = await module[method](...args);
      if (method === "parse") {
        // The public adapter consumes type/diagnostics/warnings, not the entire graph IR.
        // Keep the Rust reports verbatim; do not invent success defaults for missing fields.
        value = { ir: { diagram_type: value?.ir?.diagram_type, diagnostics: value?.ir?.diagnostics }, warnings: value?.warnings };
      }
      const text = typeof value === "string" ? value : JSON.stringify(value);
      if (typeof text !== "string" || text.length > MAX_REPLY_BYTES || encoder.encode(text).length > MAX_REPLY_BYTES) {
        throw workerError("Render worker output exceeds the 16 MiB reply limit.", "worker-output-limit");
      }
      post({ kind: "result", requestId, value });
    } catch (error) {
      post({ kind: "failed", requestId, message: String(error?.message || error).slice(0, 8192),
        code: error instanceof MermaidError ? error.code : "worker-error", fatal: error instanceof WebAssembly.RuntimeError });
    } finally { busy = false; }
  });
}

if (typeof DedicatedWorkerGlobalScope !== "undefined" && globalThis instanceof DedicatedWorkerGlobalScope &&
    globalThis.location.hash === WORKER_FRAGMENT) serveWorker(globalThis);
