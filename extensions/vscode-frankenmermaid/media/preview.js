/* The engine owns Mermaid semantics. This controller owns only webview lifecycle and presentation.
 * Export the same controller to Node tests; no source rewriting or alternate render path. */
function createPreviewController({ document, window, vscode,
  loadEngine = (url) => import(/* @vite-ignore */ url),
  yieldToHost = () => new Promise((resolve) => setTimeout(resolve, 0)),
}) {
  const root = document.getElementById("preview");
  const status = document.getElementById("status");
  const { wasmModule, wasmBinary, styleNonce } = document.body.dataset;
  if (!root || !status) throw new Error("FrankenMermaid preview roots are missing");
  let engine;
  let disposed = false;
  let starting = false;
  let revision = 0;
  let latestRequestId = -1;

  const errorText = (error) => error instanceof Error ? error.message : String(error);
  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  }

  async function start() {
    if (disposed || starting || engine) return;
    starting = true;
    status.textContent = "Loading FrankenMermaid…";
    try {
      const wasm = await loadEngine(wasmModule);
      if (disposed) return;
      await wasm.default({ module_or_path: wasmBinary });
      if (disposed) return;
      if (typeof wasm.renderSvg !== "function") throw new Error("Engine has no renderSvg export.");
      engine = wasm;
      root.replaceChildren();
      status.textContent = "Waiting for document…";
      vscode.postMessage({ type: "ready" });
    } catch (error) {
      if (disposed) return;
      status.textContent = `Unable to initialize FrankenMermaid: ${errorText(error)}`;
      const retry = element("button", "Retry engine initialization");
      retry.addEventListener("click", () => void start());
      root.replaceChildren(retry);
    } finally {
      starting = false;
    }
  }

  function svgView(svg) {
    if (typeof svg !== "string") throw new Error("Engine returned non-text SVG.");
    const parsed = new window.DOMParser().parseFromString(svg, "image/svg+xml");
    if (parsed.querySelector("parsererror") || parsed.documentElement.localName !== "svg"
      || parsed.documentElement.namespaceURI !== "http://www.w3.org/2000/svg") {
      throw new Error("Engine returned invalid SVG.");
    }
    const svgElement = document.importNode(parsed.documentElement, true);
    // Script-free even if a renderer bug ever permits author text into active markup.
    for (const script of svgElement.querySelectorAll("script")) script.remove();
    for (const node of [svgElement, ...svgElement.querySelectorAll("*")]) {
      for (const attribute of [...node.attributes]) {
        if (/^on/iu.test(attribute.name)) node.removeAttribute(attribute.name);
      }
    }
    for (const style of svgElement.querySelectorAll("style")) style.setAttribute("nonce", styleNonce);
    const host = element("div");
    // Every SVG keeps its original source-bound IDs. Separate tree scopes prevent one diagram's
    // markers/gradients/CSS from binding to identically named definitions in another diagram.
    const shadow = host.attachShadow({ mode: "open" });
    const style = element("style", "svg{display:block;max-width:100%;height:auto}");
    style.setAttribute("nonce", styleNonce);
    shadow.append(style, svgElement);
    // A preview must not navigate its webview when an authored `click` link is activated.
    svgElement.addEventListener("click", (event) => event.preventDefault());
    return { host, svgElement };
  }

  async function render(message) {
    if (!engine || disposed || message.requestId <= latestRequestId) return;
    latestRequestId = message.requestId;
    const current = ++revision;
    root.setAttribute("aria-busy", "true");
    status.textContent = "Rendering…";
    const cards = [];
    let failed = 0;
    for (const diagram of message.diagrams) {
      if (disposed || current !== revision) return;
      const card = element("section");
      card.append(element("h2", `${message.title} — diagram ${diagram.id + 1} (line ${diagram.startLine + 1})`));
      try {
        const svg = engine.renderSvg(diagram.source);
        const view = svgView(svg);
        card.append(view.host);
      } catch (error) {
        failed += 1;
        card.append(element("p", `Unable to render this diagram: ${errorText(error)}`, "error"));
      }
      cards.push(card);
      // Yield between diagrams so newer edits/disposal can cancel the unfinished document render.
      await yieldToHost();
    }
    if (disposed || current !== revision) return;
    root.replaceChildren(...cards);
    root.setAttribute("aria-busy", "false");
    status.textContent = cards.length === 0 ? "No Mermaid code fences found in this document."
      : `${cards.length} diagram${cards.length === 1 ? "" : "s"}${failed ? `; ${failed} could not render` : ""}.`;
  }

  function isRenderMessage(message) {
    return message !== null && typeof message === "object" && message.type === "render"
      && Number.isSafeInteger(message.requestId) && message.requestId >= 0
      && Number.isSafeInteger(message.documentVersion) && message.documentVersion >= 0
      && typeof message.title === "string" && Array.isArray(message.diagrams)
      && message.diagrams.length <= 64
      && message.diagrams.every((diagram, index) => diagram !== null && typeof diagram === "object"
        && diagram.id === index && typeof diagram.source === "string"
        && diagram.source.length <= 2 * 1024 * 1024
        && Number.isSafeInteger(diagram.startLine) && diagram.startLine >= 0);
  }

  function onMessage(event) {
    const message = event.data;
    if (disposed) return;
    if (message?.type === "preview-error" && Number.isSafeInteger(message.requestId)
      && message.requestId > latestRequestId && typeof message.message === "string") {
      latestRequestId = message.requestId;
      revision += 1;
      root.replaceChildren();
      root.setAttribute("aria-busy", "false");
      status.textContent = message.message;
    } else if (isRenderMessage(message)) {
      void render(message);
    }
  }

  function dispose() {
    disposed = true;
    revision += 1;
    window.removeEventListener("message", onMessage);
    window.removeEventListener("pagehide", dispose);
  }
  window.addEventListener("message", onMessage);
  window.addEventListener("pagehide", dispose, { once: true });
  return { start, dispose };
}

if (typeof module === "object" && module.exports) {
  module.exports = { createPreviewController };
} else {
  void createPreviewController({ document, window, vscode: acquireVsCodeApi() }).start();
}
