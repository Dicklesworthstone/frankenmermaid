// Browser adapter for the Rust ParseLens (bd-1t7l.1). This edits SOURCE SPANS, not semantic
// nodes: a node's binding may cover a whole statement, including an edge and another node.
// Parsing and all source mutations stay in WASM. The host owns selection and revision safety.

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

/** Mount an opt-in SVG/source editor. loadModule must resolve the initialized WASM exports. */
export function mountSourceEditor({ sourceEl, outEl, panelEl, loadModule, onChange }) {
  const document = panelEl.ownerDocument;
  const listeners = [];
  let epoch = 0;
  let disposed = false;
  let session = null;
  let selection = null;
  let displayedSource = null;
  let elements = new Map();

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
    clearSelection();
    chooser.replaceChildren();
    chooser.disabled = true;
    elements = new Map();
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
  invalidate();

  return {
    async render(source) {
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
        chooser.disabled = bindings.size === 0;
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
      outEl.setAttribute("aria-busy", "false");
    },
  };
}
