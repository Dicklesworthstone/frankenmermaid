// Graph-deck authoring surface (bd-z7g6k). Rust resolves slides and geometry; the canonical
// CLI runtime presents them. Draft composition publishes ordinary source, not shadow state.
import { createSourceEditorBackend } from "./fm-source-editor.js";

export function deckJson(value) {
  return JSON.stringify(value, (_key, item) => {
    if (typeof item === "number" && !Number.isFinite(item)) throw new Error("Deck contains non-finite geometry.");
    if (item instanceof Map) {
      if ([...item.keys()].some((key) => typeof key !== "string")) throw new Error("Deck map keys must be strings.");
      return Object.fromEntries(item);
    }
    return item;
  });
}

function scriptJson(value) {
  // A JSON string is NOT safe inside an HTML script until raw-text closing tags are escaped.
  return deckJson(value).replace(/[<>&\u2028\u2029]/g, (ch) =>
    `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`);
}
function htmlText(value) {
  return value.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);
}
function abortError() {
  const error = new Error("Deck preview superseded or closed.");
  error.name = "AbortError";
  return error;
}

export function checkedDeck(result) {
  if (typeof result?.svg !== "string" || !Array.isArray(result.warnings)) throw new Error("Invalid graph-deck result.");
  if (result.manifest == null) throw new Error("No presentation resolved. Add a deck directive for a supported diagram family.");
  // Preserve Map-backed WASM records when producing portable JSON; never silently export {}.
  const manifest = JSON.parse(deckJson(result.manifest));
  if (!/^1\.\d+\.\d+$/.test(manifest.schemaVersion || "")) throw new Error("Unsupported deck manifest version.");
  const rect = (value) => value && [value.x, value.y, value.width, value.height].every(Number.isFinite) &&
    value.width >= 0 && value.height >= 0;
  const natural = (value) => Number.isSafeInteger(value) && value >= 0;
  if (!rect(manifest.viewBox) || manifest.viewBox.width <= 0 || manifest.viewBox.height <= 0 ||
      !manifest.options || !Number.isFinite(manifest.options.fitMargin) || manifest.options.fitMargin < 0 ||
      !Number.isFinite(manifest.options.zoomMax) || manifest.options.zoomMax <= 0 ||
      !Number.isFinite(manifest.options.dimOpacity) || manifest.options.dimOpacity < 0 || manifest.options.dimOpacity > 1 ||
      !Number.isFinite(manifest.options.autoAdvanceMs) || manifest.options.autoAdvanceMs < 0 ||
      !Array.isArray(manifest.slides) || !manifest.slides.length) throw new Error("Deck has no usable slides or camera geometry.");
  const ids = new Set();
  for (const slide of manifest.slides) {
    if (!slide || typeof slide.id !== "string" || !slide.id || ids.has(slide.id) ||
        typeof slide.title !== "string" || !rect(slide.bounds) ||
        !Number.isFinite(slide.fitMargin) || slide.fitMargin < 0 ||
        !Number.isFinite(slide.zoomMax) || slide.zoomMax <= 0 || !natural(slide.maxStep) ||
        !Array.isArray(slide.nodes)) throw new Error("Deck has an invalid slide.");
    ids.add(slide.id);
    for (const members of [slide.nodes, slide.edges ?? [], slide.clusters ?? []]) {
      if (!Array.isArray(members) || members.some((member) => !member ||
          typeof member.elementId !== "string" || !member.elementId || !natural(member.step) || member.step > slide.maxStep)) {
        throw new Error("Deck has invalid slide membership.");
      }
    }
    if (!Array.isArray(slide.steps ?? []) || (slide.steps ?? []).some((step) => !step ||
        !natural(step.step) || step.step > slide.maxStep || !Array.isArray(step.elementIds) ||
        step.elementIds.some((id) => typeof id !== "string" || !id))) throw new Error("Deck has invalid reveal steps.");
  }
  return { svg: result.svg, manifest, warnings: result.warnings };
}

export function checkDeckElements(deck, DOMParserClass = globalThis.DOMParser) {
  const svg = new DOMParserClass().parseFromString(deck.svg, "image/svg+xml");
  if (svg.querySelector("parsererror") || svg.documentElement.localName !== "svg" ||
      svg.documentElement.namespaceURI !== "http://www.w3.org/2000/svg") throw new Error("The deck renderer returned invalid SVG.");
  const ids = new Set();
  for (const element of svg.querySelectorAll("[id]")) {
    if (ids.has(element.id)) throw new Error(`Duplicate SVG element ID: ${element.id}`);
    ids.add(element.id);
  }
  const required = new Set();
  for (const slide of deck.manifest.slides) {
    for (const member of [...slide.nodes, ...(slide.edges || []), ...(slide.clusters || [])]) required.add(member.elementId);
    for (const step of slide.steps || []) for (const id of step.elementIds) required.add(id);
  }
  for (const id of Object.keys(deck.manifest.nodeGeometry || {})) required.add(id);
  for (const [id, ends] of Object.entries(deck.manifest.edgeEndpoints || {})) {
    required.add(id);
    required.add(ends.fromElementId);
    required.add(ends.toElementId);
  }
  for (const id of required) if (!ids.has(id)) throw new Error(`Deck references a missing SVG element: ${id}`);
}

const TOKENS = ["{{TITLE}}", "{{BG}}", "{{FG}}", "{{MANIFEST_JSON}}", "{{SVG_JS_STRING}}", "RUNTIME_JS"];
function checkAssets({ runtime, template }) {
  if (typeof runtime !== "string" || !runtime.includes("window.FmDeckRuntime") || /<\/script[\s/>]/i.test(runtime)) {
    throw new Error("The canonical deck runtime is missing or cannot be embedded safely.");
  }
  if (typeof template !== "string" || TOKENS.some((token) => template.split(token).length !== 2) ||
      template.split('<meta charset="utf-8">').length !== 2 || template.split("<script>").length !== 3 ||
      template.split("</body>").length !== 2) throw new Error("The canonical deck template is missing or incompatible.");
}

/** Load tracked CLI assets in a checkout, or their explicitly staged counterparts on Pages. */
export function createDeckAssetLoader(fetchImpl = globalThis.fetch) {
  let pending;
  async function load(paths, valid) {
    for (const path of paths) {
      try {
        const response = await fetchImpl(new URL(path, import.meta.url));
        if (!response.ok) continue;
        const text = await response.text();
        if (valid(text)) return text;
      } catch { /* Try the source-checkout location, not a CDN or an unrelated runtime. */ }
    }
    throw new Error("Deck assets unavailable. Serve the repository root or stage the deck runtime and template with the web assets.");
  }
  return () => {
    if (!pending) {
      pending = Promise.all([
        load(["./fm-deck-runtime.js", "../crates/fm-cli/src/deck_runtime.js"], (text) => text.includes("window.FmDeckRuntime") && !text.trimStart().startsWith("<")),
        load(["./fm-deck-template.html", "../crates/fm-cli/src/deck_template.html"], (text) => TOKENS.every((token) => text.includes(token))),
      ]).then(([runtime, template]) => {
        const assets = { runtime, template };
        checkAssets(assets);
        return assets;
      }).catch((error) => { pending = null; throw error; });
    }
    return pending;
  };
}

/** Bundle a paired engine result with the SAME runtime/template used by the CLI. No WASM is
 * needed to play the exported file. A nonce policy blocks input-supplied handlers/scripts and
 * external resources; data-URI images/fonts remain usable in the self-contained document. */
export function buildDeckHtml(result, assets, { nonce, background = "#f8fafc", foreground = "#0f172a" } = {}) {
  const deck = checkedDeck(result);
  checkAssets(assets);
  if (nonce === undefined) {
    const bytes = globalThis.crypto.getRandomValues(new Uint8Array(24));
    nonce = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  }
  if (!/^[a-f0-9]{48}$/.test(nonce)) throw new Error("Invalid deck script nonce.");
  if (![background, foreground].every((color) => /^#[0-9a-f]{3}([0-9a-f]{3})?$/i.test(color))) throw new Error("Deck chrome colors must be hex colors.");
  const title = typeof deck.manifest.title === "string" ? deck.manifest.title : "Diagram presentation";
  const values = {
    "{{TITLE}}": htmlText(title), "{{BG}}": background, "{{FG}}": foreground,
    "{{MANIFEST_JSON}}": scriptJson(deck.manifest), "{{SVG_JS_STRING}}": scriptJson(deck.svg),
    RUNTIME_JS: assets.runtime,
  };
  const csp = `default-src 'none'; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'`;
  // Apply template-only changes BEFORE inserting data. Source containing a placeholder must
  // remain literal text, never become a second template expansion or a script boundary.
  let template = assets.template.replaceAll("<script>", `<script nonce="${nonce}">`)
    .replace('<meta charset="utf-8">', `<meta charset="utf-8">\n<meta http-equiv="Content-Security-Policy" content="${csp}">`);
  const guard = `<script nonce="${nonce}">
window.addEventListener("error", function(event) {
  window.__fmDeckFailed = true;
  if (parent !== window) parent.postMessage({kind:"fm-deck-error", token:"${nonce}", reason:event.message}, "*");
});
</script>\n`;
  template = template.replace(`<script nonce="${nonce}">`, guard + `<script nonce="${nonce}">`);
  template = template.replace("</body>", `<script nonce="${nonce}">
if (!window.__fmDeckFailed && document.querySelector("#deck-stage .fm-deck-viewport svg")) {
  if (parent !== window) parent.postMessage({kind:"fm-deck-ready", token:"${nonce}"}, "*");
}
</script>\n</body>`);
  const html = template.replace(/\{\{(?:TITLE|BG|FG|MANIFEST_JSON|SVG_JS_STRING)\}\}|RUNTIME_JS/g, (token) => values[token]);
  return { html, nonce, manifest: deck.manifest };
}

const MAX_DECK_SOURCE = 8 * 1024 * 1024;
const MAX_DECK_BLOCK = 32 * 1024;

function authorText(value, maximum, name) {
  if (typeof value !== "string" || value.length > maximum) throw new Error(`${name} exceeds the authoring limit or is not text.`);
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw new Error(`${name} contains an unpaired surrogate.`);
  }
  return value;
}

function freezeDeck(value) {
  if (value && typeof value === "object") {
    Object.values(value).forEach(freezeDeck);
    Object.freeze(value);
  }
  return value;
}

/** Validate an authored deck, not a resolved manifest. JSON is the editable interchange;
 * existing JSON5 directives stay byte-for-byte in the source, never reserialized here. */
export function deckDraft(json) {
  authorText(json, MAX_DECK_BLOCK, "Deck draft");
  const spec = JSON.parse(json);
  function object(value, keys, name) {
    if (!value || Array.isArray(value) || typeof value !== "object") throw new Error(`${name} must be an object.`);
    for (const key of Object.keys(value)) if (!keys.includes(key)) throw new Error(`Unknown ${name} field: ${key}`);
  }
  function caption(value, name) {
    authorText(value, 1024, name);
    if (Array.from(value).length > 512) throw new Error(`${name} exceeds 512 characters.`);
  }
  function number(value, minimum, maximum, name) {
    if (typeof value !== "number" || !Number.isFinite(value) || value < minimum || value > maximum) throw new Error(`Invalid ${name}.`);
  }
  function selectors(values, name) {
    if (!Array.isArray(values) || !values.length || values.length > 512) throw new Error(`${name} needs 1–512 selectors.`);
    const seen = new Set();
    for (const value of values) {
      authorText(value, 1024, name);
      if (!value.trim() || seen.has(value)) throw new Error(`${name} has an empty or repeated selector.`);
      seen.add(value);
    }
  }
  object(spec, ["title", "options", "overview", "tips", "slides"], "deck");
  if (spec.title !== undefined) caption(spec.title, "Deck title");
  if (spec.options !== undefined) {
    object(spec.options, ["fitMargin", "zoomMax", "dimOpacity", "autoAdvanceMs"], "options");
    for (const [key, value] of Object.entries(spec.options)) {
      number(value, key === "zoomMax" ? Number.MIN_VALUE : 0,
        key === "dimOpacity" ? 1 : key === "autoAdvanceMs" ? 600000 : 100000, key);
      if (key === "autoAdvanceMs" && !Number.isInteger(value)) throw new Error("Autoplay must use whole milliseconds.");
    }
  }
  if (spec.overview !== undefined) {
    object(spec.overview, ["title", "caption", "enabled", "tour"], "overview");
    for (const [key, value] of Object.entries(spec.overview)) {
      if (["enabled", "tour"].includes(key)) {
        if (typeof value !== "boolean") throw new Error(`Overview ${key} must be a boolean.`);
      } else caption(value, `Overview ${key}`);
    }
  }
  if (spec.tips !== undefined) {
    if (!spec.tips || typeof spec.tips !== "object" || Array.isArray(spec.tips)) throw new Error("Tips must be an object.");
    for (const [key, value] of Object.entries(spec.tips)) {
      authorText(key, 1024, "Tooltip node ID");
      caption(value, "Tooltip");
    }
  }
  if (!Array.isArray(spec.slides) || !spec.slides.length || spec.slides.length > 64) throw new Error("A deck needs 1–64 slides.");
  const ids = new Set();
  for (const slide of spec.slides) {
    object(slide, ["id", "title", "caption", "nodes", "reveal", "edges", "fitMargin", "zoomMax"], "slide");
    authorText(slide.id, 128, "Slide ID");
    if (!slide.id.trim() || ids.has(slide.id)) throw new Error("Slide IDs must be nonempty and unique.");
    ids.add(slide.id);
    for (const key of ["title", "caption"]) if (slide[key] !== undefined) caption(slide[key], `Slide ${key}`);
    selectors(slide.nodes, "Slide membership");
    if (slide.reveal !== undefined && slide.reveal !== "auto") {
      if (!Array.isArray(slide.reveal) || slide.reveal.length > 64) throw new Error("Reveal must be auto or up to 64 selector groups.");
      const revealed = new Set();
      for (const group of slide.reveal) {
        selectors(group, "Reveal group");
        for (const selector of group) {
          if (revealed.has(selector)) throw new Error("A selector cannot appear in two reveal groups.");
          revealed.add(selector);
        }
      }
    }
    if (slide.edges !== undefined && !["induced", "touching", "none"].includes(slide.edges)) throw new Error("Invalid slide edge policy.");
    if (slide.fitMargin !== undefined) number(slide.fitMargin, 0, 100000, "slide margin");
    if (slide.zoomMax !== undefined) number(slide.zoomMax, Number.MIN_VALUE, 100000, "slide zoom");
  }
  return freezeDeck(spec);
}

/** Append ONE independent directive. No scan of Mermaid labels, comments or JSON5 is needed.
 * A composition session remembers its own exact suffix and can replace it on subsequent edits. */
export function appendDeckDraft(source, json) {
  authorText(source, MAX_DECK_SOURCE, "Source");
  const spec = deckDraft(json);
  const newline = /\r\n|\r|\n/u.exec(source)?.[0] || "\n";
  // Escaping physical line separators and % also prevents a caption containing }%% from
  // looking like the lexical end of a directive. JSON decoding restores the exact text.
  const payload = JSON.stringify(spec).replace(/[%\u2028\u2029]/gu,
    character => `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`);
  const block = `%%{deck: ${payload}}%%${newline}`;
  if (new TextEncoder().encode(block).length > MAX_DECK_BLOCK) throw new Error("Deck directive exceeds 32 KiB. Split it into smaller compositions.");
  const separator = source && !/[\r\n]$/u.test(source) ? newline : "";
  const updatedSource = source + separator + block;
  authorText(updatedSource, MAX_DECK_SOURCE, "Updated source");
  return Object.freeze({ source, updatedSource, block, spec });
}

/** Read author IDs only from the engine's addressable SVG groups. This does not parse source
 * or mount SVG. Large catalogs are searchable by the UI without creating one control per node. */
export function deckNodeCatalog(svg, DOMParserClass = globalThis.DOMParser) {
  authorText(svg, 16 * 1024 * 1024, "SVG");
  const parsed = new DOMParserClass().parseFromString(svg, "image/svg+xml");
  const root = parsed.documentElement;
  if (parsed.doctype || parsed.querySelector("parsererror") || root?.localName !== "svg" ||
      root.namespaceURI !== "http://www.w3.org/2000/svg") throw new Error("The engine returned invalid SVG for node selection.");
  const nodes = new Map();
  for (const element of root.querySelectorAll('[id^="fm-node-"][data-id]')) {
    const id = element.getAttribute("data-id");
    if (id && !nodes.has(id)) {
      const label = element.querySelector("title")?.textContent || id;
      nodes.set(id, Object.freeze({ id, label }));
    }
  }
  return Object.freeze([...nodes.values()]);
}

function checkAuthoredSlides(spec, before, after) {
  const previous = before?.slides || [], resolved = after.slides;
  if (resolved.length !== previous.length + spec.slides.length ||
      previous.some((slide, index) => slide.id !== resolved[index].id)) {
    throw new Error("The engine dropped slides or changed the existing slide order. The source was not changed.");
  }
  for (const [index, slide] of spec.slides.entries()) {
    const actual = resolved[previous.length + index];
    if (actual.id !== slide.id || actual.title !== (slide.title ?? slide.id) ||
        (actual.caption || "") !== (slide.caption || "") || !actual.nodes.length) {
      throw new Error(`Slide '${slide.id}' was renamed, truncated or could not be resolved.`);
    }
    const members = new Map(actual.nodes.map(node => [node.sourceId, node.step]));
    const literal = selector => selector !== "*" && !selector.startsWith("subgraph:");
    for (const selector of slide.nodes.filter(literal)) {
      if (!members.has(selector)) throw new Error(`Slide '${slide.id}' is missing node '${selector}'.`);
    }
    if (Array.isArray(slide.reveal)) for (const [step, group] of slide.reveal.entries()) {
      for (const selector of group.filter(literal)) {
        if (members.get(selector) !== step + 1) throw new Error(`Slide '${slide.id}' could not reveal '${selector}' at step ${step + 1}.`);
      }
    }
    if (slide.reveal === undefined && actual.maxStep !== 0) throw new Error(`Slide '${slide.id}' has unexpected reveal steps.`);
    if (slide.edges === "none" && actual.edges?.length) throw new Error(`Slide '${slide.id}' still contains edges.`);
  }
}

/** Compose against an exact source revision. The engine resolves every selector/reveal and
 * supplies the paired SVG/manifest; JavaScript never computes graph membership or geometry. */
export class DeckAuthoringSession {
  #render;
  #cancel;
  #checkElements;
  #source = "";
  #base = "";
  #revision = 0;
  #prepared = null;
  #disposed = false;
  #change = null;

  constructor({ renderDeck, cancelRender = () => {}, checkElements = checkDeckElements }) {
    if (![renderDeck, cancelRender, checkElements].every(fn => typeof fn === "function")) throw new TypeError("Deck authoring requires a renderer and SVG validator.");
    this.#render = renderDeck;
    this.#cancel = cancelRender;
    this.#checkElements = checkElements;
  }
  get source() { return this.#source; }
  get ownsComposition() { return this.#base !== this.#source; }
  get canUndo() { return !this.#disposed && this.#change !== null && this.#source === this.#change.after && this.#change.before !== this.#change.after; }
  get canRedo() { return !this.#disposed && this.#change !== null && this.#source === this.#change.before && this.#change.before !== this.#change.after; }
  setSource(source) {
    if (this.#disposed) throw abortError();
    this.cancel();
    this.#change = null;
    authorText(source, MAX_DECK_SOURCE, "Source");
    this.#source = this.#base = source;
  }
  observeSource(source) {
    if (this.#disposed) throw abortError();
    this.cancel(); this.#change = null;
    authorText(source, MAX_DECK_SOURCE, "Source");
    // A known, unchanged trailing block can survive edits to the diagram before it. This
    // is an exact suffix check, not a search for similar directives or a source parser.
    // Document switches MUST use setSource, even if their text happens to be identical.
    const suffix = this.#source.slice(this.#base.length);
    const retained = !!suffix && source.endsWith(suffix);
    this.#base = retained ? source.slice(0, -suffix.length) : source;
    this.#source = source;
    return retained;
  }
  cancel() { this.#revision += 1; this.#prepared = null; this.#cancel(); }
  async prepare(json) {
    if (this.#disposed) throw abortError();
    this.cancel();
    const version = this.#revision, source = this.#source;
    const plan = appendDeckDraft(this.#base, json);
    const live = () => {
      if (this.#disposed || version !== this.#revision || source !== this.#source) throw abortError();
    };
    // Inspect existing slides separately so duplicate IDs / truncation cannot masquerade as
    // a successfully appended slide. Each artifact still comes from ONE engine invocation.
    const original = await this.#render(this.#base);
    live();
    if (typeof original?.svg !== "string" || !Array.isArray(original.warnings)) throw new Error("Invalid original deck result.");
    const previous = original.manifest == null ? null : checkedDeck(original).manifest;
    if ((previous?.slides.length || 0) + plan.spec.slides.length > 64) throw new Error("The existing presentation plus this composition exceeds 64 slides.");
    for (const slide of plan.spec.slides) if (previous?.slides.some(old => old.id === slide.id)) {
      throw new Error(`Slide ID '${slide.id}' is already used in the source. Choose another ID.`);
    }
    const result = await this.#render(plan.updatedSource);
    live();
    const deck = checkedDeck(result);
    checkAuthoredSlides(plan.spec, previous, deck.manifest);
    await this.#checkElements(deck);
    live();
    // Own all response data: a caller or a transport cannot mutate a reviewed confirmation.
    const snapshot = JSON.parse(deckJson(deck));
    this.#prepared = freezeDeck({ source, updatedSource: plan.updatedSource,
      spec: plan.spec, deck: snapshot, existingSlides: previous?.slides.length || 0 });
    return this.#prepared;
  }
  commit(prepared) {
    if (this.#disposed || !prepared || prepared !== this.#prepared || prepared.source !== this.#source) throw abortError();
    this.#change = { before: this.#source, after: prepared.updatedSource };
    this.#source = prepared.updatedSource;
    this.#prepared = null;
    this.#revision += 1;
    return this.#source;
  }
  undo() {
    if (!this.canUndo) throw abortError();
    this.cancel(); this.#source = this.#change.before; return this.#source;
  }
  redo() {
    if (!this.canRedo) throw abortError();
    this.cancel(); this.#source = this.#change.after; return this.#source;
  }
  dispose() { if (!this.#disposed) { this.#disposed = true; this.cancel(); } }
}

/** Visual composition owns a draft, not a second document. Applying publishes one normal
 * source-input event, so recovery, history, sharing and exports see the same directive. */
export function mountDeckComposer({ sourceEl, panelEl, backend, loadAssets = createDeckAssetLoader(),
  timeoutMs = 60000, saveFile }) {
  if (!backend || ![backend.renderDeck, backend.cancel, backend.dispose].every(fn => typeof fn === "function")) throw new TypeError("Deck composition requires a rendering backend.");
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000) throw new RangeError("Invalid composition timeout.");
  const document = panelEl.ownerDocument, host = document.defaultView;
  const session = new DeckAuthoringSession({ renderDeck: source => backend.renderDeck(source),
    cancelRender: () => backend.cancel(), checkElements: deck => checkDeckElements(deck, host.DOMParser) });
  session.setSource(sourceEl.value);
  let spec = { slides: [] }, selected = -1, catalog = [], usedIds = new Set();
  let prepared = null, frame = null, frameWait = null, frameNonce = null, operation = 0, cancelWait = null, deadline;
  let disposed = false, suspended = false, busy = false, ownInput = false, dirty = false;
  const listeners = [], urls = new Map();
  function make(tag, text, parent = panelEl, suffix) {
    const element = document.createElement(tag);
    if (text) element.textContent = text;
    if (suffix) element.id = `deck-compose-${suffix}`;
    parent.append(element);
    return element;
  }
  function listen(target, type, fn) {
    target.addEventListener(type, fn);
    listeners.push(() => target.removeEventListener(type, fn));
  }
  function button(text, suffix, parent = panelEl) {
    const element = make("button", text, parent, suffix); element.type = "button"; return element;
  }
  function field(text, tag, suffix, parent = panelEl) {
    const label = make("label", text, parent);
    label.style.display = "block";
    const element = make(tag, "", label, suffix);
    element.style.cssText = "display:block;box-sizing:border-box;width:100%;max-width:100%";
    if (tag === "textarea") element.rows = 2;
    return element;
  }
  function options(select, entries) {
    for (const [value, text] of entries) { const option = make("option", text, select); option.value = value; }
  }
  make("h3", "Compose presentation slides");
  make("p", "Add slides without rewriting existing directives. Select nodes, arrange the draft, then validate and preview before applying it to source. Existing slides remain first. The draft stays here when this panel closes; download its JSON to keep it across page reloads.");
  const deckTitle = field("Presentation title (blank inherits existing title)", "input", "title");
  const loadNodes = button("Load nodes from current diagram", "load-nodes");
  const catalogStatus = make("p", "Node choices have not been loaded.", panelEl, "catalog-status");
  const slideList = field("Slides in this composition", "select", "slides");
  const actions = make("div");
  const add = button("Add slide", "add", actions), duplicate = button("Duplicate slide", "duplicate", actions);
  const up = button("Move earlier", "up", actions), down = button("Move later", "down", actions);
  const remove = button("Remove from draft", "remove", actions);
  const fields = make("fieldset", "", panelEl, "fields");
  make("legend", "Selected slide", fields);
  const id = field("Slide ID", "input", "id", fields);
  const title = field("Slide title (blank uses ID)", "input", "slide-title", fields);
  const caption = field("Narration / caption", "textarea", "caption", fields);
  const members = field('Node selectors as JSON (IDs, "subgraph:KEY", or "*")', "textarea", "members", fields);
  const all = button("Use whole diagram (*)", "all", fields);
  const search = field("Find explicit node selectors", "input", "search", fields); search.type = "search";
  const picker = make("div", "", fields, "nodes");
  picker.style.cssText = "max-height:180px;overflow:auto";
  const count = make("p", "", fields, "node-count");
  const reveal = field("Reveal order", "select", "reveal", fields);
  options(reveal, [["none", "All visible on entry"], ["auto", "Automatic — engine dependency order"], ["groups", "Authored step groups"]]);
  const groups = field('Reveal groups as JSON (for example [["b"],["c"]])', "textarea", "groups", fields);
  const edges = field("Edges", "select", "edges", fields);
  options(edges, [["induced", "Between slide members"], ["touching", "All touching edges"], ["none", "No edges"]]);
  const margin = field("Camera margin (blank inherits)", "input", "margin", fields);
  const zoom = field("Maximum zoom (blank inherits)", "input", "zoom", fields);
  for (const input of [margin, zoom]) { input.type = "number"; input.min = "0"; input.step = "any"; }
  const validate = button("Validate and preview composition", "validate");
  const apply = button("Apply composition to source", "apply");
  const cancel = button("Cancel review", "cancel");
  const undo = button("Undo source application", "undo"), redo = button("Redo source application", "redo");
  const downloadDraft = button("Download composition JSON", "download");
  const importDraft = field("Replace draft from composition JSON", "input", "import");
  importDraft.type = "file"; importDraft.accept = ".json,application/json";
  const status = make("p", "No source changes until Apply.", panelEl, "status"); status.setAttribute("role", "status");
  const review = make("pre", "", panelEl, "review"); review.style.cssText = "white-space:pre-wrap;overflow-wrap:anywhere;max-height:220px;overflow:auto";
  const preview = make("div", "", panelEl, "preview");

  function controls() {
    const locked = disposed || suspended;
    for (const control of [deckTitle, loadNodes, slideList, add, downloadDraft, importDraft]) control.disabled = locked || busy;
    fields.disabled = locked || busy || selected < 0;
    duplicate.disabled = locked || busy || selected < 0 || spec.slides.length >= 64;
    remove.disabled = locked || busy || selected < 0;
    up.disabled = locked || busy || selected <= 0;
    down.disabled = locked || busy || selected < 0 || selected >= spec.slides.length - 1;
    add.disabled ||= spec.slides.length >= 64;
    validate.disabled = locked || busy || !spec.slides.length;
    apply.disabled = locked || busy || !prepared || prepared.source !== sourceEl.value;
    undo.disabled = locked || busy || !session.canUndo || session.source !== sourceEl.value;
    redo.disabled = locked || busy || !session.canRedo || session.source !== sourceEl.value;
    cancel.disabled = locked || (!busy && !prepared);
    groups.parentElement.hidden = reveal.value !== "groups";
    groups.parentElement.style.display = reveal.value === "groups" ? "block" : "none";
    panelEl.setAttribute("aria-busy", String(busy));
  }
  function invalidate(message) {
    operation += 1; session.cancel(); prepared = null; busy = false;
    host.clearTimeout(deadline);
    if (cancelWait) { cancelWait(abortError()); cancelWait = null; }
    if (frameWait) { frameWait.reject(abortError()); frameWait = null; }
    frame?.remove(); frame = null; frameNonce = null; review.textContent = "";
    if (message) status.textContent = message;
    controls();
  }
  function changed(event) {
    dirty = true; invalidate("Draft changed. Validate again before applying.");
    if (event?.target === members) fillNodes();
  }
  function saveCurrent() {
    if (selected >= 0) {
      const entry = { ...spec.slides[selected], id: id.value, nodes: JSON.parse(members.value) };
      for (const [key, control] of [["title", title], ["caption", caption], ["fitMargin", margin], ["zoomMax", zoom]]) {
        if (control.value === "") delete entry[key];
        else entry[key] = control === margin || control === zoom ? Number(control.value) : control.value;
      }
      entry.edges = edges.value;
      if (reveal.value === "none") delete entry.reveal;
      else entry.reveal = reveal.value === "auto" ? "auto" : JSON.parse(groups.value);
      spec.slides[selected] = entry;
    }
    if (deckTitle.value) spec.title = deckTitle.value; else delete spec.title;
  }
  function fillNodes() {
    picker.replaceChildren();
    let chosen;
    try { chosen = JSON.parse(members.value); if (!Array.isArray(chosen)) return; }
    catch { count.textContent = "Repair the selector JSON before using node checkboxes."; return; }
    const query = search.value.toLowerCase();
    let matches = 0;
    for (const [index, node] of catalog.entries()) {
      if (!`${node.id} ${node.label}`.toLowerCase().includes(query)) continue;
      matches += 1;
      if (matches > 200) continue;
      const label = make("label", "", picker); label.style.display = "block";
      const box = make("input", "", label); box.type = "checkbox"; box.checked = chosen.includes(node.id);
      box.dataset.nodeIndex = String(index);
      label.append(document.createTextNode(` ${node.id} — ${node.label}`));
    }
    count.textContent = `${chosen.length} explicit selectors. Showing ${Math.min(matches, 200)} of ${matches} matching nodes; search to narrow. Group selectors remain in the JSON list.`;
  }
  function fill() {
    slideList.replaceChildren();
    for (const [index, slide] of spec.slides.entries()) {
      const option = make("option", `${index + 1}. ${slide.title || slide.id}`, slideList); option.value = String(index);
    }
    slideList.value = String(selected);
    const slide = spec.slides[selected];
    id.value = slide?.id || ""; title.value = slide?.title || ""; caption.value = slide?.caption || "";
    members.value = JSON.stringify(slide?.nodes || []);
    reveal.value = slide?.reveal === "auto" ? "auto" : Array.isArray(slide?.reveal) ? "groups" : "none";
    groups.value = JSON.stringify(Array.isArray(slide?.reveal) ? slide.reveal : []);
    edges.value = slide?.edges || "induced";
    margin.value = slide?.fitMargin ?? ""; zoom.value = slide?.zoomMax ?? "";
    deckTitle.value = spec.title || "";
    fillNodes(); controls();
  }
  function nextId() {
    let i = 1;
    while (usedIds.has(`slide-${i}`) || spec.slides.some(slide => slide.id === `slide-${i}`)) i++;
    return `slide-${i}`;
  }
  function edit(fn) {
    try { saveCurrent(); fn(); changed(); fill(); }
    catch (error) { slideList.value = String(selected); status.textContent = `Draft not changed: ${error.message}`; }
  }
  async function execute(label, work) {
    if (disposed || suspended) return;
    invalidate();
    const version = operation, source = sourceEl.value;
    const live = () => !disposed && !suspended && version === operation && source === sourceEl.value;
    const check = () => { if (!live()) throw abortError(); };
    busy = true; controls(); status.textContent = label;
    const cancelled = new Promise((_resolve, reject) => { cancelWait = reject; });
    deadline = host.setTimeout(() => invalidate("Composition work timed out. Your source and draft were retained."), timeoutMs);
    try { await Promise.race([Promise.resolve().then(() => work(check, source)), cancelled]); }
    catch (error) {
      if (live()) {
        session.cancel(); prepared = null; frame?.remove(); frame = null;
        status.textContent = `Composition unavailable: ${error.message || error}`;
      }
    } finally {
      if (version === operation) {
        host.clearTimeout(deadline); cancelWait = null; frameWait = null; busy = false; controls();
      }
    }
  }
  async function loadCatalog() {
    return execute("Reading node IDs from the current engine SVG…", async (check, source) => {
      const result = await backend.renderDeck(source); check();
      catalog = deckNodeCatalog(result.svg, host.DOMParser);
      usedIds = new Set(result.manifest == null ? [] : checkedDeck(result).manifest.slides.map(slide => slide.id));
      catalogStatus.textContent = `${catalog.length} addressable node IDs loaded. Existing slides: ${usedIds.size}.`;
      fillNodes();
      status.textContent = catalog.length ? "Choose slide members from the node list or enter group selectors." : "This diagram has no addressable node groups; deck composition may not be supported.";
    });
  }
  async function prepare() {
    try { saveCurrent(); }
    catch (error) { invalidate(`Repair draft JSON: ${error.message}`); return; }
    const json = JSON.stringify(spec);
    return execute("Resolving the draft presentation in the engine…", async (check, source) => {
      if (session.source !== source) session.observeSource(source);
      const candidate = await session.prepare(json); check();
      const assets = await loadAssets(); check();
      const built = buildDeckHtml(candidate.deck, assets);
      frameNonce = built.nonce;
      frame = make("iframe", "", preview);
      frame.title = "Draft presentation preview — source not yet changed";
      frame.setAttribute("sandbox", "allow-scripts"); frame.setAttribute("allow", "fullscreen");
      frame.setAttribute("referrerpolicy", "no-referrer");
      frame.style.cssText = "width:100%;height:360px;border:1px solid #cbd5e1";
      const ready = new Promise((resolve, reject) => { frameWait = { nonce: built.nonce, resolve, reject }; });
      frame.srcdoc = built.html;
      await ready; check();
      const manifest = candidate.deck.manifest;
      review.textContent = manifest.slides.map((slide, index) =>
        `${index + 1}. ${slide.title} [${slide.id}] — ${slide.nodes.length} nodes, ${slide.maxStep} reveal steps\n` +
        slide.nodes.map(node => `  ${node.sourceId} (step ${node.step})`).join("\n")).join("\n\n");
      const warnings = candidate.deck.warnings.map(warning => typeof warning === "string" ? warning : warning.message || "Deck warning");
      if (warnings.length) review.textContent += `\n\nEngine warnings:\n${warnings.join("\n")}`;
      prepared = candidate;
      status.textContent = `${spec.slides.length} draft slides resolved; ${candidate.existingSlides} existing slides retained. Review the presentation${warnings.length ? " and warnings" : ""}, then Apply. Source is unchanged.`;
    });
  }
  function applyDraft() {
    try {
      if (disposed || suspended || busy || !prepared || prepared.source !== sourceEl.value) throw abortError();
      const updated = session.commit(prepared);
      invalidate(); dirty = false;
      ownInput = true;
      try { sourceEl.value = updated; sourceEl.dispatchEvent(new host.Event("input", { bubbles: true })); }
      finally { ownInput = false; }
      if (sourceEl.value !== updated) { sourceChanged(true); return; }
      status.textContent = "Composition applied to source. You can revise these draft slides again; only this composition's exact block will be replaced.";
      controls();
    } catch (error) { invalidate(`Apply refused: ${error.message}`); }
  }
  function restoreSource(direction) {
    try {
      if (disposed || suspended || busy || session.source !== sourceEl.value) throw abortError();
      const updated = direction === "undo" ? session.undo() : session.redo();
      invalidate(); dirty = true;
      ownInput = true;
      try { sourceEl.value = updated; sourceEl.dispatchEvent(new host.Event("input", { bubbles: true })); }
      finally { ownInput = false; }
      if (sourceEl.value !== updated) { sourceChanged(true); return; }
      status.textContent = `Source ${direction === "undo" ? "application undone" : "application restored"}. The draft is retained; validate before applying more changes.`;
      controls();
    } catch (error) { invalidate(`Source restore refused: ${error.message}`); }
  }
  function sourceChanged(force = false) {
    if (disposed || (ownInput && !force) || (!force && sourceEl.value === session.source)) return;
    invalidate();
    catalog = []; usedIds.clear(); catalogStatus.textContent = "Source changed; reload node choices.";
    try {
      const retained = force ? (session.setSource(sourceEl.value), false) : session.observeSource(sourceEl.value);
      status.textContent = retained
        ? "Diagram source changed. Draft retained and its unchanged source block is still editable. Reload nodes and validate again."
        : "The source or document changed. Draft retained, but its old source block is no longer editable here. Revalidate as a new composition; existing IDs must be unique.";
    } catch (error) { status.textContent = error.message; }
    fillNodes();
    controls();
  }
  function download(file) {
    if (saveFile) return saveFile(file);
    const url = host.URL.createObjectURL(new host.Blob([file.text], { type: file.mime }));
    const link = make("a", "", document.body); link.href = url; link.download = file.filename;
    try { link.click(); } finally {
      link.remove(); urls.set(url, host.setTimeout(() => { host.URL.revokeObjectURL(url); urls.delete(url); }, 1000));
    }
  }
  listen(host, "message", event => {
    if (!frameNonce || event.source !== frame?.contentWindow || event.data?.token !== frameNonce) return;
    if (event.data.kind === "fm-deck-ready") frameWait?.resolve();
    else if (event.data.kind === "fm-deck-error") invalidate(`Draft presentation failed: ${event.data.reason || "Runtime error"}. Source and draft retained.`);
  });
  listen(host, "beforeunload", event => { if (dirty) { event.preventDefault(); event.returnValue = ""; } });
  listen(sourceEl, "input", () => sourceChanged());
  for (const control of [deckTitle, id, title, caption, members, groups, margin, zoom]) listen(control, "input", changed);
  for (const control of [reveal, edges]) listen(control, "change", changed);
  listen(search, "input", fillNodes);
  listen(picker, "change", event => {
    if (event.target.dataset.nodeIndex === undefined) return;
    const node = catalog[Number(event.target.dataset.nodeIndex)];
    if (!node) return;
    edit(() => {
      const entry = spec.slides[selected];
      entry.nodes = event.target.checked ? [...new Set([...entry.nodes, node.id])] : entry.nodes.filter(id => id !== node.id);
    });
  });
  listen(slideList, "change", () => { const index = Number(slideList.value); edit(() => { selected = index; }); });
  listen(add, "click", () => edit(() => { spec.slides.push({ id: nextId(), nodes: [] }); selected = spec.slides.length - 1; }));
  listen(duplicate, "click", () => edit(() => { spec.slides.splice(selected + 1, 0, { ...JSON.parse(JSON.stringify(spec.slides[selected])), id: nextId() }); selected++; }));
  listen(remove, "click", () => edit(() => { spec.slides.splice(selected, 1); selected = Math.min(selected, spec.slides.length - 1); }));
  for (const [control, delta] of [[up, -1], [down, 1]]) listen(control, "click", () => edit(() => {
    const next = selected + delta;
    [spec.slides[selected], spec.slides[next]] = [spec.slides[next], spec.slides[selected]]; selected = next;
  }));
  listen(all, "click", () => edit(() => { spec.slides[selected].nodes = ["*"]; }));
  listen(loadNodes, "click", () => { void loadCatalog(); });
  listen(validate, "click", () => { void prepare(); });
  listen(apply, "click", applyDraft);
  listen(cancel, "click", () => invalidate("Review cancelled. Source and draft retained."));
  listen(undo, "click", () => restoreSource("undo"));
  listen(redo, "click", () => restoreSource("redo"));
  listen(downloadDraft, "click", async () => {
    try { saveCurrent(); const json = JSON.stringify(deckDraft(JSON.stringify(spec)), null, 2);
      await download({ text: json + "\n", filename: "diagram-composition.json", mime: "application/json;charset=utf-8" });
    } catch (error) { if (!disposed) status.textContent = `Draft download failed: ${error.message}`; }
  });
  listen(importDraft, "change", () => {
    const file = importDraft.files?.[0]; importDraft.value = "";
    if (!file) return;
    void execute("Reading composition JSON…", async check => {
      if (file.size > MAX_DECK_BLOCK) throw new Error("Composition JSON exceeds 32 KiB.");
      const bytes = await file.arrayBuffer(); check();
      if (bytes.byteLength !== file.size || bytes.byteLength > MAX_DECK_BLOCK) throw new Error("Composition file size changed.");
      const imported = deckDraft(new host.TextDecoder("utf-8", { fatal: true }).decode(bytes));
      spec = JSON.parse(JSON.stringify(imported)); selected = 0; dirty = true; fill();
      status.textContent = "Composition JSON loaded into the draft. Validate it before changing source.";
    });
  });
  fill();
  return { prepare, apply: applyDraft, loadCatalog, sourceChanged,
    suspend() { suspended = true; invalidate("Draft retained while the presentation panel is closed."); },
    resume() { if (!disposed) { suspended = false; sourceChanged(); controls(); } },
    dispose() {
      if (disposed) return;
      disposed = true; invalidate(); session.dispose(); backend.dispose();
      for (const remove of listeners) remove();
      for (const [url, timer] of urls) { host.clearTimeout(timer); host.URL.revokeObjectURL(url); }
      urls.clear();
    },
  };
}

/** An opt-in live deck preview. Hosts call sourceChanged for programmatic edits as well as
 * typing. Closing the panel destroys the iframe and cancels work; reopening reuses no stale
 * presentation. Browser I/O is injectable for tests, never a second rendering implementation. */
export function mountDeckEditor({ sourceEl, panelEl, loadModule, workerOptions = {},
  loadAssets = createDeckAssetLoader(), saveFile, previewTimeoutMs = 10000 }) {
  if (!Number.isSafeInteger(previewTimeoutMs) || previewTimeoutMs <= 0) throw new Error("Invalid deck preview timeout.");
  const document = panelEl.ownerDocument;
  const host = document.defaultView;
  const backend = createSourceEditorBackend({ loadModule, ...workerOptions });
  let revision = 0;
  let disposed = false;
  let active = true;
  let observedSource = sourceEl.value;
  let timer;
  let frame;
  let waiting;
  let artifact;
  let composer = null;
  const downloads = new Map();
  const listeners = [];
  function listen(element, event, handler) {
    element.addEventListener(event, handler);
    listeners.push(() => element.removeEventListener(event, handler));
  }
  function make(tag, text, id) {
    const element = document.createElement(tag);
    element.textContent = text;
    if (id) element.id = id;
    panelEl.append(element);
    return element;
  }
  make("h2", "Graph deck");
  make("p", "Compose slides visually or author a %%{deck: …}%% directive. Preview the presentation, then save one playable HTML file. External resources are blocked; embed images as data URIs.");
  const example = make("details", "");
  const exampleTitle = document.createElement("summary");
  exampleTitle.textContent = "Deck directive example (replace a, b, c with your node IDs)";
  const exampleSource = document.createElement("pre");
  exampleSource.textContent = "%%{deck: {\n  title: 'My presentation',\n  slides: [\n    {id: 'start', title: 'Start', nodes: ['a', 'b'], reveal: [['b']]},\n    {id: 'next', title: 'Next', nodes: ['b', 'c']}\n  ]\n}}%%";
  example.append(exampleTitle, exampleSource);
  const composition = make("details", "", "deck-editor-composition");
  const compositionTitle = document.createElement("summary");
  compositionTitle.textContent = "Compose slides visually";
  const compositionPanel = document.createElement("section");
  compositionPanel.setAttribute("aria-label", "Presentation composition");
  compositionPanel.style.cssText = "max-height:75vh;overflow:auto;padding:8px";
  composition.append(compositionTitle, compositionPanel);
  listen(composition, "toggle", () => {
    if (disposed) return;
    if (composition.open && !composer) {
      // Do not compete with the live presentation's worker or allocate another WASM instance
      // merely to show the form. The composer owns its worker after the first explicit load.
      let authorBackend = null;
      composer = mountDeckComposer({ sourceEl, panelEl: compositionPanel, loadAssets, saveFile,
        backend: {
          renderDeck(source) {
            authorBackend ||= createSourceEditorBackend({ loadModule, ...workerOptions });
            return authorBackend.renderDeck(source);
          },
          cancel() { authorBackend?.cancel(); },
          dispose() { authorBackend?.dispose(); },
        },
      });
    }
    if (composition.open && active) composer?.resume(); else composer?.suspend();
  });
  const refresh = make("button", "Refresh presentation", "deck-editor-refresh");
  const saveHtml = make("button", "Save presentation (.html)", "deck-editor-save-html");
  const saveManifest = make("button", "Save manifest (.json)", "deck-editor-save-manifest");
  const close = make("button", "Close presentation", "deck-editor-close");
  for (const button of [refresh, saveHtml, saveManifest, close]) button.type = "button";
  const automaticLabel = make("label", " Refresh while typing ");
  const automatic = document.createElement("input");
  automatic.type = "checkbox";
  automatic.checked = true;
  automaticLabel.prepend(automatic);
  const status = make("p", "", "deck-editor-status");
  status.setAttribute("role", "status");
  const preview = make("div", "", "deck-editor-preview");
  function sync() {
    const current = !disposed && artifact?.source === sourceEl.value;
    saveHtml.disabled = !current;
    saveManifest.disabled = !current;
    refresh.disabled = disposed;
  }
  function invalidate() {
    revision += 1;
    backend.cancel();
    host.clearTimeout(timer);
    if (waiting) { waiting.reject(abortError()); waiting = null; }
    frame?.remove();
    frame = null;
    artifact = null;
    preview.setAttribute("aria-busy", "false");
    sync();
  }
  function requireCurrent() {
    if (disposed || !artifact || artifact.source !== sourceEl.value) throw new Error("Render the current source before exporting its presentation.");
    return artifact;
  }
  function exportHtml() { return { text: requireCurrent().html, filename: "diagram-deck.html", mime: "text/html;charset=utf-8" }; }
  function exportManifest() { return { text: deckJson(requireCurrent().manifest), filename: "diagram-deck.json", mime: "application/json;charset=utf-8" }; }
  function download(file) {
    const url = host.URL.createObjectURL(new host.Blob([file.text], { type: file.mime }));
    const link = document.createElement("a");
    link.href = url;
    link.download = file.filename;
    document.body.append(link);
    try { link.click(); }
    finally {
      link.remove();
      downloads.set(url, host.setTimeout(() => { host.URL.revokeObjectURL(url); downloads.delete(url); }, 1000));
    }
  }
  listen(host, "message", (event) => {
    const token = waiting?.nonce ?? artifact?.nonce;
    if (!token || event.source !== frame?.contentWindow || event.data?.token !== token) return;
    if (waiting && event.data.kind === "fm-deck-ready") { waiting.resolve(); waiting = null; }
    else if (event.data.kind === "fm-deck-error") {
      const error = new Error(event.data.reason || "Deck runtime failed.");
      if (waiting) { waiting.reject(error); waiting = null; }
      else { invalidate(); status.textContent = `Presentation unavailable: ${error.message}`; }
    }
  });
  async function render() {
    if (disposed || !active) return;
    invalidate();
    const version = revision;
    const source = sourceEl.value;
    observedSource = source;
    const live = () => !disposed && active && version === revision && source === sourceEl.value;
    preview.setAttribute("aria-busy", "true");
    status.textContent = `Building presentation — ${backend.target}…`;
    let deadline;
    try {
      const result = await backend.renderDeck(source);
      if (!live()) return;
      const warningText = (result.warnings || []).map((warning) => typeof warning === "string" ? warning : warning.message || "Deck warning").join(" ");
      if (result.manifest == null) {
        status.textContent = `No presentation resolved. Add a deck directive for a supported diagram family. ${warningText}`;
        return;
      }
      const deck = checkedDeck(result);
      checkDeckElements(deck, host.DOMParser);
      const assets = await loadAssets();
      if (!live()) return;
      const built = buildDeckHtml(deck, assets);
      frame = document.createElement("iframe");
      frame.title = "Graph deck presentation preview";
      frame.setAttribute("sandbox", "allow-scripts");
      frame.setAttribute("allow", "fullscreen");
      frame.setAttribute("referrerpolicy", "no-referrer");
      frame.style.cssText = "width:100%;height:420px;border:1px solid #cbd5e1";
      const ready = new Promise((resolve, reject) => {
        const pending = { resolve, reject, nonce: built.nonce };
        waiting = pending;
        deadline = host.setTimeout(() => {
          if (waiting === pending) { waiting.reject(new Error("Presentation did not initialize. Check the runtime assets and content security policy.")); waiting = null; }
        }, previewTimeoutMs);
      });
      frame.srcdoc = built.html;
      preview.append(frame);
      await ready;
      if (!live()) return;
      artifact = { source, ...built };
      status.textContent = `${deck.manifest.slides.length} slides ready — ${backend.target}. ${warningText}` +
        (backend.fallbackReason ? ` Worker unavailable: ${backend.fallbackReason}` : "");
      sync();
    } catch (error) {
      if (live()) {
        frame?.remove(); frame = null;
        status.textContent = `Presentation unavailable: ${error.message || error}`;
      }
    } finally {
      host.clearTimeout(deadline);
      if (live()) preview.setAttribute("aria-busy", "false");
    }
  }
  function sourceChanged(force = false) {
    if (disposed) return;
    composer?.sourceChanged(force);
    if (!force && observedSource === sourceEl.value) return;
    observedSource = sourceEl.value;
    invalidate();
    status.textContent = "Source changed. Refresh the presentation to preview or export this revision.";
    if (active && automatic.checked) timer = host.setTimeout(() => { void render(); }, 350);
  }
  listen(sourceEl, "input", () => sourceChanged());
  listen(refresh, "click", () => { void render(); });
  listen(automatic, "change", () => {
    host.clearTimeout(timer);
    if (automatic.checked && !artifact) void render();
  });
  listen(close, "click", () => { active = false; composer?.suspend(); invalidate(); panelEl.hidden = true; });
  for (const [button, exportFile] of [[saveHtml, exportHtml], [saveManifest, exportManifest]]) {
    listen(button, "click", async () => {
      try { await (saveFile || download)(exportFile()); }
      catch (error) { if (!disposed) status.textContent = `Save failed: ${error.message || error}`; }
    });
  }
  panelEl.hidden = false;
  sync();
  return {
    render, sourceChanged, exportHtml, exportManifest,
    documentChanged() { sourceChanged(true); },
    open() { if (!disposed) { active = true; panelEl.hidden = false; if (composition.open) composer?.resume(); return render(); } },
    dispose() {
      if (disposed) return;
      disposed = true; invalidate(); composer?.dispose(); backend.dispose();
      for (const remove of listeners) remove();
      for (const [url, timeout] of downloads) { host.clearTimeout(timeout); host.URL.revokeObjectURL(url); }
      downloads.clear();
    },
  };
}
