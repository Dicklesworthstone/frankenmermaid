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
for (const id of ["deck-prev", "deck-next", "deck-overview-btn"]) element(id, "BUTTON", controls);
element("deck-manifest").textContent = JSON.stringify({ slides: [{ id: "a" }, { id: "b" }] });
const calls = { next: 0, prev: 0, go: 0, gestures: 0 };
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
