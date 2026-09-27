// Browser adapter for the Rust ParseLens (bd-1t7l.1). This edits SOURCE SPANS, not semantic
// nodes: a node's binding may cover a whole statement, including an edge and another node.
// Parsing and source-span replacements stay in WASM. The host owns revision safety,
// selection, exact-source history, and export.

function byteOffsets(source) {
  const offsets = new Map([[0, 0]]);
  let bytes = 0;
  let units = 0;
  for (const character of source) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) {
      throw new Error("Source contains an unpaired surrogate; repair it before editing.");
    }
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    units += character.length;
    offsets.set(bytes, units);
  }
  return offsets;
}

function checkedBindings(source, snapshot) {
  if (!snapshot || !Array.isArray(snapshot.bindings)) {
    throw new Error("The WASM package did not return a ParseLens snapshot.");
  }
  const offsets = byteOffsets(source);
  const bindings = new Map();
  for (const binding of snapshot.bindings) {
    if (binding.textRange == null) continue; // Synthesized elements can have no source span.
    const { startByte, endByte } = binding.textRange;
    const start = offsets.get(startByte);
    const end = offsets.get(endByte);
    if (typeof binding.elementId !== "string" || !binding.elementId ||
        !Number.isSafeInteger(startByte) || !Number.isSafeInteger(endByte) ||
        start === undefined || end === undefined || start > end) {
      throw new Error("The source map contains an invalid UTF-8 range; editing was disabled.");
    }
    const snippet = source.slice(start, end);
    if (binding.snippet != null && binding.snippet !== snippet) {
      throw new Error("The source map does not match the source; editing was disabled.");
    }
    if (bindings.has(binding.elementId)) throw new Error("Duplicate source-map element ID.");
    bindings.set(binding.elementId, Object.freeze({
      elementId: binding.elementId,
      sourceId: binding.sourceId,
      kind: binding.kind,
      start,
      end,
      snippet,
    }));
  }
  return bindings;
}

const MAX_SOURCE_BATCH_EDITS = 1024;

// Copy before the first await (including worker initialization). A caller changing its edits
// array later must not change the transaction the user is about to review.
function copySourceBatchEdits(edits) {
  if (!Array.isArray(edits) || edits.length > MAX_SOURCE_BATCH_EDITS) {
    throw new RangeError(`A source batch requires at most ${MAX_SOURCE_BATCH_EDITS} edits.`);
  }
  const ids = new Set();
  const copied = [];
  for (const edit of edits) {
    if (!edit || typeof edit.elementId !== "string" || !edit.elementId ||
        typeof edit.replacement !== "string") {
      throw new TypeError("Each batch edit requires an element ID and replacement text.");
    }
    if (ids.has(edit.elementId)) throw new Error(`Duplicate batch element: ${edit.elementId}`);
    ids.add(edit.elementId);
    byteOffsets(edit.replacement);
    copied.push(Object.freeze({ elementId: edit.elementId, replacement: edit.replacement }));
  }
  return Object.freeze(copied);
}

function planSourceBatch(source, bindings, edits) {
  const offsets = new Map([...byteOffsets(source)].map(([bytes, units]) => [units, bytes]));
  const entries = copySourceBatchEdits(edits).map((edit) => {
    const binding = bindings.get(edit.elementId);
    if (!binding) throw new Error(`No editable source span for ${edit.elementId}`);
    return Object.freeze({ ...binding, replacement: edit.replacement,
      startByte: offsets.get(binding.start), endByte: offsets.get(binding.end) });
  });
  const ordered = [...entries].sort((a, b) => a.start - b.start || a.end - b.end);
  const parts = [];
  let end = 0;
  for (let index = 0; index < ordered.length; index += 1) {
    const entry = ordered[index];
    if (index && (entry.start < end || entry.start === ordered[index - 1].start)) {
      throw new Error(`Overlapping source spans: ${ordered[index - 1].elementId} and ${entry.elementId}`);
    }
    parts.push(source.slice(end, entry.start), entry.replacement);
    end = entry.end;
  }
  parts.push(source.slice(end));
  return { entries, updatedSource: parts.join("") };
}

function checkBatchChange(entry, change) {
  if (!change || change.elementId !== entry.elementId ||
      typeof change.appliedElementId !== "string" || !change.appliedElementId ||
      change.previousSnippet !== entry.snippet || change.replacement !== entry.replacement ||
      change.replacedRange?.startByte !== entry.startByte || change.replacedRange?.endByte !== entry.endByte) {
    throw new Error("The batch response does not match the requested source transaction.");
  }
}

function checkedBatchResponse(plan, response) {
  const entries = plan.entries.filter((entry) => entry.replacement !== entry.snippet);
  const result = response?.result;
  if (result?.updatedSource !== plan.updatedSource || !Array.isArray(result.changes) ||
      result.changes.length !== entries.length) {
    throw new Error("The batch response changed unrelated text or omitted requested edits.");
  }
  for (let index = 0; index < entries.length; index += 1) checkBatchChange(entries[index], result.changes[index]);
  const bindings = checkedBindings(plan.updatedSource, response.snapshot);
  // Never retain the transport's mutable response object as a confirmation token.
  const preview = Object.freeze({ kind: "batch", updatedSource: plan.updatedSource,
    changes: Object.freeze(entries.map((entry) => Object.freeze({ elementId: entry.elementId,
      sourceId: entry.sourceId, startByte: entry.startByte, endByte: entry.endByte,
      previousSnippet: entry.snippet, replacement: entry.replacement }))),
    warnings: Object.freeze([...(response.snapshot.parsed?.warnings || [])].map(String)),
  });
  return { preview, bindings };
}

export class SourceEditSession {
  #api;
  #source = "";
  #bindings = new Map();
  #selection = null;
  #revision = 0;
  #prepared = null;
  #batch = null;

  constructor(api = null) {
    if (api !== null && (typeof api?.parseLens !== "function" || typeof api.applyParseLensEdit !== "function")) {
      throw new Error("This WASM build lacks source-editing APIs; rebuild the shipped package.");
    }
    this.#api = api;
  }

  get source() { return this.#source; }
  get bindings() { return [...this.#bindings.values()]; }

  #reset(source) {
    if (typeof source !== "string") throw new TypeError("Source must be text.");
    // Invalidate BEFORE parsing: a failed parse must never leave the previous document editable.
    this.#revision += 1;
    this.#selection = null;
    this.#prepared = null;
    this.#batch = null;
    this.#bindings = new Map();
    this.#source = source;
    byteOffsets(source); // Reject strings WASM's UTF-8 encoder would silently change.
  }

  setSource(source) {
    this.#reset(source);
    const snapshot = this.#api.parseLens(source);
    this.#bindings = checkedBindings(source, snapshot);
    return snapshot;
  }

  // Adopt a worker-produced snapshot through the SAME UTF-8/source validation as a local parse.
  setSnapshot(source, snapshot) {
    this.#reset(source);
    this.#bindings = checkedBindings(source, snapshot);
    return snapshot;
  }

  select(elementId) {
    const binding = this.#bindings.get(elementId);
    if (!binding) throw new Error("This diagram element has no editable source span.");
    this.#prepared = null;
    this.#selection = Object.freeze({ ...binding, revision: this.#revision });
    return this.#selection;
  }

  bindingAt(start, end = start) {
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start) return null;
    return this.bindings.filter((binding) => binding.start <= start && end <= binding.end &&
      (start !== end || start < binding.end))
      .sort((a, b) => (a.end - a.start) - (b.end - b.start))[0] || null;
  }

  #checkReplacement(selection, replacement) {
    if (!selection || selection !== this.#selection || selection.revision !== this.#revision) {
      throw new Error("The selection is stale; select the element again before applying an edit.");
    }
    if (typeof replacement !== "string") throw new TypeError("Replacement must be text.");
    byteOffsets(replacement);
  }

  replace(selection, replacement) {
    this.#checkReplacement(selection, replacement);
    const response = this.#api.applyParseLensEdit(this.#source, selection.elementId, replacement);
    return this.replaceWithResponse(selection, replacement, response);
  }

  // A remote edit is not trusted merely because its request finished. Recheck the selection
  // and exact splice at the commit boundary, after any intervening typing/selection changes.
  replaceWithResponse(selection, replacement, response) {
    this.#checkReplacement(selection, replacement);
    const updated = response?.result?.updatedSource;
    const expected = this.#source.slice(0, selection.start) + replacement + this.#source.slice(selection.end);
    if (updated !== expected) throw new Error("The edit response changed text outside the selected span.");
    // Validate the NEW snapshot before committing anything. IDs and ranges may all have changed.
    const bindings = checkedBindings(updated, response.snapshot);
    this.#source = updated;
    this.#bindings = bindings;
    this.#selection = null;
    this.#prepared = null;
    this.#batch = null;
    this.#revision += 1;
    return updated;
  }

  // Begin against ONE immutable revision, not whichever selection happens to be active after
  // the worker replies. Selection may change while staging; a source change invalidates it all.
  beginBatch(edits) {
    const copied = copySourceBatchEdits(edits);
    const plan = planSourceBatch(this.#source, this.#bindings, copied);
    const transaction = Object.freeze({ source: this.#source, edits: copied });
    this.#prepared = null;
    this.#batch = { transaction, plan, revision: this.#revision, preview: null };
    return transaction;
  }

  prepareBatch(transaction, response) {
    const batch = this.#batch;
    if (!batch || batch.transaction !== transaction || batch.revision !== this.#revision) {
      throw new Error("The batch is stale; prepare the edits against the current source.");
    }
    batch.preview = null;
    const { preview, bindings } = checkedBatchResponse(batch.plan, response);
    batch.preview = preview;
    batch.bindings = bindings;
    return preview;
  }

  cancelBatch() { this.#batch = null; }

  commitBatch(preview) {
    const batch = this.#batch;
    if (!preview || !batch || batch.preview !== preview || batch.revision !== this.#revision) {
      throw new Error("Batch preview is stale; prepare the edits again.");
    }
    this.#source = preview.updatedSource;
    this.#bindings = batch.bindings;
    this.#selection = null;
    this.#prepared = null;
    this.#batch = null;
    this.#revision += 1;
    return this.#source;
  }

  // Rust chooses the actual splice (including indentation/line terminators). Validate its
  // locality and UTF-8 boundaries, never regenerate Mermaid or infer semantic node deletion.
  // Preparation is read-only; a private, single-use token binds confirmation to this selection.
  prepareStructural(selection, kind, text, response) {
    this.#prepared = null;
    this.#batch = null;
    this.#checkReplacement(selection, text);
    if (kind !== "delete" && kind !== "insert") throw new Error("Unknown source operation.");
    const result = response?.result;
    const offsets = byteOffsets(this.#source);
    const { startByte, endByte } = result?.replacedRange || {};
    const start = offsets.get(startByte);
    const end = offsets.get(endByte);
    if (result?.elementId !== selection.elementId || !Number.isSafeInteger(startByte) ||
        !Number.isSafeInteger(endByte) || start === undefined || end === undefined || start > end ||
        typeof result.replacement !== "string" || typeof result.updatedSource !== "string" ||
        result.previousSnippet !== this.#source.slice(start, end)) {
      throw new Error("The structural edit returned an invalid source transaction.");
    }
    byteOffsets(result.replacement);
    const lineStart = this.#source.slice(0, selection.start).lastIndexOf("\n") + 1;
    const newline = this.#source.indexOf("\n", selection.end);
    const lineEnd = newline === -1 ? this.#source.length : newline + 1;
    if (kind === "delete") {
      const exactSpan = start === selection.start && end === selection.end;
      const wholeLine = start === lineStart && end === lineEnd &&
        /^\p{White_Space}*$/u.test(this.#source.slice(start, selection.start)) &&
        /^\p{White_Space}*$/u.test(this.#source.slice(selection.end, end));
      if (result.replacement !== "" || (!exactSpan && !wholeLine)) {
        throw new Error("Deletion changed text outside the selected span and its empty line.");
      }
    } else {
      // Only the submitted text plus line framing may be inserted. Rust remains responsible
      // for choosing indentation and LF/CRLF; the exact result is shown before confirmation.
      const framing = newline === -1 ? /^\r?\n[ \t]*$/ : /^[ \t]*$/;
      const framed = ["\r\n", "\n"].some((ending) => {
        if (!result.replacement.endsWith(ending)) return false;
        const body = result.replacement.slice(0, -ending.length);
        return body.endsWith(text) && framing.test(body.slice(0, body.length - text.length));
      });
      if (start !== lineEnd || end !== start || !framed) {
        throw new Error("Insertion did not preserve the requested text at the selected line boundary.");
      }
    }
    const expected = this.#source.slice(0, start) + result.replacement + this.#source.slice(end);
    if (result.updatedSource !== expected) throw new Error("The structural edit changed unrelated source text.");
    const bindings = checkedBindings(expected, response.snapshot);
    const preview = Object.freeze({ kind, startByte, endByte,
      previousSnippet: result.previousSnippet, replacement: result.replacement, updatedSource: expected,
      warnings: Object.freeze([...(response.snapshot.parsed?.warnings || [])].map(String)),
    });
    this.#prepared = { preview, selection, bindings };
    return preview;
  }

  cancelStructural() { this.#prepared = null; }

  commitStructural(preview) {
    const prepared = this.#prepared;
    if (!prepared || preview !== prepared.preview) throw new Error("Preview is stale; prepare the change again.");
    this.#checkReplacement(prepared.selection, "");
    this.#source = preview.updatedSource;
    this.#bindings = prepared.bindings;
    this.#selection = null;
    this.#prepared = null;
    this.#batch = null;
    this.#revision += 1;
    return this.#source;
  }
}

class SourceWorkerUnavailable extends Error {
  name = "SourceWorkerUnavailable";
}
function cancelledOperation() {
  const error = new Error("Source operation superseded or editor closed.");
  error.name = "AbortError";
  return error;
}

// The fallback uses the same per-edit session checks and final transaction validator as the
// worker client. All intermediate state is private. Parsing and replacements remain in Rust;
// this host code only resolves current bindings and yields between synchronous WASM calls.
async function applyLocalSourceBatch(api, input, edits, live) {
  const working = new SourceEditSession(api);
  let snapshot = working.setSource(input);
  const plan = planSourceBatch(input, new Map(working.bindings.map((binding) => [binding.elementId, binding])), edits);
  const changes = new Map();
  for (const entry of [...plan.entries].sort((a, b) => b.start - a.start)) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (!live()) throw cancelledOperation();
    if (entry.replacement === entry.snippet) continue;
    const matches = (binding) => binding.start === entry.start && binding.end === entry.end &&
      binding.snippet === entry.snippet && binding.sourceId === entry.sourceId && binding.kind === entry.kind;
    const originalId = working.bindings.find((binding) => binding.elementId === entry.elementId);
    const candidates = originalId && matches(originalId) ? [originalId] : working.bindings.filter(matches);
    if (candidates.length !== 1) throw new Error(`Source binding changed or became ambiguous during batch: ${entry.elementId}`);
    const binding = candidates[0];
    const selection = working.select(binding.elementId);
    const response = api.applyParseLensEdit(working.source, binding.elementId, entry.replacement);
    const result = response?.result;
    if (result?.elementId !== binding.elementId) throw new Error("The batch edit returned the wrong element ID.");
    const change = { elementId: entry.elementId, appliedElementId: binding.elementId,
      replacedRange: result.replacedRange, previousSnippet: result.previousSnippet, replacement: result.replacement };
    checkBatchChange(entry, change);
    working.replaceWithResponse(selection, entry.replacement, response);
    snapshot = response.snapshot;
    changes.set(entry.elementId, change);
  }
  if (!live()) throw cancelledOperation();
  const response = { result: { updatedSource: working.source,
    changes: plan.entries.flatMap((entry) => changes.has(entry.elementId) ? [changes.get(entry.elementId)] : []) }, snapshot };
  checkedBatchResponse(plan, response);
  return response;
}

/** A bounded, latest-request-only client for fm-render.worker.js source authoring. */
export function createSourceWorkerClient({ WorkerClass = globalThis.Worker, workerUrl, moduleUrl,
  config, initTimeoutMs = 10000, requestTimeoutMs = 60000 } = {}) {
  if (![initTimeoutMs, requestTimeoutMs].every((value) => Number.isSafeInteger(value) && value > 0)) {
    throw new RangeError("Worker timeouts must be positive safe integers.");
  }
  let worker;
  try {
    if (typeof WorkerClass !== "function") throw new Error("Worker API not supported");
    worker = new WorkerClass(workerUrl || new URL("./fm-render.worker.js", import.meta.url), { type: "module" });
  } catch (error) { throw new SourceWorkerUnavailable(String(error.message || error)); }
  let ready = false;
  let deckRendering = false;
  let sourceDeletion = false;
  let sourceInsertion = false;
  let sourceBatchEditing = false;
  let closed = false;
  let failure = null;
  let pending = null;
  let nextId = 1;
  let initTimer;

  function settle(error, value) {
    if (!pending) return;
    const operation = pending;
    pending = null;
    clearTimeout(operation.timer);
    if (error) operation.reject(error);
    else operation.resolve(value);
  }
  function fail(reason) {
    if (closed || failure) return;
    failure = new SourceWorkerUnavailable(String(reason));
    clearTimeout(initTimer);
    settle(failure);
    worker.terminate();
  }
  function sendPending() {
    if (!ready || !pending || pending.sent || closed || failure) return;
    if (pending.message.kind === "sourceBatch" && !sourceBatchEditing) {
      settle(new Error("The worker package lacks atomic batch editing; update the worker and WASM package."));
      return;
    }
    if (pending.message.kind === "sourceDeck" && !deckRendering) {
      settle(new Error("The worker package lacks graph-deck rendering; update the worker and WASM package."));
      return;
    }
    if ((pending.message.kind === "sourceDelete" && !sourceDeletion) ||
        (pending.message.kind === "sourceInsert" && !sourceInsertion)) {
      settle(new Error("The worker package lacks this structural edit; update the worker and WASM package."));
      return;
    }
    pending.sent = true;
    pending.timer = setTimeout(() => fail("source worker request timed out"), requestTimeoutMs);
    try { worker.postMessage(pending.message); }
    catch (error) { fail(error.message || error); }
  }
  function cancel() {
    if (!pending) return;
    const operation = pending;
    settle(cancelledOperation());
    if (operation.sent && !closed && !failure) {
      try { worker.postMessage({ kind: "cancel", requestId: operation.message.requestId }); }
      catch (error) { fail(error.message || error); }
    }
  }
  worker.onmessage = ({ data: message }) => {
    if (closed || failure) return;
    if (!message || typeof message !== "object") { fail("invalid source worker response"); return; }
    if (message.kind === "ready") {
      if (ready) return;
      if (message.sourceEditing !== true) { fail("worker package lacks source-editing support"); return; }
      clearTimeout(initTimer);
      ready = true;
      deckRendering = message.deckRendering === true;
      sourceDeletion = message.sourceDeletion === true;
      sourceInsertion = message.sourceInsertion === true;
      sourceBatchEditing = message.sourceBatchEditing === true;
      sendPending();
      return;
    }
    if (message.kind === "failed" && (!ready || message.requestId == null)) {
      fail(message.reason || "source worker initialization failed");
      return;
    }
    if (!pending || message.requestId !== pending.message.requestId) return;
    if (message.kind === "noReply") { settle(cancelledOperation()); return; }
    // Engine errors do not trigger synchronous fallback: retrying malformed input on the UI
    // thread would do the same failing work again and defeat isolation.
    if (message.kind === "failed") { settle(new Error(message.reason || "source operation failed")); return; }
    if (pending.message.kind === "sourceDeck") {
      if (message.kind !== "sourceDeckRendered" || typeof message.svg !== "string" ||
          !Array.isArray(message.warnings) ||
          (message.manifest != null && (typeof message.manifest !== "object" || Array.isArray(message.manifest)))) {
        fail("mismatched graph-deck worker response");
        return;
      }
      settle(null, { svg: message.svg, manifest: message.manifest ?? null, warnings: message.warnings });
      return;
    }
    const rendering = pending.message.kind === "sourceRender";
    const editReply = pending.message.kind === "sourceDelete" ? "sourceDeleted" :
      pending.message.kind === "sourceInsert" ? "sourceInserted" :
      pending.message.kind === "sourceBatch" ? "sourceBatchEdited" : "sourceEdited";
    const valid = rendering
      ? message.kind === "sourceRendered" && typeof message.svg === "string" && Array.isArray(message.snapshot?.bindings)
      : message.kind === editReply && typeof message.response?.result?.updatedSource === "string" &&
        Array.isArray(message.response?.snapshot?.bindings) &&
        (pending.message.kind !== "sourceBatch" || Array.isArray(message.response.result.changes));
    if (!valid) { fail("mismatched source worker response"); return; }
    settle(null, rendering ? { svg: message.svg, snapshot: message.snapshot } : message.response);
  };
  worker.onerror = (event) => fail(event.message || "source worker crashed");
  worker.onmessageerror = () => fail("source worker response could not be decoded");
  initTimer = setTimeout(() => fail("source worker initialization timed out"), initTimeoutMs);
  try {
    worker.postMessage({ kind: "init", moduleUrl, config,
      capabilities: { worker: true, offscreenCanvas: false, canvasTransferred: false } });
  } catch (error) { fail(error.message || error); }

  function request(kind, input, extra = {}) {
    if (closed) return Promise.reject(cancelledOperation());
    if (failure) return Promise.reject(failure);
    cancel();
    if (failure) return Promise.reject(failure);
    if (!Number.isSafeInteger(nextId)) return Promise.reject(new Error("Worker request IDs exhausted; reopen the editor."));
    return new Promise((resolve, reject) => {
      pending = { resolve, reject, sent: false, message: { kind, requestId: nextId++, input, ...extra } };
      sendPending();
    });
  }
  return {
    renderSource: (input) => request("sourceRender", input),
    renderDeck: (input) => request("sourceDeck", input),
    editSource: (input, elementId, replacement) => request("sourceEdit", input, { elementId, replacement }),
    deleteSource: (input, elementId) => request("sourceDelete", input, { elementId }),
    insertSource: (input, elementId, text) => request("sourceInsert", input, { elementId, text }),
    batchSource(input, edits) {
      try {
        if (typeof input !== "string") throw new TypeError("Source must be text.");
        byteOffsets(input);
        return request("sourceBatch", input, { edits: copySourceBatchEdits(edits) });
      } catch (error) { return Promise.reject(error); }
    },
    cancel,
    dispose() {
      if (closed) return;
      closed = true;
      clearTimeout(initTimer);
      settle(cancelledOperation());
      worker.terminate();
    },
  };
}

/** Worker-first authoring, with explicit synchronous fallback only for transport failures. */
export function createSourceEditorBackend({ loadModule, ...workerOptions }) {
  let client = null;
  let localModule = null;
  let generation = 0;
  let disposed = false;
  let fallbackReason = "";
  let target = "source worker";
  try { client = createSourceWorkerClient(workerOptions); }
  catch (error) {
    if (!(error instanceof SourceWorkerUnavailable)) throw error;
    fallbackReason = error.message;
    target = "main-thread SVG fallback (synchronous)";
  }
  function ensureLocalModule() {
    if (!localModule) {
      localModule = Promise.resolve().then(loadModule).catch((error) => { localModule = null; throw error; });
    }
    return localModule;
  }
  async function execute(kind, input, elementId, replacement, edits) {
    if (disposed) throw cancelledOperation();
    if (kind === "batch") edits = copySourceBatchEdits(edits);
    const version = ++generation;
    client?.cancel();
    const live = () => !disposed && version === generation;
    if (typeof input !== "string") throw new TypeError("Source must be text.");
    byteOffsets(input);
    if (["edit", "delete", "insert"].includes(kind) && (typeof elementId !== "string" || !elementId)) {
      throw new TypeError("An element ID is required for source edits.");
    }
    if (kind === "edit" || kind === "insert") {
      if (typeof replacement !== "string") throw new TypeError("Replacement must be text.");
      byteOffsets(replacement);
    }
    if (client) {
      try {
        const result = await (kind === "deck" ? client.renderDeck(input) :
          kind === "batch" ? client.batchSource(input, edits) :
          kind === "delete" ? client.deleteSource(input, elementId) :
          kind === "insert" ? client.insertSource(input, elementId, replacement) :
          kind === "render" ? client.renderSource(input) : client.editSource(input, elementId, replacement));
        if (!live()) throw cancelledOperation();
        return result;
      } catch (error) {
        if (!live()) throw cancelledOperation();
        if (!(error instanceof SourceWorkerUnavailable)) throw error;
        fallbackReason = error.message;
        client.dispose();
        client = null;
        target = "main-thread SVG fallback (synchronous)";
      }
    }
    const api = await ensureLocalModule();
    if (!live()) throw cancelledOperation();
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (!live()) throw cancelledOperation();
    if (kind === "batch") {
      const result = await applyLocalSourceBatch(api, input, edits, live);
      if (!live()) throw cancelledOperation();
      return result;
    }
    if (kind === "edit") return api.applyParseLensEdit(input, elementId, replacement);
    if (kind === "delete" || kind === "insert") {
      const name = kind === "delete" ? "applyParseLensDelete" : "applyParseLensInsertLineAfter";
      if (typeof api[name] !== "function") throw new Error(`This WASM build lacks ${name}; rebuild the shipped package.`);
      return kind === "delete" ? api[name](input, elementId) : api[name](input, elementId, replacement);
    }
    if (kind === "deck") {
      if (typeof api.renderDeck !== "function") throw new Error("This WASM build lacks graph-deck rendering; rebuild the shipped package.");
      return api.renderDeck(input, workerOptions.config);
    }
    return { snapshot: api.parseLens(input), svg: api.renderSvg(input, workerOptions.config) };
  }
  return {
    get target() { return target; },
    get fallbackReason() { return fallbackReason; },
    renderSource: (input) => execute("render", input),
    renderDeck: (input) => execute("deck", input),
    editSource: (input, elementId, replacement) => execute("edit", input, elementId, replacement),
    deleteSource: (input, elementId) => execute("delete", input, elementId),
    insertSource: (input, elementId, text) => execute("insert", input, elementId, text),
    batchSource: (input, edits) => execute("batch", input, undefined, undefined, edits),
    cancel() { generation += 1; client?.cancel(); },
    dispose() { disposed = true; generation += 1; client?.dispose(); },
  };
}

// History owns exact SOURCE snapshots, independent of parsing/rendering success. Undo must
// still work when the last edit introduced invalid Mermaid or the renderer is unavailable.
export class SourceHistory {
  #entries;
  #index = 0;
  #units;
  #maxEntries;
  #maxCodeUnits;

  constructor(source, { maxEntries = 100, maxCodeUnits = 2 * 1024 * 1024 } = {}) {
    if (typeof source !== "string") throw new TypeError("Source must be text.");
    if (!Number.isSafeInteger(maxEntries) || maxEntries < 1 ||
        !Number.isSafeInteger(maxCodeUnits) || maxCodeUnits < 1) {
      throw new RangeError("History limits must be positive safe integers.");
    }
    this.#entries = [source];
    this.#units = source.length;
    this.#maxEntries = maxEntries;
    this.#maxCodeUnits = maxCodeUnits;
  }

  get source() { return this.#entries[this.#index]; }
  get canUndo() { return this.#index > 0; }
  get canRedo() { return this.#index + 1 < this.#entries.length; }

  record(source) {
    if (typeof source !== "string") throw new TypeError("Source must be text.");
    if (source === this.source) return;
    for (const discarded of this.#entries.splice(this.#index + 1)) this.#units -= discarded.length;
    this.#entries.push(source);
    this.#index += 1;
    this.#units += source.length;
    // Retain the current document even when it alone exceeds the budget. Never truncate text.
    while (this.#entries.length > 1 &&
           (this.#entries.length > this.#maxEntries || this.#units > this.#maxCodeUnits)) {
      this.#units -= this.#entries.shift().length;
      this.#index -= 1;
    }
  }

  undo() {
    if (this.canUndo) this.#index -= 1;
    return this.source;
  }

  redo() {
    if (this.canRedo) this.#index += 1;
    return this.source;
  }
}

function searchText(value) {
  return String(value ?? "").normalize("NFKC").toLowerCase().replace(/\s+/gu, " ").trim();
}

/** Literal, Unicode-normalized search over engine bindings, not another source parser. */
export function searchSourceBindings(bindings, query, kind = "all", labels = new Map()) {
  const words = searchText(query).split(" ").filter(Boolean);
  return bindings.filter((binding) => {
    if (kind !== "all" && String(binding.kind).toLowerCase() !== kind) return false;
    const text = searchText([binding.sourceId, binding.elementId, binding.snippet,
      labels.get(binding.elementId)].join(" "));
    return words.every((word) => text.includes(word));
  });
}

/** Pick a visual neighbor, with aligned boxes before diagonals and stable input-order ties.
 * Rectangles are in screen space, so SVG nesting, orientation and zoom need no special cases.
 * This is spatial navigation, NOT graph connectivity (source spans do not encode adjacency).
 */
export function directionalSourceBinding(items, fromId, direction) {
  const vector = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[direction];
  const valid = (rect) => rect && [rect.x, rect.y, rect.width, rect.height].every(Number.isFinite) &&
    rect.width >= 0 && rect.height >= 0 && (rect.width > 0 || rect.height > 0);
  const origin = items.find((item) => item.elementId === fromId);
  if (!vector || !origin || !valid(origin.rect)) return null;
  const a = origin.rect;
  const horizontal = vector[0] !== 0;
  let best = null;
  let bestScore = null;
  for (const item of items) {
    if (item.elementId === fromId || !valid(item.rect)) continue;
    const b = item.rect;
    const dx = b.x + b.width / 2 - a.x - a.width / 2;
    const dy = b.y + b.height / 2 - a.y - a.height / 2;
    const forward = dx * vector[0] + dy * vector[1];
    if (forward <= 0) continue;
    const cross = Math.abs(horizontal ? dy : dx);
    const overlap = horizontal
      ? Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y)
      : Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
    const score = [overlap >= 0 ? 0 : 1, Math.hypot(forward, cross) + 2 * cross];
    if (!bestScore || score[0] < bestScore[0] || (score[0] === bestScore[0] && score[1] < bestScore[1])) {
      best = item;
      bestScore = score;
    }
  }
  return best?.elementId ?? null;
}

/** Mount a worker-first SVG/source editor. loadModule initializes the fallback WASM only. */
export function mountSourceEditor({ sourceEl, outEl, panelEl, loadModule, onChange, saveFile, workerOptions = {} }) {
  const document = panelEl.ownerDocument;
  const listeners = [];
  const history = new SourceHistory(sourceEl.value);
  const downloads = new Map();
  const backend = createSourceEditorBackend({ loadModule, ...workerOptions });
  let epoch = 0;
  let disposed = false;
  let session = null;
  let selection = null;
  let activeEdit = null;
  let preparedEdit = null;
  let displayedSource = null;
  let renderedSvg = null;
  let elements = new Map();
  let originalTabIndices = new Map();
  let matchedBindings = [];
  let renderedLabels = new Map();
  let staged = new Map();
  let stagedSession = null;
  let stagedEpoch = 0;
  let batchRevision = 0;
  let preparedBatch = null;
  let sourceComposing = false;
  let replacementComposing = false;

  function download(artifact) {
    const host = document.defaultView;
    const url = host.URL.createObjectURL(new host.Blob([artifact.text], { type: artifact.mime }));
    const link = document.createElement("a");
    link.href = url;
    link.download = artifact.filename;
    document.body.append(link);
    try {
      link.click();
    } finally {
      link.remove();
      downloads.set(url, host.setTimeout(() => {
        host.URL.revokeObjectURL(url);
        downloads.delete(url);
      }, 0));
    }
  }

  function make(tag, text, id, parent = panelEl) {
    const element = document.createElement(tag);
    if (text) element.textContent = text;
    if (id) element.id = id;
    parent.append(element);
    return element;
  }
  function listen(element, event, handler, options) {
    element.addEventListener(event, handler, options);
    listeners.push(() => element.removeEventListener(event, handler, options));
  }
  make("p", "Select a diagram element or source text. Edit the exact source span below; it may include a whole statement, not just a label.");
  const undo = make("button", "Undo source change", "source-editor-undo");
  const redo = make("button", "Redo source change", "source-editor-redo");
  const saveSource = make("button", "Save source (.mmd)", "source-editor-save-source");
  const saveSvg = make("button", "Save SVG", "source-editor-save-svg");
  for (const button of [undo, redo, saveSource, saveSvg]) button.type = "button";
  const searchLabel = make("label", "Find diagram elements (ID, label or source)", "source-editor-search-label");
  const search = make("input", "", "source-editor-search");
  search.type = "search";
  searchLabel.htmlFor = search.id;
  const kindLabel = make("label", "Element kind", "source-editor-kind-label");
  const kindFilter = make("select", "", "source-editor-kind");
  kindLabel.htmlFor = kindFilter.id;
  for (const [value, title] of [["all", "All elements"], ["node", "Nodes"], ["edge", "Edges"], ["cluster", "Clusters"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = title;
    kindFilter.append(option);
  }
  const previousMatch = make("button", "Previous match", "source-editor-previous");
  const nextMatch = make("button", "Next match", "source-editor-next");
  for (const button of [previousMatch, nextMatch]) button.type = "button";
  const searchStatus = make("p", "", "source-editor-search-status");
  searchStatus.setAttribute("role", "status");
  make("p", "Enter in search selects the next match; Shift+Enter goes back. On the diagram, arrow keys move to a matching element of the same kind by visual position; Home/End go to the first/last. Enter edits its source. Tab leaves the diagram.");
  const label = make("label", "Source element", "source-editor-label");
  const chooser = make("select", "", "source-editor-elements");
  label.htmlFor = chooser.id;
  const snippetLabel = make("label", "Replacement source", "source-editor-snippet-label");
  const snippet = make("textarea", "", "source-editor-snippet");
  snippetLabel.htmlFor = snippet.id;
  snippet.rows = 3;
  snippet.spellcheck = false;
  const apply = make("button", "Apply source edit", "source-editor-apply");
  apply.type = "button";
  const stage = make("button", "Stage replacement for batch", "source-editor-stage");
  const batchPanel = make("fieldset", "", "source-editor-batch");
  batchPanel.style.minWidth = "0";
  make("legend", "Batch source edits", "", batchPanel);
  make("p", "Select elements, edit their replacement source, and stage each change. Preview the whole batch before applying it as one undoable change. Shared or overlapping statement spans cannot be staged together. Apply or clear staged changes before using single-span edits.", "", batchPanel);
  const batchStatus = make("p", "", "source-editor-batch-status", batchPanel);
  batchStatus.setAttribute("role", "status");
  const batchList = make("ol", "", "source-editor-batch-list", batchPanel);
  batchList.style.maxHeight = "20rem";
  batchList.style.overflow = "auto";
  const batchPrepare = make("button", "Preview staged edits", "source-editor-batch-prepare", batchPanel);
  const batchClear = make("button", "Clear staged edits", "source-editor-batch-clear", batchPanel);
  const batchPreview = make("pre", "", "source-editor-batch-preview", batchPanel);
  batchPreview.style.whiteSpace = "pre-wrap";
  batchPreview.style.overflowWrap = "anywhere";
  batchPreview.setAttribute("tabindex", "0");
  batchPreview.setAttribute("aria-label", "Exact batch source changes");
  const batchApply = make("button", "Apply all staged edits", "source-editor-batch-apply", batchPanel);
  const batchCancel = make("button", "Discard batch preview", "source-editor-batch-cancel", batchPanel);
  for (const button of [stage, batchPrepare, batchClear, batchApply, batchCancel]) button.type = "button";
  make("p", "Insert source after the selected span's line, or remove that span. A shared statement may contain several nodes and edges; deletion does not remove other references. Review the exact change before applying it.");
  const insertLabel = make("label", "Source to insert (Rust preserves line framing)", "source-editor-insert-label");
  const insertText = make("textarea", "", "source-editor-insert-text");
  insertLabel.htmlFor = insertText.id;
  insertText.rows = 3;
  insertText.spellcheck = false;
  const insert = make("button", "Preview insertion after selected line", "source-editor-insert");
  const remove = make("button", "Preview deletion of selected span", "source-editor-delete");
  const preview = make("pre", "", "source-editor-structural-preview");
  preview.style.whiteSpace = "pre-wrap";
  preview.style.overflowWrap = "anywhere";
  preview.setAttribute("aria-live", "polite");
  const confirm = make("button", "Apply previewed source change", "source-editor-confirm");
  const cancel = make("button", "Discard preview", "source-editor-cancel");
  for (const button of [insert, remove, confirm, cancel]) button.type = "button";
  const message = make("p", "", "source-editor-message");
  message.setAttribute("role", "status");
  panelEl.hidden = false;

  function syncControls() {
    undo.disabled = disposed || !history.canUndo;
    redo.disabled = disposed || !history.canRedo;
    saveSource.disabled = disposed;
    saveSvg.disabled = !current() || renderedSvg === null;
    const editable = current() && selection !== null && activeEdit === null;
    for (const control of [apply, snippet, insert, remove, insertText]) control.disabled = !editable;
    // A single-span mutation would invalidate the staged batch. Require an explicit clear
    // rather than silently throwing away replacements the user already prepared.
    for (const control of [apply, insert, remove]) control.disabled ||= staged.size > 0;
    confirm.disabled = !editable || preparedEdit === null;
    cancel.disabled = disposed;
    search.disabled = !current();
    kindFilter.disabled = !current();
    previousMatch.disabled = nextMatch.disabled = !current() || matchedBindings.length === 0;
    const composing = sourceComposing || replacementComposing;
    stage.disabled = !editable || composing || !stagedCurrent();
    stage.textContent = staged.has(selection?.elementId) ? "Update staged replacement" : "Stage replacement for batch";
    batchPrepare.disabled = !current() || !stagedCurrent() || !staged.size || activeEdit !== null || composing;
    batchClear.disabled = disposed || (!staged.size && activeEdit?.kind !== "batch" && !preparedBatch);
    batchApply.disabled = !current() || !stagedCurrent() || !preparedBatch || activeEdit !== null || composing;
    batchApply.hidden = !preparedBatch;
    batchCancel.hidden = !preparedBatch && activeEdit?.kind !== "batch";
    batchCancel.disabled = disposed;
    batchCancel.textContent = activeEdit?.kind === "batch" ? "Cancel batch preparation" : "Discard batch preview";
  }

  function stagedCurrent() {
    return !staged.size || (current() && session === stagedSession && stagedEpoch === epoch);
  }

  function clearBatchPreview() {
    if (activeEdit?.kind === "batch") {
      activeEdit = null;
      backend.cancel();
      outEl.setAttribute("aria-busy", "false");
    }
    preparedBatch = null;
    session?.cancelBatch();
    batchPreview.textContent = "";
    batchPreview.hidden = true;
    batchApply.hidden = true;
    batchCancel.hidden = true;
  }

  function rebuildStaged() {
    batchList.replaceChildren();
    const live = stagedCurrent();
    for (const edit of staged.values()) {
      const item = make("li", "", "", batchList);
      const name = edit.sourceId || edit.elementId;
      const choose = make("button", `Edit staged replacement for ${name}`, "", item);
      choose.type = "button";
      choose.setAttribute("data-batch-select", edit.elementId);
      choose.disabled = disposed || !live;
      const detail = make("pre", `From: ${JSON.stringify(edit.snippet)}\nTo: ${JSON.stringify(edit.replacement)}`, "", item);
      detail.style.whiteSpace = "pre-wrap";
      detail.style.overflowWrap = "anywhere";
      const unstage = make("button", `Unstage ${name}`, "", item);
      unstage.type = "button";
      unstage.setAttribute("data-batch-remove", edit.elementId);
      unstage.disabled = disposed;
    }
    batchStatus.textContent = !staged.size ? "No staged edits." : live
      ? `${staged.size} staged edits. Source is unchanged until the batch is applied.`
      : `Source revision changed. ${staged.size} staged replacements are retained for copying only; clear them and stage again. They cannot be applied to the new source.`;
    for (const [id, element] of elements) {
      if (live && staged.has(id)) element.setAttribute("data-source-staged", "true");
      else element.removeAttribute("data-source-staged");
    }
    syncControls();
  }

  function clearStaged() {
    clearBatchPreview();
    staged = new Map();
    stagedSession = null;
    batchRevision += 1;
    rebuildStaged();
  }

  listen(stage, "click", () => {
    if (!current() || !selection || activeEdit || sourceComposing || replacementComposing) return;
    if (!stagedCurrent()) {
      message.textContent = "Staged replacements belong to an older source revision. Clear them before staging new edits.";
      return;
    }
    clearStructural();
    clearBatchPreview();
    try {
      const proposed = new Map(staged);
      if (snippet.value === selection.snippet) proposed.delete(selection.elementId);
      else proposed.set(selection.elementId, Object.freeze({ elementId: selection.elementId,
        sourceId: selection.sourceId, snippet: selection.snippet, replacement: snippet.value }));
      // Read-only preflight before changing the queue. Reject overlapping node/edge aliases
      // even when their different element IDs make them look like independent selections.
      planSourceBatch(session.source, new Map(session.bindings.map((binding) => [binding.elementId, binding])), [...proposed.values()]);
      staged = proposed;
      stagedSession = session;
      stagedEpoch = epoch;
      batchRevision += 1;
      rebuildStaged();
      message.textContent = `${staged.size} source edits staged. Select another element or preview the batch. Source is unchanged.`;
    } catch (error) {
      message.textContent = `Cannot stage replacement: ${error.message || error}`;
      syncControls();
    }
  });
  listen(batchList, "click", (event) => {
    const unstage = event.target.closest?.("[data-batch-remove]");
    const choose = event.target.closest?.("[data-batch-select]");
    if (disposed) return;
    if (unstage && batchList.contains(unstage)) {
      clearBatchPreview();
      staged.delete(unstage.getAttribute("data-batch-remove"));
      batchRevision += 1;
      rebuildStaged();
    } else if (choose && batchList.contains(choose) && stagedCurrent()) {
      const id = choose.getAttribute("data-batch-select");
      if (staged.has(id)) {
        select(id);
        snippet.value = staged.get(id).replacement;
        snippet.focus();
      }
    }
  });
  listen(batchClear, "click", () => {
    if (disposed) return;
    clearStaged();
    message.textContent = "Staged edits cleared. Source is unchanged.";
  });
  listen(batchCancel, "click", () => {
    if (disposed) return;
    clearBatchPreview();
    syncControls();
    message.textContent = "Batch preview discarded. Staged replacements remain; source is unchanged.";
  });
  listen(batchPrepare, "click", async () => {
    if (!current() || !stagedCurrent() || !staged.size || activeEdit || sourceComposing || replacementComposing) return;
    clearStructural();
    clearBatchPreview();
    const operation = { kind: "batch", version: epoch, revision: batchRevision, session, source: sourceEl.value };
    let started = false;
    const live = () => current() && operation.version === epoch && operation.revision === batchRevision &&
      operation.session === session && operation.source === sourceEl.value && activeEdit === operation;
    try {
      const transaction = session.beginBatch([...staged.values()]);
      activeEdit = operation;
      started = true;
      syncControls();
      outEl.setAttribute("aria-busy", "true");
      message.textContent = `Preparing ${staged.size} source edits — ${backend.target}…`;
      const response = await backend.batchSource(transaction.source, transaction.edits);
      if (!live()) return;
      const change = session.prepareBatch(transaction, response);
      preparedBatch = { operation, change };
      batchPreview.textContent = change.changes.map((edit, index) =>
        `${index + 1}. ${edit.sourceId || edit.elementId} — UTF-8 bytes ${edit.startByte}..${edit.endByte}\n` +
        `Remove: ${JSON.stringify(edit.previousSnippet)}\nInsert: ${JSON.stringify(edit.replacement)}`
      ).join("\n\n") + (change.warnings.length ? `\n\nParser warnings: ${change.warnings.join(" ")}` : "");
      batchPreview.hidden = false;
      message.textContent = `Review all ${change.changes.length} source changes, then apply or discard the batch. Source is unchanged.`;
    } catch (error) {
      if (live() || (!started && current() && operation.version === epoch && operation.revision === batchRevision)) {
        message.textContent = `Cannot prepare batch: ${error.message || error}`;
      }
    } finally {
      if (activeEdit === operation) {
        activeEdit = null;
        outEl.setAttribute("aria-busy", "false");
        syncControls();
      }
    }
  });
  listen(batchApply, "click", () => {
    const prepared = preparedBatch;
    const operation = prepared?.operation;
    if (!prepared || !current() || !stagedCurrent() || activeEdit || sourceComposing || replacementComposing ||
        operation.version !== epoch || operation.revision !== batchRevision ||
        operation.session !== session || operation.source !== sourceEl.value) {
      clearBatchPreview();
      rebuildStaged();
      if (!disposed) message.textContent = "Batch preview is stale; prepare the edits again.";
      return;
    }
    try {
      sourceEl.value = session.commitBatch(prepared.change);
      history.record(sourceEl.value); // Exactly one history entry for the entire transaction.
      clearStaged();
      invalidate();
      message.textContent = "Batch applied. One Undo restores the entire previous source.";
      onChange();
    } catch (error) {
      if (!disposed) message.textContent = `Cannot apply batch: ${error.message || error}`;
    }
  });

  function syncTabStops() {
    const id = matchedBindings.find((binding) => binding.elementId === selection?.elementId && elements.has(binding.elementId))?.elementId ??
      matchedBindings.find((binding) => elements.has(binding.elementId))?.elementId;
    for (const [elementId, element] of elements) element.setAttribute("tabindex", elementId === id ? "0" : "-1");
  }
  function rebuildMatches() {
    matchedBindings = current() ? searchSourceBindings(session.bindings, search.value, kindFilter.value, renderedLabels) : [];
    chooser.replaceChildren();
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = matchedBindings.length ? "Select an element…" : "No matching elements";
    chooser.append(placeholder);
    for (const binding of matchedBindings) {
      const option = document.createElement("option");
      option.value = binding.elementId;
      const text = (renderedLabels.get(binding.elementId) || binding.snippet || "").replace(/\s+/gu, " ").trim();
      option.textContent = `${binding.kind}: ${binding.sourceId || binding.elementId} — ${text.slice(0, 100)}`;
      chooser.append(option);
    }
    chooser.value = selection?.elementId || "";
    chooser.disabled = !matchedBindings.length;
    searchStatus.textContent = current() ? `${matchedBindings.length} of ${session.bindings.length} source elements match.` : "Render the current source to navigate.";
    syncTabStops();
    syncControls();
  }

  function navigateTo(elementId, focusDiagram = false) {
    if (!current()) return;
    select(elementId, false);
    // Retain diagram/search focus during navigation, but keep the source selection paired.
    sourceEl.setSelectionRange(selection.start, selection.end);
    const element = elements.get(elementId);
    if (element) {
      if (focusDiagram) element.focus({ preventScroll: true });
      element.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
    const index = matchedBindings.findIndex((binding) => binding.elementId === elementId);
    searchStatus.textContent = `Match ${index + 1} of ${matchedBindings.length}: ${selection.kind} ${selection.sourceId || elementId}.` +
      (element ? "" : " This source span has no addressable SVG element.");
  }
  function cycleMatch(delta) {
    if (!current() || !matchedBindings.length) return;
    const index = matchedBindings.findIndex((binding) => binding.elementId === selection?.elementId);
    const next = index < 0 ? (delta < 0 ? matchedBindings.length - 1 : 0) :
      (index + delta + matchedBindings.length) % matchedBindings.length;
    navigateTo(matchedBindings[next].elementId);
  }
  listen(search, "input", () => { clearSelection(); rebuildMatches(); });
  listen(kindFilter, "change", () => { clearSelection(); rebuildMatches(); });
  listen(previousMatch, "click", () => cycleMatch(-1));
  listen(nextMatch, "click", () => cycleMatch(1));
  listen(search, "keydown", (event) => {
    if (event.isComposing || event.ctrlKey || event.metaKey || event.altKey || event.key !== "Enter") return;
    event.preventDefault();
    cycleMatch(event.shiftKey ? -1 : 1);
  });

  function clearStructural() {
    preparedEdit = null;
    session?.cancelStructural();
    preview.textContent = "";
    preview.hidden = true;
    confirm.hidden = true;
    cancel.hidden = true;
    confirm.disabled = true;
  }

  function clearSelection() {
    clearStructural();
    if (activeEdit && activeEdit.kind !== "batch") {
      activeEdit = null;
      backend.cancel();
      outEl.setAttribute("aria-busy", "false");
    }
    for (const element of elements.values()) element.removeAttribute("data-source-selected");
    selection = null;
    chooser.value = "";
    snippet.value = "";
    snippet.disabled = true;
    apply.disabled = true;
    syncTabStops();
    syncControls();
  }
  function invalidate() {
    epoch += 1;
    backend.cancel();
    clearBatchPreview();
    displayedSource = null;
    renderedSvg = null;
    clearSelection();
    chooser.replaceChildren();
    chooser.disabled = true;
    for (const [element, tabindex] of originalTabIndices) {
      element.removeAttribute("data-source-editable");
      element.removeAttribute("data-source-staged");
      if (tabindex === null) element.removeAttribute("tabindex");
      else element.setAttribute("tabindex", tabindex);
    }
    originalTabIndices = new Map();
    elements = new Map();
    renderedLabels = new Map();
    matchedBindings = [];
    searchStatus.textContent = "Render the current source to navigate.";
    // Keep obsolete drafts visible for recovery, but never revive them when source text returns
    // to an earlier value. A new render is a new revision even when its bytes are identical.
    rebuildStaged();
    syncControls();
  }
  function current() {
    return !disposed && displayedSource !== null && sourceEl.value === displayedSource;
  }
  function select(elementId, selectText = true) {
    if (!current()) return;
    if (selection?.elementId === elementId) {
      if (selectText) {
        sourceEl.focus({ preventScroll: true });
        sourceEl.setSelectionRange(selection.start, selection.end);
      }
      return;
    }
    clearSelection();
    selection = session.select(elementId);
    chooser.value = elementId;
    snippet.value = stagedCurrent() && staged.has(elementId) ? staged.get(elementId).replacement : selection.snippet;
    snippet.disabled = false;
    apply.disabled = false;
    syncControls();
    elements.get(elementId)?.setAttribute("data-source-selected", "true");
    syncTabStops();
    if (selectText) {
      sourceEl.focus({ preventScroll: true });
      sourceEl.setSelectionRange(selection.start, selection.end);
    }
    message.textContent = `Editing ${selection.kind} ${selection.sourceId || elementId}. Only the shown source span will be replaced.`;
  }
  function elementFor(target) {
    for (let element = target; element && element !== outEl; element = element.parentElement) {
      if (elements.get(element.id) === element) return element.id;
    }
    return null;
  }
  listen(outEl, "click", (event) => {
    // The old SVG may remain visible after a failed render. Its links must not navigate away
    // from unsaved edits just because that stale preview no longer has live source bindings.
    if (event.target.closest?.("a")) event.preventDefault();
    const id = elementFor(event.target);
    if (!id || !current()) return;
    event.preventDefault(); // In edit mode a node link selects source instead of navigating away.
    event.stopPropagation();
    select(id);
  }, true);
  listen(outEl, "keydown", (event) => {
    if (event.isComposing || event.ctrlKey || event.metaKey || event.altKey ||
        event.target.closest?.("input,textarea,select,[contenteditable]:not([contenteditable=false])")) return;
    const from = elementFor(event.target);
    if (from && current() && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) {
      const kind = session.bindings.find((binding) => binding.elementId === from)?.kind;
      const candidates = matchedBindings.filter((binding) => binding.kind === kind).flatMap((binding) => {
        const element = elements.get(binding.elementId);
        if (!element || document.defaultView.getComputedStyle(element).visibility === "hidden") return [];
        const rect = element.getBoundingClientRect();
        return rect.width > 0 || rect.height > 0 ? [{ elementId: binding.elementId, rect }] : [];
      });
      const next = event.key === "Home" ? candidates[0]?.elementId : event.key === "End" ? candidates.at(-1)?.elementId :
        directionalSourceBinding(candidates, from, event.key);
      event.preventDefault();
      event.stopPropagation();
      if (next) navigateTo(next, true);
      return;
    }
    if (event.key !== "Enter" && event.key !== " ") return;
    const id = elementFor(event.target);
    if (!id || !current()) return;
    event.preventDefault();
    event.stopPropagation();
    select(id);
  }, true);
  listen(chooser, "change", () => {
    if (chooser.value) select(chooser.value);
    else clearSelection();
  });
  listen(sourceEl, "select", () => {
    if (!current()) return;
    // Several bindings can share one statement. A programmatic range change must not
    // replace an explicitly selected B with the first binding A for that same range.
    if (selection?.start === sourceEl.selectionStart && selection?.end === sourceEl.selectionEnd) return;
    const binding = session.bindingAt(sourceEl.selectionStart, sourceEl.selectionEnd);
    if (binding) select(binding.elementId, false);
    else clearSelection();
  });
  listen(sourceEl, "input", () => {
    history.record(sourceEl.value);
    // Hosts normally call render on input. Also invalidate when this editor is mounted alone;
    // do not cancel a render the host already started (its displayedSource is already null).
    if (displayedSource !== null && sourceEl.value !== displayedSource) {
      invalidate();
      outEl.setAttribute("aria-busy", "false");
      message.textContent = "Source changed; render again before editing a span.";
    }
  });
  function discardPreview() {
    clearBatchPreview();
    if (activeEdit?.kind) {
      activeEdit = null;
      backend.cancel();
      outEl.setAttribute("aria-busy", "false");
    }
    clearStructural();
    syncControls();
  }
  listen(insertText, "input", discardPreview);
  listen(snippet, "input", discardPreview);
  for (const [element, source] of [[sourceEl, true], [snippet, false]]) {
    listen(element, "compositionstart", () => {
      if (source) sourceComposing = true;
      else replacementComposing = true;
      discardPreview();
    });
    listen(element, "compositionend", () => {
      if (source) sourceComposing = false;
      else replacementComposing = false;
      syncControls();
    });
  }
  listen(cancel, "click", () => {
    discardPreview();
    message.textContent = "Preview discarded. Source is unchanged.";
  });
  async function prepareStructural(kind) {
    if (!current() || !selection || activeEdit || staged.size) return;
    clearStructural();
    const operation = { kind, version: epoch, source: sourceEl.value, selection,
      text: kind === "insert" ? insertText.value : "" };
    const live = () => current() && operation.version === epoch && operation.source === sourceEl.value &&
      operation.selection === selection && activeEdit === operation &&
      (kind !== "insert" || operation.text === insertText.value);
    try {
      activeEdit = operation;
      syncControls();
      outEl.setAttribute("aria-busy", "true");
      message.textContent = `Preparing ${kind === "insert" ? "insertion" : "deletion"} — ${backend.target}…`;
      const response = kind === "insert"
        ? await backend.insertSource(operation.source, selection.elementId, operation.text)
        : await backend.deleteSource(operation.source, selection.elementId);
      if (!live()) return;
      const change = session.prepareStructural(selection, kind, operation.text, response);
      preparedEdit = { operation, change };
      // JSON escaping makes CRLF, tabs and trailing newlines visible, and never injects HTML.
      preview.textContent = `UTF-8 bytes ${change.startByte}..${change.endByte}\n` +
        `Remove: ${JSON.stringify(change.previousSnippet)}\nInsert: ${JSON.stringify(change.replacement)}` +
        (change.warnings.length ? `\nParser warnings: ${change.warnings.join(" ")}` : "");
      preview.hidden = false;
      confirm.hidden = false;
      cancel.hidden = false;
      message.textContent = "Review the exact source change, then apply or discard it. Source is unchanged.";
    } catch (error) {
      if (live()) message.textContent = `Cannot prepare source change: ${error.message || error}`;
    } finally {
      if (activeEdit === operation) {
        activeEdit = null;
        outEl.setAttribute("aria-busy", "false");
        syncControls();
      }
    }
  }
  listen(insert, "click", () => prepareStructural("insert"));
  listen(remove, "click", () => prepareStructural("delete"));
  listen(confirm, "click", () => {
    const prepared = preparedEdit;
    const operation = prepared?.operation;
    if (!prepared || !current() || activeEdit || operation.version !== epoch ||
        operation.selection !== selection || operation.source !== sourceEl.value ||
        (operation.kind === "insert" && operation.text !== insertText.value)) {
      discardPreview();
      if (!disposed) message.textContent = "Preview is stale; prepare the change again.";
      return;
    }
    try {
      sourceEl.value = session.commitStructural(prepared.change);
      history.record(sourceEl.value);
      invalidate();
      message.textContent = "Source change applied. Undo restores the previous source exactly.";
      onChange();
    } catch (error) {
      if (!disposed) message.textContent = String(error.message || error);
    }
  });
  listen(apply, "click", async () => {
    if (!current() || !selection) {
      if (!disposed) message.textContent = "Source changed; select an element from the new preview.";
      return;
    }
    if (activeEdit || staged.size) return;
    clearStructural();
    const operation = { version: epoch, source: sourceEl.value, selection, replacement: snippet.value };
    const live = () => !disposed && operation.version === epoch && operation.source === sourceEl.value &&
      operation.selection === selection && activeEdit === operation && snippet.value === operation.replacement;
    try {
      activeEdit = operation;
      syncControls();
      apply.disabled = true;
      snippet.disabled = true;
      outEl.setAttribute("aria-busy", "true");
      const response = await backend.editSource(operation.source, selection.elementId, operation.replacement);
      if (!live()) return;
      sourceEl.value = session.replaceWithResponse(operation.selection, operation.replacement, response);
      history.record(sourceEl.value);
      invalidate();
      onChange();
    } catch (error) {
      if (live()) message.textContent = String(error.message || error);
    } finally {
      if (activeEdit === operation) {
        activeEdit = null;
        syncControls();
        outEl.setAttribute("aria-busy", "false");
      }
    }
  });
  function restore(direction) {
    if (disposed) return;
    // Catch programmatic changes too: an undo must never silently discard an unseen document.
    history.record(sourceEl.value);
    sourceEl.value = direction === "undo" ? history.undo() : history.redo();
    invalidate();
    onChange();
  }
  listen(undo, "click", () => restore("undo"));
  listen(redo, "click", () => restore("redo"));
  function historyKey(event) {
    if (disposed || event.isComposing || event.altKey || !(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    const direction = key === "z" ? (event.shiftKey ? "redo" : "undo") : key === "y" ? "redo" : null;
    if (!direction) return;
    event.preventDefault();
    restore(direction);
  }
  // Leave the replacement textarea's native undo stack alone: it is an uncommitted draft.
  listen(sourceEl, "keydown", historyKey);
  listen(outEl, "keydown", historyKey);
  function exportSource() {
    if (disposed) throw new Error("The source editor is closed.");
    byteOffsets(sourceEl.value); // A Blob must not silently replace malformed UTF-16 on export.
    return { text: sourceEl.value, filename: "diagram.mmd", mime: "text/plain;charset=utf-8" };
  }
  function exportSvg() {
    if (!current() || renderedSvg === null) throw new Error("Wait for a successful render of the current source before saving SVG.");
    // Preserve the engine's exact bytes, not DOM serialization with editor-only attributes.
    return { text: renderedSvg, filename: "diagram.svg", mime: "image/svg+xml;charset=utf-8" };
  }
  for (const [button, artifact] of [[saveSource, exportSource], [saveSvg, exportSvg]]) {
    listen(button, "click", async () => {
      try { await (saveFile || download)(artifact()); }
      catch (error) {
        if (!disposed) message.textContent = `Save failed: ${error.message || error}`;
      }
    });
  }
  invalidate();

  return {
    exportSource,
    exportSvg,
    async render(source) {
      if (disposed || source !== sourceEl.value) return;
      // Record edits before any asynchronous work: rapid typing and failed renders are undoable.
      history.record(source);
      invalidate();
      const version = epoch;
      message.textContent = `Building SVG and source map — ${backend.target}…`;
      outEl.setAttribute("aria-busy", "true");
      const live = () => !disposed && version === epoch && sourceEl.value === source;
      try {
        const { snapshot, svg } = await backend.renderSource(source);
        if (!live()) return;
        if (typeof svg !== "string") throw new Error("The renderer did not return SVG.");
        const candidate = new SourceEditSession();
        candidate.setSnapshot(source, snapshot);
        session = candidate;
        outEl.innerHTML = svg; // Only the renderer's sanitized SVG, never source or snippets.
        const bindings = new Map(session.bindings.map((binding) => [binding.elementId, binding]));
        for (const element of outEl.querySelectorAll("[id]")) {
          if (!bindings.has(element.id)) continue;
          elements.set(element.id, element);
          originalTabIndices.set(element, element.getAttribute("tabindex"));
          element.setAttribute("data-source-editable", "true");
          renderedLabels.set(element.id, element.textContent);
        }
        displayedSource = source;
        renderedSvg = svg;
        rebuildMatches();
        const warnings = snapshot.parsed?.warnings || [];
        message.textContent = `${bindings.size} editable source spans — ${backend.target}. ${warnings.join(" ")}` +
          (backend.fallbackReason ? ` Worker unavailable: ${backend.fallbackReason}` : "");
      } catch (error) {
        if (live()) message.textContent = `Source editor unavailable: ${error.message || error}`;
      } finally {
        if (live()) outEl.setAttribute("aria-busy", "false");
      }
    },
    dispose() {
      disposed = true;
      invalidate();
      backend.dispose();
      for (const remove of listeners) remove();
      const host = document.defaultView;
      for (const [url, timer] of downloads) {
        host.clearTimeout(timer);
        host.URL.revokeObjectURL(url);
      }
      downloads.clear();
      outEl.setAttribute("aria-busy", "false");
    },
  };
}
