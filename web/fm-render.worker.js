// Dedicated render worker for frankenmermaid (bd-2u0.6, scope items 1-4).
//
// The wasm module owns the protocol AND the scheduling. `workerHandleMessage` takes one
// `WorkerRenderMessage` as JSON text and returns a `WorkerRenderResponse` as JSON text, or null when
// the message needs no reply — a cancel, or a stale request id that is no longer the live one.
//
// Rust owns the render protocol and execution state. The host only coalesces requests that
// have NOT entered synchronous WASM yet. Yielding alone is insufficient: several queued messages
// can each resume and do expensive parse/layout/render work before Rust has seen the newer ID.
// The pre-execution gates below discard obsolete work; they do not run a second Rust scheduler,
// synthesize completed renders, or claim to interrupt a render once synchronous WASM has started.
//
// JSON text on both sides is what lets the same payload be used from the main thread, from this
// worker, and from a native Rust test.
//
// Source authoring has additional host messages (bd-1t7l.1): `sourceRender` returns
// SVG plus its matching ParseLens bindings; `sourceEdit` returns a Rust-applied source edit.
// `sourceDelete` and `sourceInsert` use Rust's formatting-preserving structural edit APIs;
// their replies carry the actual changed range AND the newly parsed source bindings.
// `sourceBatch` applies disjoint source-span replacements as ONE cancellable transaction. Its
// `edits: [{ elementId, replacement }]` all address the original input snapshot. The sole success
// reply is `sourceBatchEdited`, with `response.result.{updatedSource,changes}` and fresh bindings.
// Changes are reported in request order; intermediate sources are never posted. A caller must
// still check its document revision before adopting the result, just as for a single sourceEdit.
// Shared/overlapping node/edge statement spans are rejected, not treated as semantic node edits.
// `sourceDeck` returns the SVG and deck manifest from ONE renderDeck invocation (bd-z7g6k).
// These use the existing WASM exports, not a second parser. Their pre-execution queue is
// separate from the Rust render protocol, which has no source-map/edit response variant.

let wasm = null;
let modulePromise = null;
let offscreenDiagram = null;
let pendingOffscreenRender = null;
let pendingSvgRender = null;
// Identity, not a numeric counter: an init creates a new rendering session even while the
// shared module import is pending. Work from an earlier session must not acquire the new target.
let renderSession = {};
let initConfigJson;
let pendingSourceOperation = null;

function releaseSourceOperation() {
  if (!pendingSourceOperation) return;
  const { requestId } = pendingSourceOperation;
  pendingSourceOperation = null;
  self.postMessage({ kind: "noReply", requestId });
}

// The editor needs bindings and warnings, not a second copy of the entire IR across the
// worker boundary. Preserve the Rust-computed ranges and snippets verbatim.
function sourceSnapshot(snapshot) {
  if (!Array.isArray(snapshot?.bindings)) throw new Error("ParseLens did not return source bindings");
  return { bindings: snapshot.bindings, parsed: { warnings: snapshot.parsed?.warnings || [] } };
}

// Bound transaction bookkeeping; yield between individual Rust edits so a large batch remains
// cancellable. This does not claim to interrupt any synchronous parse/edit already in progress.
const MAX_SOURCE_BATCH_EDITS = 1024;

function sourceByteOffsets(source) {
  const offsets = new Map([[0, 0]]);
  let bytes = 0;
  let units = 0;
  for (const character of source) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) {
      throw new Error("Source batch contains an unpaired surrogate; UTF-8 conversion would change it.");
    }
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    units += character.length;
    offsets.set(bytes, units);
  }
  return offsets;
}

function validateSourceBatch(edits) {
  if (!Array.isArray(edits) || edits.length > MAX_SOURCE_BATCH_EDITS) {
    throw new Error(`sourceBatch requires an edits array of at most ${MAX_SOURCE_BATCH_EDITS} replacements`);
  }
  const ids = new Set();
  for (const edit of edits) {
    if (!edit || typeof edit.elementId !== "string" || !edit.elementId ||
        typeof edit.replacement !== "string") {
      throw new Error("Each sourceBatch edit requires an elementId and string replacement");
    }
    if (ids.has(edit.elementId)) throw new Error(`Duplicate sourceBatch elementId: ${edit.elementId}`);
    ids.add(edit.elementId);
    sourceByteOffsets(edit.replacement);
  }
}

// Validate engine ranges against the exact source at every step. These are UTF-8 byte ranges,
// whereas JS slicing uses UTF-16 code units; neither numeric equality nor a guessed label is safe.
function sourceBatchBindings(source, snapshot) {
  if (!Array.isArray(snapshot?.bindings)) throw new Error("ParseLens did not return source bindings");
  const offsets = sourceByteOffsets(source);
  const bindings = new Map();
  for (const binding of snapshot.bindings) {
    if (binding.textRange == null) continue;
    const { startByte, endByte } = binding.textRange;
    const start = offsets.get(startByte);
    const end = offsets.get(endByte);
    if (typeof binding.elementId !== "string" || !binding.elementId ||
        !Number.isSafeInteger(startByte) || !Number.isSafeInteger(endByte) ||
        start === undefined || end === undefined || start > end) {
      throw new Error("Source batch received an invalid UTF-8 source range");
    }
    const snippet = source.slice(start, end);
    if (binding.snippet != null && binding.snippet !== snippet) {
      throw new Error("Source batch bindings do not match the source");
    }
    if (bindings.has(binding.elementId)) throw new Error("Duplicate source-map element ID");
    bindings.set(binding.elementId, { elementId: binding.elementId, sourceId: binding.sourceId,
      kind: binding.kind, startByte, endByte, start, end, snippet });
  }
  return bindings;
}

async function applySourceBatch(input, edits, isLive) {
  if (typeof wasm.parseLens !== "function" || typeof wasm.applyParseLensEdit !== "function") {
    throw new Error("This WASM build lacks source-editing APIs; rebuild the shipped package.");
  }
  sourceByteOffsets(input);
  let snapshot = wasm.parseLens(input);
  let bindings = sourceBatchBindings(input, snapshot);
  const plan = edits.map((edit, index) => {
    const binding = bindings.get(edit.elementId);
    if (!binding) throw new Error(`No editable source span for ${edit.elementId}`);
    return { ...binding, replacement: edit.replacement, index };
  }).sort((a, b) => b.startByte - a.startByte || b.endByte - a.endByte);
  for (let index = 1; index < plan.length; index += 1) {
    const later = plan[index - 1];
    const earlier = plan[index];
    if (earlier.endByte > later.startByte || earlier.startByte === later.startByte) {
      throw new Error(`Overlapping source spans: ${earlier.elementId} and ${later.elementId}`);
    }
  }
  let updatedSource = input;
  const changes = [];
  const checkLive = () => {
    if (!isLive()) {
      const error = new Error("Source batch superseded or cancelled");
      error.name = "AbortError";
      throw error;
    }
  };
  for (const edit of plan) {
    await yieldToMessages();
    checkLive();
    if (edit.replacement === edit.snippet) continue;

    // Right-to-left application keeps every remaining original range at its original offset.
    // IDs may nevertheless change after a reparse: rebind ONLY by the same source identity,
    // kind, exact range and snippet. A changed/ambiguous binding aborts rather than guessing.
    const matches = (binding) => binding.startByte === edit.startByte &&
      binding.endByte === edit.endByte && binding.snippet === edit.snippet &&
      binding.sourceId === edit.sourceId && binding.kind === edit.kind;
    const originalId = bindings.get(edit.elementId);
    const candidates = originalId && matches(originalId) ? [originalId] :
      [...bindings.values()].filter(matches);
    if (candidates.length !== 1) {
      throw new Error(`Source binding changed or became ambiguous during batch: ${edit.elementId}`);
    }
    const binding = candidates[0];
    const response = wasm.applyParseLensEdit(updatedSource, binding.elementId, edit.replacement);
    const result = response?.result;
    const expected = updatedSource.slice(0, edit.start) + edit.replacement + updatedSource.slice(edit.end);
    if (result?.updatedSource !== expected || result.elementId !== binding.elementId ||
        result.replacement !== edit.replacement || result.previousSnippet !== edit.snippet ||
        result.replacedRange?.startByte !== edit.startByte || result.replacedRange?.endByte !== edit.endByte) {
      throw new Error("Source batch edit changed text outside its selected span or returned an invalid transaction");
    }
    const nextBindings = sourceBatchBindings(expected, response.snapshot);
    changes.push({ index: edit.index, elementId: edit.elementId, appliedElementId: binding.elementId,
      replacedRange: { startByte: edit.startByte, endByte: edit.endByte },
      previousSnippet: edit.snippet, replacement: edit.replacement });
    updatedSource = expected;
    snapshot = response.snapshot;
    bindings = nextBindings;
  }
  checkLive();
  changes.sort((a, b) => a.index - b.index);
  return { result: { updatedSource, changes: changes.map(({ index, ...change }) => change) }, snapshot };
}

async function handleSourceOperation(message) {
  const { kind, requestId, input } = message;
  const editing = kind === "sourceEdit" || kind === "sourceDelete" || kind === "sourceInsert";
  if (!Number.isSafeInteger(requestId) || requestId < 0 || typeof input !== "string" ||
      (editing && (typeof message.elementId !== "string" || !message.elementId)) ||
      (kind === "sourceEdit" && typeof message.replacement !== "string") ||
      (kind === "sourceInsert" && typeof message.text !== "string")) {
    throw new Error("source operations require a non-negative safe integer requestId, string input, an elementId for edits, and string replacement/text for replacement/insertion");
  }
  // Malformed batches must not evict otherwise valid queued work.
  if (kind === "sourceBatch") validateSourceBatch(message.edits);
  // Claim at message arrival, BEFORE module loading yields. A burst during a cold import
  // retains just the newest operation. Object identity also handles cancellation + ID reuse.
  releaseSourceOperation();
  const operation = { requestId };
  pendingSourceOperation = operation;
  try {
    await ensureModule(message.moduleUrl);
    if (pendingSourceOperation !== operation) return;
    await yieldToMessages();
    if (pendingSourceOperation !== operation) return;

    if (kind === "sourceBatch") {
      const response = await applySourceBatch(input, message.edits,
        () => pendingSourceOperation === operation);
      if (pendingSourceOperation !== operation) return;
      self.postMessage({ kind: "sourceBatchEdited", requestId,
        response: { result: response.result, snapshot: sourceSnapshot(response.snapshot) } });
    } else if (kind === "sourceDeck") {
      if (typeof wasm.renderDeck !== "function") {
        throw new Error("This WASM build lacks graph-deck rendering; rebuild the shipped package.");
      }
      const configJson = message.configJson ?? initConfigJson;
      const config = configJson == null ? undefined : JSON.parse(configJson);
      // Do not independently call renderSvg or build a manifest from ParseLens bindings: the
      // deck's element IDs and camera geometry belong to the layout that produced THIS SVG.
      const deck = wasm.renderDeck(input, config);
      if (typeof deck?.svg !== "string" || !Array.isArray(deck.warnings) ||
          (deck.manifest != null && (typeof deck.manifest !== "object" || Array.isArray(deck.manifest)))) {
        throw new Error("The renderer did not return a graph-deck result");
      }
      self.postMessage({ kind: "sourceDeckRendered", requestId,
        svg: deck.svg, manifest: deck.manifest ?? null, warnings: deck.warnings });
    } else if (kind === "sourceRender") {
      const configJson = message.configJson ?? initConfigJson;
      const config = configJson == null ? undefined : JSON.parse(configJson);
      const snapshot = sourceSnapshot(wasm.parseLens(input));
      const svg = wasm.renderSvg(input, config);
      if (typeof svg !== "string") throw new Error("The renderer did not return SVG");
      self.postMessage({ kind: "sourceRendered", requestId, svg, snapshot });
    } else {
      const exportName = kind === "sourceDelete" ? "applyParseLensDelete" :
        kind === "sourceInsert" ? "applyParseLensInsertLineAfter" : "applyParseLensEdit";
      if (typeof wasm[exportName] !== "function") {
        throw new Error(`This WASM build lacks ${exportName}; rebuild the shipped package.`);
      }
      const response = kind === "sourceDelete" ? wasm[exportName](input, message.elementId) :
        wasm[exportName](input, message.elementId, kind === "sourceInsert" ? message.text : message.replacement);
      if (typeof response?.result?.updatedSource !== "string") {
        throw new Error("ParseLens did not return an edited source");
      }
      self.postMessage({
        kind: kind === "sourceDelete" ? "sourceDeleted" : kind === "sourceInsert" ? "sourceInserted" : "sourceEdited", requestId,
        response: { result: response.result, snapshot: sourceSnapshot(response.snapshot) },
      });
    }
  } catch (error) {
    // Superseded import failures must not create a second terminal reply for an old request.
    if (pendingSourceOperation === operation) throw error;
  } finally {
    if (pendingSourceOperation === operation) pendingSourceOperation = null;
  }
}

async function ensureModule(moduleUrl) {
  if (wasm) return wasm;
  // onmessage handlers overlap while imports and WASM initialization await I/O. Share the
  // entire operation, not just the imported namespace: its exports are unusable until default()
  // completes. Publish only a ready module, and let a failed attempt be retried.
  if (!modulePromise) {
    modulePromise = (async () => {
      const module = !moduleUrl || moduleUrl === "../pkg/frankenmermaid.js"
        ? await import("../pkg/frankenmermaid.js")
        : await import(/* @vite-ignore */ `${moduleUrl}`);
      if (module.default) await module.default();
      wasm = module;
      return module;
    })().catch((error) => {
      modulePromise = null;
      throw error;
    });
  }
  return modulePromise;
}

// Yield BEFORE synchronous WASM so already-queued edits/cancels can invalidate this request.
// A message arriving after synchronous rendering begins cannot interrupt that render.
function yieldToMessages() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function isRenderRequest(message) {
  return (
    message.kind === "render" &&
    Number.isSafeInteger(message.requestId) &&
    message.requestId >= 0 &&
    typeof message.input === "string"
  );
}

async function renderOffscreenIfStillLive(message) {
  const { requestId } = message;
  // Use operation identity, not just a caller's id. Reusing an id after cancellation must not
  // revive the old input, and reinitializing must not draw old work into a replacement canvas.
  const pending = { requestId, cancelled: false, diagram: offscreenDiagram };
  pendingOffscreenRender = pending;

  // `Diagram.render` is synchronous, so once it starts a later postMessage cannot interrupt it.
  // Yielding BEFORE that call is still load-bearing: rapid typing can replace or cancel this queued
  // request before it draws stale pixels into the transferred canvas.
  await yieldToMessages();
  if (pendingOffscreenRender !== pending) {
    if (pending.cancelled) return;
    self.postMessage({ kind: "noReply", requestId });
    return;
  }

  try {
    const stats = pending.diagram.render(
      message.input,
      message.configJson ? JSON.parse(message.configJson) : undefined,
    );
    self.postMessage({ kind: "completed", requestId, target: "offscreenInWorker", stats });
  } finally {
    if (pendingOffscreenRender === pending) pendingOffscreenRender = null;
  }
}

function forwardSvgMessage(message) {
  // `null` means the module decided this message needs no reply — a cancel, or a superseded id.
  // Forwarding a synthetic response here would tell the UI a render finished when none did.
  // Preserve initialization configuration on SVG fallback, while explicit per-render config
  // remains authoritative. Do not rewrite cancel messages or parse the Rust response in JS.
  const request = message.kind === "render" && message.configJson == null && initConfigJson !== undefined
    ? { ...message, configJson: initConfigJson }
    : message;
  const responseJson = wasm.workerHandleMessage(JSON.stringify(request));
  if (responseJson === null || responseJson === undefined) {
    self.postMessage({ kind: "noReply", requestId: message.requestId });
    return;
  }

  // Forwarded verbatim: the response already carries timings and the parse diagnostics the CLI
  // shows (scope item 4), and re-deriving any of it here would only lose fidelity.
  self.postMessage(JSON.parse(responseJson));
}

function releaseSvgRender() {
  if (!pendingSvgRender) return;
  const { requestId } = pendingSvgRender;
  pendingSvgRender = null;
  self.postMessage({ kind: "noReply", requestId });
}

async function renderSvgIfStillLive(message) {
  const { requestId } = message;
  // Preserve monotonic request ordering among queued renders without replacing Rust's own
  // stale-ID checks for requests that have already executed. Equal IDs use operation identity.
  if (pendingSvgRender && requestId < pendingSvgRender.requestId) {
    self.postMessage({ kind: "noReply", requestId });
    return;
  }
  releaseSvgRender();
  const pending = { requestId };
  pendingSvgRender = pending;
  try {
    await yieldToMessages();
    if (pendingSvgRender !== pending) return;
    forwardSvgMessage(message);
  } finally {
    if (pendingSvgRender === pending) pendingSvgRender = null;
  }
}

self.onmessage = async (event) => {
  const message = event.data || {};
  const sessionAtArrival = renderSession;
  const validRender = isRenderRequest(message);

  try {
    if (["sourceRender", "sourceEdit", "sourceDelete", "sourceInsert", "sourceDeck", "sourceBatch"].includes(message.kind)) {
      await handleSourceOperation(message);
      return;
    }
    if (message.kind === "cancel" && pendingSourceOperation && pendingSourceOperation.requestId === message.requestId) {
      releaseSourceOperation();
      return;
    }
    if (message.kind === "init") {
      renderSession = {};
      releaseSvgRender();
      releaseSourceOperation();
      await ensureModule(message.moduleUrl);
      initConfigJson = message.config == null ? undefined : JSON.stringify(message.config);
      pendingOffscreenRender = null;
      const previousDiagram = offscreenDiagram;
      offscreenDiagram = null;
      if (previousDiagram) previousDiagram.free();

      // THE DECISION IS MADE IN RUST, not here. `chooseCanvasTarget` is the same function the
      // native tests cover; re-deriving the ladder in JavaScript would drift from it, and the drift
      // would only appear in degraded environments — the ones nobody tests in.
      const capabilities = message.capabilities || {
        offscreenCanvas: false,
        worker: true,
        canvasTransferred: false,
      };
      const decision = JSON.parse(wasm.chooseCanvasTarget(JSON.stringify(capabilities)));

      let fallbackReason;
      if (decision.target === "offscreenInWorker" && message.canvas) {
        // The canvas was transferred by the page, so pixels never cross postMessage again.
        try {
          offscreenDiagram = wasm.Diagram.fromOffscreenCanvas(message.canvas, message.config);
        } catch (error) {
          // A transferable canvas does not guarantee an available 2D context. Keep parse/layout
          // off the UI thread when only canvas setup failed; the initialized SVG engine is usable.
          fallbackReason = `offscreen canvas initialization failed: ${String((error && error.message) || error)}`;
        }
      }

      // Report what was actually set up, not what was asked for: if the transfer arrived but the
      // decision said otherwise, or vice versa, the page must see the truth rather than its request
      // echoed back.
      self.postMessage({
        kind: "ready",
        requested: decision.target,
        target: offscreenDiagram ? "offscreenInWorker" : "svgInWorker",
        sourceEditing: ["parseLens", "renderSvg", "applyParseLensEdit"].every((name) => typeof wasm[name] === "function"),
        sourceBatchEditing: ["parseLens", "applyParseLensEdit"].every((name) => typeof wasm[name] === "function"),
        sourceDeletion: typeof wasm.applyParseLensDelete === "function",
        sourceInsertion: typeof wasm.applyParseLensInsertLineAfter === "function",
        deckRendering: typeof wasm.renderDeck === "function",
        ...(fallbackReason ? { fallbackReason } : {}),
      });
      return;
    }

    await ensureModule(message.moduleUrl);
    // Capture at message arrival, not after import: an old request can otherwise resume
    // alongside a newer init and render its source into that init's replacement canvas/config.
    if (validRender && sessionAtArrival !== renderSession) {
      self.postMessage({ kind: "noReply", requestId: message.requestId });
      return;
    }

    if (offscreenDiagram) {
      if (message.kind === "cancel") {
        if (pendingOffscreenRender && pendingOffscreenRender.requestId === message.requestId) {
          pendingOffscreenRender.cancelled = true;
          pendingOffscreenRender = null;
        }
        self.postMessage({ kind: "noReply", requestId: message.requestId });
        return;
      }
      if (!isRenderRequest(message)) {
        self.postMessage({
          kind: "failed",
          requestId: message.requestId,
          reason: "offscreen render requires a non-negative safe integer requestId and string input",
        });
        return;
      }
      // The Rust coordinator owns the SVG path. The offscreen renderer cannot use it because its
      // output is canvas pixels, so this tiny pre-render gate is the sole host state: it prevents
      // a queued obsolete request from entering synchronous canvas rendering.
      await renderOffscreenIfStillLive(message);
      return;
    }

    if (message.kind === "cancel" && pendingSvgRender && pendingSvgRender.requestId === message.requestId) {
      releaseSvgRender();
      return;
    }
    if (isRenderRequest(message)) {
      await renderSvgIfStillLive(message);
      return;
    }

    // Leave invalid/unknown messages and unmatched cancels to Rust's protocol validation.
    // In particular, a malformed render must not evict an otherwise valid queued render.
    await yieldToMessages();
    forwardSvgMessage(message);
  } catch (error) {
    if (validRender && sessionAtArrival !== renderSession) {
      // A rejected shared import still settles work invalidated by a newer init as obsolete.
      // The init/current-session request reports the actual failure and can be retried.
      self.postMessage({ kind: "noReply", requestId: message.requestId });
      return;
    }
    // Never fail silently: from the UI a dead worker is indistinguishable from a slow one.
    self.postMessage({
      kind: "failed",
      requestId: message.requestId,
      reason: String((error && error.message) || error),
    });
  }
};
