// Source-document lifecycle for the playground. File I/O never depends on a successful
// parse/render. The textarea is an LF projection; the document retains original line endings
// and a UTF-8 BOM, splicing only changed text back into that complement.

const MAX_FILE_BYTES = 8 * 1024 * 1024;
const SOURCE_EXTENSIONS = /\.(mmd|mermaid|dot|gv|txt)$/i;

function sourceView(source, hasBom) {
  return (hasBom ? source.slice(1) : source).replace(/\r\n?/g, "\n");
}
function unicodeText(text) {
  if (typeof text !== "string") throw new TypeError("Source must be text.");
  for (const character of text) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) {
      throw new Error("Source contains an unpaired surrogate; repair it before downloading.");
    }
  }
}
function fileName(name) {
  const leaf = String(name || "diagram.mmd").split(/[\\/]/).at(-1);
  const safe = leaf.replace(/[\u0000-\u001f\u007f]/g, "").trim();
  return !safe || safe === "." || safe === ".." ? "diagram.mmd" : safe.slice(0, 240);
}
function preferredNewline(source) {
  const endings = source.match(/\r\n|\r|\n/g) || [];
  const counts = new Map();
  let preferred = "\n", largest = 0;
  for (const ending of endings) {
    const count = (counts.get(ending) || 0) + 1;
    counts.set(ending, count);
    if (count > largest) { largest = count; preferred = ending; }
  }
  return preferred;
}
function rawOffset(raw, projectedOffset, hasBom) {
  let index = hasBom ? 1 : 0;
  for (let projected = 0; projected < projectedOffset; projected += 1) {
    index += raw[index] === "\r" && raw[index + 1] === "\n" ? 2 : 1;
  }
  return index;
}
function updateRawSource(raw, view, newline, hasBom) {
  const before = sourceView(raw, hasBom);
  let start = 0;
  while (start < before.length && start < view.length && before[start] === view[start]) start += 1;
  let tail = 0;
  while (tail < before.length - start && tail < view.length - start &&
         before[before.length - tail - 1] === view[view.length - tail - 1]) tail += 1;
  return raw.slice(0, rawOffset(raw, start, hasBom)) +
    view.slice(start, view.length - tail).replace(/\n/g, newline) +
    raw.slice(rawOffset(raw, before.length - tail, hasBom));
}

/** Decode an explicitly selected source file without replacing malformed UTF-8 with U+FFFD. */
export async function readSourceFile(file, maxBytes = MAX_FILE_BYTES) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) throw new RangeError("Invalid file size limit.");
  if (!file || typeof file.arrayBuffer !== "function" || !SOURCE_EXTENSIONS.test(file.name || "")) {
    throw new Error("Choose a .mmd, .mermaid, .dot, .gv, or .txt source file, not a rendered image.");
  }
  if (!Number.isSafeInteger(file.size) || file.size < 0 || file.size > maxBytes) {
    throw new Error(`Source file exceeds the ${maxBytes} byte limit.`);
  }
  const bytes = await file.arrayBuffer();
  if (!(bytes instanceof ArrayBuffer) || bytes.byteLength > maxBytes || bytes.byteLength !== file.size) {
    throw new Error("Source file size changed while reading or exceeds the limit.");
  }
  let source;
  try { source = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes); }
  catch { throw new Error("Source file is not valid UTF-8. Convert its encoding before opening it."); }
  if (source.includes("\0")) throw new Error("Source file contains NUL bytes; binary files cannot be opened as diagrams.");
  return { source, name: fileName(file.name) };
}

/** Exact source plus a revision and export baseline, independent of editor/renderer state. */
export class SourceDocument {
  #source;
  #baseline;
  #name;
  #newline;
  #hasBom;
  #revision = 0;
  #identity = 0;
  #ticket = 0;
  #lastExport = 0;
  #exports = new WeakSet();

  constructor(source, name = "diagram.mmd") { this.open(source, name); }
  get source() { return this.#source; }
  get view() { return sourceView(this.#source, this.#hasBom); }
  get name() { return this.#name; }
  get revision() { return this.#revision; }
  get dirty() { return this.#source !== this.#baseline; }

  open(source, name) {
    if (typeof source !== "string") throw new TypeError("Source must be text.");
    this.#source = source;
    this.#baseline = source;
    this.#name = fileName(name);
    this.#newline = preferredNewline(source);
    this.#hasBom = source.startsWith("\uFEFF");
    this.#identity += 1;
    this.#revision += 1;
    this.#lastExport = 0;
    this.#exports = new WeakSet();
  }
  edit(view) {
    if (typeof view !== "string") throw new TypeError("Source must be text.");
    view = view.replace(/\r\n?/g, "\n");
    if (view === this.view) return false;
    // Preserve even temporarily malformed UTF-16 while typing; only encoding/export rejects it.
    this.#source = view === sourceView(this.#baseline, this.#hasBom) ? this.#baseline :
      updateRawSource(this.#source, view, this.#newline, this.#hasBom);
    this.#revision += 1;
    return true;
  }
  prepareExport() {
    unicodeText(this.#source);
    const artifact = Object.freeze({ text: this.#source, filename: this.#name,
      mime: "text/plain;charset=utf-8", identity: this.#identity, ticket: ++this.#ticket });
    this.#exports.add(artifact);
    return artifact;
  }
  exported(artifact) {
    if (!this.#exports.has(artifact) || artifact.identity !== this.#identity || artifact.ticket <= this.#lastExport) return false;
    this.#baseline = artifact.text;
    this.#lastExport = artifact.ticket;
    return true;
  }
}

/** Mount file open/drop/new/download controls around the existing authoritative textarea. */
export function mountDocumentWorkspace({ sourceEl, panelEl, onChange, onDocumentChange = () => {},
  saveFile, confirmReplace, initialSource = sourceEl.value, maxBytes = MAX_FILE_BYTES }) {
  const document = panelEl.ownerDocument;
  const host = document.defaultView;
  const model = new SourceDocument(initialSource);
  const listeners = [], downloads = new Map();
  let disposed = false, operation = 0;
  const confirm = confirmReplace || ((message) => host.confirm(message));
  function listen(target, name, handler) {
    target.addEventListener(name, handler);
    listeners.push(() => target.removeEventListener(name, handler));
  }
  function make(tag, text, id) {
    const element = document.createElement(tag);
    element.id = id;
    if (text) element.textContent = text;
    panelEl.append(element);
    return element;
  }
  const open = make("button", "Open source file", "document-open");
  const fresh = make("button", "New diagram", "document-new");
  const save = make("button", "Download source", "document-save");
  for (const button of [open, fresh, save]) button.type = "button";
  const picker = make("input", "", "document-file");
  picker.type = "file";
  picker.accept = ".mmd,.mermaid,.dot,.gv,.txt";
  picker.hidden = true;
  const title = make("span", "", "document-name");
  const status = make("p", "Open or drop a UTF-8 source file. Downloads do not overwrite the original file.", "document-message");
  status.setAttribute("role", "status");
  function sync() {
    if (disposed) return;
    model.edit(sourceEl.value);
    title.textContent = `${model.name}${model.dirty ? " — unexported changes" : ""}`;
  }
  function download(artifact) {
    const url = host.URL.createObjectURL(new host.Blob([artifact.text], { type: artifact.mime }));
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName(artifact.filename);
    document.body.append(link);
    try { link.click(); }
    finally {
      link.remove();
      downloads.set(url, host.setTimeout(() => { host.URL.revokeObjectURL(url); downloads.delete(url); }, 1000));
    }
  }
  function changedDocument(source, name) {
    model.open(source, name);
    sourceEl.value = model.view;
    // A different file must not inherit another file's selection tokens or undo history.
    onDocumentChange();
    sync();
    onChange();
  }
  async function replaceDocument(read) {
    sync();
    if (disposed) return false;
    const request = ++operation, revision = model.revision;
    const live = () => !disposed && request === operation && revision === model.revision;
    status.textContent = "Reading source…";
    try {
      const incoming = await read();
      sync(); // Catch programmatic source edits as well as input events.
      if (!live()) {
        if (!disposed && request === operation) status.textContent = "Open cancelled: source changed while reading. Open the file again to replace it.";
        return false;
      }
      if (model.dirty) {
        const approved = await confirm(`Replace unexported changes in ${model.name} with ${incoming.name}? Download your current source first to keep a file copy.`);
        sync();
        if (!live()) {
          if (!disposed && request === operation) status.textContent = "Open cancelled: source changed while confirming.";
          return false;
        }
        if (!approved) { status.textContent = "Open cancelled; current source retained."; return false; }
      }
      changedDocument(incoming.source, incoming.name);
      status.textContent = `Opened ${model.name}. The original file is unchanged; use Download source to save a copy.`;
      return true;
    } catch (error) {
      if (!disposed && request === operation) status.textContent = `Open failed: ${error.message || error}`;
      return false;
    }
  }
  async function saveSource() {
    sync();
    if (disposed) return false;
    let artifact;
    try {
      artifact = model.prepareExport();
      await (saveFile || download)(artifact);
      if (disposed) return false;
      if (model.exported(artifact)) {
        sync();
        status.textContent = `Download requested for ${artifact.filename}. Keep the downloaded file; the original was not overwritten.`;
      }
      return true;
    } catch (error) {
      if (!disposed) status.textContent = `Download failed: ${error.message || error}`;
      return false;
    }
  }
  const openFile = (file) => replaceDocument(() => readSourceFile(file, maxBytes));
  listen(open, "click", () => picker.click());
  listen(picker, "change", () => {
    const files = Array.from(picker.files || []);
    picker.value = ""; // Selecting the same file again is a real operation.
    if (files.length === 1) void openFile(files[0]);
  });
  listen(fresh, "click", () => { void replaceDocument(() => ({ name: "diagram.mmd", source: "flowchart TD\n" })); });
  listen(save, "click", () => { void saveSource(); });
  listen(sourceEl, "input", sync);
  listen(sourceEl, "dragover", (event) => {
    if (Array.from(event.dataTransfer?.types || []).includes("Files")) event.preventDefault();
  });
  listen(sourceEl, "drop", (event) => {
    const files = Array.from(event.dataTransfer?.files || []);
    if (!files.length) return;
    event.preventDefault();
    if (files.length !== 1) { status.textContent = "Open one source file at a time; current source retained."; return; }
    void openFile(files[0]);
  });
  listen(host, "beforeunload", (event) => {
    sync();
    if (!disposed && model.dirty) { event.preventDefault(); event.returnValue = ""; }
  });
  sync();
  return {
    sourceChanged: sync, openFile, saveSource,
    saveArtifact(artifact) {
      if (disposed) throw new Error("The document workspace is closed.");
      if (artifact.mime?.startsWith("text/plain")) return saveSource();
      const stem = model.name.replace(/\.[^.]+$/, "");
      const filename = artifact.filename === "diagram.svg" ? `${stem}.svg` : artifact.filename;
      return (saveFile || download)({ ...artifact, filename });
    },
    dispose() {
      disposed = true;
      operation += 1;
      for (const remove of listeners) remove();
      for (const [url, timer] of downloads) { host.clearTimeout(timer); host.URL.revokeObjectURL(url); }
      downloads.clear();
      for (const button of [open, fresh, save]) button.disabled = true;
    },
  };
}
