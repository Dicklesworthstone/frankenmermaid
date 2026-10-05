/* Isolated Rust-WASM execution for the VS Code preview. This file is both the browser
 * transport and the worker endpoint; tests execute these same implementations.
 * Only extension-owned JavaScript enters the module worker. Diagram source is always
 * structured-cloned data, never executable text. No dynamic worker imports. */
((root) => {
  "use strict";
  const MAX_INPUT_BYTES = 2 * 1024 * 1024;
  const METHODS = ["renderSvg", "parseLens", "parse", "applyParseLensEdit"];
  const encoder = new TextEncoder();
  const abortError = () => Object.assign(new Error("Diagram operation cancelled."), { name: "AbortError" });
  const errorText = (error) => String(error?.message || error).slice(0, 16000);
  function checkedText(value, limit, label) {
    if (typeof value !== "string" || value.length > limit || encoder.encode(value).length > limit) {
      throw new Error(`${label} exceeds its size limit or is not text.`);
    }
    return value;
  }
  // Keep diagnostics and bindings, not a second full graph IR on the UI thread.
  function inspected(parsed) {
    return { warnings: parsed?.warnings || [], ir: { diagnostics: parsed?.ir?.diagnostics || [] } };
  }
  function attach(scope, engine) {
    let initialization;
    scope.addEventListener("message", async ({ data: message }) => {
      if (!message || !Number.isSafeInteger(message.id) || message.id < 0) return;
      try {
        if (message.method === "init") {
          if (initialization) throw new Error("Worker is already initialized.");
          if (!(message.bytes instanceof ArrayBuffer) || !message.bytes.byteLength) throw new Error("Missing WASM bytes.");
          initialization = Promise.resolve().then(() => engine.init({ module_or_path: message.bytes }));
          await initialization;
          if (typeof engine.renderSvg !== "function") throw new Error("Engine has no renderSvg export.");
          scope.postMessage({ id: message.id, ok: true, value: METHODS.filter((name) => typeof engine[name] === "function") });
          return;
        }
        if (!initialization || !METHODS.includes(message.method) || typeof engine[message.method] !== "function") {
          throw new Error("Unavailable engine operation.");
        }
        await initialization;
        const source = checkedText(message.source, MAX_INPUT_BYTES, "Diagram source");
        let value;
        if (message.method === "applyParseLensEdit") {
          const id = checkedText(message.elementId, 4096, "Element ID");
          if (!id) throw new Error("Missing element ID.");
          const replacement = checkedText(message.replacement, MAX_INPUT_BYTES, "Replacement");
          value = engine.applyParseLensEdit(source, id, replacement);
          // The native host verifies the entire receipt and exact source splice before writing.
          value = { result: value?.result, snapshot: { bindings: value?.snapshot?.bindings,
            parsed: inspected(value?.snapshot?.parsed) } };
        } else if (message.method === "parseLens") {
          const lens = engine.parseLens(source);
          value = { bindings: lens?.bindings, parsed: inspected(lens?.parsed) };
        } else if (message.method === "parse") value = inspected(engine.parse(source));
        else value = checkedText(engine.renderSvg(source), 16 * 1024 * 1024, "Rendered SVG");
        scope.postMessage({ id: message.id, ok: true, value });
      } catch (error) { scope.postMessage({ id: message.id, ok: false, error: errorText(error) }); }
    });
  }

  function create({ moduleUrl, binaryUrl, workerUrl, fetchFile = root.fetch.bind(root),
    WorkerType = root.Worker, BlobType = root.Blob, urls = root.URL,
    setTimer = root.setTimeout.bind(root), clearTimer = root.clearTimeout.bind(root), timeoutMs = 15000,
  }) {
    if (typeof WorkerType !== "function") throw new Error("This preview requires Web Worker support.");
    if (!Number.isFinite(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000) throw new Error("Invalid engine time budget.");
    let resources, loading, state, downloads;
    let epoch = 0, nextId = 0, disposed = false;
    const api = { default: initialize, renderSvg: (source) => call("renderSvg", { source }), cancelPending, destroy };
    function assertLive(generation) { if (disposed || generation !== epoch) throw abortError(); }
    async function loadResources() {
      if (resources) return resources;
      if (!loading) {
        const download = new AbortController();
        downloads = download;
        let timer;
        const deadline = new Promise((_, reject) => {
          timer = setTimer(() => {
            reject(new Error("Engine asset loading exceeded its time budget. Retry initialization."));
            download.abort();
          }, timeoutMs);
          download.signal.addEventListener("abort", () => reject(abortError()), { once: true });
        });
        const transfer = (async () => {
          const responses = await Promise.all([moduleUrl, workerUrl, binaryUrl].map(async (url) => {
            const response = await fetchFile(url, { signal: download.signal });
            if (!response.ok) throw new Error(`Unable to load engine asset (HTTP ${response.status}).`);
            return response;
          }));
          const [moduleSource, workerSource, bytes] = await Promise.all([
            responses[0].text(), responses[1].text(), responses[2].arrayBuffer(),
          ]);
          if (disposed || download.signal.aborted) throw abortError();
          if (!moduleSource || !workerSource || !bytes.byteLength) throw new Error("Empty engine asset.");
          // The pinned wasm-bindgen web target exports __wbg_init as default. Explicit bytes
          // avoid its import.meta.url-based default WASM path inside this self-contained blob.
          const bootstrap = "\n;globalThis.FmPreviewEngineWorker.attach(self, {init: __wbg_init, renderSvg, " +
            "parseLens: typeof parseLens === 'function' ? parseLens : undefined, " +
            "parse: typeof parse === 'function' ? parse : undefined, " +
            "applyParseLensEdit: typeof applyParseLensEdit === 'function' ? applyParseLensEdit : undefined});\n";
          const url = urls.createObjectURL(new BlobType([moduleSource, "\n", workerSource, bootstrap], { type: "text/javascript" }));
          resources = { url, bytes };
          return resources;
        })();
        loading = Promise.race([transfer, deadline]).catch((error) => {
          download.abort(); loading = undefined; throw error;
        }).finally(() => clearTimer(timer));
      }
      return loading;
    }
    function stop(error, expected = state) {
      if (!expected || state !== expected) return;
      state = undefined;
      expected.worker.removeEventListener("message", expected.onMessage);
      expected.worker.removeEventListener("error", expected.onError);
      expected.worker.removeEventListener("messageerror", expected.onError);
      expected.worker.terminate();
      for (const pending of expected.pending.values()) { clearTimer(pending.timer); pending.reject(error); }
      expected.pending.clear();
    }
    function request(workerState, method, payload, transfer = []) {
      if (state !== workerState || disposed) return Promise.reject(abortError());
      if (workerState.pending.size >= 64) return Promise.reject(new Error("Too many queued engine operations."));
      const id = ++nextId;
      return new Promise((resolve, reject) => {
        const timer = setTimer(() => stop(new Error(`Engine ${method} exceeded its ${timeoutMs} ms time budget. Retry the diagram or simplify its source.`), workerState), timeoutMs);
        workerState.pending.set(id, { resolve, reject, timer });
        try { workerState.worker.postMessage({ id, method, ...payload }, transfer); }
        catch (error) { stop(error, workerState); }
      });
    }
    function ensureWorker(loaded) {
      if (state) return state;
      const worker = new WorkerType(loaded.url, { type: "module", name: "FrankenMermaid engine" });
      const next = { worker, pending: new Map() };
      state = next;
      next.onMessage = ({ data: message }) => {
        if (state !== next || !message || !Number.isSafeInteger(message.id)) return;
        const pending = next.pending.get(message.id);
        if (!pending) return;
        if (typeof message.ok !== "boolean" || (!message.ok && typeof message.error !== "string")) {
          stop(new Error("Invalid engine worker response."), next); return;
        }
        clearTimer(pending.timer); next.pending.delete(message.id);
        if (message.ok) pending.resolve(message.value); else pending.reject(new Error(message.error));
      };
      next.onError = (event) => {
        event.preventDefault?.();
        stop(new Error(`Engine worker failed: ${event.message || "unreadable response"}`), next);
      };
      worker.addEventListener("message", next.onMessage);
      worker.addEventListener("error", next.onError);
      worker.addEventListener("messageerror", next.onError);
      const bytes = loaded.bytes.slice(0); // Retain exact bytes for restart; transfer only a copy.
      next.ready = request(next, "init", { bytes }, [bytes]).then((capabilities) => {
        if (!Array.isArray(capabilities) || !capabilities.includes("renderSvg") || capabilities.some((name) => !METHODS.includes(name))) {
          throw new Error("Invalid engine capabilities.");
        }
        return capabilities;
      }).catch((error) => { stop(error, next); throw error; });
      return next;
    }
    async function initialize() {
      const generation = epoch;
      assertLive(generation);
      const loaded = await loadResources(); assertLive(generation);
      const capabilities = await ensureWorker(loaded).ready; assertLive(generation);
      for (const method of capabilities) {
        if (method === "applyParseLensEdit") api[method] = (source, elementId, replacement) => call(method, { source, elementId, replacement });
        else api[method] = (source) => call(method, { source });
      }
    }
    async function call(method, payload) {
      const generation = epoch;
      assertLive(generation);
      checkedText(payload.source, MAX_INPUT_BYTES, "Diagram source");
      const loaded = await loadResources(); assertLive(generation);
      const workerState = ensureWorker(loaded);
      const capabilities = await workerState.ready; assertLive(generation);
      if (!capabilities.includes(method)) throw new Error(`Engine has no ${method} export.`);
      return request(workerState, method, payload);
    }
    function cancelPending() {
      epoch += 1;
      // Synchronous WASM cannot process cancellation messages while executing. Termination
      // interrupts the computation itself, not just the eventual display of its obsolete reply.
      if (state?.pending.size) stop(abortError());
    }
    function destroy() {
      if (disposed) return;
      disposed = true; epoch += 1; downloads?.abort(); stop(abortError());
      if (resources) urls.revokeObjectURL(resources.url);
      resources = undefined;
    }
    return api;
  }
  const exported = { attach, create };
  if (typeof module === "object" && module.exports) module.exports = exported;
  else root.FmPreviewEngineWorker = exported;
})(globalThis);
