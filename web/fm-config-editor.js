// Source-backed configuration authoring. This module locates preamble source ranges only;
// the Rust validator owns JSON5 syntax, supported keys, types, and configuration semantics.
const MAX_SOURCE_UNITS = 8 * 1024 * 1024;
const MAX_CONFIG_UNITS = 64 * 1024;
const MAX_DIRECTIVES = 256;

function cancelled() {
  const error = new Error("Configuration edit cancelled or superseded. Reload the current source.");
  error.name = "AbortError";
  return error;
}

function text(value, limit, label) {
  if (typeof value !== "string" || value.length > limit) throw new RangeError(`${label} must be text within the size limit.`);
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw new Error(`${label} contains an unpaired surrogate.`);
  }
  return value;
}

function lineEnd(source, start) {
  let end = start;
  while (end < source.length && source[end] !== "\r" && source[end] !== "\n") end++;
  return { end, next: end + (source[end] === "\r" && source[end + 1] === "\n" ? 2 : end < source.length ? 1 : 0) };
}

function trivia(source, start, limit = source.length) {
  let at = start;
  while (at < limit) {
    if (/\s/u.test(source[at])) { at++; continue; }
    if (source.startsWith("//", at)) { at = lineEnd(source, at).end; continue; }
    if (source.startsWith("/*", at)) {
      const close = source.indexOf("*/", at + 2);
      if (close < 0 || close + 2 > limit) throw new Error("Unclosed configuration comment. Edit the source directly.");
      at = close + 2;
      continue;
    }
    break;
  }
  return at;
}

function stringEnd(source, start) {
  const quote = source[start];
  for (let at = start + 1; at < source.length; at++) {
    if (source[at] === "\\") { at++; continue; }
    if (source[at] === quote) return at + 1;
  }
  throw new Error("Unclosed configuration string. Edit the source directly.");
}

// A lexical boundary scan, not a JSON5 decoder. In particular, delimiter-looking text in
// strings/comments never becomes another editable range. Iterative and depth-bounded.
function valueEnd(source, start) {
  const first = source[start];
  if (first === '"' || first === "'") return stringEnd(source, start);
  if (first !== "{" && first !== "[") {
    let at = start;
    while (at < source.length && !/[\s,}\]]/u.test(source[at]) && !source.startsWith("//", at) && !source.startsWith("/*", at)) at++;
    if (at === start) throw new Error("Missing configuration value.");
    return at;
  }
  const stack = [first];
  let at = start + 1;
  while (at < source.length) {
    at = trivia(source, at);
    const character = source[at];
    if (character === '"' || character === "'") { at = stringEnd(source, at); continue; }
    if (character === "{" || character === "[") {
      if (stack.length >= 128) throw new Error("Configuration nesting exceeds the editing limit.");
      stack.push(character);
    } else if (character === "}" || character === "]") {
      if (stack.pop() !== (character === "}" ? "{" : "[")) throw new Error("Mismatched configuration delimiters.");
      if (!stack.length) return at + 1;
    }
    at++;
  }
  throw new Error("Unclosed configuration directive. Edit the source directly.");
}

function initMembers(source, start, end) {
  const ranges = [];
  let at = trivia(source, start + 1, end);
  while (at < end - 1) {
    let key;
    if (source[at] === '"' || source[at] === "'") {
      const finish = stringEnd(source, at);
      key = source.slice(at + 1, finish - 1);
      // Do not guess the meaning of JSON5-escaped property names at an editing boundary.
      if (key.includes("\\")) throw new Error("Escaped directive keys must be edited in the source, not the configuration panel.");
      at = finish;
    } else {
      const match = /^[A-Za-z_$][\w$]*/u.exec(source.slice(at, end));
      if (!match) throw new Error("Cannot locate directive properties safely. Edit the source directly.");
      key = match[0];
      at += key.length;
    }
    at = trivia(source, at, end);
    if (source[at++] !== ":") throw new Error("Missing directive property separator.");
    at = trivia(source, at, end);
    const from = at;
    at = valueEnd(source, at);
    if (key === "init") {
      if (source[from] !== "{") throw new Error("Initialization configuration must be an object. Repair it in the source.");
      ranges.push({ from, to: at, draft: source.slice(from, at) });
    }
    at = trivia(source, at, end);
    if (source[at] === ",") at = trivia(source, at + 1, end);
    else if (at !== end - 1) throw new Error("Cannot locate the end of a directive property safely.");
  }
  return ranges;
}

function afterFrontMatter(source) {
  let at = source.startsWith("\uFEFF") ? 1 : 0;
  const first = lineEnd(source, at);
  if (source.slice(at, first.end).trim() !== "---") return at;
  at = first.next;
  while (at < source.length) {
    const line = lineEnd(source, at);
    const marker = source.slice(at, line.end).trim();
    if (marker === "---" || marker === "...") return line.next;
    at = line.next;
  }
  throw new Error("Unclosed YAML front matter. Repair it before adding configuration.");
}

/** Locate initialization objects BEFORE the diagram header. Never interpret diagram labels
 * as configuration. Later inline directives remain untouched and may override a preamble. */
export function configurationDocument(source) {
  text(source, MAX_SOURCE_UNITS, "Source");
  const targets = [];
  let at = afterFrontMatter(source), count = 0;
  while (at < source.length) {
    const line = lineEnd(source, at);
    let first = at;
    while (first < line.end && /[ \t]/u.test(source[first])) first++;
    if (first === line.end) { at = line.next; continue; }
    if (!source.startsWith("%%", first)) break;
    if (!source.startsWith("%%{", first)) { at = line.next; continue; }
    if (++count > MAX_DIRECTIVES) throw new Error("Too many preamble directives to edit safely.");
    const end = valueEnd(source, first + 2);
    if (!source.startsWith("%%", end)) throw new Error("Unclosed directive terminator. Repair it in the source.");
    const tail = lineEnd(source, end + 2);
    const remainder = source.slice(end + 2, tail.end).trim();
    if (remainder && !remainder.startsWith("%%")) throw new Error("A preamble directive shares a line with diagram text. Edit it in the source.");
    targets.push(...initMembers(source, first + 2, end));
    if (targets.length > MAX_DIRECTIVES) throw new Error("Too many initialization objects to edit safely.");
    at = tail.next;
  }
  let previous = 0, line = 1;
  for (const target of targets) {
    line += (source.slice(previous, target.from).match(/\r\n|\r|\n/g) || []).length;
    previous = target.from;
    target.line = line;
    Object.freeze(target);
  }
  return Object.freeze({ source, insertion: at, newline: /\r\n|\r|\n/u.exec(source)?.[0] || "\n",
    targets: Object.freeze(targets) });
}

export function planConfigurationEdit(document, draft, index = document.targets.length - 1) {
  text(draft, MAX_CONFIG_UNITS, "Configuration");
  const body = draft.trim();
  if (!body.startsWith("{") || valueEnd(body, 0) !== body.length) {
    throw new Error("Configuration must be exactly one object, not another directive or diagram statement.");
  }
  if (!Number.isInteger(index) || index < -1 || index >= document.targets.length || (index === -1 && document.targets.length)) {
    throw new RangeError("Select an existing initialization directive.");
  }
  const target = document.targets[index];
  const from = target?.from ?? document.insertion, to = target?.to ?? from;
  const separator = from && !/[\r\n\uFEFF]/u.test(document.source[from - 1]) ? document.newline : "";
  const replacement = target ? body : `${separator}%%{init: ${body}}%%${document.newline}`;
  const updatedSource = document.source.slice(0, from) + replacement + document.source.slice(to);
  text(updatedSource, MAX_SOURCE_UNITS, "Updated source");
  return Object.freeze({ source: document.source, updatedSource, from, to, replacement, draft: body, index });
}

export class ConfigurationValidationError extends Error {
  constructor(errors) {
    super(errors.map(error => `${error.field}: ${error.message}`).join("\n"));
    this.name = "ConfigurationValidationError";
    this.errors = Object.freeze(errors.map(error => Object.freeze({ ...error })));
  }
}

function validationReport(raw) {
  const report = typeof raw === "string" ? JSON.parse(raw) : raw;
  if (report?.schemaVersion !== "1.0.0" || !Array.isArray(report.errors) || report.errors.some(error =>
      !error || typeof error.field !== "string" || typeof error.message !== "string" || typeof error.value !== "string")) {
    throw new Error("Unsupported configuration validation report. Update the WASM package.");
  }
  if (report.errors.length) throw new ConfigurationValidationError(report.errors);
}

/** Prepared edits are single-use capabilities tied to an exact source revision, including
 * edit-and-revert or document switches whose text happens to be equal. No write during await. */
export class ConfigurationEditSession {
  #getSource;
  #loadValidator;
  #document;
  #generation = 0;
  #prepared = null;
  #stale = true;
  #disposed = false;

  constructor({ getSource, loadValidator }) {
    if (typeof getSource !== "function" || typeof loadValidator !== "function") throw new TypeError("Source and validator functions are required.");
    this.#getSource = getSource;
    this.#loadValidator = loadValidator;
  }
  load() {
    if (this.#disposed) throw cancelled();
    this.cancel();
    this.#stale = true;
    this.#document = configurationDocument(this.#getSource());
    this.#stale = false;
    return this.#document;
  }
  cancel() { this.#generation++; this.#prepared = null; }
  sourceChanged() { this.cancel(); this.#stale = true; }
  async prepare(draft, index = this.#document?.targets.length - 1) {
    if (this.#disposed || this.#stale || this.#getSource() !== this.#document?.source) throw cancelled();
    this.cancel();
    const generation = this.#generation;
    const plan = planConfigurationEdit(this.#document, draft, index);
    const check = () => {
      if (this.#disposed || this.#stale || generation !== this.#generation || this.#getSource() !== plan.source) throw cancelled();
    };
    const validator = await this.#loadValidator();
    check();
    if (typeof validator?.validateInitDirectives !== "function") throw new Error("This WASM package lacks strict directive validation. Update it before editing configuration.");
    const report = await validator.validateInitDirectives(plan.updatedSource);
    check();
    validationReport(report);
    this.#prepared = plan;
    return plan;
  }
  commit(plan) {
    if (this.#disposed || this.#stale || !plan || plan !== this.#prepared || this.#getSource() !== plan.source) throw cancelled();
    this.cancel();
    this.#stale = true; // The host must install this source and explicitly load the new revision.
    return plan.updatedSource;
  }
  dispose() { this.#disposed = true; this.sourceChanged(); }
}

/** Visual controls modify strict JSON drafts only; JSON5 source remains editable as text and
 * goes through the engine's validator. Unknown/unrelated fields survive visual edits. */
export function patchConfigurationDraft(draft, path, value) {
  text(draft, MAX_CONFIG_UNITS, "Configuration");
  if (!Array.isArray(path) || !path.length || path.some(key => typeof key !== "string" || !key ||
      ["__proto__", "prototype", "constructor"].includes(key))) throw new Error("Invalid configuration field path.");
  const root = JSON.parse(draft);
  if (!root || typeof root !== "object" || Array.isArray(root)) throw new Error("Configuration must be an object.");
  if (value !== undefined && !["string", "boolean", "number"].includes(typeof value)) throw new Error("A scalar configuration value is required.");
  if (typeof value === "number" && !Number.isFinite(value)) throw new Error("Configuration numbers must be finite.");
  const parents = [];
  let owner = root;
  for (const key of path.slice(0, -1)) {
    if (!Object.hasOwn(owner, key)) {
      if (value === undefined) return JSON.stringify(root, null, 2);
      owner[key] = {};
    }
    if (!owner[key] || typeof owner[key] !== "object" || Array.isArray(owner[key])) throw new Error(`Configuration field ${key} is not an object.`);
    parents.push([owner, key]);
    owner = owner[key];
  }
  const key = path.at(-1);
  if (value === undefined) {
    delete owner[key];
    for (const [parent, field] of parents.reverse()) {
      if (!Object.keys(parent[field]).length) delete parent[field];
      else break;
    }
  } else owner[key] = value;
  return text(JSON.stringify(root, null, 2), MAX_CONFIG_UNITS, "Configuration");
}

/** Opt-in configuration workspace. Applying emits one normal source input event, so the
 * existing document recovery/history, preview, sharing and export paths see the same text.
 * Hosts must call sourceChanged for equal-text document replacements; observeSource covers
 * programmatic text edits without invalidating harmless redraws. Closing preserves drafts. */
export function mountConfigurationEditor({ sourceEl, panelEl, loadModule, saveArtifact,
  validationTimeoutMs = 15000 }) {
  if (typeof sourceEl?.value !== "string" || !panelEl?.ownerDocument || typeof loadModule !== "function" ||
      !Number.isSafeInteger(validationTimeoutMs) || validationTimeoutMs < 1 || validationTimeoutMs > 60000) {
    throw new TypeError("Configuration editing requires source, panel, module loader and a bounded timeout.");
  }
  const document = panelEl.ownerDocument, host = document.defaultView;
  const session = new ConfigurationEditSession({ getSource: () => sourceEl.value, loadValidator: loadModule });
  const listeners = [], urls = new Map(), fields = [];
  let current = null, loadedDraft = "", selected = -1, dirty = false, stale = true;
  let disposed = false, installing = false, busy = false, operation = 0, prepared = null, timer = null;
  let lastChange = null;
  function make(tag, id, content, parent = panelEl) {
    const element = document.createElement(tag);
    if (id) element.id = id;
    if (content) element.textContent = content;
    parent.append(element);
    return element;
  }
  function listen(element, event, handler) {
    element.addEventListener(event, handler);
    listeners.push(() => element.removeEventListener(event, handler));
  }
  function button(id, label, parent) {
    const element = make("button", id, label, parent);
    element.type = "button";
    return element;
  }
  make("h2", "config-editor-heading", "Diagram configuration");
  make("p", "config-editor-help", "Edit a preamble init directive, validate it with the engine, then apply it to source. Later directives and explicit diagram syntax can override these settings. Exports and share links include applied settings; unapplied drafts do not.");
  const targetLabel = make("label", "", "Initialization directive ");
  const target = make("select", "config-editor-target", "", targetLabel);
  const controls = make("fieldset", "config-editor-fields");
  make("legend", "", "Visual settings (blank means inherit)", controls);
  const booleanOptions = [["", "Inherit"], ["true", "Yes"], ["false", "No"]];
  function field(id, label, path, type = "text", options) {
    const wrapper = make("label", "", label + " ", controls);
    const input = make(options ? "select" : "input", `config-editor-${id}`, "", wrapper);
    if (options) for (const [value, caption] of options) {
      const option = make("option", "", caption, input);
      option.value = value;
    }
    else {
      input.type = type;
      if (type === "number") { input.min = "0"; input.step = "any"; }
      else input.maxLength = 4096;
    }
    const record = { input, path, type, options };
    fields.push(record);
    listen(input, "change", () => {
      try {
        if (input.validity.badInput) throw new Error("Enter a valid finite number, or leave the field blank to inherit.");
        let value = input.value === "" ? undefined : input.value;
        if (value !== undefined && type === "number") value = Number(value);
        if (value !== undefined && type === "boolean") value = value === "true";
        let updated = draft.value;
        if (id === "direction") updated = patchConfigurationDraft(updated, ["flowchart", "rankDir"], undefined);
        draft.value = patchConfigurationDraft(updated, path, value);
        draftChanged();
      } catch (error) { status.textContent = error.message; }
    });
    return input;
  }
  const theme = field("theme", "Theme name", ["theme"]);
  theme.placeholder = "default / dark / blueprint";
  field("direction", "Flowchart direction", ["flowchart", "direction"], "text",
    [["", "Inherit"], ["lr", "Left to right"], ["rl", "Right to left"], ["tb", "Top to bottom"], ["td", "Top down"], ["bt", "Bottom to top"]]);
  field("node-spacing", "Node spacing", ["flowchart", "nodeSpacing"], "number");
  field("rank-spacing", "Rank spacing", ["flowchart", "rankSpacing"], "number");
  field("curve", "Edge curve", ["flowchart", "curve"]);
  field("primary-color", "Primary color", ["themeVariables", "primaryColor"]);
  field("line-color", "Line color", ["themeVariables", "lineColor"]);
  field("background", "Background", ["themeVariables", "background"]);
  field("mirror-actors", "Mirror sequence actors", ["sequence", "mirrorActors"], "boolean", booleanOptions);
  field("sequence-numbers", "Sequence numbers", ["sequence", "showSequenceNumbers"], "boolean", booleanOptions);
  field("gantt-top-axis", "Gantt top axis", ["gantt", "topAxis"], "boolean", booleanOptions);
  const visualNote = make("p", "config-editor-visual-note");
  const draftLabel = make("label", "", "Configuration object (JSON or existing JSON5) ");
  const draft = make("textarea", "config-editor-draft", "", draftLabel);
  draft.rows = 10;
  draft.maxLength = MAX_CONFIG_UNITS;
  draft.spellcheck = false;
  const actions = make("div", "config-editor-actions");
  const validate = button("config-editor-validate", "Validate settings", actions);
  const apply = button("config-editor-apply", "Apply validated settings", actions);
  const reload = button("config-editor-reload", "Discard draft / reload source", actions);
  const cancel = button("config-editor-cancel", "Cancel pending work", actions);
  const undo = button("config-editor-undo", "Undo configuration edit", actions);
  const importButton = button("config-editor-import", "Import configuration", actions);
  const file = make("input", "config-editor-file", "", actions);
  file.type = "file"; file.accept = ".json,.json5"; file.hidden = true;
  const download = button("config-editor-download", "Download validated configuration", actions);
  const close = button("config-editor-close", "Close settings", actions);
  const status = make("p", "config-editor-status");
  status.setAttribute("role", "status");
  const errors = make("pre", "config-editor-errors");
  errors.setAttribute("role", "alert");
  const preview = make("pre", "config-editor-preview");
  preview.hidden = true;

  function readDraft() {
    try {
      const value = JSON.parse(draft.value);
      return value && typeof value === "object" && !Array.isArray(value) ? value : null;
    } catch { return null; }
  }
  function sync() {
    const value = readDraft();
    const locked = disposed || stale;
    controls.disabled = locked || busy || value === null;
    draft.disabled = disposed;
    target.disabled = locked || busy || dirty || (current?.targets.length || 0) < 2;
    validate.disabled = locked || busy;
    apply.disabled = locked || busy || !prepared || prepared.updatedSource === sourceEl.value;
    download.disabled = locked || busy || !prepared;
    reload.disabled = disposed;
    cancel.disabled = disposed || !busy;
    importButton.disabled = locked || busy;
    file.disabled = importButton.disabled;
    close.disabled = disposed;
    undo.disabled = locked || busy || dirty || !lastChange ||
      sourceEl.value !== (lastChange.undone ? lastChange.before : lastChange.after);
    undo.textContent = lastChange?.undone ? "Redo configuration edit" : "Undo configuration edit";
    visualNote.textContent = value === null
      ? "Visual controls need a JSON object. JSON5 is retained verbatim: edit it in the text field and use engine validation. Nothing has been applied."
      : "Visual controls preserve other fields. Empty fields remove that override; false and zero remain explicit values.";
    for (const {input, path, options} of fields) {
      let found = value;
      for (const key of path) found = found && typeof found === "object" && Object.hasOwn(found, key) ? found[key] : undefined;
      if (path.join(".") === "flowchart.direction" && found === undefined && value?.flowchart && Object.hasOwn(value.flowchart, "rankDir")) found = value.flowchart.rankDir;
      for (const option of input.querySelectorAll('[data-current-value]')) option.remove();
      const displayed = found === undefined || found === null || typeof found === "object" ? "" : String(found);
      if (options && !options.some(([key]) => key === displayed)) {
        const option = make("option", "", `Current: ${displayed}`, input);
        option.value = displayed;
        option.dataset.currentValue = "true";
      }
      input.value = displayed;
    }
  }
  function revoke() {
    operation++;
    session.cancel();
    host.clearTimeout(timer);
    timer = null;
    busy = false;
    prepared = null;
    preview.hidden = true;
  }
  function setDraftFromTarget() {
    selected = Number(target.value);
    draft.value = current.targets[selected]?.draft || "{}";
    loadedDraft = draft.value;
    dirty = false;
    errors.textContent = "";
  }
  function reloadSource() {
    if (disposed) return;
    revoke();
    try {
      current = session.load();
      stale = false;
      target.replaceChildren();
      if (!current.targets.length) {
        const option = make("option", "", "New initialization directive", target);
        option.value = "-1";
      } else for (const [index, item] of current.targets.entries()) {
        const option = make("option", "", `Directive ${index + 1} — source line ${item.line}`, target);
        option.value = String(index);
      }
      if (selected < 0 || selected >= current.targets.length) selected = current.targets.length - 1;
      target.value = String(selected);
      setDraftFromTarget();
      status.textContent = "Settings loaded from source. Validate before applying a change.";
    } catch (error) { stale = true; status.textContent = error.message; }
    sync();
  }
  function draftChanged() {
    revoke();
    dirty = draft.value !== loadedDraft;
    errors.textContent = "";
    status.textContent = stale ? "Source changed. Your draft is retained but cannot be applied until you reload the current source."
      : "Draft changed. Validate it before applying; diagram source is unchanged.";
    sync();
  }
  function sourceChanged() {
    if (disposed || installing) return;
    const retain = dirty || busy || prepared !== null;
    revoke();
    session.sourceChanged();
    lastChange = null;
    if (retain) {
      stale = true;
      status.textContent = "Source changed. Your draft is retained but cannot be applied until you reload the current source.";
      sync();
    } else reloadSource();
  }
  function installSource(source) {
    installing = true;
    try {
      sourceEl.value = source;
      sourceEl.dispatchEvent(new host.Event("input", {bubbles: true}));
    } finally { installing = false; }
    reloadSource();
  }
  async function validateDraft() {
    if (disposed || stale || busy) return;
    revoke();
    const version = operation;
    busy = true;
    status.textContent = "Validating the proposed source with the Rust configuration validator…";
    errors.textContent = "";
    sync();
    timer = host.setTimeout(() => {
      if (disposed || version !== operation) return;
      revoke();
      status.textContent = "Configuration validation timed out. Source and draft are unchanged; retry after the engine is available.";
      sync();
    }, validationTimeoutMs);
    try {
      const plan = await session.prepare(draft.value, selected);
      if (disposed || version !== operation) return;
      host.clearTimeout(timer); timer = null; busy = false; prepared = plan;
      preview.textContent = `Proposed ${selected < 0 ? "new initialization directive" : "initialization object"}:\n${plan.replacement}`;
      preview.hidden = false;
      status.textContent = plan.updatedSource === sourceEl.value
        ? "Configuration is valid. No source change is needed."
        : "Configuration is valid. Review the proposed change, then apply it to source.";
    } catch (error) {
      if (disposed || version !== operation) return;
      host.clearTimeout(timer); timer = null; busy = false;
      errors.textContent = error.message || String(error);
      status.textContent = "Configuration was not applied. Correct the error or reload the source.";
    }
    sync();
  }
  function save(artifact) {
    if (saveArtifact) return saveArtifact(artifact);
    const url = host.URL.createObjectURL(new host.Blob([artifact.text], {type:artifact.mime}));
    const anchor = document.createElement("a");
    anchor.href = url; anchor.download = artifact.filename; document.body.append(anchor);
    try { anchor.click(); }
    finally {
      anchor.remove();
      urls.set(url, host.setTimeout(() => { host.URL.revokeObjectURL(url); urls.delete(url); }, 1000));
    }
  }
  listen(draft, "input", draftChanged);
  listen(sourceEl, "input", sourceChanged);
  listen(target, "change", () => { revoke(); setDraftFromTarget(); sync(); });
  listen(reload, "click", reloadSource);
  listen(validate, "click", () => { void validateDraft(); });
  listen(cancel, "click", () => { revoke(); status.textContent = "Validation cancelled. Your draft and source are unchanged."; sync(); });
  listen(apply, "click", () => {
    try {
      const before = sourceEl.value;
      const after = session.commit(prepared);
      lastChange = { before, after, undone:false };
      installSource(after);
      status.textContent = "Settings applied to source. Previews, recovery, source downloads, image exports and share links now use that source.";
    } catch (error) { revoke(); errors.textContent = error.message; sync(); }
  });
  listen(undo, "click", () => {
    if (undo.disabled) return;
    lastChange.undone = !lastChange.undone;
    installSource(lastChange.undone ? lastChange.before : lastChange.after);
    status.textContent = lastChange.undone ? "Configuration edit undone." : "Configuration edit redone.";
  });
  listen(importButton, "click", () => { file.value = ""; file.click(); });
  listen(file, "change", async () => {
    const chosen = file.files?.[0];
    if (!chosen || disposed || stale) return;
    revoke();
    const version = operation;
    busy = true;
    status.textContent = "Reading configuration as an unapplied draft…";
    sync();
    try {
      if (!/\.(json|json5)$/i.test(chosen.name) || chosen.size > MAX_CONFIG_UNITS * 4) throw new Error("Choose a UTF-8 .json or .json5 configuration within the 256 KiB import limit.");
      const bytes = await chosen.arrayBuffer();
      if (disposed || version !== operation) return;
      if (bytes.byteLength !== chosen.size || bytes.byteLength > MAX_CONFIG_UNITS * 4) throw new Error("Configuration file changed size while reading.");
      const imported = new host.TextDecoder("utf-8", {fatal:true}).decode(bytes);
      draft.value = text(imported, MAX_CONFIG_UNITS, "Configuration");
      draftChanged();
      status.textContent = "Configuration imported as an unapplied draft. Validate and review it before changing source.";
    } catch (error) {
      if (!disposed && version === operation) {
        revoke(); errors.textContent = error.message;
        status.textContent = "Configuration import failed. Source and draft are unchanged.";
        sync();
      }
    }
  });
  listen(download, "click", async () => {
    if (!prepared || disposed || stale || prepared.source !== sourceEl.value) return;
    const version = operation;
    const json = readDraft() !== null;
    try {
      await save({text: prepared.draft + "\n", mime: json ? "application/json;charset=utf-8" : "text/plain;charset=utf-8",
        filename: `diagram.config.${json ? "json" : "json5"}`});
      if (!disposed && version === operation) status.textContent = "Configuration download requested. Editable diagram source is unchanged.";
    } catch (error) { if (!disposed && version === operation) errors.textContent = error.message; }
  });
  function closePanel() {
    if (disposed) return;
    revoke();
    panelEl.hidden = true;
    sync();
  }
  listen(close, "click", closePanel);
  reloadSource();
  return {
    sourceChanged,
    observeSource() { if (!disposed && current?.source !== sourceEl.value) sourceChanged(); },
    open() {
      if (disposed) return;
      if (current?.source !== sourceEl.value) sourceChanged();
      panelEl.hidden = false;
      draft.focus();
    },
    close: closePanel,
    dispose() {
      if (disposed) return;
      disposed = true;
      revoke(); session.dispose();
      for (const remove of listeners) remove();
      for (const [url, timeout] of urls) { host.clearTimeout(timeout); host.URL.revokeObjectURL(url); }
      urls.clear();
      sync();
    },
  };
}
