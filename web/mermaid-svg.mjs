// Browser-only SVG presentation adapter. Mermaid parsing and rendering stay in Rust.
const SVG_NS = "http://www.w3.org/2000/svg";
const XML_NS = "http://www.w3.org/XML/1998/namespace";
const XLINK_NS = "http://www.w3.org/1999/xlink";
const FORBIDDEN = new Set(["script", "foreignObject", "iframe", "object", "embed", "animate", "animateMotion", "animateTransform", "set"]);
const ID_RE = /^[A-Za-z_][A-Za-z0-9_-]{0,127}$/u;

export function checkRenderId(id) {
  if (typeof id !== "string" || !ID_RE.test(id)) {
    throw new TypeError("Render ID must start with a letter or underscore and contain at most 128 letters, digits, underscores or hyphens.");
  }
  return id;
}

// Do not accept executable or remote resource URLs from a renderer. Authored hyperlinks are
// different from fetched resources: links may be ordinary web/mail/tel destinations, but a
// paint server, image, stylesheet or <use> can refer only to a definition in this same SVG.
function localUrl(value, ids) {
  const target = value.trim();
  if (!target.startsWith("#") || !ids.has(target.slice(1))) {
    throw new Error("SVG contains an unresolved or external resource reference.");
  }
  return `#${ids.get(target.slice(1))}`;
}

function paintUrls(value, ids) {
  if (!/url\s*\(/iu.test(value)) return value;
  const pattern = /url\s*\(\s*(?:"([^"\r\n]*)"|'([^'\r\n]*)'|([^()'"\s]*))\s*\)/giu;
  if (/url\s*\(/iu.test(value.replace(pattern, ""))) throw new Error("SVG contains a malformed resource URL.");
  return value.replace(pattern, (_, double, single, bare) => `url("${localUrl(double ?? single ?? bare, ids)}")`);
}

function linkUrl(value, ids, document) {
  const trimmed = value.trim();
  if (/^[\s\S]*[\u0000-\u0020\u007f]/u.test(trimmed)) throw new Error("SVG hyperlink contains control characters or whitespace.");
  if (trimmed.startsWith("#") && ids.has(trimmed.slice(1))) return localUrl(trimmed, ids);
  const url = new URL(trimmed, document.baseURI);
  if (!["http:", "https:", "mailto:", "tel:"].includes(url.protocol)) {
    throw new Error("SVG contains an unsafe hyperlink.");
  }
  return trimmed;
}

// CSSOM, not a second CSS parser, handles selector lists, strings and nested media/supports
// rules. Scope both the SVG root and descendants. Keyframe names need the same isolation as
// element IDs: otherwise two independently themed diagrams share a document-global animation.
function scopedStyles(styleNodes, ids, id, window) {
  if (styleNodes.length && typeof window.CSSStyleSheet !== "function") throw new Error("SVG styling requires browser CSSOM support.");
  const sheets = styleNodes.map((node) => {
    const sheet = new window.CSSStyleSheet();
    const text = node.textContent;
    // CSSOM drops @import in constructed stylesheets; reject it rather than silently change it.
    // Rust emits no imports, escape-encoded identifiers, or document-global font definitions.
    if (/\\|@import\b|@font-face\b|@namespace\b/iu.test(text)) throw new Error("SVG contains unsupported external or escaped CSS.");
    sheet.replaceSync(text);
    if (text.trim() && !sheet.cssRules.length) throw new Error("SVG stylesheet cannot be parsed.");
    return sheet;
  });
  const animations = new Map();
  const collect = (rules) => {
    for (const rule of rules) {
      if (rule.type === 7) animations.set(rule.name, `${id}--animation-${animations.size}`);
      else if (rule.cssRules) collect(rule.cssRules);
    }
  };
  sheets.forEach((sheet) => collect(sheet.cssRules));
  function declarations(style) {
    if (/\\|(?:image-set|cross-fade|paint)\s*\(/iu.test(style.cssText)) throw new Error("SVG contains unsupported CSS resources.");
    for (const property of [...style]) {
      let value = paintUrls(style.getPropertyValue(property), ids);
      if (property === "animation" || property === "animation-name") {
        value = value.replace(/[A-Za-z_][A-Za-z0-9_-]*/gu, (word) => animations.get(word) || word);
      }
      style.setProperty(property, value, style.getPropertyPriority(property));
    }
  }
  function transform(rules, inKeyframes = false) {
    for (const rule of rules) {
      if (rule.type === 7) {
        rule.name = animations.get(rule.name);
        transform(rule.cssRules, true);
      } else if (rule.type === 1) {
        // Engine selectors use ASCII IDs. Match a complete token, never a prefix or a color.
        let selector = rule.selectorText.replace(/#[A-Za-z_][A-Za-z0-9_-]*/gu,
          (token) => ids.has(token.slice(1)) ? `#${ids.get(token.slice(1))}` : token);
        selector = selector.replace(/:root\b/gu, `#${id}`);
        rule.selectorText = `#${id}:is(${selector}), #${id} :is(${selector})`;
        declarations(rule.style);
      } else if (inKeyframes && rule.type === 8) declarations(rule.style);
      else if ((rule.type === 4 || rule.type === 12) && rule.cssRules) transform(rule.cssRules);
      else throw new Error("SVG contains an unsupported document-global CSS rule.");
    }
  }
  sheets.forEach((sheet, index) => {
    transform(sheet.cssRules);
    styleNodes[index].textContent = [...sheet.cssRules].map((rule) => rule.cssText).join("\n");
  });
  return declarations;
}

/** Namespace the exact engine SVG for insertion into a shared browser document. */
export function prepareSvg(svg, id, document) {
  checkRenderId(id);
  const window = document?.defaultView;
  if (!window?.DOMParser || !window.XMLSerializer) throw new Error("Rendering requires a browser Document with XML DOM support.");
  if (typeof svg !== "string" || svg.length > 16 * 1024 * 1024 || new TextEncoder().encode(svg).length > 16 * 1024 * 1024) {
    throw new Error("Engine SVG must be text within the 16 MiB limit.");
  }
  const parsed = new window.DOMParser().parseFromString(svg, "image/svg+xml");
  const root = parsed.documentElement;
  if (parsed.doctype || parsed.querySelector("parsererror") || root?.localName !== "svg" || root.namespaceURI !== SVG_NS) {
    throw new Error("Engine returned invalid SVG.");
  }
  const elements = [root, ...root.querySelectorAll("*")];
  const ids = new Map();
  for (const element of elements) {
    if (element.namespaceURI !== SVG_NS || FORBIDDEN.has(element.localName)) throw new Error("Engine SVG contains active or unsupported markup.");
    const original = element.getAttribute("id");
    if (original !== null) {
      if (!/^[A-Za-z_][A-Za-z0-9_-]*$/u.test(original) || ids.has(original)) throw new Error("Engine SVG has invalid or duplicate IDs.");
      ids.set(original, element === root ? id : `${id}--${ids.size}`);
    }
  }
  for (const element of elements) {
    for (const attr of [...element.attributes]) {
      const name = attr.localName;
      if (/^on/iu.test(name) || name === "base" || (attr.namespaceURI && ![XML_NS, XLINK_NS, "http://www.w3.org/2000/xmlns/"].includes(attr.namespaceURI))) {
        throw new Error("Engine SVG contains active attributes.");
      }
      let value = attr.value;
      if (name === "id") value = ids.get(value);
      else if (name === "href") value = element.localName === "a" ? linkUrl(value, ids, document) : localUrl(value, ids);
      else if (["aria-labelledby", "aria-describedby", "aria-controls", "aria-owns"].includes(name)) {
        value = value.trim().split(/\s+/u).map((token) => {
          if (!ids.has(token)) throw new Error("SVG accessibility reference is unresolved.");
          return ids.get(token);
        }).join(" ");
      } else if (name !== "style") value = paintUrls(value, ids);
      element.setAttributeNS(attr.namespaceURI, attr.name, value);
    }
    if (element.localName === "a" && element.getAttribute("target") === "_blank") element.setAttribute("rel", "noopener noreferrer");
  }
  root.setAttribute("id", id);
  const declarations = scopedStyles([...root.querySelectorAll("style")], ids, id, window);
  if (declarations) for (const element of elements) if (element.hasAttribute("style")) declarations(element.style);
  return { svg: new window.XMLSerializer().serializeToString(root), element: document.importNode(root, true) };
}

/** Callback names are opaque registry keys, never expressions or window property paths. */
export function checkCallbackName(name) {
  if (typeof name !== "string" || name.length > 128 ||
      !/^[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*$/u.test(name) ||
      name.split(".").some((part) => ["__proto__", "prototype", "constructor"].includes(part))) {
    throw new TypeError("Callback name must be a safe identifier or dotted registry key within 128 characters.");
  }
  return name;
}

/** Capture an admitted SVG, then bind only an unchanged copy inside the caller's container.
 * No global function lookup, evaluation, source parsing, or callback argument interpretation.
 * The returned cleanup is idempotent; binding the same live root twice returns that cleanup.
 */
export function createSvgBinder(element, { resolveCallback = () => undefined,
  isLive = () => true, reportError = () => {}, onBind = () => {} } = {}) {
  if (![resolveCallback, isLive, reportError, onBind].every((fn) => typeof fn === "function")) {
    throw new TypeError("SVG binding requires callback resolution and lifecycle functions.");
  }
  if (element?.namespaceURI !== SVG_NS || element.localName !== "svg") throw new TypeError("Binding requires a prepared SVG.");
  const template = element.cloneNode(true), id = checkRenderId(template.id);
  const bindings = new WeakMap();
  const report = (error) => {
    try { reportError(error); } catch { /* Reporting must not escape a DOM event or rejected callback. */ }
  };
  return function bindFunctions(container) {
    if (!isLive()) throw new Error("This diagram's interaction owner has been disposed.");
    if (container?.nodeType !== 1 || typeof container.querySelectorAll !== "function") {
      throw new TypeError("bindFunctions requires the SVG or its containing element.");
    }
    const candidates = [...container.querySelectorAll(`[id="${id}"]`)];
    if (container.id === id) candidates.unshift(container);
    if (candidates.length !== 1 || candidates[0].namespaceURI !== SVG_NS || candidates[0].localName !== "svg") {
      throw new Error("The container must contain exactly one SVG from this render result.");
    }
    const root = candidates[0], document = root.ownerDocument;
    const existing = bindings.get(root);
    if (existing) return existing;
    if (!root.isEqualNode(template)) throw new Error("SVG changed before binding; insert this render result before binding its interactions.");

    // Plan all bindings before touching the DOM. Nodes without registered handlers stay inert.
    const plans = new Map(), missing = new Set();
    for (const node of root.querySelectorAll("[data-callback]")) {
      const name = node.getAttribute("data-callback"), sourceId = node.getAttribute("data-id");
      try { checkCallbackName(name); }
      catch (error) { report(error); continue; }
      const callback = resolveCallback(name);
      if (typeof callback !== "function") { missing.add(name); continue; }
      if (!sourceId || node.localName !== "g" || node.closest("svg") !== root) {
        throw new Error("Engine callback metadata has no addressable source node.");
      }
      plans.set(node, { name, sourceId, callback });
    }
    const restorations = [];
    let disposed = false, pressed = null;
    function attribute(node, name, value) {
      const previous = node.getAttribute(name);
      if (previous === value) return;
      node.setAttribute(name, value);
      restorations.push(() => {
        // Do not undo a later host edit while removing our own accessibility decoration.
        if (node.getAttribute(name) !== value) return;
        if (previous === null) node.removeAttribute(name); else node.setAttribute(name, previous);
      });
    }
    function target(event) {
      if (disposed || !isLive() || !root.isConnected || root.ownerDocument !== document || event.defaultPrevented) return null;
      let node = event.target?.nodeType === 1 ? event.target : event.target?.parentElement;
      if (!node || !root.contains(node) || node.closest("svg") !== root) return null;
      if (node.closest('[inert], [hidden], [aria-hidden="true"], [aria-disabled="true"]')) return null;
      for (; node && node !== root; node = node.parentElement) {
        // A nested hyperlink owns its normal browser click and Enter behavior.
        if (node.localName === "a" && (node.hasAttribute("href") || node.hasAttributeNS(XLINK_NS, "href"))) return null;
        const plan = plans.get(node);
        if (plan && node.getAttribute("data-id") === plan.sourceId &&
            node.getAttribute("data-callback") === plan.name && resolveCallback(plan.name) === plan.callback) return node;
      }
      return null;
    }
    function activate(node) {
      const plan = plans.get(node);
      try { Promise.resolve(plan.callback(plan.sourceId)).catch(report); }
      catch (error) { report(error); }
    }
    const modified = (event) => event.altKey || event.ctrlKey || event.metaKey || event.shiftKey;
    function click(event) {
      if (event.button !== 0 || modified(event)) return;
      const node = target(event);
      if (node) { event.preventDefault(); activate(node); }
    }
    function keydown(event) {
      if (modified(event) || event.isComposing || !["Enter", " "].includes(event.key)) return;
      const node = target(event);
      if (!node) return;
      event.preventDefault();
      if (event.repeat) return;
      if (event.key === "Enter") { pressed = null; activate(node); }
      else pressed = node;
    }
    function keyup(event) {
      if (event.key !== " ") return;
      const node = pressed;
      pressed = null;
      if (!modified(event) && !event.isComposing && node && target(event) === node) {
        event.preventDefault(); activate(node);
      }
    }
    function blur() { pressed = null; }
    function cleanup() {
      if (disposed) return;
      disposed = true;
      pressed = null;
      root.removeEventListener("click", click);
      root.removeEventListener("keydown", keydown);
      root.removeEventListener("keyup", keyup);
      root.removeEventListener("blur", blur, true);
      for (const restore of restorations.reverse()) restore();
      bindings.delete(root);
    }
    try {
      if (plans.size) {
        // Chromium otherwise adds an implicit Tab stop to an SVG with a click listener.
        // The actionable node buttons, not the event-delegation root, own keyboard focus.
        if (!root.hasAttribute("tabindex")) attribute(root, "tabindex", "-1");
        // An outer role=img makes all descendants presentational to assistive technology.
        // Expose the interactive children, retaining the engine's accessible name/description.
        if (root.getAttribute("role") === "img") attribute(root, "role", "group");
        for (const node of plans.keys()) {
          attribute(node, "role", "button");
          attribute(node, "tabindex", "0");
          attribute(node, "aria-keyshortcuts", "Enter Space");
          if (!node.hasAttribute("aria-label") && !node.hasAttribute("aria-labelledby")) {
            attribute(node, "aria-label", node.getAttribute("title") || plans.get(node).sourceId);
          }
        }
        root.addEventListener("click", click);
        root.addEventListener("keydown", keydown);
        root.addEventListener("keyup", keyup);
        root.addEventListener("blur", blur, true);
      }
      bindings.set(root, cleanup);
      onBind(root, cleanup);
    } catch (error) { cleanup(); throw error; }
    for (const name of missing) report(new Error(`No callback registered for '${name}'; the diagram node remains inert.`));
    return cleanup;
  };
}
