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
  let committedMessage;
  let elementRegistry = new Map();
  let selectedNodes = [];
  let sourceEditor;
  let nextEditId = 0;

  function highlight(nodes) {
    for (const node of selectedNodes) node.removeAttribute("aria-current");
    selectedNodes = [...new Set(nodes)];
    for (const node of selectedNodes) node.setAttribute("aria-current", "true");
  }

  const errorText = (error) => error instanceof Error ? error.message : String(error);
  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  }

  function invalidateSourceEditor() {
    if (!sourceEditor || sourceEditor.applied) return;
    sourceEditor.stale = true;
    sourceEditor.apply.disabled = true;
    sourceEditor.notice.textContent = "Source changed. This draft is kept for copying, but cannot be applied. Discard it and select a current element to edit again.";
  }

  function openSourceEditor(message, diagram, binding, current) {
    if (disposed || current !== revision || committedMessage !== message) return;
    if (sourceEditor && (sourceEditor.pending || (!sourceEditor.applied
      && sourceEditor.input.value !== sourceEditor.originalInput))) {
      sourceEditor.notice.textContent = "An unapplied draft is already open. Apply it or discard it before editing another fragment.";
      sourceEditor.input.focus();
      return;
    }
    if (typeof engine.applyParseLensEdit !== "function" || typeof binding.snippet !== "string") {
      status.textContent = "This engine build does not provide editable source fragments.";
      return;
    }
    sourceEditor?.panel.remove();
    const panel = element("section");
    const input = element("textarea");
    input.setAttribute("aria-label", "Source fragment replacement");
    input.spellcheck = false;
    input.value = binding.snippet;
    const apply = element("button", "Apply source edit");
    const cancel = element("button", "Discard draft");
    const notice = element("p"); notice.setAttribute("role", "status");
    panel.append(element("h2", `Edit source fragment — diagram ${diagram.id + 1}`),
      element("p", "This is the complete engine-owned source fragment, not a label rename. A statement may include multiple nodes and edges. Applying makes one undoable editor change; it does not save the file."),
      input, apply, cancel, notice);
    const draft = { panel, input, apply, cancel, notice, message, diagram, binding,
      originalInput: input.value, stale: false, pending: false, applied: false };
    sourceEditor = draft;
    cancel.addEventListener("click", () => {
      if (draft.pending || sourceEditor !== draft) return;
      panel.remove(); sourceEditor = undefined;
    });
    apply.addEventListener("click", () => {
      if (disposed || sourceEditor !== draft || draft.pending || draft.applied || draft.stale) return;
      if (committedMessage !== message || current !== revision) { invalidateSourceEditor(); return; }
      try {
        // Textarea.value uses LF. Match the engine input's convention before calling Rust;
        // the host separately restores Markdown's stripped container indent and native EOL.
        const eol = diagram.source.match(/\r\n|\n|\r/u)?.[0] || "\n";
        const replacement = input.value.replace(/\r\n|\n|\r/gu, eol);
        if (new TextEncoder().encode(replacement).length > 2 * 1024 * 1024) throw new Error("Replacement exceeds the 2 MiB limit.");
        for (const character of replacement) {
          const code = character.codePointAt(0);
          if (code >= 0xd800 && code <= 0xdfff) throw new Error("Replacement contains an unpaired surrogate.");
        }
        const response = engine.applyParseLensEdit(diagram.source, binding.elementId, replacement);
        if (!response?.result || !Array.isArray(response.snapshot?.bindings)) throw new Error("Engine did not return a complete source-edit receipt.");
        draft.editId = ++nextEditId;
        draft.pending = true;
        apply.disabled = true; cancel.disabled = true;
        notice.textContent = "Applying through the source editor…";
        vscode.postMessage({ type: "apply-source-edit", requestId: message.requestId,
          documentVersion: message.documentVersion, diagramId: diagram.id, elementId: binding.elementId,
          editId: draft.editId, replacement, result: response.result });
      } catch (error) {
        draft.pending = false;
        apply.disabled = false; cancel.disabled = false;
        notice.textContent = `Edit not applied: ${errorText(error)}`;
      }
    });
    root.append(panel);
    input.focus();
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
    // Capture the sanitized rendered artifact BEFORE adding preview-only nonces, focus state or
    // source navigation roles. Exports never re-render and never serialize the decorated preview.
    const exportSvg = new window.XMLSerializer().serializeToString(svgElement);
    for (const style of svgElement.querySelectorAll("style")) style.setAttribute("nonce", styleNonce);
    const host = element("div");
    // Every SVG keeps its original source-bound IDs. Separate tree scopes prevent one diagram's
    // markers/gradients/CSS from binding to identically named definitions in another diagram.
    const shadow = host.attachShadow({ mode: "open" });
    const style = element("style", "svg{display:block;max-width:100%;height:auto} [aria-current=true]{filter:drop-shadow(0 0 3px var(--vscode-focusBorder,#06f))} [role=button]:focus-visible{outline:2px solid var(--vscode-focusBorder,#06f)}");
    style.setAttribute("nonce", styleNonce);
    shadow.append(style, svgElement);
    // A preview must not navigate its webview when an authored `click` link is activated.
    svgElement.addEventListener("click", (event) => event.preventDefault());
    return { host, svgElement, svg: exportSvg };
  }

  function sourceInsight(source) {
    const lens = typeof engine.parseLens === "function" ? engine.parseLens(source) : undefined;
    const parsed = lens?.parsed || (typeof engine.parse === "function" ? engine.parse(source) : undefined);
    const diagnostics = [];
    const seen = new Set();
    const add = (value) => {
      const message = typeof value.message === "string" ? value.message.slice(0, 16000) : "";
      if (!message) return;
      const key = `${message}:${value.span?.start?.line || 0}`;
      if (seen.has(key)) return;
      seen.add(key);
      if (diagnostics.length < 500) diagnostics.push({ message, span: value.span,
        severity: String(value.severity || "warning").toLowerCase(), suggestion: value.suggestion });
    };
    for (const diagnostic of parsed?.ir?.diagnostics || []) add(diagnostic);
    for (const warning of parsed?.warnings || []) {
      if (typeof warning === "string" && !diagnostics.some((diagnostic) => diagnostic.message === warning)) {
        add({ message: warning, severity: "warning" });
      }
    }
    if (seen.size > diagnostics.length) diagnostics.push({ message: "Additional diagnostics were omitted (500-item limit).", severity: "info" });
    if (!lens) add({ message: "This engine build has no source bindings; element navigation is unavailable.", severity: "info" });
    return { bindings: Array.isArray(lens?.bindings) ? lens.bindings : [], diagnostics };
  }

  function sourceControls(card, view, insight, message, diagram, current, registry) {
    const send = (detail) => {
      if (!disposed && current === revision) vscode.postMessage({ type: "reveal", requestId: message.requestId,
        documentVersion: message.documentVersion, diagramId: diagram.id, ...detail });
    };
    const tools = element("div");
    const showSource = element("button", "Show source");
    showSource.addEventListener("click", () => send({}));
    tools.append(showSource);
    card.append(tools);
    if (view) {
      for (const [action, label] of [["save", "Save SVG"], ["copy", "Copy SVG"]]) {
        const button = element("button", label);
        button.addEventListener("click", () => {
          if (!disposed && current === revision) vscode.postMessage({ type: "export-svg", action,
            requestId: message.requestId, documentVersion: message.documentVersion,
            diagramId: diagram.id, svg: view.svg });
        });
        tools.append(button);
      }
      const bindings = new Map(insight.bindings.filter((binding) => binding?.textRange)
        .map((binding) => [binding.elementId, binding]));
      const nodes = new Map();
      registry.set(diagram.id, nodes);
      let selectedBinding;
      const edit = element("button", "Edit selected source fragment");
      edit.disabled = true;
      edit.addEventListener("click", () => {
        if (selectedBinding) openSourceEditor(message, diagram, selectedBinding, current);
      });
      tools.append(edit);
      for (const node of view.svgElement.querySelectorAll("[id]")) {
        const binding = bindings.get(node.id);
        if (!binding) continue;
        nodes.set(binding.elementId, node);
        node.setAttribute("role", "button");
        node.setAttribute("tabindex", "0");
        node.setAttribute("aria-label", `Show source for ${String(binding.sourceId || binding.elementId).slice(0, 200)}`);
        const select = (event) => {
          event.preventDefault(); event.stopPropagation();
          if (disposed || current !== revision) return;
          highlight([node]);
          selectedBinding = binding;
          edit.disabled = typeof engine.applyParseLensEdit !== "function" || typeof binding.snippet !== "string";
          send({ elementId: binding.elementId });
        };
        node.addEventListener("click", select);
        node.addEventListener("keydown", (event) => {
          if (event.key === "Enter" || event.key === " ") select(event);
        });
      }
    }
    if (insight.diagnostics.length) {
      const details = element("details");
      details.open = insight.diagnostics.some((diagnostic) => diagnostic.severity === "error");
      details.append(element("summary", `${insight.diagnostics.length} engine diagnostic(s)`));
      const list = element("ul");
      insight.diagnostics.forEach((diagnostic, index) => {
        const item = element("li");
        const link = element("button", `${diagnostic.severity}: ${diagnostic.message}`);
        link.addEventListener("click", () => send({ diagnosticIndex: index }));
        item.append(link);
        if (diagnostic.suggestion) item.append(element("p", String(diagnostic.suggestion)));
        list.append(item);
      });
      details.append(list); card.append(details);
    }
  }

  async function render(message) {
    if (!engine || disposed || message.requestId <= latestRequestId) return;
    latestRequestId = message.requestId;
    const current = ++revision;
    invalidateSourceEditor();
    committedMessage = undefined;
    elementRegistry = new Map();
    root.setAttribute("aria-busy", "true");
    status.textContent = "Rendering…";
    const cards = [];
    const reports = [];
    const registry = new Map();
    let failed = 0;
    for (const diagram of message.diagrams) {
      if (disposed || current !== revision) return;
      const card = element("section");
      card.append(element("h2", `${message.title} — diagram ${diagram.id + 1} (line ${diagram.startLine + 1})`));
      let view;
      let insight = { bindings: [], diagnostics: [] };
      // Failure of optional inspection must never suppress an otherwise renderable diagram.
      try { insight = sourceInsight(diagram.source); }
      catch (error) { insight.diagnostics.push({ severity: "warning", message: `Source inspection failed: ${errorText(error)}` }); }
      try {
        const svg = engine.renderSvg(diagram.source);
        view = svgView(svg);
        card.append(view.host);
      } catch (error) {
        failed += 1;
        card.append(element("p", `Unable to render this diagram: ${errorText(error)}`, "error"));
        insight.diagnostics.push({ severity: "error", message: `Rendering failed: ${errorText(error)}` });
      }
      sourceControls(card, view, insight, message, diagram, current, registry);
      reports.push({ id: diagram.id, ...insight });
      cards.push(card);
      // Yield between diagrams so newer edits/disposal can cancel the unfinished document render.
      await yieldToHost();
    }
    if (disposed || current !== revision) return;
    root.replaceChildren(...cards, ...(sourceEditor ? [sourceEditor.panel] : []));
    highlight([]);
    committedMessage = message;
    elementRegistry = registry;
    root.setAttribute("aria-busy", "false");
    status.textContent = cards.length === 0 ? "No Mermaid code fences found in this document."
      : `${cards.length} diagram${cards.length === 1 ? "" : "s"}${failed ? `; ${failed} could not render` : ""}.`;
    vscode.postMessage({ type: "rendered", requestId: message.requestId,
      documentVersion: message.documentVersion, reports });
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
    if (message?.type === "source-edit-result" && sourceEditor?.pending
      && message.editId === sourceEditor.editId && message.requestId === sourceEditor.message.requestId
      && message.documentVersion === sourceEditor.message.documentVersion && typeof message.ok === "boolean"
      && typeof message.message === "string") {
      sourceEditor.pending = false;
      sourceEditor.cancel.disabled = false;
      sourceEditor.applied = message.ok;
      sourceEditor.input.readOnly = message.ok;
      sourceEditor.apply.disabled = message.ok || sourceEditor.stale;
      sourceEditor.notice.textContent = message.message;
    } else if (message && committedMessage && message.requestId === committedMessage.requestId
      && message.documentVersion === committedMessage.documentVersion && message.type === "select-source"
      && Array.isArray(message.targets) && message.targets.length <= 256
      && message.targets.every((target) => Number.isSafeInteger(target?.diagramId) && typeof target.elementId === "string")) {
      highlight(message.targets.map((target) => elementRegistry.get(target.diagramId)?.get(target.elementId)).filter(Boolean));
    } else if (message?.type === "export-complete" && committedMessage
      && message.requestId === committedMessage.requestId && message.documentVersion === committedMessage.documentVersion) {
      status.textContent = message.action === "save" ? "SVG saved." : "SVG copied to clipboard.";
    } else if (message?.type === "preview-error" && Number.isSafeInteger(message.requestId)
      && message.requestId > latestRequestId && typeof message.message === "string") {
      latestRequestId = message.requestId;
      revision += 1;
      invalidateSourceEditor();
      committedMessage = undefined;
      elementRegistry = new Map();
      highlight([]);
      root.replaceChildren(...(sourceEditor ? [sourceEditor.panel] : []));
      root.setAttribute("aria-busy", "false");
      status.textContent = message.message;
    } else if (isRenderMessage(message)) {
      void render(message);
    }
  }

  function dispose() {
    disposed = true;
    revision += 1;
    committedMessage = undefined;
    elementRegistry = new Map();
    highlight([]);
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
