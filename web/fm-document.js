// Source-document lifecycle for the playground. File I/O never depends on a successful
// parse/render. The textarea is an LF projection; the document retains original line endings
// and a UTF-8 BOM, splicing only changed text back into that complement.

const MAX_FILE_BYTES = 8 * 1024 * 1024;
const SOURCE_EXTENSIONS = /\.(mmd|mermaid|dot|gv|txt)$/i;
const RECOVERY_PREFIX = "fm.playground.recovery.v1.";

function checkedSnapshot(snapshot) {
  if (!snapshot || typeof snapshot.source !== "string" || typeof snapshot.baseline !== "string" ||
      typeof snapshot.name !== "string" || !snapshot.name || snapshot.name !== fileName(snapshot.name) ||
      !["\n", "\r", "\r\n"].includes(snapshot.newline) || typeof snapshot.hasBom !== "boolean" ||
      (snapshot.hasBom && (!snapshot.source.startsWith("\uFEFF") || !snapshot.baseline.startsWith("\uFEFF")))) {
    throw new Error("Invalid source recovery snapshot; the stored copy was left untouched.");
  }
  return Object.freeze({ source: snapshot.source, baseline: snapshot.baseline, name: snapshot.name,
    newline: snapshot.newline, hasBom: snapshot.hasBom });
}

/** Independent writer keys: reloads and restored copies fork instead of overwriting another tab. */
export class DraftRepository {
  #storage;
  #newId;
  #now;
  #maxUnits;
  #owned = new Set();

  constructor({ getStorage, newId = () => globalThis.crypto.randomUUID(), now = Date.now,
    maxRecordUnits = 20 * 1024 * 1024 }) {
    if (typeof getStorage !== "function" || !Number.isSafeInteger(maxRecordUnits) || maxRecordUnits < 1) {
      throw new TypeError("Recovery requires storage access and a positive record limit.");
    }
    this.#storage = getStorage;
    this.#newId = newId;
    this.#now = now;
    this.#maxUnits = maxRecordUnits;
  }
  createKey() {
    const key = RECOVERY_PREFIX + this.#newId();
    if (!/^fm\.playground\.recovery\.v1\.[a-zA-Z0-9-]{16,80}$/.test(key) ||
        this.#owned.has(key) || this.#storage().getItem(key) !== null) {
      throw new Error("Could not allocate an independent recovery copy; download your source.");
    }
    this.#owned.add(key);
    return key;
  }
  write(key, snapshot) {
    if (!this.#owned.has(key)) throw new Error("Cannot overwrite another document's recovery copy.");
    const document = checkedSnapshot(snapshot);
    if (document.source.length + document.baseline.length > this.#maxUnits) throw new Error("Recovery copy exceeds the storage record limit; download your source.");
    const updatedAt = this.#now();
    if (!Number.isSafeInteger(updatedAt) || updatedAt < 0 || updatedAt > 8640000000000000) throw new Error("Invalid recovery timestamp.");
    const encoded = JSON.stringify({ schemaVersion: 1, ...document, updatedAt });
    if (encoded.length > this.#maxUnits) throw new Error("Recovery copy exceeds the storage record limit; download your source.");
    // A single setItem replaces this writer's record atomically. No shared mutable index,
    // last-document pointer, read/increment/write counter, or automatic deletion of old drafts.
    this.#storage().setItem(key, encoded);
  }
  read(key) {
    if (typeof key !== "string" || !key.startsWith(RECOVERY_PREFIX)) throw new Error("Unknown recovery key.");
    const encoded = this.#storage().getItem(key);
    if (encoded === null) throw new Error("Recovery copy is no longer available.");
    if (encoded.length > this.#maxUnits) throw new Error("Recovery record is too large; it was left untouched.");
    const record = JSON.parse(encoded);
    if (!record || record.schemaVersion !== 1 || !Number.isSafeInteger(record.updatedAt) ||
        record.updatedAt < 0 || record.updatedAt > 8640000000000000) {
      throw new Error("Unsupported or damaged recovery record; it was left untouched.");
    }
    return Object.freeze({ ...checkedSnapshot(record), key, updatedAt: record.updatedAt });
  }
  list() {
    const storage = this.#storage(), keys = [];
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index);
      if (key?.startsWith(RECOVERY_PREFIX)) keys.push(key);
    }
    const drafts = [], unreadable = [];
    for (const key of keys) {
      try { drafts.push(this.read(key)); }
      catch { unreadable.push(key); }
    }
    drafts.sort((a, b) => b.updatedAt - a.updatedAt || a.key.localeCompare(b.key));
    return { drafts, unreadable };
  }
}

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
  snapshot() {
    return Object.freeze({ source: this.#source, baseline: this.#baseline, name: this.#name,
      newline: this.#newline, hasBom: this.#hasBom });
  }
  restore(snapshot) {
    const checked = checkedSnapshot(snapshot);
    this.open(checked.source, checked.name);
    this.#baseline = checked.baseline;
    this.#newline = checked.newline;
    this.#hasBom = checked.hasBom;
  }

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
  saveFile, confirmReplace, initialSource = sourceEl.value, maxBytes = MAX_FILE_BYTES,
  getStorage, newRecoveryId, recoveryDelayMs = 400 }) {
  const document = panelEl.ownerDocument;
  const host = document.defaultView;
  const model = new SourceDocument(initialSource);
  if (!Number.isSafeInteger(recoveryDelayMs) || recoveryDelayMs < 0) throw new RangeError("Invalid recovery delay.");
  const drafts = new DraftRepository({ getStorage: getStorage || (() => host.localStorage),
    newId: newRecoveryId || (() => {
      const bytes = host.crypto.getRandomValues(new Uint8Array(16));
      return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
    }) });
  const listeners = [], downloads = new Map();
  let disposed = false, operation = 0;
  let recoveryKey = null, recoveryTimer = null, recoveryPending = false;
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
  const recoveryStatus = make("p", "Recovery copies stay in this browser; download source for a durable file.", "document-recovery-status");
  recoveryStatus.setAttribute("role", "status");
  const recoveryLabel = make("label", "Recover a previous document", "document-recovery-label");
  const recoveryChoice = make("select", "", "document-recovery");
  recoveryLabel.htmlFor = recoveryChoice.id;
  const restore = make("button", "Restore recovery copy", "document-restore");
  const saveRecovery = make("button", "Download recovery copy", "document-save-recovery");
  const retryRecovery = make("button", "Retry recovery save", "document-retry-recovery");
  for (const button of [restore, saveRecovery, retryRecovery]) button.type = "button";
  retryRecovery.hidden = true;
  function recoveryControls() {
    restore.disabled = saveRecovery.disabled = disposed || !recoveryChoice.value;
  }
  function refreshRecovery() {
    if (disposed) return;
    try {
      const previous = recoveryChoice.value;
      const { drafts: available, unreadable } = drafts.list();
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "Choose a recovery copy…";
      recoveryChoice.replaceChildren(placeholder);
      for (const entry of available) {
        if (entry.key === recoveryKey) continue;
        const option = document.createElement("option");
        option.value = entry.key;
        option.textContent = `${entry.name} — ${new Date(entry.updatedAt).toLocaleString()}`;
        recoveryChoice.append(option);
      }
      recoveryChoice.value = previous;
      if (unreadable.length) recoveryStatus.textContent = `${unreadable.length} unreadable recovery copies were left untouched.`;
      recoveryControls();
    } catch (error) {
      recoveryStatus.textContent = `Browser recovery unavailable: ${error.message || error}. Download your source to keep it.`;
      retryRecovery.hidden = false;
      recoveryControls();
    }
  }
  function scheduleRecovery() {
    recoveryPending = true;
    recoveryStatus.textContent = "Recovery save pending…";
    // Do not reset the deadline on every keystroke: continuous typing still checkpoints.
    if (recoveryTimer === null) recoveryTimer = host.setTimeout(flushRecovery, recoveryDelayMs);
  }
  function flushRecovery() {
    if (disposed) return false;
    sync();
    host.clearTimeout(recoveryTimer);
    recoveryTimer = null;
    if (!recoveryPending) return true;
    try {
      if (recoveryKey === null) recoveryKey = drafts.createKey();
      drafts.write(recoveryKey, model.snapshot());
      recoveryPending = false;
      retryRecovery.hidden = true;
      recoveryStatus.textContent = "Recovery copy saved in this browser. Download source for a durable file.";
      return true;
    } catch (error) {
      recoveryStatus.textContent = `Recovery save failed: ${error.message || error}. Current source is still open; download it now.`;
      retryRecovery.hidden = false;
      return false;
    }
  }
  function sync() {
    if (disposed) return;
    if (model.edit(sourceEl.value)) scheduleRecovery();
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
  function changedDocument(incoming) {
    // Retain the outgoing document under its own key. Restoring a copy NEVER adopts that
    // copy's writer key, so two open tabs can independently edit the same recovered source.
    flushRecovery();
    recoveryKey = null;
    if (incoming.recovery) model.restore(incoming.recovery);
    else model.open(incoming.source, incoming.name);
    sourceEl.value = model.view;
    scheduleRecovery();
    flushRecovery();
    refreshRecovery();
    // A different file must not inherit another file's selection tokens or undo history.
    const problems = [];
    try { onDocumentChange(); } catch (error) { problems.push(String(error.message || error)); }
    sync();
    // Even if a host's editor reset fails, notify its render boundary so old results become
    // stale. Source has already opened successfully; do not misreport a preview error as data loss.
    try { onChange(); } catch (error) { problems.push(String(error.message || error)); }
    return problems.length ? ` Preview update failed: ${problems.join("; ")}` : "";
  }
  async function replaceDocument(read, isCurrent = () => true) {
    sync();
    if (disposed) return false;
    const request = ++operation, revision = model.revision;
    const live = () => !disposed && request === operation && revision === model.revision && isCurrent();
    status.textContent = "Reading source…";
    try {
      const incoming = await read();
      sync(); // Catch programmatic source edits as well as input events.
      if (!live()) {
        if (!disposed && request === operation) status.textContent = isCurrent() ? "Open cancelled: source changed while reading. Open the file again to replace it." : "Open cancelled: the shared link is no longer current.";
        return false;
      }
      if (model.dirty) {
        const approved = await confirm(`Replace unexported changes in ${model.name} with ${incoming.name}? Download your current source first to keep a file copy.`);
        sync();
        if (!live()) {
          if (!disposed && request === operation) status.textContent = isCurrent() ? "Open cancelled: source changed while confirming." : "Open cancelled: the shared link is no longer current.";
          return false;
        }
        if (!approved) { status.textContent = "Open cancelled; current source retained."; return false; }
      }
      const previewProblem = changedDocument(incoming);
      status.textContent = `Opened ${model.name}. The original file is unchanged; use Download source to save a copy.${previewProblem}`;
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
        scheduleRecovery();
        flushRecovery();
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
  listen(recoveryChoice, "change", recoveryControls);
  listen(restore, "click", () => {
    const key = recoveryChoice.value;
    if (!key) return;
    void replaceDocument(() => {
      const recovery = drafts.read(key);
      return { name: recovery.name, source: recovery.source, recovery };
    });
  });
  listen(saveRecovery, "click", async () => {
    try {
      const entry = drafts.read(recoveryChoice.value);
      unicodeText(entry.source);
      await (saveFile || download)({ text: entry.source, filename: entry.name, mime: "text/plain;charset=utf-8" });
      if (!disposed) status.textContent = `Download requested for recovery copy ${entry.name}. Current document unchanged.`;
    } catch (error) {
      if (!disposed) status.textContent = `Recovery download failed: ${error.message || error}`;
    }
  });
  listen(retryRecovery, "click", () => { refreshRecovery(); flushRecovery(); });
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
  listen(host, "pagehide", flushRecovery);
  listen(document, "visibilitychange", () => { if (document.visibilityState === "hidden") flushRecovery(); });
  listen(host, "storage", (event) => {
    if (event.key === null || event.key?.startsWith(RECOVERY_PREFIX)) refreshRecovery();
  });
  refreshRecovery(); // Recovery is opt-in: never replace fresh typing automatically.
  sync();
  return {
    sourceChanged: sync, openFile, saveSource, flushRecovery,
    sourceSnapshot() {
      if (disposed) throw new Error("The document workspace is closed.");
      sync();
      // Sharing must retain BOM/line endings, but must not disclose the previous baseline.
      return Object.freeze({ source: model.source, name: model.name, revision: model.revision });
    },
    openSharedSource(read, { isCurrent = () => true } = {}) {
      return replaceDocument(async () => {
        const incoming = await read();
        unicodeText(incoming?.source);
        if (typeof incoming.name !== "string" || incoming.source.includes("\0")) throw new Error("Invalid shared source document.");
        if (incoming.source.length > maxBytes || new TextEncoder().encode(incoming.source).byteLength > maxBytes) {
          throw new Error("Shared source exceeds the document size limit.");
        }
        return { source: incoming.source, name: fileName(incoming.name) };
      }, isCurrent);
    },
    saveArtifact(artifact) {
      if (disposed) throw new Error("The document workspace is closed.");
      if (artifact.mime?.startsWith("text/plain")) return saveSource();
      const stem = model.name.replace(/\.[^.]+$/, "");
      const filename = artifact.filename === "diagram.svg" ? `${stem}.svg` : artifact.filename;
      return (saveFile || download)({ ...artifact, filename });
    },
    dispose() {
      if (disposed) return;
      flushRecovery();
      disposed = true;
      host.clearTimeout(recoveryTimer);
      operation += 1;
      for (const remove of listeners) remove();
      for (const [url, timer] of downloads) { host.clearTimeout(timer); host.URL.revokeObjectURL(url); }
      downloads.clear();
      for (const button of [open, fresh, save, restore, saveRecovery, retryRecovery]) button.disabled = true;
    },
  };
}
