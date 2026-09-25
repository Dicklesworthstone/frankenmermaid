// Dedicated render worker for frankenmermaid (bd-2u0.6, scope items 1-4).
//
// The wasm module owns the protocol AND the scheduling. `workerHandleMessage` takes one
// `WorkerRenderMessage` as JSON text and returns a `WorkerRenderResponse` as JSON text, or null when
// the message needs no reply — a cancel, or a stale request id that is no longer the live one.
//
// THIS HOST IS DELIBERATELY A PIPE. An earlier draft of this file tracked "is a render running" in
// JS and called a separate scheduler; that would be a second state machine beside the Rust
// coordinator, and the two would disagree precisely under fast typing, which is the case the feature
// exists for. Supersession, cancellation and staleness are decided in one place, and it is not here.
//
// JSON text on both sides is what lets the same payload be used from the main thread, from this
// worker, and from a native Rust test.
//
// Source authoring has additional host messages (bd-1t7l.1): `sourceRender` returns
// SVG plus its matching ParseLens bindings; `sourceEdit` returns a Rust-applied source edit.
// `sourceDeck` returns the SVG and deck manifest from ONE renderDeck invocation (bd-z7g6k).
// These use the existing WASM exports, not a second parser. Their pre-execution queue is
// separate from the Rust render protocol, which has no source-map/edit response variant.

let wasm = null;
let modulePromise = null;
let offscreenDiagram = null;
let pendingOffscreenRender = null;
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

async function handleSourceOperation(message) {
  const { kind, requestId, input } = message;
  if (!Number.isSafeInteger(requestId) || requestId < 0 || typeof input !== "string" ||
      (kind === "sourceEdit" &&
       (typeof message.elementId !== "string" || !message.elementId || typeof message.replacement !== "string"))) {
    throw new Error("source operations require a non-negative safe integer requestId, string input, and an elementId/replacement for edits");
  }
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

    if (kind === "sourceDeck") {
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
      const response = wasm.applyParseLensEdit(input, message.elementId, message.replacement);
      if (typeof response?.result?.updatedSource !== "string") {
        throw new Error("ParseLens did not return an edited source");
      }
      self.postMessage({
        kind: "sourceEdited", requestId,
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

// A render is scheduled as a macrotask so a `cancel` posted mid-render is actually delivered.
// Without the yield the worker sits inside one synchronous render and only observes the
// cancellation after the work it was meant to abandon has already finished.
function yieldToMessages() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function isOffscreenRenderRequest(message) {
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

self.onmessage = async (event) => {
  const message = event.data || {};

  try {
    if (message.kind === "sourceRender" || message.kind === "sourceEdit" || message.kind === "sourceDeck") {
      await handleSourceOperation(message);
      return;
    }
    if (message.kind === "cancel" && pendingSourceOperation?.requestId === message.requestId) {
      releaseSourceOperation();
      return;
    }
    if (message.kind === "init") {
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
        deckRendering: typeof wasm.renderDeck === "function",
        ...(fallbackReason ? { fallbackReason } : {}),
      });
      return;
    }

    await ensureModule(message.moduleUrl);

    if (offscreenDiagram) {
      if (message.kind === "cancel") {
        if (pendingOffscreenRender && pendingOffscreenRender.requestId === message.requestId) {
          pendingOffscreenRender.cancelled = true;
          pendingOffscreenRender = null;
        }
        self.postMessage({ kind: "noReply", requestId: message.requestId });
        return;
      }
      if (!isOffscreenRenderRequest(message)) {
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

    await yieldToMessages();

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
  } catch (error) {
    // Never fail silently: from the UI a dead worker is indistinguishable from a slow one.
    self.postMessage({
      kind: "failed",
      requestId: message.requestId,
      reason: String((error && error.message) || error),
    });
  }
};
