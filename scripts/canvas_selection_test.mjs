// Browser-controller unit tests, NOT Rust geometry or compiled-WASM qualification.
// The exact public entry module is evaluated; only the existing worker factory is stubbed.
// A stateful Diagram boundary double records native calls, tokens and rollback behavior.
// Run: node --experimental-vm-modules --test scripts/canvas_selection_test.mjs
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import vm from "node:vm";
import { test } from "node:test";

const entry = new URL("../web/mermaid.mjs", import.meta.url);
const source = await fs.readFile(entry, "utf8");
const module = new vm.SourceTextModule(source, {
  identifier: entry.href,
  initializeImportMeta: meta => { meta.url = entry.href; },
});
await module.link(specifier => {
  assert.equal(specifier, "./mermaid-compat.mjs");
  return new vm.SyntheticModule(["createMermaid", "createWorkerMermaid", "MermaidError"], function () {
    this.setExport("createMermaid", () => ({}));
    this.setExport("createWorkerMermaid", () => ({}));
    this.setExport("MermaidError", Error);
  });
});
await module.evaluate();
const { bindCanvasSelection } = module.namespace;

function event(target, type, values = {}) {
  const ev = new Event(type, { cancelable: true, bubbles: true });
  for (const [key, value] of Object.entries({ pointerId: 7, pointerType: "mouse", isPrimary: true,
    button: 0, buttons: 1, clientX: 30, clientY: 40, ...values })) {
    Object.defineProperty(ev, key, { value });
  }
  target.dispatchEvent(ev);
  return ev;
}
class HostWindow extends EventTarget {
  frames = new Map();
  nextFrame = 0;
  requestAnimationFrame(fn) { const id = ++this.nextFrame; this.frames.set(id, fn); return id; }
  cancelAnimationFrame(id) { this.frames.delete(id); }
  flush() { const work = [...this.frames.values()]; this.frames.clear(); for (const fn of work) fn(16); }
  getComputedStyle(canvas) { return canvas.computed; }
}
class Canvas extends EventTarget {
  width = 400;
  height = 200;
  captures = new Set();
  attributes = new Map();
  style = { touchAction: "pan-y" };
  rect = { left: 10, top: 20, width: 200, height: 100 };
  computed = { width: "200px", height: "100px", boxSizing: "content-box" };
  focused = false;
  getBoundingClientRect() { return this.rect; }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  setAttribute(name, value) { this.attributes.set(name, value); }
  removeAttribute(name) { this.attributes.delete(name); }
  focus() { this.focused = true; }
  setPointerCapture(id) { this.captures.add(id); }
  hasPointerCapture(id) { return this.captures.has(id); }
  releasePointerCapture(id) { this.captures.delete(id); event(this, "lostpointercapture", { pointerId: id }); }
}
class Diagram {
  calls = [];
  ids = ["A"];
  primary = "A";
  token = null;
  nextToken = 0;
  frameAvailable = true;
  rejectBegin = false;
  rejectCommit = false;
  selectionState() { return { selectedNodeIds: [...this.ids], primaryNodeId: this.primary,
    activeGesture: this.token, frameAvailable: this.frameAvailable }; }
  selectAt(...args) { this.calls.push(["click", ...args]); this.ids = ["clicked"]; this.primary = "clicked"; return true; }
  navigateSelection(...args) { this.calls.push(["navigate", ...args]); this.ids = ["next"]; this.primary = "next"; return true; }
  clearSelection() { this.calls.push(["clear"]); const changed = this.ids.length > 0; this.ids = []; this.primary = null; return changed; }
  beginSelectionGesture(...args) {
    this.calls.push(["begin", ...args]);
    if (this.rejectBegin) return null;
    assert.equal(this.token, null, "only one native gesture");
    this.baseline = [...this.ids]; this.baselinePrimary = this.primary;
    return this.token = String(++this.nextToken);
  }
  previewSelectionBox(...args) {
    this.calls.push(["box", ...args]); assert.equal(args[0], this.token);
    this.ids = ["preview"]; this.primary = "preview"; return true;
  }
  previewSelectionLasso(token, points) {
    assert.ok(points instanceof Float64Array); this.calls.push(["lasso", token, [...points]]);
    assert.equal(token, this.token); this.ids = ["preview"]; this.primary = "preview"; return true;
  }
  commitSelectionGesture(token) {
    this.calls.push(["commit", token]); assert.equal(token, this.token);
    if (this.rejectCommit) { this.ids = [...this.baseline]; this.primary = this.baselinePrimary; }
    this.token = null; return !this.rejectCommit;
  }
  cancelSelectionGesture(token) {
    this.calls.push(["cancel", token]);
    if (token !== this.token) return false;
    const changed = JSON.stringify(this.ids) !== JSON.stringify(this.baseline);
    this.ids = [...this.baseline]; this.primary = this.baselinePrimary; this.token = null; return changed;
  }
  redraw() { this.calls.push(["redraw"]); }
}
function setup(options = {}) {
  const window = new HostWindow();
  const document = new EventTarget(); document.defaultView = window; document.hidden = false;
  const canvas = new Canvas(); canvas.ownerDocument = document;
  const diagram = new Diagram(); const errors = [], changes = [];
  const controller = bindCanvasSelection({ canvas, diagram, overlay: false,
    onError: error => errors.push(error), onSelectionChange: state => changes.push(state), ...options });
  return { canvas, diagram, controller, window, document, errors, changes,
    calls: name => diagram.calls.filter(call => call[0] === name) };
}
function drag(f, options = {}) {
  event(f.canvas, "pointerdown", options);
  event(f.canvas, "pointermove", { clientX: 90, clientY: 80 });
  f.window.flush();
}

test("old WASM fails before installing listeners or taking canvas ownership", () => {
  const f = setup(); f.controller.dispose();
  assert.throws(() => bindCanvasSelection({ canvas: f.canvas, diagram: {}, overlay: false }), /Rebuild.*WASM/);
  assert.equal(f.canvas.style.touchAction, "pan-y");
  const next = bindCanvasSelection({ canvas: f.canvas, diagram: f.diagram, overlay: false }); next.dispose();
});
test("one owner per canvas, with idempotent disposal and reuse", () => {
  const f = setup();
  assert.throws(() => bindCanvasSelection({ canvas: f.canvas, diagram: f.diagram, overlay: false }), /already/);
  f.controller.dispose(); f.controller.dispose();
  const next = bindCanvasSelection({ canvas: f.canvas, diagram: f.diagram, overlay: false }); next.dispose();
});
test("click maps CSS to backing pixels once and never starts a gesture", () => {
  const f = setup(); event(f.canvas, "pointerdown"); event(f.canvas, "pointerup", { buttons: 0 });
  assert.deepEqual(f.calls("click"), [["click", 40, 40, "replace"]]);
  assert.equal(f.calls("begin").length, 0); assert.equal(f.canvas.captures.size, 0);
  assert.ok(f.canvas.focused); f.controller.dispose();
});
test("content-box and border-box layouts account for scaled borders and padding", () => {
  for (const boxSizing of ["content-box", "border-box"]) {
    const f = setup(); f.canvas.rect = { left: 10, top: 20, width: 480, height: 280 };
    f.canvas.computed = { boxSizing, width: boxSizing === "border-box" ? "240px" : "200px",
      height: boxSizing === "border-box" ? "140px" : "100px", borderLeftWidth: "10px", borderRightWidth: "10px",
      paddingLeft: "10px", paddingRight: "10px", borderTopWidth: "5px", borderBottomWidth: "5px",
      paddingTop: "15px", paddingBottom: "15px" };
    event(f.canvas, "pointerdown", { clientX: 150, clientY: 110 });
    event(f.canvas, "pointerup", { clientX: 150, clientY: 110, buttons: 0 });
    assert.deepEqual(f.calls("click"), [["click", 100, 50, "replace"]]); f.controller.dispose();
  }
});
test("modifier precedence is explicit and captured at pointer-down", () => {
  for (const [modifiers, mode] of [[{}, "replace"], [{ shiftKey: true }, "add"], [{ ctrlKey: true }, "toggle"],
    [{ metaKey: true }, "toggle"], [{ altKey: true, shiftKey: true }, "remove"]]) {
    const f = setup(); drag(f, modifiers);
    assert.deepEqual(f.calls("begin"), [["begin", mode, "intersects"]]);
    event(f.canvas, "pointerup", { clientX: 90, clientY: 80, buttons: 0 });
    assert.equal(f.calls("commit").length, 1); f.controller.dispose();
  }
});
test("small jitter remains a click and preserves the click modifiers", () => {
  const f = setup(); event(f.canvas, "pointerdown", { shiftKey: true });
  event(f.canvas, "pointermove", { clientX: 31, clientY: 41 }); f.window.flush();
  event(f.canvas, "pointerup", { clientX: 31, clientY: 41, buttons: 0 });
  assert.equal(f.calls("begin").length, 0);
  assert.deepEqual(f.calls("click"), [["click", 42, 42, "add"]]); f.controller.dispose();
});
test("move bursts coalesce into one native preview and one redraw", () => {
  const f = setup(); event(f.canvas, "pointerdown");
  for (let x = 50; x <= 90; x += 10) event(f.canvas, "pointermove", { clientX: x, clientY: 80 });
  assert.equal(f.calls("box").length, 0); assert.equal(f.window.frames.size, 1); f.window.flush();
  assert.deepEqual(f.calls("box"), [["box", "1", 40, 40, 160, 120]]);
  assert.equal(f.calls("redraw").length, 1); f.controller.dispose();
});
test("pointer-up flushes its final position before commit, without waiting for RAF", () => {
  const f = setup(); event(f.canvas, "pointerdown"); event(f.canvas, "pointermove", { clientX: 60, clientY: 60 });
  event(f.canvas, "pointerup", { clientX: 110, clientY: 90, buttons: 0 });
  assert.deepEqual(f.calls("box"), [["box", "1", 40, 40, 200, 140]]);
  assert.equal(f.calls("commit").length, 1); assert.equal(f.window.frames.size, 0);
  const count = f.diagram.calls.length; f.window.flush(); assert.equal(f.diagram.calls.length, count); f.controller.dispose();
});
test("a drag represented only by down/up still previews and commits", () => {
  const f = setup(); event(f.canvas, "pointerdown"); event(f.canvas, "pointerup", { clientX: 100, clientY: 90, buttons: 0 });
  assert.equal(f.calls("box").length, 1); assert.equal(f.calls("commit").length, 1); assert.equal(f.calls("click").length, 0);
  f.controller.dispose();
});
test("lasso forwards coalesced points as a bounded typed array in backing coordinates", () => {
  const f = setup({ tool: "lasso", containment: "contains" }); event(f.canvas, "pointerdown");
  event(f.canvas, "pointermove", { clientX: 90, clientY: 80,
    getCoalescedEvents: () => [{ clientX: 50, clientY: 40 }, { clientX: 70, clientY: 70 }] });
  f.window.flush();
  assert.deepEqual(f.calls("begin"), [["begin", "replace", "contains"]]);
  assert.deepEqual(f.calls("lasso"), [["lasso", "1", [40, 40, 80, 40, 120, 100, 160, 120]]]);
  f.controller.dispose();
});
test("oversized lasso batches cancel rather than silently truncating the path", () => {
  const f = setup({ tool: "lasso" }); drag(f);
  event(f.canvas, "pointermove", { clientX: 100, clientY: 90,
    getCoalescedEvents: () => Array.from({ length: 4097 }, () => ({ clientX: 50, clientY: 50 })) });
  assert.equal(f.diagram.token, null); assert.deepEqual(f.diagram.ids, ["A"]);
  assert.match(f.errors[0].message, /4096/); assert.equal(f.calls("commit").length, 0); f.controller.dispose();
});
test("cancellation restores the baseline and never commits on lost capture", () => {
  for (const name of ["pointercancel", "lostpointercapture"]) {
    const f = setup(); drag(f); assert.deepEqual(f.diagram.ids, ["preview"]);
    event(f.canvas, name); assert.deepEqual(f.diagram.ids, ["A"]);
    assert.equal(f.calls("commit").length, 0); assert.equal(f.canvas.captures.size, 0);
    event(f.canvas, "pointerup", { buttons: 0 }); assert.equal(f.calls("click").length, 0); f.controller.dispose();
  }
});
test("Escape rolls back a drag, then clears committed selection on a second press", () => {
  const f = setup(); drag(f); event(f.canvas, "keydown", { key: "Escape" });
  assert.deepEqual(f.diagram.ids, ["A"]); assert.equal(f.calls("clear").length, 0);
  event(f.canvas, "keydown", { key: "Escape" }); assert.deepEqual(f.diagram.ids, []); f.controller.dispose();
});
test("keyboard arrows preserve native extension semantics and respect host shortcuts", () => {
  const f = setup(); const arrow = event(f.canvas, "keydown", { key: "ArrowRight", shiftKey: true });
  assert.ok(arrow.defaultPrevented); assert.deepEqual(f.calls("navigate"), [["navigate", "right", true]]);
  const shortcut = event(f.canvas, "keydown", { key: "ArrowLeft", ctrlKey: true });
  assert.equal(shortcut.defaultPrevented, false); assert.equal(f.calls("navigate").length, 1); f.controller.dispose();
});
test("pointer IDs isolate unrelated moves, ups and cancellation events", () => {
  const f = setup(); drag(f);
  for (const name of ["pointermove", "pointerup", "pointercancel", "lostpointercapture"]) event(f.canvas, name, { pointerId: 9 });
  assert.equal(f.diagram.token, "1"); assert.equal(f.calls("commit").length, 0); f.controller.dispose();
});
test("a second touch cancels selection before handing the surface back", () => {
  const f = setup(); drag(f, { pointerType: "touch" });
  event(f.canvas, "pointerdown", { pointerId: 8, pointerType: "touch", isPrimary: false });
  assert.equal(f.diagram.token, null); assert.deepEqual(f.diagram.ids, ["A"]); f.controller.dispose();
});
test("a captured pointer can leave the canvas while its region remains valid", () => {
  const f = setup(); event(f.canvas, "pointerdown");
  event(f.canvas, "pointermove", { clientX: -50, clientY: -30 }); f.window.flush();
  assert.deepEqual(f.calls("box"), [["box", "1", 40, 40, -120, -100]]); f.controller.dispose();
});
test("resize or CSS movement revokes a live gesture before pointer-up can commit", () => {
  for (const change of [f => { f.canvas.width = 800; }, f => { f.canvas.rect.left += 10; }]) {
    const f = setup(); drag(f); change(f);
    event(f.canvas, "pointerup", { clientX: 90, clientY: 80, buttons: 0 });
    assert.deepEqual(f.diagram.ids, ["A"]); assert.equal(f.calls("commit").length, 0); f.controller.dispose();
  }
});
test("beforeRender rolls back, while external native replacement revokes delayed preview", () => {
  const f = setup(); drag(f); f.controller.beforeRender(); assert.deepEqual(f.diagram.ids, ["A"]);
  f.controller.refresh();
  event(f.canvas, "pointerdown"); event(f.canvas, "pointermove", { clientX: 90, clientY: 80 });
  f.diagram.ids = ["replacement"]; f.diagram.primary = "replacement"; f.diagram.token = null;
  f.window.flush(); event(f.canvas, "pointerup", { clientX: 90, clientY: 80, buttons: 0 });
  assert.deepEqual(f.diagram.ids, ["replacement"]); assert.equal(f.calls("commit").length, 0); f.controller.dispose();
});
test("input is suspended between beforeRender and refresh", () => {
  const f = setup(); f.controller.beforeRender();
  event(f.canvas, "pointerdown"); event(f.canvas, "pointerup", { buttons: 0 });
  event(f.canvas, "keydown", { key: "ArrowRight" });
  assert.equal(f.calls("click").length, 0); assert.equal(f.calls("navigate").length, 0);
  f.controller.refresh(); event(f.canvas, "pointerdown"); event(f.canvas, "pointerup", { buttons: 0 });
  assert.equal(f.calls("click").length, 1); f.controller.dispose();
});
test("a late cancelled frame cannot steal a newer gesture's scheduled frame", () => {
  const f = setup(); event(f.canvas, "pointerdown"); event(f.canvas, "pointermove", { clientX: 90, clientY: 80 });
  const stale = [...f.window.frames.values()][0]; f.controller.cancel();
  event(f.canvas, "pointerdown"); event(f.canvas, "pointermove", { clientX: 100, clientY: 90 });
  stale(16); event(f.canvas, "pointermove", { clientX: 110, clientY: 90 });
  assert.equal(f.window.frames.size, 1); f.window.flush();
  assert.deepEqual(f.calls("box"), [["box", "2", 40, 40, 200, 140]]); f.controller.dispose();
});
test("observer reentrancy cannot commit a preview after the host replaces its source", () => {
  let f;
  f = setup({ onSelectionChange: state => {
    if (state.activeGesture) { f.diagram.token = null; f.diagram.ids = ["replacement"]; f.diagram.primary = "replacement"; f.controller.refresh(); }
  } });
  event(f.canvas, "pointerdown"); event(f.canvas, "pointerup", { clientX: 90, clientY: 80, buttons: 0 });
  assert.deepEqual(f.diagram.ids, ["replacement"]); assert.equal(f.calls("commit").length, 0); f.controller.dispose();
});
test("a refused native commit cannot leave a visible stale preview", () => {
  const f = setup(); f.diagram.rejectCommit = true; drag(f);
  event(f.canvas, "pointerup", { clientX: 90, clientY: 80, buttons: 0 });
  assert.deepEqual(f.diagram.ids, ["A"]); assert.equal(f.canvas.captures.size, 0); f.controller.dispose();
});
test("begin rejection releases capture without pretending that a drag was accepted", () => {
  const f = setup(); f.diagram.rejectBegin = true; drag(f);
  assert.equal(f.calls("box").length, 0); assert.equal(f.canvas.captures.size, 0);
  assert.equal(f.calls("commit").length, 0); f.controller.dispose();
});
test("a drag suppresses its generated click but not an independent keyboard click", () => {
  const f = setup(); drag(f); event(f.canvas, "pointerup", { clientX: 90, clientY: 80, buttons: 0 });
  const keyboard = event(f.canvas, "click", { detail: 0 }); assert.equal(keyboard.defaultPrevented, false);
  const pointer = event(f.canvas, "click", { detail: 1 }); assert.equal(pointer.defaultPrevented, true);
  assert.equal(event(f.canvas, "click", { detail: 1 }).defaultPrevented, false); f.controller.dispose();
});
test("missing pixels, hidden/invalid dimensions and secondary buttons leave the canvas alone", () => {
  for (const change of [f => { f.diagram.frameAvailable = false; }, f => { f.canvas.width = 0; },
    f => { f.canvas.rect.width = 0; }, f => { f.canvas.rect.left = NaN; }]) {
    const f = setup(); change(f); const ev = event(f.canvas, "pointerdown");
    assert.equal(ev.defaultPrevented, false); assert.equal(f.canvas.captures.size, 0); f.controller.dispose();
  }
  const f = setup(); event(f.canvas, "pointerdown", { button: 2 }); assert.equal(f.canvas.captures.size, 0); f.controller.dispose();
});
test("non-finite pointer-up cancels instead of committing the last preview", () => {
  const f = setup(); drag(f); event(f.canvas, "pointerup", { clientX: NaN, buttons: 0 });
  assert.deepEqual(f.diagram.ids, ["A"]); assert.equal(f.calls("commit").length, 0); f.controller.dispose();
});
test("window blur and page hiding both roll back a gesture", () => {
  for (const trigger of [f => event(f.window, "blur"), f => { f.document.hidden = true; event(f.document, "visibilitychange"); }]) {
    const f = setup(); drag(f); trigger(f); assert.deepEqual(f.diagram.ids, ["A"]);
    assert.equal(f.canvas.captures.size, 0); f.controller.dispose();
  }
});
test("changing tools cancels the old gesture and validates the new mode", () => {
  const f = setup(); drag(f); f.controller.setTool("lasso"); assert.equal(f.diagram.token, null);
  assert.deepEqual(f.diagram.ids, ["A"]); assert.throws(() => f.controller.setTool("ellipse"), /tool/); f.controller.dispose();
});
test("disposal restores owned attributes and removes input listeners and queued work", () => {
  const f = setup(); drag(f); assert.equal(f.canvas.getAttribute("tabindex"), "0");
  f.controller.dispose(); assert.equal(f.canvas.getAttribute("tabindex"), null);
  assert.equal(f.canvas.style.touchAction, "pan-y"); assert.deepEqual(f.diagram.ids, ["A"]);
  const count = f.diagram.calls.length;
  event(f.canvas, "pointerdown"); event(f.canvas, "keydown", { key: "ArrowRight" }); f.window.flush();
  assert.equal(f.diagram.calls.length, count); assert.equal(f.canvas.captures.size, 0);
});
test("disposal does not overwrite host edits to tabindex or touch-action", () => {
  const f = setup(); f.canvas.setAttribute("tabindex", "3"); f.canvas.style.touchAction = "manipulation";
  f.controller.dispose(); assert.equal(f.canvas.getAttribute("tabindex"), "3"); assert.equal(f.canvas.style.touchAction, "manipulation");
});
test("selection callbacks get immutable snapshots and their failures remain observational", () => {
  const f = setup({ onSelectionChange: state => { assert.ok(Object.isFrozen(state)); assert.ok(Object.isFrozen(state.selectedNodeIds)); throw new Error("observer"); } });
  event(f.canvas, "pointerdown"); event(f.canvas, "pointerup", { buttons: 0 });
  assert.equal(f.errors.length, 1); assert.equal(f.errors[0].message, "observer"); assert.deepEqual(f.diagram.ids, ["clicked"]);
  f.controller.dispose();
});
