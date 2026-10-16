/* Isolated Rust-WASM execution for the VS Code preview. This file is both the browser
 * transport and the worker endpoint; tests execute these same implementations.
 * Only extension-owned JavaScript enters the module worker. Diagram source is always
 * structured-cloned data, never executable text. No dynamic worker imports. */
((root) => {
  "use strict";
  const MAX_INPUT_BYTES = 2 * 1024 * 1024;
  const READ_METHODS = ["renderSvg", "parseLens", "parse"];
  const EDIT_METHODS = ["applyParseLensEdit", "applyParseLensDelete", "applyParseLensInsertLineAfter"];
  const METHODS = [...READ_METHODS, ...EDIT_METHODS];
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

  // Retain serialized, immutable read results, never live source-edit receipts or DOM. A hit
  // gets a fresh object graph; a caller mutating its diagnostics/bindings cannot poison later
  // renders. Only lossless JSON trees qualify: Maps, undefined, -0, aliases, etc. pass through
  // uncached rather than silently changing their structured-clone semantics.
  function readResultJson(value, maxUnits) {
    const queue = [{ value, depth: 0 }], seen = new Set();
    let units = 0, entries = 0;
    while (queue.length) {
      const { value: item, depth } = queue.pop();
      if (++entries > 100000 || depth > 32) return null;
      if (typeof item === "string") units += item.length;
      else if (item === null || typeof item === "boolean") { /* JSON primitives. */ }
      else if (typeof item === "number") {
        if (!Number.isFinite(item) || Object.is(item, -0)) return null;
      } else {
        if (!item || typeof item !== "object" || seen.has(item)) return null;
        const array = Array.isArray(item);
        if (Object.getPrototypeOf(item) !== (array ? Array.prototype : Object.prototype)) return null;
        seen.add(item);
        const keys = Reflect.ownKeys(item);
        if (keys.length + entries + queue.length > 100000) return null;
        if (array && keys.length !== item.length + 1) return null;
        for (const key of keys) {
          if (array && key === "length") continue;
          const descriptor = Object.getOwnPropertyDescriptor(item, key);
          if (typeof key !== "string" || !descriptor.enumerable || !("value" in descriptor)
            || (array && !/^(?:0|[1-9][0-9]*)$/u.test(key))) return null;
          units += key.length;
          queue.push({ value: descriptor.value, depth: depth + 1 });
        }
      }
      if (units > maxUnits) return null;
    }
    const json = JSON.stringify(value);
    return typeof json === "string" && json.length <= maxUnits ? json : null;
  }

  function createReadCache(maxBytes, maxEntries) {
    const entries = new Map();
    let bytes = 0;
    return {
      get(key) {
        const entry = entries.get(key);
        if (!entry) return undefined;
        entries.delete(key); entries.set(key, entry); // Least recently used first.
        return { value: JSON.parse(entry.json) };
      },
      put(key, value) {
        if (!maxEntries || !maxBytes) return;
        // Budget UTF-16 key + serialized-value storage, plus a separate entry-count bound.
        // This is a retained-payload bound, not a claim about the engine's total heap size.
        const available = Math.floor(maxBytes / 2) - key.length;
        if (available < 1) return;
        const json = readResultJson(value, available);
        if (json === null) return;
        const cost = 2 * (key.length + json.length);
        if (entries.has(key)) { bytes -= entries.get(key).cost; entries.delete(key); }
        while (entries.size && (entries.size >= maxEntries || bytes + cost > maxBytes)) {
          const oldest = entries.keys().next().value;
          bytes -= entries.get(oldest).cost; entries.delete(oldest);
        }
        entries.set(key, { json, cost }); bytes += cost;
      },
      clear() { entries.clear(); bytes = 0; },
    };
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
        if (EDIT_METHODS.includes(message.method)) {
          const id = checkedText(message.elementId, 4096, "Element ID");
          if (!id) throw new Error("Missing element ID.");
          if (message.method === "applyParseLensDelete") value = engine.applyParseLensDelete(source, id);
          else {
            const replacement = checkedText(message.replacement, MAX_INPUT_BYTES, "Replacement");
            value = engine[message.method](source, id, replacement);
          }
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
    maxCachedBytes = 32 * 1024 * 1024, maxCachedEntries = 192,
  }) {
    if (typeof WorkerType !== "function") throw new Error("This preview requires Web Worker support.");
    if (!Number.isFinite(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000) throw new Error("Invalid engine time budget.");
    if (!Number.isSafeInteger(maxCachedBytes) || maxCachedBytes < 0 || maxCachedBytes > 128 * 1024 * 1024
      || !Number.isSafeInteger(maxCachedEntries) || maxCachedEntries < 0 || maxCachedEntries > 2048) {
      throw new Error("Invalid engine read-cache limits.");
    }
    let resources, loading, state, downloads;
    let epoch = 0, nextId = 0, disposed = false;
    // One immutable engine asset set per client. Exact source AND method are the key: document
    // line numbers, node indexes and hashes are not identities. Completed reads survive source
    // invalidation/worker restart, but never leave this client or survive its destruction.
    const readCache = createReadCache(maxCachedBytes, maxCachedEntries);
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
            "applyParseLensEdit: typeof applyParseLensEdit === 'function' ? applyParseLensEdit : undefined, " +
            "applyParseLensDelete: typeof applyParseLensDelete === 'function' ? applyParseLensDelete : undefined, " +
            "applyParseLensInsertLineAfter: typeof applyParseLensInsertLineAfter === 'function' ? applyParseLensInsertLineAfter : undefined});\n";
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
        if (EDIT_METHODS.includes(method)) api[method] = (source, elementId, replacement) => call(method, { source, elementId, replacement });
        else api[method] = (source) => call(method, { source });
      }
    }
    async function call(method, payload) {
      const generation = epoch;
      assertLive(generation);
      checkedText(payload.source, MAX_INPUT_BYTES, "Diagram source");
      if (EDIT_METHODS.includes(method)) {
        if (!checkedText(payload.elementId, 4096, "Element ID")) throw new Error("Missing element ID.");
        if (method !== "applyParseLensDelete") checkedText(payload.replacement, MAX_INPUT_BYTES, "Replacement");
      }
      const key = READ_METHODS.includes(method) ? `${method}:${payload.source}` : null;
      if (key !== null) {
        // Keep even a cache hit behind the same synchronous cancellation/disposal fence as
        // a worker request. It must not revive an obsolete render in a later microtask.
        await Promise.resolve(); assertLive(generation);
        const cached = readCache.get(key);
        if (cached) return cached.value;
      }
      const loaded = await loadResources(); assertLive(generation);
      const workerState = ensureWorker(loaded);
      const capabilities = await workerState.ready; assertLive(generation);
      if (!capabilities.includes(method)) throw new Error(`Engine has no ${method} export.`);
      const value = await request(workerState, method, payload);
      assertLive(generation);
      if (key !== null) readCache.put(key, value);
      return value;
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
      readCache.clear();
      if (resources) urls.revokeObjectURL(resources.url);
      resources = undefined;
    }
    return api;
  }
  const exported = { attach, create };
  if (typeof module === "object" && module.exports) module.exports = exported;
  else root.FmPreviewEngineWorker = exported;
})(globalThis);
