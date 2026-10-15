// Source-checkout entry. No WASM is imported or executed on the browser UI thread.
import { createWorkerMermaid } from "./mermaid-compat.mjs";
export { createMermaid, createWorkerMermaid, MermaidError } from "./mermaid-compat.mjs";
export default createWorkerMermaid({ autoStart: true,
  engineUrl: new URL("../pkg/frankenmermaid.js", import.meta.url) });

const canvasOwners = new WeakMap();
const selectionMethods = ["selectionState", "selectAt", "navigateSelection", "clearSelection",
  "beginSelectionGesture", "previewSelectionBox", "previewSelectionLasso",
  "commitSelectionGesture", "cancelSelectionGesture", "redraw"];
const MAX_SELECTION_POINTS = 4096;

/** Bind pointer/keyboard input to a native Diagram's retained CanvasSession (bd-vmk).
 * The host owns the Diagram and source renders. Call beforeRender() before replacing source,
 * then refresh() after rendering. This adapter never parses, lays out, follows a link, or
 * implements polygon geometry. Coordinates sent to Rust are backing-store pixels, exactly once.
 * Default mapping supports positive, axis-aligned CSS scaling, borders and padding. Hosts with
 * rotated/skewed ancestors must supply mapCanvas() returning their axis-aligned input surface.
 * Selection requires the new Diagram session exports; an older WASM package fails explicitly.
 */
export function bindCanvasSelection({ canvas, diagram, tool = "box", containment = "intersects",
  dragThreshold = 4, overlay = true, mapCanvas, onSelectionChange = () => {},
  onError = error => globalThis.console?.error(error),
} = {}) {
  if (!canvas || typeof canvas.addEventListener !== "function" || typeof canvas.setPointerCapture !== "function") {
    throw new TypeError("Canvas selection requires a pointer-capable HTML canvas.");
  }
  if (canvasOwners.has(canvas)) throw new Error("This canvas already has a selection controller.");
  for (const name of selectionMethods) if (typeof diagram?.[name] !== "function") {
    throw new Error(`Diagram has no ${name} method. Rebuild the frankenmermaid WASM package for canvas selection.`);
  }
  const checkTool = value => {
    if (!["box", "lasso"].includes(value)) throw new RangeError("Selection tool must be box or lasso.");
    return value;
  };
  checkTool(tool);
  if (!["intersects", "contains"].includes(containment)) throw new RangeError("Invalid selection containment.");
  if (!Number.isFinite(dragThreshold) || dragThreshold < 0 || dragThreshold > 64) {
    throw new RangeError("Selection dragThreshold must be between 0 and 64 CSS pixels.");
  }
  if ((mapCanvas !== undefined && typeof mapCanvas !== "function")
    || typeof onSelectionChange !== "function" || typeof onError !== "function") {
    throw new TypeError("Selection callbacks must be functions.");
  }
  const document = canvas.ownerDocument;
  const window = document?.defaultView || globalThis;
  if (overlay && !document?.createElementNS) throw new TypeError("The selection overlay requires a browser Document.");
  let active = null, disposed = false, suspended = false, frame = null, suppressedPointer = null;
  let drawing = null, outline = null, previousSelection = null;
  const listeners = [];
  const originalTabIndex = canvas.getAttribute?.("tabindex");
  const originalTouchAction = canvas.style?.touchAction;
  const addedTabIndex = originalTabIndex == null;
  const owner = {};
  let resizeObserver;
  let mutationObserver;
  const raf = callback => window.requestAnimationFrame(callback);
  const cancelRaf = id => window.cancelAnimationFrame(id);
  const report = error => { try { onError(error); } catch { /* Error reporters are observational. */ } };
  const modeOf = event => event.altKey ? "remove" : event.ctrlKey || event.metaKey ? "toggle" : event.shiftKey ? "add" : "replace";

  function metrics() {
    const rect = canvas.getBoundingClientRect();
    let box;
    if (mapCanvas) box = mapCanvas(canvas);
    else {
      const style = window.getComputedStyle?.(canvas);
      const px = name => parseFloat(style?.[name]) || 0;
      const borderX = px("borderLeftWidth") + px("borderRightWidth");
      const borderY = px("borderTopWidth") + px("borderBottomWidth");
      const paddingX = px("paddingLeft") + px("paddingRight");
      const paddingY = px("paddingTop") + px("paddingBottom");
      // Computed width is fractional; offsetWidth would round and mis-map scaled borders.
      const cssWidth = parseFloat(style?.width);
      const cssHeight = parseFloat(style?.height);
      const borderBox = style?.boxSizing === "border-box";
      const totalWidth = cssWidth + (borderBox ? 0 : borderX + paddingX);
      const totalHeight = cssHeight + (borderBox ? 0 : borderY + paddingY);
      const sx = Number.isFinite(totalWidth) && totalWidth > 0 ? rect.width / totalWidth : 1;
      const sy = Number.isFinite(totalHeight) && totalHeight > 0 ? rect.height / totalHeight : 1;
      box = { left: rect.left + (px("borderLeftWidth") + px("paddingLeft")) * sx,
        top: rect.top + (px("borderTopWidth") + px("paddingTop")) * sy,
        width: rect.width - (borderX + paddingX) * sx,
        height: rect.height - (borderY + paddingY) * sy };
    }
    const result = { left: box?.left, top: box?.top, width: box?.width, height: box?.height,
      pixelsWide: canvas.width, pixelsHigh: canvas.height };
    if (!Object.values(result).every(Number.isFinite) || result.width <= 0 || result.height <= 0
      || result.pixelsWide <= 0 || result.pixelsHigh <= 0) return null;
    return result;
  }
  const sameMetrics = (a, b) => a && b && Object.keys(a).every(key => a[key] === b[key]);
  const point = (event, box) => [
    (event.clientX - box.left) * box.pixelsWide / box.width,
    (event.clientY - box.top) * box.pixelsHigh / box.height,
  ];
  const validPoint = value => value.every(Number.isFinite);

  function snapshot() {
    const state = diagram.selectionState();
    if (!state || !Array.isArray(state.selectedNodeIds)
      || state.selectedNodeIds.some(id => typeof id !== "string")
      || !(state.primaryNodeId == null || typeof state.primaryNodeId === "string")
      || !(state.activeGesture == null || typeof state.activeGesture === "string")
      || typeof state.frameAvailable !== "boolean") throw new Error("Invalid Diagram selectionState response.");
    return Object.freeze({ selectedNodeIds: Object.freeze([...state.selectedNodeIds]),
      primaryNodeId: state.primaryNodeId ?? null, activeGesture: state.activeGesture ?? null,
      frameAvailable: state.frameAvailable });
  }
  function notify() {
    if (disposed) return;
    const state = snapshot();
    const key = JSON.stringify(state);
    if (key === previousSelection) return;
    previousSelection = key;
    try { onSelectionChange(state); } catch (error) { report(error); }
  }
  function drawRegion(job) {
    if (!overlay || disposed || active !== job || !job.token) return;
    if (!drawing) {
      const ns = "http://www.w3.org/2000/svg";
      drawing = document.createElementNS(ns, "svg");
      drawing.setAttribute("aria-hidden", "true");
      drawing.setAttribute("focusable", "false");
      drawing.style.cssText = "position:fixed;pointer-events:none;overflow:hidden;z-index:2147483646;";
      outline = document.createElementNS(ns, "path");
      outline.setAttribute("fill", "#2563eb");
      outline.setAttribute("fill-opacity", "0.10");
      outline.setAttribute("fill-rule", "evenodd");
      outline.setAttribute("stroke", "#2563eb");
      outline.setAttribute("stroke-width", "1.5");
      outline.setAttribute("vector-effect", "non-scaling-stroke");
      drawing.appendChild(outline);
      (document.body || document.documentElement).appendChild(drawing);
    }
    const m = job.metrics;
    drawing.style.left = `${m.left}px`; drawing.style.top = `${m.top}px`;
    drawing.style.width = `${m.width}px`; drawing.style.height = `${m.height}px`;
    drawing.setAttribute("viewBox", `0 0 ${m.pixelsWide} ${m.pixelsHigh}`);
    const [x, y] = job.start, [ex, ey] = job.end;
    const points = job.tool === "box" ? [[x, y], [ex, y], [ex, ey], [x, ey]] : job.points;
    outline.setAttribute("d", points.map(([px, py], index) => `${index ? "L" : "M"} ${px} ${py}`).join(" ") + " Z");
  }
  function clearRegion() {
    drawing?.remove();
    drawing = outline = null;
  }
  function release(job) {
    try { if (canvas.hasPointerCapture?.(job.pointerId)) canvas.releasePointerCapture(job.pointerId); }
    catch { /* The browser may already have implicitly released capture. */ }
  }
  function detach(job) {
    if (active !== job) return;
    active = null; // Revoke before releasePointerCapture can dispatch lostpointercapture.
    if (frame !== null) cancelRaf(frame);
    frame = null;
    clearRegion();
    release(job);
  }
  function cancel() {
    const job = active;
    if (!job) return false;
    detach(job);
    if (job.token) {
      const changed = diagram.cancelSelectionGesture(job.token);
      if (changed) diagram.redraw();
    }
    notify();
    return true;
  }
  function fail(error) {
    try { cancel(); } catch (cancelError) { report(cancelError); }
    if (!disposed) report(error);
  }
  function current(job) {
    if (disposed || active !== job) return false;
    if (!sameMetrics(job.metrics, metrics())) { cancel(); return false; }
    if (job.token && snapshot().activeGesture !== job.token) {
      detach(job); // Source replacement or another native command already revoked the token.
      notify();
      return false;
    }
    return true;
  }
  function flush(job) {
    if (frame !== null) cancelRaf(frame);
    frame = null;
    if (!current(job) || !job.token) return;
    let changed = false;
    if (job.tool === "box") {
      changed = diagram.previewSelectionBox(job.token, ...job.start, ...job.end);
    } else if (job.points.length >= 3) {
      changed = diagram.previewSelectionLasso(job.token, new Float64Array(job.points.flat()));
    }
    if (!current(job)) return;
    if (changed) diagram.redraw();
    drawRegion(job);
    notify();
  }
  function schedule(job) {
    if (frame !== null) return;
    const id = raf(() => {
      if (frame !== id || active !== job || disposed) return;
      frame = null;
      try { flush(job); } catch (error) { fail(error); }
    });
    frame = id;
  }
  function begin(job) {
    const token = diagram.beginSelectionGesture(job.mode, containment);
    if (token == null) { detach(job); return false; }
    if (typeof token !== "string" || !/^[1-9][0-9]{0,19}$/.test(token)) {
      throw new Error("Invalid Diagram selection gesture token.");
    }
    job.token = token;
    return current(job);
  }
  function append(job, event) {
    const next = point(event, job.metrics);
    if (!validPoint(next)) return false;
    job.end = next;
    if (job.tool === "lasso") {
      const previous = job.points.at(-1);
      if (previous[0] !== next[0] || previous[1] !== next[1]) {
        if (job.points.length >= MAX_SELECTION_POINTS) {
          throw new RangeError("Lasso exceeds 4096 points; the gesture was cancelled.");
        }
        job.points.push(next);
      }
    }
    return true;
  }
  function down(event) {
    if (disposed || suspended || event.defaultPrevented) return;
    if (active) {
      if (event.pointerType === "touch" && event.pointerId !== active.pointerId) cancel();
      return;
    }
    if (event.button !== 0 || event.isPrimary === false) return;
    suppressedPointer = null;
    const m = metrics();
    if (!m || !snapshot().frameAvailable) return;
    const start = point(event, m);
    if (!validPoint(start) || start[0] < 0 || start[1] < 0 || start[0] >= m.pixelsWide || start[1] >= m.pixelsHigh) return;
    const job = { pointerId: event.pointerId, metrics: m, start, end: start, points: [start],
      clientStart: [event.clientX, event.clientY], mode: modeOf(event), tool, token: null };
    canvas.setPointerCapture(event.pointerId); // No ownership when capture fails.
    active = job;
    canvas.focus?.({ preventScroll: true });
    if (active === job && !disposed) event.preventDefault();
  }
  function move(event) {
    const job = active;
    if (!job || event.pointerId !== job.pointerId || !current(job)) return;
    if (event.buttons === 0) { cancel(); return; }
    const distance = Math.hypot(event.clientX - job.clientStart[0], event.clientY - job.clientStart[1]);
    if (!Number.isFinite(distance)) return;
    if (!job.token && distance < dragThreshold) return;
    if (!job.token && !begin(job)) return;
    const samples = job.tool === "lasso" && typeof event.getCoalescedEvents === "function"
      ? event.getCoalescedEvents() : [];
    if (samples.length > MAX_SELECTION_POINTS) throw new RangeError("Lasso input batch exceeds 4096 points.");
    for (const sample of samples) if (sample.pointerId === undefined || sample.pointerId === job.pointerId) append(job, sample);
    if (!append(job, event)) return;
    schedule(job);
    event.preventDefault();
  }
  function up(event) {
    const job = active;
    if (!job || event.pointerId !== job.pointerId || !current(job)) return;
    if (!validPoint(point(event, job.metrics))) { cancel(); return; }
    const distance = Math.hypot(event.clientX - job.clientStart[0], event.clientY - job.clientStart[1]);
    if (!job.token && distance >= dragThreshold && !begin(job)) return;
    if (job.token) {
      append(job, event);
      flush(job); // The final pointer position must be included even before the scheduled frame.
      if (!current(job)) return;
      const committed = diagram.commitSelectionGesture(job.token);
      if (!committed) {
        diagram.cancelSelectionGesture(job.token);
        diagram.redraw();
      }
      suppressedPointer = job.pointerId;
      detach(job);
    } else {
      detach(job);
      if (diagram.selectAt(...point(event, job.metrics), job.mode)) diagram.redraw();
    }
    notify();
    event.preventDefault();
  }
  function key(event) {
    if (disposed || suspended || event.defaultPrevented || event.target !== canvas) return;
    if (event.key === "Escape") {
      if (!cancel() && diagram.clearSelection()) { diagram.redraw(); notify(); }
      event.preventDefault();
      return;
    }
    const direction = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" }[event.key];
    if (!direction || event.altKey || event.ctrlKey || event.metaKey) return;
    cancel();
    if (disposed) return;
    if (diagram.navigateSelection(direction, event.shiftKey === true)) diagram.redraw();
    notify();
    event.preventDefault();
  }
  function listen(target, name, handler, options) {
    if (!target?.addEventListener) return;
    const guarded = event => { if (!disposed) { try { handler(event); } catch (error) { fail(error); } } };
    target.addEventListener(name, guarded, options);
    listeners.push(() => target.removeEventListener(name, guarded, options));
  }
  function geometryChanged() {
    if (!active || disposed) return;
    try { if (!sameMetrics(active.metrics, metrics())) cancel(); }
    catch (error) { fail(error); }
  }
  function dispose() {
    if (disposed) return;
    disposed = true;
    try { cancel(); } catch (error) { report(error); }
    for (const remove of listeners) remove();
    resizeObserver?.disconnect(); mutationObserver?.disconnect();
    clearRegion();
    if (addedTabIndex && canvas.getAttribute?.("tabindex") === "0") canvas.removeAttribute("tabindex");
    if (canvas.style?.touchAction === "none") canvas.style.touchAction = originalTouchAction || "";
    if (canvasOwners.get(canvas) === owner) canvasOwners.delete(canvas);
  }
  canvasOwners.set(canvas, owner);
  try {
    if (addedTabIndex) canvas.setAttribute?.("tabindex", "0");
    if (canvas.style) canvas.style.touchAction = "none";
    listen(canvas, "pointerdown", down);
    listen(canvas, "pointermove", move);
    listen(canvas, "pointerup", up);
    for (const name of ["pointercancel", "lostpointercapture"]) listen(canvas, name, event => {
      if (active?.pointerId === event.pointerId) cancel();
    });
    listen(canvas, "keydown", key);
    listen(canvas, "click", event => {
      if (suppressedPointer === null || event.detail === 0) return;
      if (event.pointerId !== undefined && event.pointerId !== suppressedPointer) return;
      suppressedPointer = null;
      event.preventDefault(); event.stopImmediatePropagation();
    }, true);
    listen(window, "blur", cancel);
    listen(window, "scroll", geometryChanged, true);
    listen(window, "resize", geometryChanged);
    listen(document, "visibilitychange", () => { if (document.hidden) cancel(); });
    if (typeof window.ResizeObserver === "function") {
      resizeObserver = new window.ResizeObserver(geometryChanged);
      resizeObserver.observe(canvas);
    }
    if (typeof window.MutationObserver === "function") {
      mutationObserver = new window.MutationObserver(geometryChanged);
      mutationObserver.observe(canvas, { attributes: true, attributeFilter: ["width", "height", "style", "class"] });
    }
  } catch (error) { dispose(); throw error; }
  return Object.freeze({
    beforeRender: () => { if (!disposed) { suspended = true; try { cancel(); } catch (error) { fail(error); } } },
    refresh: () => { if (!disposed) { try { cancel(); suspended = false; notify(); } catch (error) { fail(error); } } },
    cancel: () => { if (disposed) return false; try { return cancel(); } catch (error) { fail(error); return false; } },
    setTool: value => { checkTool(value); if (disposed || value === tool) return; cancel(); tool = value; },
    dispose,
  });
}
