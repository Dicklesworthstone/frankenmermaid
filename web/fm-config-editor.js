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
