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

let wasm = null;
let offscreenDiagram = null;
let pendingOffscreenRequestId = null;
const cancelledOffscreenRequestIds = new Set();

async function ensureModule(moduleUrl) {
  if (wasm) return wasm;
  wasm = await import(moduleUrl);
  if (wasm.default) await wasm.default();
  return wasm;
}

// A render is scheduled as a macrotask so a `cancel` posted mid-render is actually delivered.
// Without the yield the worker sits inside one synchronous render and only observes the
// cancellation after the work it was meant to abandon has already finished.
function yieldToMessages() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function isOffscreenRenderRequest(message) {
  return message.kind === "render"
    && Number.isSafeInteger(message.requestId)
    && typeof message.input === "string";
}

async function renderOffscreenIfStillLive(message) {
  const { requestId } = message;
  pendingOffscreenRequestId = requestId;

  // `Diagram.render` is synchronous, so once it starts a later postMessage cannot interrupt it.
  // Yielding BEFORE that call is still load-bearing: rapid typing can replace or cancel this queued
  // request before it draws stale pixels into the transferred canvas.
  await yieldToMessages();
  if (pendingOffscreenRequestId !== requestId) {
    if (cancelledOffscreenRequestIds.delete(requestId)) return;
    self.postMessage({ kind: "noReply", requestId });
    return;
  }

  const stats = offscreenDiagram.render(
    message.input,
    message.configJson ? JSON.parse(message.configJson) : undefined,
  );
  if (pendingOffscreenRequestId !== requestId) {
    self.postMessage({ kind: "noReply", requestId });
    return;
  }
  pendingOffscreenRequestId = null;
  self.postMessage({ kind: "completed", requestId, target: "offscreenInWorker", stats });
}

self.onmessage = async (event) => {
  const message = event.data || {};

  try {
    if (message.kind === "init") {
      await ensureModule(message.moduleUrl);

      // THE DECISION IS MADE IN RUST, not here. `chooseCanvasTarget` is the same function the
      // native tests cover; re-deriving the ladder in JavaScript would drift from it, and the drift
      // would only appear in degraded environments — the ones nobody tests in.
      const capabilities = message.capabilities || {
        offscreenCanvas: false,
        worker: true,
        canvasTransferred: false,
      };
      const decision = JSON.parse(wasm.chooseCanvasTarget(JSON.stringify(capabilities)));

      if (decision.target === "offscreenInWorker" && message.canvas) {
        // The canvas was transferred by the page, so pixels never cross postMessage again.
        offscreenDiagram = wasm.Diagram.fromOffscreenCanvas(message.canvas, message.config);
      }

      // Report what was actually set up, not what was asked for: if the transfer arrived but the
      // decision said otherwise, or vice versa, the page must see the truth rather than its request
      // echoed back.
      self.postMessage({
        kind: "ready",
        requested: decision.target,
        target: offscreenDiagram ? "offscreenInWorker" : "svgInWorker",
      });
      return;
    }

    await ensureModule(message.moduleUrl);

    if (offscreenDiagram) {
      if (message.kind === "cancel") {
        if (pendingOffscreenRequestId === message.requestId) {
          pendingOffscreenRequestId = null;
          cancelledOffscreenRequestIds.add(message.requestId);
        }
        self.postMessage({ kind: "noReply", requestId: message.requestId });
        return;
      }
      if (!isOffscreenRenderRequest(message)) {
        self.postMessage({
          kind: "failed",
          requestId: message.requestId,
          reason: "offscreen render requires an integer requestId and string input",
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
    const responseJson = wasm.workerHandleMessage(JSON.stringify(message));
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
