/* The engine owns Mermaid semantics. This controller owns only webview lifecycle and presentation.
 * Export the same controller to Node tests; no source rewriting or alternate render path. */
function createPreviewController({ document, window, vscode,
  loadEngine = (moduleUrl, binaryUrl, workerUrl) => window.FmPreviewEngineWorker.create({ moduleUrl, binaryUrl, workerUrl }),
  yieldToHost = () => new Promise((resolve) => setTimeout(resolve, 0)),
}) {
  const root = document.getElementById("preview");
  const status = document.getElementById("status");
  const control = (id) => {
    const node = document.getElementById(id);
    return node?.localName === "button" ? node : undefined;
  };
  const stopButton = control("stop-render"), retryButton = control("retry-render");
  const { wasmModule, wasmBinary, engineWorker, styleNonce } = document.body.dataset;
  if (!root || !status) throw new Error("FrankenMermaid preview roots are missing");
  let engine;
  let initializingEngine;
  let disposed = false;
  let starting = false;
  let revision = 0;
  let latestRequestId = -1;
  let committedMessage;
  let elementRegistry = new Map();
  let selectedNodes = [];
  let sourceEditor;
  let stagedBatch;
  let nextEditId = 0;
  let rendering = false;
  let retryRequested = false;
  const editMethods = { replace: "applyParseLensEdit", "insert-after": "applyParseLensInsertLineAfter", delete: "applyParseLensDelete" };

  function updateControls() {
    if (stopButton) stopButton.disabled = disposed || !rendering;
    if (retryButton) retryButton.disabled = disposed || !engine || latestRequestId < 0
      || rendering || retryRequested || Boolean(sourceEditor?.pending || stagedBatch?.pending);
  }

  function invalidateView() {
    revision += 1;
    engine?.cancelPending?.();
    invalidateSourceEditor();
    committedMessage = undefined;
    elementRegistry = new Map();
    highlight([]);
    rendering = false;
    retryRequested = false;
    root.setAttribute("aria-busy", "false");
    updateControls();
  }

  function stop() {
    if (disposed || !rendering) return;
    invalidateView();
    status.textContent = "Rendering stopped. Retry preview to render the current source.";
  }

  function retry() {
    if (disposed || !engine || latestRequestId < 0 || rendering || retryRequested
      || sourceEditor?.pending || stagedBatch?.pending) return;
    retryRequested = true;
    updateControls();
    status.textContent = "Requesting the current source…";
    vscode.postMessage({ type: "retry-render", requestId: latestRequestId });
  }

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
    if (stagedBatch && !stagedBatch.applied) {
      stagedBatch.stale = true;
      stagedBatch.apply.disabled = true;
      stagedBatch.notice.textContent = "Source changed. Staged replacements are kept for copying, but must be recreated against the current diagram before applying.";
    }
    if (!sourceEditor || sourceEditor.applied) return;
    sourceEditor.stale = true;
    sourceEditor.apply.disabled = true;
    sourceEditor.stage.disabled = true;
    sourceEditor.notice.textContent = "Source changed. This draft is kept for copying, but cannot be applied. Discard it and select a current element to edit again.";
  }

  // Worker calls yield to source changes and disposal. Freeze the submitted text and prevent
  // double submission until either the worker fails or the native host owns the transaction.
  function beginDraftComputation(draft) {
    draft.pending = true; draft.computing = true;
    draft.input.readOnly = true;
    draft.apply.disabled = true; draft.stage.disabled = true; draft.cancel.disabled = true;
    draft.notice.textContent = "Checking source edit in the engine…";
    updateControls();
  }

  function finishDraftComputation(draft) {
    if (!draft.computing) return;
    draft.computing = false; draft.pending = false;
    draft.input.readOnly = draft.applied || draft.operation === "delete";
    draft.apply.disabled = draft.stale || draft.applied;
    draft.stage.disabled = draft.stale || draft.applied;
    draft.cancel.disabled = false;
    updateControls();
  }

  function currentDraft(draft, current) {
    return !disposed && sourceEditor === draft && !draft.stale
      && committedMessage === draft.message && current === revision;
  }

  async function captureSourceEdit(draft) {
    const { diagram, binding, input } = draft;
    // Textarea.value uses LF. Match the engine input before asking Rust for the edit.
    const eol = diagram.source.match(/\r\n|\n|\r/u)?.[0] || "\n";
    const replacement = draft.operation === "delete" ? "" : input.value.replace(/\r\n|\n|\r/gu, eol);
    if (new TextEncoder().encode(replacement).length > 2 * 1024 * 1024) throw new Error("Replacement exceeds the 2 MiB limit.");
    for (const character of replacement) {
      const code = character.codePointAt(0);
      if (code >= 0xd800 && code <= 0xdfff) throw new Error("Replacement contains an unpaired surrogate.");
    }
    const method = editMethods[draft.operation];
    const response = draft.operation === "delete"
      ? await engine[method](diagram.source, binding.elementId)
      : await engine[method](diagram.source, binding.elementId, replacement);
    const result = response?.result;
    if (!result || !Array.isArray(response.snapshot?.bindings) || typeof result.updatedSource !== "string") {
      throw new Error("Engine did not return a complete source-edit receipt.");
    }
    // Retain scalar values only, never a mutable engine response/snapshot as a later write token.
    return { diagramId: diagram.id, elementId: binding.elementId, replacement,
      ...(draft.operation === "replace" ? {} : { operation: draft.operation }),
      result: { elementId: result.elementId, previousSnippet: result.previousSnippet,
        replacement: result.replacement, updatedSource: result.updatedSource,
        replacedRange: { startByte: result.replacedRange?.startByte, endByte: result.replacedRange?.endByte } } };
  }

  function refreshBatch(batch) {
    batch.title.textContent = `${batch.edits.size} staged source fragment(s)`;
    batch.apply.disabled = batch.pending || batch.stale || batch.applied || batch.edits.size === 0;
    batch.list.replaceChildren();
    for (const [key, edit] of batch.edits) {
      const row = element("details");
      const action = edit.operation === "insert-after" ? "Insert after" : edit.operation === "delete" ? "Delete" : "Replace";
      row.append(element("summary", `${action} — diagram ${edit.diagramId + 1}: ${edit.elementId}`));
      if (edit.operation === "insert-after") row.append(element("pre", `Selected source:\n${batch.anchors.get(key).snippet}`));
      row.append(element("pre", `Before:\n${edit.result.previousSnippet}\n\nAfter:\n${edit.result.replacement}`));
      const remove = element("button", "Remove staged fragment");
      remove.disabled = batch.pending || batch.applied;
      remove.addEventListener("click", () => {
        if (disposed || stagedBatch !== batch || batch.pending || batch.applied) return;
        batch.edits.delete(key); batch.anchors.delete(key); refreshBatch(batch);
      });
      row.append(remove); batch.list.append(row);
    }
  }

  async function stageSourceEdit(draft, current) {
    if (disposed || sourceEditor !== draft || draft.pending || draft.applied || draft.stale) return;
    if (committedMessage !== draft.message || current !== revision) { invalidateSourceEditor(); return; }
    try {
      if (stagedBatch?.pending) throw new Error("Staged edits are already being applied.");
      if (stagedBatch && !stagedBatch.applied && stagedBatch.message !== draft.message) {
        throw new Error("Discard the stale batch before staging edits from another render.");
      }
      beginDraftComputation(draft);
      const edit = await captureSourceEdit(draft);
      if (!currentDraft(draft, current)) return;
      if (edit.result.updatedSource === draft.diagram.source) throw new Error("This fragment has no changes to stage.");
      if (!stagedBatch || stagedBatch.applied) {
        stagedBatch?.panel.remove();
        const panel = element("section"), title = element("h2"), list = element("div");
        const apply = element("button", "Apply staged edits");
        const cancel = element("button", "Discard staged edits");
        const notice = element("p", "Review each fragment below. The entire batch applies as one undoable editor change.");
        notice.setAttribute("role", "status");
        const batch = { panel, title, list, apply, cancel, notice, message: draft.message,
          edits: new Map(), anchors: new Map(), pending: false, stale: false, applied: false };
        stagedBatch = batch;
        panel.append(title, list, apply, cancel, notice); root.append(panel);
        cancel.addEventListener("click", () => {
          if (batch.pending || stagedBatch !== batch) return;
          panel.remove(); stagedBatch = undefined;
        });
        apply.addEventListener("click", () => {
          if (disposed || stagedBatch !== batch || batch.pending || batch.stale || batch.applied || !batch.edits.size) return;
          if (committedMessage !== batch.message) { invalidateSourceEditor(); return; }
          if (sourceEditor && (sourceEditor.pending || (!sourceEditor.applied
            && (sourceEditor.operation === "delete" || sourceEditor.input.value !== sourceEditor.originalInput)))) {
            notice.textContent = "Stage or discard the open fragment draft before applying the batch."; return;
          }
          batch.editId = ++nextEditId;
          batch.pending = true; cancel.disabled = true; refreshBatch(batch);
          updateControls();
          notice.textContent = "Applying all staged fragments through the source editor…";
          vscode.postMessage({ type: "apply-source-batch", requestId: batch.message.requestId,
            documentVersion: batch.message.documentVersion, editId: batch.editId, edits: [...batch.edits.values()] });
        });
      }
      const batch = stagedBatch;
      const key = `${edit.diagramId}:${edit.elementId}`;
      if (batch.edits.has(key)) throw new Error("This fragment is already staged. Remove it from the batch before replacing it.");
      if (batch.edits.size >= 64) throw new Error("A batch supports at most 64 source fragments.");
      const range = edit.result.replacedRange;
      const overlaps = (a, b) => a.startByte === b.startByte
        || (a.startByte < b.endByte && b.startByte < a.endByte);
      for (const [previousKey, previous] of batch.edits) {
        if (previous.diagramId !== edit.diagramId) continue;
        const other = previous.result.replacedRange;
        if (overlaps(range, other)) {
          throw new Error("This fragment overlaps a staged statement. Edit the shared statement only once.");
        }
        if ((edit.operation === "insert-after" && overlaps(draft.binding.textRange, other))
          || (previous.operation === "insert-after" && overlaps(batch.anchors.get(previousKey), range))) {
          throw new Error("An insertion anchor is changed by a staged edit. Apply those changes separately.");
        }
      }
      const encoder = new TextEncoder();
      const bytes = [...batch.edits.values(), edit].reduce((sum, item) => sum + encoder.encode(JSON.stringify(item)).length, 0);
      if (bytes > 16 * 1024 * 1024) throw new Error("Staged receipts exceed the 16 MiB limit.");
      batch.edits.set(key, edit);
      batch.anchors.set(key, { startByte: draft.binding.textRange.startByte, endByte: draft.binding.textRange.endByte,
        snippet: draft.binding.snippet });
      refreshBatch(batch);
      draft.panel.remove(); sourceEditor = undefined;
      batch.notice.textContent = "Fragment staged. Select another diagram element, or apply the batch together.";
    } catch (error) {
      if (currentDraft(draft, current)) draft.notice.textContent = `Fragment not staged: ${errorText(error)}`;
    } finally { finishDraftComputation(draft); }
  }

  function openSourceEditor(message, diagram, binding, current, operation = "replace") {
    if (disposed || current !== revision || committedMessage !== message) return;
    if (stagedBatch?.pending) { stagedBatch.notice.textContent = "Staged edits are being applied; another fragment cannot be opened yet."; return; }
    if (sourceEditor && (sourceEditor.pending || (!sourceEditor.applied
      && (sourceEditor.operation === "delete" || sourceEditor.input.value !== sourceEditor.originalInput)))) {
      sourceEditor.notice.textContent = "An unapplied draft is already open. Apply it or discard it before editing another fragment.";
      sourceEditor.input.focus();
      return;
    }
    if (typeof engine[editMethods[operation]] !== "function" || typeof binding.snippet !== "string") {
      status.textContent = "This engine build does not provide editable source fragments.";
      return;
    }
    sourceEditor?.panel.remove();
    const panel = element("section");
    const input = element("textarea");
    input.setAttribute("aria-label", operation === "insert-after" ? "New source after selected line"
      : operation === "delete" ? "Source fragment to delete" : "Source fragment replacement");
    input.spellcheck = false;
    input.value = operation === "insert-after" ? "" : binding.snippet;
    input.readOnly = operation === "delete";
    const apply = element("button", operation === "insert-after" ? "Insert source"
      : operation === "delete" ? "Delete source fragment" : "Apply source edit");
    const stage = element("button", "Stage fragment");
    const cancel = element("button", "Discard draft");
    const notice = element("p"); notice.setAttribute("role", "status");
    panel.append(element("h2", `${operation === "insert-after" ? "Insert source" : operation === "delete" ? "Delete source fragment" : "Edit source fragment"} — diagram ${diagram.id + 1}`),
      element("p", operation === "insert-after"
        ? "Insert Mermaid source after the last line of the selected fragment. The engine copies its indentation onto the first new line; additional lines keep your indentation. This is one undoable editor change and does not save the file."
        : operation === "delete"
          ? "Review the complete source fragment below before deleting it. It may contain several nodes or edges. The engine also removes an otherwise empty line, but does not remove references elsewhere. Deletion is undoable and does not save the file."
          : "This is the complete engine-owned source fragment, not a label rename. A statement may include multiple nodes and edges. Applying makes one undoable editor change; it does not save the file."),
      ...(operation === "insert-after" ? [element("pre", `Selected source:\n${binding.snippet}`)] : []),
      input, apply, stage, cancel, notice);
    const draft = { panel, input, apply, stage, cancel, notice, message, diagram, binding, operation,
      originalInput: input.value, stale: false, pending: false, applied: false };
    sourceEditor = draft;
    stage.addEventListener("click", () => stageSourceEdit(draft, current));
    cancel.addEventListener("click", () => {
      if (draft.pending || sourceEditor !== draft) return;
      panel.remove(); sourceEditor = undefined;
    });
    apply.addEventListener("click", async () => {
      if (disposed || sourceEditor !== draft || draft.pending || draft.applied || draft.stale) return;
      if (committedMessage !== message || current !== revision) { invalidateSourceEditor(); return; }
      if (stagedBatch && !stagedBatch.applied && stagedBatch.edits.size) {
        notice.textContent = "Stage this fragment, then apply all staged edits together."; return;
      }
      try {
        beginDraftComputation(draft);
        const edit = await captureSourceEdit(draft);
        if (!currentDraft(draft, current)) return;
        draft.editId = ++nextEditId;
        draft.pending = true;
        input.readOnly = true;
        apply.disabled = true; stage.disabled = true; cancel.disabled = true;
        notice.textContent = "Applying through the source editor…";
        vscode.postMessage({ type: "apply-source-edit", requestId: message.requestId,
          documentVersion: message.documentVersion, editId: draft.editId, ...edit });
        draft.computing = false; // Only the native acknowledgement may now unlock this draft.
      } catch (error) {
        if (currentDraft(draft, current)) notice.textContent = `Edit not applied: ${errorText(error)}`;
      } finally { finishDraftComputation(draft); }
    });
    root.append(panel);
    input.focus();
  }

  async function start() {
    if (disposed || starting || engine) return;
    starting = true;
    status.textContent = "Loading FrankenMermaid…";
    let wasm;
    try {
      wasm = await loadEngine(wasmModule, wasmBinary, engineWorker);
      if (disposed) { wasm.destroy?.(); return; }
      initializingEngine = wasm;
      await wasm.default({ module_or_path: wasmBinary });
      if (disposed) return;
      if (typeof wasm.renderSvg !== "function") throw new Error("Engine has no renderSvg export.");
      engine = wasm;
      root.replaceChildren();
      status.textContent = "Waiting for document…";
      vscode.postMessage({ type: "ready" });
    } catch (error) {
      wasm?.destroy?.();
      if (disposed) return;
      status.textContent = `Unable to initialize FrankenMermaid: ${errorText(error)}`;
      const retry = element("button", "Retry engine initialization");
      retry.addEventListener("click", () => void start());
      root.replaceChildren(retry);
    } finally {
      initializingEngine = undefined;
      starting = false;
      updateControls();
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

  async function sourceInsight(source, current) {
    const lens = typeof engine.parseLens === "function" ? await engine.parseLens(source) : undefined;
    if (disposed || current !== revision) return { bindings: [], diagnostics: [] };
    const parsed = lens?.parsed || (typeof engine.parse === "function" ? await engine.parse(source) : undefined);
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
      const structuralButtons = [];
      for (const [operation, label] of [["insert-after", "Insert after selected source"], ["delete", "Delete selected source fragment"]]) {
        if (typeof engine[editMethods[operation]] !== "function") continue;
        const button = element("button", label);
        button.disabled = true;
        button.addEventListener("click", () => {
          if (selectedBinding) openSourceEditor(message, diagram, selectedBinding, current, operation);
        });
        structuralButtons.push(button); tools.append(button);
      }
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
          for (const button of structuralButtons) button.disabled = typeof binding.snippet !== "string";
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
    invalidateView();
    const current = revision;
    rendering = true;
    updateControls();
    root.setAttribute("aria-busy", "true");
    status.textContent = "Rendering…";
    const cards = [];
    const reports = [];
    const registry = new Map();
    let failed = 0;
    for (const diagram of message.diagrams) {
      if (disposed || current !== revision) return;
      status.textContent = `Rendering diagram ${diagram.id + 1} of ${message.diagrams.length}…`;
      const card = element("section");
      card.append(element("h2", `${message.title} — diagram ${diagram.id + 1} (line ${diagram.startLine + 1})`));
      let view;
      let insight = { bindings: [], diagnostics: [] };
      // Failure of optional inspection must never suppress an otherwise renderable diagram.
      try { insight = await sourceInsight(diagram.source, current); }
      catch (error) { insight.diagnostics.push({ severity: "warning", message: `Source inspection failed: ${errorText(error)}` }); }
      if (disposed || current !== revision) return;
      try {
        const svg = await engine.renderSvg(diagram.source);
        if (disposed || current !== revision) return;
        view = svgView(svg);
        card.append(view.host);
      } catch (error) {
        if (disposed || current !== revision) return;
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
    root.replaceChildren(...cards, ...(sourceEditor ? [sourceEditor.panel] : []), ...(stagedBatch ? [stagedBatch.panel] : []));
    highlight([]);
    committedMessage = message;
    elementRegistry = registry;
    rendering = false;
    updateControls();
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
      sourceEditor.input.readOnly = message.ok || sourceEditor.operation === "delete";
      sourceEditor.apply.disabled = message.ok || sourceEditor.stale;
      sourceEditor.stage.disabled = message.ok || sourceEditor.stale;
      sourceEditor.notice.textContent = message.message;
      updateControls();
    } else if (message?.type === "source-edit-result" && stagedBatch?.pending
      && message.editId === stagedBatch.editId && message.requestId === stagedBatch.message.requestId
      && message.documentVersion === stagedBatch.message.documentVersion && typeof message.ok === "boolean"
      && typeof message.message === "string") {
      stagedBatch.pending = false;
      stagedBatch.applied = message.ok;
      stagedBatch.cancel.disabled = false;
      stagedBatch.notice.textContent = message.message;
      refreshBatch(stagedBatch);
      updateControls();
    } else if (message && committedMessage && message.requestId === committedMessage.requestId
      && message.documentVersion === committedMessage.documentVersion && message.type === "select-source"
      && Array.isArray(message.targets) && message.targets.length <= 256
      && message.targets.every((target) => Number.isSafeInteger(target?.diagramId) && typeof target.elementId === "string")) {
      highlight(message.targets.map((target) => elementRegistry.get(target.diagramId)?.get(target.elementId)).filter(Boolean));
    } else if (message?.type === "export-complete" && committedMessage
      && message.requestId === committedMessage.requestId && message.documentVersion === committedMessage.documentVersion) {
      status.textContent = message.action === "save" ? "SVG saved." : "SVG copied to clipboard.";
    } else if (message?.type === "invalidate" && Number.isSafeInteger(message.requestId)
      && message.requestId > latestRequestId && Number.isSafeInteger(message.documentVersion)
      && message.documentVersion >= 0) {
      latestRequestId = message.requestId;
      invalidateView();
      status.textContent = "Source changed. Waiting for updated preview…";
    } else if (message?.type === "preview-error" && Number.isSafeInteger(message.requestId)
      && message.requestId > latestRequestId && typeof message.message === "string") {
      latestRequestId = message.requestId;
      invalidateView();
      root.replaceChildren(...(sourceEditor ? [sourceEditor.panel] : []), ...(stagedBatch ? [stagedBatch.panel] : []));
      root.setAttribute("aria-busy", "false");
      status.textContent = message.message;
    } else if (isRenderMessage(message)) {
      void render(message);
    }
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    revision += 1;
    engine?.destroy?.();
    initializingEngine?.destroy?.();
    committedMessage = undefined;
    elementRegistry = new Map();
    highlight([]);
    updateControls();
    stopButton?.removeEventListener("click", stop);
    retryButton?.removeEventListener("click", retry);
    window.removeEventListener("message", onMessage);
    window.removeEventListener("pagehide", dispose);
  }
  window.addEventListener("message", onMessage);
  window.addEventListener("pagehide", dispose, { once: true });
  stopButton?.addEventListener("click", stop);
  retryButton?.addEventListener("click", retry);
  updateControls();
  return { start, dispose, stop, retry };
}

if (typeof module === "object" && module.exports) {
  module.exports = { createPreviewController };
} else {
  void createPreviewController({ document, window, vscode: acquireVsCodeApi() }).start();
}
