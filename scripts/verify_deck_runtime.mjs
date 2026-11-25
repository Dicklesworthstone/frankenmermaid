// Guard the graph-deck runtime's three-way duplication and the showcase demo deck
// (bd-f2njj, epic bd-z7g6k).
//
// The presentation runtime deliberately exists in more than one place (plan decision D10):
//   * crates/fm-cli/src/deck_runtime.js         — the CANONICAL bytes (include_str! by the CLI)
//   * index.html + frankenmermaid_demo_showcase.html — an inline copy between
//     `>>> deck-runtime:start` / `>>> deck-runtime:end` markers (the showcase stays
//     self-contained; its four-candidate pkg loader shows how hostile multi-URL-prefix
//     script loading is)
// This script makes the duplication machine-checked instead of hoped-for:
//   1. the fenced runtime block in BOTH html files is byte-identical to the canonical file;
//   2. the two DECK_DEMO_SOURCE copies (between `>>> deck-demo-source:start/end`) are
//      identical across the html files;
//   3. the demo deck renders through the SHIPPED pkg/ WASM `renderDeck` with a >=1-slide
//      manifest and zero warning diagnostics (demo drift against the deployed engine fails
//      here the same way sample drift fails verify_demo_samples.mjs);
//   4. the canonical runtime still carries its two load-bearing code paths, string-asserted:
//      the SVG pin (the coordinate contract) and the stage-scoped stopPropagation (the
//      spotlight arrow-key conflict tripwire).
//   5. the actual standalone template bootstrap keeps nested control gestures out of the
//      stage camera and preserves native button and navigation-dot keyboard activation.
//      This DOM/event harness does not substitute for fullscreen geometry checks in
//      deck_runtime_e2e.py, which require a real browser.
// The CLI talk.html surface is guarded elsewhere at BYTE level by the
// `deck_talk_html_matches_checked_in_golden` test — no weaker smoke is duplicated here.
//
// Usage (after `./build-wasm.sh` so pkg/ is current):
//   node scripts/verify_deck_runtime.mjs
// Exit 0 = all good; non-zero prints every failed check.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { runInNewContext } from "node:vm";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..");

const failures = [];
function check(name, ok, detail) {
  if (ok) {
    console.log(`ok: ${name}`);
  } else {
    failures.push(name);
    console.error(`FAIL: ${name}${detail ? ` — ${detail}` : ""}`);
  }
}

function fenced(text, startMarker, endMarker, file) {
  const start = text.indexOf(startMarker);
  const end = text.indexOf(endMarker);
  if (start === -1 || end === -1 || end <= start) {
    throw new Error(`markers ${startMarker} / ${endMarker} not found in ${file}`);
  }
  const afterStartLine = text.indexOf("\n", start) + 1;
  const endLineStart = text.lastIndexOf("\n", end);
  return text.slice(afterStartLine, endLineStart + 1);
}

const canonical = readFileSync(join(repo, "crates/fm-cli/src/deck_runtime.js"), "utf8");
const htmlFiles = ["index.html", "frankenmermaid_demo_showcase.html"];
const demoSources = [];

for (const file of htmlFiles) {
  const html = readFileSync(join(repo, file), "utf8");

  const runtimeBlock = fenced(html, ">>> deck-runtime:start", ">>> deck-runtime:end", file);
  check(
    `${file}: inline runtime is byte-identical to crates/fm-cli/src/deck_runtime.js`,
    runtimeBlock === canonical.replace(/\n$/, "") + "\n" || runtimeBlock === canonical,
    `inline ${runtimeBlock.length} bytes vs canonical ${canonical.length}`,
  );

  const demoBlock = fenced(html, ">>> deck-demo-source:start", ">>> deck-demo-source:end", file);
  demoSources.push(demoBlock);
}

check("DECK_DEMO_SOURCE identical across both html files", demoSources[0] === demoSources[1]);

// Load-bearing code paths in the canonical runtime (cheap tripwires for the two bugs a
// refactor would most plausibly reintroduce).
check(
  "runtime pins the SVG to viewBox pixels at mount",
  canonical.includes('svgRoot.style.width = worldWidth + "px"'),
  "the coordinate contract: 1 SVG user unit == 1 CSS px at scale 1",
);
check(
  "runtime keyboard handler stays stage-scoped",
  canonical.includes("event.stopPropagation()"),
  "the showcase's window keydown claims bare arrows for the spotlight",
);

// Execute the shipping template's event wiring with the canonical keyboard handler.
// DOM defaults (native button click after Space/Enter) are deliberately separate from
// keydown dispatch so swallowing a default, or the stage's Next shortcut stealing Space
// from Previous, fails independently of pointer navigation.
const template = readFileSync(join(repo, "crates/fm-cli/src/deck_template.html"), "utf8");
const elements = new Map();
function element(id, tagName = "DIV", parent = null) {
  const el = {
    id, tagName, parent, role: "", textContent: "", listeners: {},
    addEventListener(type, listener) { (this.listeners[type] ??= []).push(listener); },
    contains(other) {
      for (; other; other = other.parent) if (other === this) return true;
      return false;
    },
    closest() {
      for (let node = this; node; node = node.parent) {
        if (node.tagName === "BUTTON" || node.role === "button") return node;
      }
      return null;
    },
    focus() {},
    click() { dispatch(this, "click"); },
  };
  elements.set(id, el);
  return el;
}
function dispatch(target, type, key) {
  const event = {
    target, type, key, stopped: false, defaultPrevented: false,
    stopPropagation() { this.stopped = true; },
    preventDefault() { this.defaultPrevented = true; },
  };
  for (let node = target; node; node = node.parent) {
    for (const listener of node.listeners[type] ?? []) listener(event);
    if (event.stopped) break;
  }
  return event;
}
const stage = element("deck-stage");
const chrome = element("deck-chrome", "DIV", stage);
const controls = element("deck-controls", "DIV", chrome);
const dots = element("deck-dots", "DIV", chrome);
for (const id of ["deck-num", "deck-title", "deck-caption"]) element(id, "DIV", chrome);
for (const id of ["deck-prev", "deck-next", "deck-overview-btn", "deck-print"]) element(id, "BUTTON", controls);
const printPages = element("deck-print-pages");
printPages.replaceChildren = () => { printPages.childElementCount = 0; };
element("deck-manifest").textContent = JSON.stringify({ slides: [{ id: "a" }, { id: "b" }] });
const calls = { next: 0, prev: 0, go: 0, gestures: 0, print: 0 };
const api = {
  next() { calls.next++; }, prev() { calls.prev++; }, go() { calls.go++; },
  overview() {}, exitFreeCam() {},
};
const keyboard = canonical.match(/function onKeyDown\(event\) \{[\s\S]*?\n    \}/)?.[0];
if (!keyboard) throw new Error("Canonical deck keyboard handler missing");
stage.addEventListener("keydown", runInNewContext(`(${keyboard})`, { api, stage, sceneCount: 3 }));
for (const type of ["pointerdown", "wheel", "dblclick"]) {
  stage.addEventListener(type, () => { calls.gestures++; });
}
const window = {
  listeners: {},
  addEventListener: stage.addEventListener,
  print() { calls.print++; },
  FmDeckRuntime: { mount(options) {
    options.ui.nextBtn.addEventListener("click", api.next);
    options.ui.prevBtn.addEventListener("click", api.prev);
    return api;
  } },
};
window.parent = window;
const bootstrap = [...template.matchAll(/<script>([\s\S]*?)<\/script>/g)].at(-1)?.[1];
if (!bootstrap) throw new Error("Standalone deck bootstrap missing");
runInNewContext(bootstrap.replace("{{SVG_JS_STRING}}", '"<svg/>"'), {
  window, document: { getElementById: (id) => elements.get(id) }, location: { hash: "" }, history: {},
});
elements.get("deck-print").click();
check("Print deck requests native printing", calls.print === 1);
check("native print preview prepares the deck", window.listeners.beforeprint?.length === 1);
printPages.childElementCount = 2;
for (const listener of window.listeners.afterprint ?? []) listener();
check("leaving print preview discards its copies", printPages.childElementCount === 0);
for (const type of ["pointerdown", "wheel", "dblclick"]) dispatch(elements.get("deck-next"), type);
check("standalone controls do not start camera gestures", calls.gestures === 0);
const previousSpace = dispatch(elements.get("deck-prev"), "keydown", " ");
check("Space on Previous preserves its native default without advancing", !previousSpace.defaultPrevented && calls.next === 0);
elements.get("deck-prev").click();
check("native Previous activation goes back once", calls.prev === 1);
const nextEnter = dispatch(elements.get("deck-next"), "keydown", "Enter");
check("Enter on Next preserves native activation", !nextEnter.defaultPrevented);
elements.get("deck-next").click();
check("native Next activation advances once", calls.next === 1);
const dot = element("dot", "I", dots);
dot.role = "button";
dot.addEventListener("click", api.go);
check("Enter activates a navigation dot once", dispatch(dot, "keydown", "Enter").defaultPrevented && calls.go === 1);
check("Space activates a navigation dot once", dispatch(dot, "keydown", " ").defaultPrevented && calls.go === 2);
dispatch(elements.get("deck-next"), "keydown", "ArrowRight");
check("stage arrow shortcuts still work from focused controls", calls.next === 2);

// Exercise the actual affine helpers with independent endpoint expectations. These checks
// cover the deformation and DOM attribute contract; browser SVG measurement/rasterization
// remains the job of deck_runtime_e2e.py. Paths are kept verbatim, so curves/arcs need no
// second SVG parser or approximation in the runtime.
const geometryHelpers = canonical.slice(canonical.indexOf("  function multiplyMatrix("),
  canonical.indexOf("  function mount("));
const motion = runInNewContext(geometryHelpers +
  "\n({captureMotion, captureEdgeMotion, moveElement, moveEdge})");
const identity = [1, 0, 0, 1, 0, 0];
function matrixObject(values) {
  return Object.fromEntries(["a", "b", "c", "d", "e", "f"].map((key, index) => [key, values[index]]));
}
function geometryElement(tag, parent, matrix = identity, attributes = {}) {
  let reads = 0;
  return {
    tagName: tag, parentNode: parent, attributes: { ...attributes },
    getCTM() { reads++; return matrixObject(matrix); },
    getAttribute(name) { return this.attributes[name] ?? null; },
    setAttribute(name, value) { this.attributes[name] = value; },
    querySelector() { return this.path; },
    geometryReads() { return reads; },
  };
}
function transformAttribute(el, point) {
  const text = el.getAttribute("transform");
  const m = text.match(/^matrix\(([^)]+)\)$/);
  if (!m) throw new Error("Expected a single affine path transform, got " + text);
  const [a, b, c, d, e, f] = m[1].split(/[, ]+/).map(Number);
  return { x: a * point.x + c * point.y + e, y: b * point.x + d * point.y + f };
}
function nearPoint(actual, expected) {
  return Math.abs(actual.x - expected.x) < 1e-6 && Math.abs(actual.y - expected.y) < 1e-6;
}
const svgParent = geometryElement("svg", null);
const sourceNode = { hx: 0, hy: 0, fx: 10, fy: 20 };
const targetNode = { hx: 100, hy: 0, fx: 130, fy: -10 };
let pathMeasurements = 0;
function measuredPath(parent, path, start, end, matrix = identity, attributes = {}) {
  const el = geometryElement("path", parent, matrix, { d: path, ...attributes });
  el.getTotalLength = () => { pathMeasurements++; return 120; };
  el.getPointAtLength = length => { pathMeasurements++; return length ? end : start; };
  return el;
}
const curve = measuredPath(svgParent, "M0 0 C25 60 75 60 100 0", { x: 0, y: 0 }, { x: 100, y: 0 }, identity,
  { style: "stroke:url(#author-paint);stroke-opacity:0.4;stroke-width:7", "marker-start": "url(#diamond)" });
const arc = measuredPath(svgParent, "M0 0 A70 30 15 0 1 100 0", { x: 0, y: 0 }, { x: 100, y: 0 });
const curvedMotion = motion.captureEdgeMotion(curve, sourceNode, targetNode, identity);
const arcMotion = motion.captureEdgeMotion(arc, sourceNode, targetNode, identity);
const readsBeforeFrames = pathMeasurements + curve.geometryReads() + arc.geometryReads() + svgParent.geometryReads();
motion.moveEdge(curvedMotion);
motion.moveEdge(arcMotion);
check("curved edge anchors follow exact endpoint displacement",
  nearPoint(transformAttribute(curve, { x: 0, y: 0 }), { x: 10, y: 20 }) &&
  nearPoint(transformAttribute(curve, { x: 100, y: 0 }), { x: 130, y: -10 }));
check("curves and parallel arcs retain their original commands and separation",
  curve.getAttribute("d") === "M0 0 C25 60 75 60 100 0" &&
  arc.getAttribute("d") === "M0 0 A70 30 15 0 1 100 0" &&
  nearPoint(transformAttribute(curve, { x: 25, y: 60 }), { x: 40, y: 72.5 }));
check("morphing retains authored paint, opacity, markers and DOM ancestry",
  curve.getAttribute("style") === "stroke:url(#author-paint);stroke-opacity:0.4;stroke-width:7" &&
  curve.getAttribute("marker-start") === "url(#diamond)" && curve.parentNode === svgParent);
check("animation does not remeasure DOM geometry",
  pathMeasurements + curve.geometryReads() + arc.geometryReads() + svgParent.geometryReads() === readsBeforeFrames);

// A scaled/translated caption group and an independently translated path must each keep
// their original coordinate space. The path's extra transform compensates caption motion.
const captionGroup = geometryElement("g", svgParent, [2, 0, 0, 3, 40, 60],
  { transform: "translate(40 60) scale(2 3)", opacity: "0.6" });
const transformedPath = measuredPath(captionGroup, "M0 0 Q25 30 50 0",
  { x: 0, y: 0 }, { x: 50, y: 0 }, [2, 0, 0, 3, 50, 81], { transform: "translate(5 7)" });
captionGroup.path = transformedPath;
const groupedMotion = motion.captureEdgeMotion(captionGroup, sourceNode, targetNode, identity);
motion.moveEdge(groupedMotion);
const localStart = transformAttribute(transformedPath, { x: 0, y: 0 });
const localEnd = transformAttribute(transformedPath, { x: 50, y: 0 });
// Parent scale/translation, then the known average caption shift (20,5).
const rootPoint = point => ({ x: 2 * point.x + 60, y: 3 * point.y + 65 });
check("caption and path transforms compose through transformed ancestors",
  nearPoint(rootPoint(localStart), { x: 60, y: 101 }) &&
  nearPoint(rootPoint(localEnd), { x: 180, y: 71 }) &&
  captionGroup.getAttribute("transform") === "translate(20.000000 5.000000) translate(40 60) scale(2 3)" &&
  captionGroup.getAttribute("opacity") === "0.6");
const nodeWithTransform = geometryElement("g", captionGroup, [0, 3, -2, 0, 40, 60], { transform: "rotate(90)" });
motion.moveElement(motion.captureMotion(nodeWithTransform, identity), 10, 12);
check("node motion retains authored transforms in parent coordinates",
  nodeWithTransform.getAttribute("transform") === "translate(5.000000 4.000000) rotate(90)");
const loop = geometryElement("g", svgParent, identity, { transform: "translate(1 2)" });
motion.moveEdge(motion.captureEdgeMotion(loop, sourceNode, sourceNode, identity));
check("self-loop geometry and original transforms move together",
  loop.getAttribute("transform") === "translate(10.000000 20.000000) translate(1 2)");
const collapsed = measuredPath(svgParent, "M0 0 Z", { x: 0, y: 0 }, { x: 0, y: 0 });
check("degenerate or unavailable path geometry requests static rendering",
  motion.captureEdgeMotion(collapsed, sourceNode, targetNode, identity) === null &&
  motion.captureEdgeMotion(geometryElement("path", svgParent), sourceNode, targetNode, identity) === null);

// Mount the complete runtime against a small DOM/event fixture. Geometry is deliberately
// fixed by the fixture; this verifies mode selection, animation, reveal and teardown without
// claiming browser layout coverage.
function runtimeDom(tag, id = "") {
  const el = geometryElement(tag, null, identity, id ? { id } : {});
  el.id = id; el.children = []; el.listeners = {}; el.clientWidth = 900; el.clientHeight = 500;
  el.style = { setProperty(name, value) { this[name] = value; } };
  const classes = new Set();
  el.classList = { add: value => classes.add(value), remove: value => classes.delete(value), contains: value => classes.has(value) };
  el.appendChild = child => { child.parentNode = el; el.children.push(child); return child; };
  el.removeChild = child => { el.children.splice(el.children.indexOf(child), 1); child.parentNode = null; };
  el.contains = child => child === el || el.children.some(item => item.contains(child));
  el.addEventListener = (type, listener) => { (el.listeners[type] ??= []).push(listener); };
  el.removeEventListener = (type, listener) => { el.listeners[type] = (el.listeners[type] || []).filter(item => item !== listener); };
  el.querySelectorAll = selector => el.children.flatMap(child =>
    [...(selector === "[id]" ? child.id ? [child] : [] : child.tagName === selector ? [child] : []), ...child.querySelectorAll(selector)]);
  el.querySelector = selector => el.querySelectorAll(selector)[0] || null;
  return el;
}
function mountedFixture(diagramType, measurable = true) {
  const svg = runtimeDom("svg");
  svg.viewBox = { baseVal: { width: 500, height: 300 } };
  const a = svg.appendChild(runtimeDom("g", "fm-node-a-0"));
  const b = svg.appendChild(runtimeDom("g", "fm-node-b-1"));
  const edge = svg.appendChild(runtimeDom("g", "fm-edge-0"));
  const path = edge.appendChild(runtimeDom("path"));
  path.setAttribute("d", "M50 180 C100 220 250 220 300 180");
  if (measurable) {
    path.getTotalLength = () => 280;
    path.getPointAtLength = length => ({ x: length ? 300 : 50, y: 180 });
  }
  const manifest = { diagramType, viewBox: { width: 500, height: 300 },
    options: { fitMargin: 20, zoomMax: 2, dimOpacity: .1 }, overview: { enabled: false },
    nodeGeometry: { [a.id]: { x: 0, y: 0, width: 100, height: 40 }, [b.id]: { x: 250, y: 0, width: 100, height: 40 } },
    edgeEndpoints: { [edge.id]: { fromElementId: a.id, toElementId: b.id } },
    slides: [{ id: "one", title: "One", bounds: { x: 0, y: 0, width: 350, height: 220 }, fitMargin: 20, zoomMax: 2,
      nodes: [{ elementId: a.id, step: 0 }, { elementId: b.id, step: 1 }],
      edges: [{ elementId: edge.id, step: 1 }], maxStep: 1, steps: [{ step: 1, elementIds: [b.id, edge.id] }] }] };
  const host = runtimeDom("div"), frames = new Map(), timers = new Map();
  let serial = 0;
  const environment = { window: {}, document: { createElement: runtimeDom, createElementNS: (_ns, tag) => runtimeDom(tag) },
    requestAnimationFrame: fn => { frames.set(++serial, fn); return serial; }, cancelAnimationFrame: id => frames.delete(id),
    setTimeout: fn => { timers.set(++serial, fn); return serial; }, clearTimeout: id => timers.delete(id),
    matchMedia: () => ({ matches: false }) };
  runInNewContext(canonical, environment);
  const deck = environment.window.FmDeckRuntime.mount({ stage: host, svg, manifest });
  const viewport = host.children[0];
  for (const [id, fn] of [...frames]) { frames.delete(id); fn(1000); }
  return { deck, host, viewport, a, b, edge, path, frames, timers };
}
const animated = mountedFixture("Flowchart");
check("complete graph runtime animates the existing routed path",
  animated.viewport.classList.contains("fm-deck-morphing") &&
  animated.path.getAttribute("transform")?.startsWith("matrix(") &&
  animated.path.getAttribute("d") === "M50 180 C100 220 250 220 300 180");
animated.deck.destroy();
check("morph teardown removes the frame loop and mount", animated.frames.size === 0 && animated.host.children.length === 0);
for (const [family, measurable] of [["Sequence", true], ["sequence", true], ["Flowchart", false]]) {
  const mounted = mountedFixture(family, measurable);
  check(`${family}/${measurable}: camera mode keeps original message paths and headers`,
    !mounted.viewport.classList.contains("fm-deck-morphing") &&
    !mounted.path.getAttribute("transform") && !mounted.a.getAttribute("transform") &&
    mounted.path.getAttribute("d") === "M50 180 C100 220 250 220 300 180" &&
    mounted.viewport.style.transform.startsWith("translate3d(") && mounted.edge.classList.contains("fm-deck-hidden"));
  mounted.deck.next();
  for (const fn of mounted.timers.values()) fn();
  check(`${family}/${measurable}: camera mode still reveals nodes and messages`,
    !mounted.b.classList.contains("fm-deck-hidden") && !mounted.edge.classList.contains("fm-deck-hidden"));
  mounted.deck.destroy();
}

// Render the demo deck through the SHIPPED WASM.
const demoMatch = demoSources[0].match(/const DECK_DEMO_SOURCE = `([\s\S]*?)`;/);
check("DECK_DEMO_SOURCE extractable", Boolean(demoMatch));
if (demoMatch) {
  const source = demoMatch[1];
  const wasm = await import(pathToFileURL(join(repo, "pkg/frankenmermaid.js")).href);
  const wasmBytes = readFileSync(join(repo, "pkg/frankenmermaid_bg.wasm"));
  await wasm.default({ module_or_path: wasmBytes });
  const output = wasm.renderDeck(source, { theme: "dark" });
  check(
    "renderDeck returns an SVG",
    typeof output.svg === "string" && output.svg.startsWith("<svg"),
  );
  check(
    "demo deck manifest has slides",
    Boolean(output.manifest) && output.manifest.slides.length >= 1,
    output.manifest ? `${output.manifest.slides.length} slides` : "manifest null",
  );
  check(
    "demo deck renders with zero deck warnings",
    Array.isArray(output.warnings) && output.warnings.length === 0,
    JSON.stringify(output.warnings ?? null).slice(0, 300),
  );
}

if (failures.length > 0) {
  console.error(`\n${failures.length} deck-runtime guard check(s) failed`);
  process.exit(1);
}
console.log("\ndeck runtime guard OK: canonical file, both inline copies, demo deck render");
