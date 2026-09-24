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

export class SourceEditSession {
  #api;
  #source = "";
  #bindings = new Map();
  #selection = null;
  #revision = 0;

  constructor(api) {
    if (typeof api?.parseLens !== "function" || typeof api.applyParseLensEdit !== "function") {
      throw new Error("This WASM build lacks source-editing APIs; rebuild the shipped package.");
    }
    this.#api = api;
  }

  get source() { return this.#source; }
  get bindings() { return [...this.#bindings.values()]; }

  setSource(source) {
    if (typeof source !== "string") throw new TypeError("Source must be text.");
    // Invalidate BEFORE parsing: a failed parse must never leave the previous document editable.
    this.#revision += 1;
    this.#selection = null;
    this.#bindings = new Map();
    this.#source = source;
    byteOffsets(source); // Reject strings WASM's UTF-8 encoder would silently change.
    const snapshot = this.#api.parseLens(source);
    this.#bindings = checkedBindings(source, snapshot);
    return snapshot;
  }

  select(elementId) {
    const binding = this.#bindings.get(elementId);
    if (!binding) throw new Error("This diagram element has no editable source span.");
    this.#selection = Object.freeze({ ...binding, revision: this.#revision });
    return this.#selection;
  }

  bindingAt(start, end = start) {
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start) return null;
    return this.bindings.filter((binding) => binding.start <= start && end <= binding.end &&
      (start !== end || start < binding.end))
      .sort((a, b) => (a.end - a.start) - (b.end - b.start))[0] || null;
  }

  replace(selection, replacement) {
    if (!selection || selection !== this.#selection || selection.revision !== this.#revision) {
      throw new Error("The selection is stale; select the element again before applying an edit.");
    }
    if (typeof replacement !== "string") throw new TypeError("Replacement must be text.");
    byteOffsets(replacement);
    const response = this.#api.applyParseLensEdit(this.#source, selection.elementId, replacement);
    const updated = response?.result?.updatedSource;
    const expected = this.#source.slice(0, selection.start) + replacement + this.#source.slice(selection.end);
    if (updated !== expected) throw new Error("The edit response changed text outside the selected span.");
    // Validate the NEW snapshot before committing anything. IDs and ranges may all have changed.
    const bindings = checkedBindings(updated, response.snapshot);
    this.#source = updated;
    this.#bindings = bindings;
    this.#selection = null;
    this.#revision += 1;
    return updated;
  }
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

/** Mount an opt-in SVG/source editor. loadModule must resolve the initialized WASM exports. */
export function mountSourceEditor({ sourceEl, outEl, panelEl, loadModule, onChange, saveFile }) {
  const document = panelEl.ownerDocument;
  const listeners = [];
  const history = new SourceHistory(sourceEl.value);
  const downloads = new Map();
  let epoch = 0;
  let disposed = false;
  let session = null;
  let selection = null;
  let displayedSource = null;
  let renderedSvg = null;
  let elements = new Map();
  let originalTabIndices = new Map();

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

  function make(tag, text, id) {
    const element = document.createElement(tag);
    if (text) element.textContent = text;
    if (id) element.id = id;
    panelEl.append(element);
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
  const message = make("p", "", "source-editor-message");
  message.setAttribute("role", "status");
  panelEl.hidden = false;

  function syncControls() {
    undo.disabled = disposed || !history.canUndo;
    redo.disabled = disposed || !history.canRedo;
    saveSource.disabled = disposed;
    saveSvg.disabled = !current() || renderedSvg === null;
  }

  function clearSelection() {
    for (const element of elements.values()) element.removeAttribute("data-source-selected");
    selection = null;
    chooser.value = "";
    snippet.value = "";
    snippet.disabled = true;
    apply.disabled = true;
  }
  function invalidate() {
    epoch += 1;
    displayedSource = null;
    renderedSvg = null;
    clearSelection();
    chooser.replaceChildren();
    chooser.disabled = true;
    for (const [element, tabindex] of originalTabIndices) {
      element.removeAttribute("data-source-editable");
      if (tabindex === null) element.removeAttribute("tabindex");
      else element.setAttribute("tabindex", tabindex);
    }
    originalTabIndices = new Map();
    elements = new Map();
    syncControls();
  }
  function current() {
    return !disposed && displayedSource !== null && sourceEl.value === displayedSource;
  }
  function select(elementId, selectText = true) {
    if (!current()) return;
    if (selection?.elementId === elementId) return;
    clearSelection();
    selection = session.select(elementId);
    chooser.value = elementId;
    snippet.value = selection.snippet;
    snippet.disabled = false;
    apply.disabled = false;
    elements.get(elementId)?.setAttribute("data-source-selected", "true");
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
    const binding = session.bindingAt(sourceEl.selectionStart, sourceEl.selectionEnd);
    if (binding) select(binding.elementId, false);
    else clearSelection();
  });
  listen(apply, "click", () => {
    try {
      if (!current()) throw new Error("Source changed; select an element from the new preview.");
      sourceEl.value = session.replace(selection, snippet.value);
      invalidate();
      onChange();
    } catch (error) {
      message.textContent = String(error.message || error);
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
      message.textContent = "Building SVG and source map on the main thread…";
      outEl.setAttribute("aria-busy", "true");
      const live = () => !disposed && version === epoch && sourceEl.value === source;
      try {
        const api = await loadModule();
        if (!live()) return;
        await new Promise((resolve) => setTimeout(resolve, 0));
        if (!live()) return;
        if (!session) session = new SourceEditSession(api);
        const snapshot = session.setSource(source);
        const svg = api.renderSvg(source);
        if (typeof svg !== "string") throw new Error("The renderer did not return SVG.");
        outEl.innerHTML = svg; // Only the renderer's sanitized SVG, never source or snippets.
        const bindings = new Map(session.bindings.map((binding) => [binding.elementId, binding]));
        for (const element of outEl.querySelectorAll("[id]")) {
          if (!bindings.has(element.id)) continue;
          elements.set(element.id, element);
          originalTabIndices.set(element, element.getAttribute("tabindex"));
          element.setAttribute("tabindex", "0");
          element.setAttribute("data-source-editable", "true");
        }
        const placeholder = document.createElement("option");
        placeholder.value = "";
        placeholder.textContent = "Select an element…";
        chooser.append(placeholder);
        for (const binding of bindings.values()) {
          const option = document.createElement("option");
          option.value = binding.elementId;
          option.textContent = `${binding.kind}: ${binding.sourceId || binding.elementId}`;
          chooser.append(option);
        }
        displayedSource = source;
        renderedSvg = svg;
        chooser.disabled = bindings.size === 0;
        syncControls();
        const warnings = snapshot.parsed?.warnings || [];
        message.textContent = `${bindings.size} editable source spans. ${warnings.join(" ")}`;
      } catch (error) {
        if (live()) message.textContent = `Source editor unavailable: ${error.message || error}`;
      } finally {
        if (live()) outEl.setAttribute("aria-busy", "false");
      }
    },
    dispose() {
      disposed = true;
      invalidate();
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
