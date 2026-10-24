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

const svgOwners = new WeakMap();

/** Retained selection for rendered FrankenMermaid SVG, independent of WASM.
 * Node identity is .fm-node[data-id], not the generated/scoped DOM id. Picking uses
 * transformed node boxes (including labels), not shape contours. While bound,
 * selection consumes node clicks/Enter/Space; it never activates authored links
 * or callbacks. Call beforeRender()/refresh() around source replacement, redraw()
 * after changing an ancestor transform, and dispose() before discarding the SVG.
 * Drag with box/lasso to select intersecting or fully contained node boxes. Every
 * preview is relative to the initial selection. Escape, capture loss, geometry
 * changes and source replacement roll back; pointer-up commits the final sample.
 * Charts without addressable node groups have an empty selection surface.
 */
export function bindSvgSelection({ svg, tool = "box", containment = "intersects", dragThreshold = 4,
  onSelectionChange = () => {},
  onError = error => globalThis.console?.error(error),
} = {}) {
  const ns = "http://www.w3.org/2000/svg";
  if (svg?.namespaceURI !== ns || svg.localName !== "svg" || !svg.ownerDocument?.defaultView) {
    throw new TypeError("SVG selection requires a browser SVG element.");
  }
  if (svgOwners.has(svg)) throw new Error("This SVG already has a selection controller.");
  if (typeof onSelectionChange !== "function" || typeof onError !== "function") {
    throw new TypeError("Selection callbacks must be functions.");
  }
  const checkTool = value => {
    if (!["box", "lasso"].includes(value)) throw new RangeError("Selection tool must be box or lasso.");
  };
  checkTool(tool);
  if (!["contains", "intersects"].includes(containment)) throw new RangeError("Invalid selection containment.");
  if (!Number.isFinite(dragThreshold) || dragThreshold < 0 || dragThreshold > 64) {
    throw new RangeError("Selection dragThreshold must be between 0 and 64 CSS pixels.");
  }
  const document = svg.ownerDocument, window = document.defaultView;
  const owner = {}, listeners = [], owned = new WeakSet(), tabIndices = new Map();
  const originalTabIndex = svg.getAttribute("tabindex");
  const originalTouchAction = svg.style.touchAction;
  const originalTouchPriority = svg.style.getPropertyPriority("touch-action");
  let nodes = [], selected = new Set(), primary = null, disposed = false, suspended = false;
  let layer = null, frame = null, observer = null, resizeObserver = null, previous = null;
  let active = null, gestureFrame = null, suppressedPointer = null, gestureSequence = 0n;
  const matrixFinite = m => m && [m.a, m.b, m.c, m.d, m.e, m.f].every(Number.isFinite)
    && Number.isFinite(m.a * m.d - m.b * m.c) && m.a * m.d !== m.b * m.c;
  const transform = (m, x, y) => [m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f];
  const modeOf = event => event.altKey ? "remove" : event.ctrlKey || event.metaKey ? "toggle" : event.shiftKey ? "add" : "replace";
  const report = error => { try { onError(error); } catch { /* Observational callbacks cannot break input. */ } };
  const checkMode = mode => {
    if (!["replace", "add", "toggle", "remove"].includes(mode)) throw new RangeError("Invalid selection mode.");
  };
  const available = () => !disposed && !suspended;

  // Screen-space polygons keep pointer coordinates, CSS transforms and SVG viewBox
  // in one coordinate system. Lasso uses even-odd fill, including its boundary.
  const epsilon = 1e-7;
  const cross = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
  function onSegment(p, a, b) {
    return Math.abs(cross(a, b, p)) <= epsilon * Math.max(1, Math.hypot(b[0] - a[0], b[1] - a[1]))
      && p[0] >= Math.min(a[0], b[0]) - epsilon && p[0] <= Math.max(a[0], b[0]) + epsilon
      && p[1] >= Math.min(a[1], b[1]) - epsilon && p[1] <= Math.max(a[1], b[1]) + epsilon;
  }
  function inside(point, polygon) {
    let result = false;
    for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
      const a = polygon[j], b = polygon[i];
      if (onSegment(point, a, b)) return true;
      if ((a[1] > point[1]) !== (b[1] > point[1])
        && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]) result = !result;
    }
    return result;
  }
  function cuts(a, b, c, d) {
    const r = [b[0] - a[0], b[1] - a[1]], s = [d[0] - c[0], d[1] - c[1]];
    const denominator = r[0] * s[1] - r[1] * s[0];
    const q = [c[0] - a[0], c[1] - a[1]];
    if (Math.abs(denominator) > epsilon) {
      const t = (q[0] * s[1] - q[1] * s[0]) / denominator;
      const u = (q[0] * r[1] - q[1] * r[0]) / denominator;
      return t >= 0 && t <= 1 && u >= 0 && u <= 1 ? [t] : [];
    }
    // Collinear overlaps still partition the tested edge at both endpoints.
    if (Math.abs(q[0] * r[1] - q[1] * r[0]) > epsilon) return [];
    const squared = r[0] * r[0] + r[1] * r[1];
    if (squared === 0) return onSegment(a, c, d) ? [0] : [];
    const t0 = (q[0] * r[0] + q[1] * r[1]) / squared;
    const t1 = ((d[0] - a[0]) * r[0] + (d[1] - a[1]) * r[1]) / squared;
    if (Math.max(t0, t1) < 0 || Math.min(t0, t1) > 1) return [];
    return [Math.max(0, Math.min(t0, t1)), Math.min(1, Math.max(t0, t1))];
  }
  function region(job) {
    const [x, y] = job.start, [ex, ey] = job.end;
    if (job.tool === "box") {
      return Math.abs(ex - x) > epsilon && Math.abs(ey - y) > epsilon
        ? [[x, y], [ex, y], [ex, ey], [x, ey]] : null;
    }
    const polygon = job.points;
    const second = polygon.find(p => Math.hypot(p[0] - x, p[1] - y) > epsilon);
    return second && polygon.some(p => Math.abs(cross(job.start, second, p)) > epsilon) ? polygon : null;
  }
  function regionContainsNode(polygon, node, containment) {
    const box = points => points.reduce((b, [x, y]) =>
      [Math.min(b[0], x), Math.min(b[1], y), Math.max(b[2], x), Math.max(b[3], y)], [Infinity, Infinity, -Infinity, -Infinity]);
    const a = box(polygon), b = box(node.points);
    if (a[0] > b[2] || a[2] < b[0] || a[1] > b[3] || a[3] < b[1]) return false;
    if (containment === "intersects") {
      if (node.points.some(p => inside(p, polygon)) || polygon.some(p => inside(p, node.points))) return true;
      return node.points.some((p, i) => polygon.some((q, j) =>
        cuts(p, node.points[(i + 1) % 4], q, polygon[(j + 1) % polygon.length]).length > 0));
    }
    if (!node.points.every(p => inside(p, polygon))) return false;
    // An inner loop can cut out a hole without ever crossing the box perimeter.
    // Conservatively reject interior boundary traces, including retraced seams.
    if (polygon.some(point => {
      const [x, y] = transform(node.inverse, ...point), b = node.bounds;
      return x > b.x + epsilon && x < b.x + b.width - epsilon
        && y > b.y + epsilon && y < b.y + b.height - epsilon;
    })) return false;
    // Corners alone are insufficient for a concave lasso: a notch can cut through
    // the middle of an edge. Test every interval between boundary intersections.
    return node.points.every((p, i) => {
      const end = node.points[(i + 1) % 4], partition = [0, 1];
      polygon.forEach((q, j) => partition.push(...cuts(p, end, q, polygon[(j + 1) % polygon.length])));
      partition.sort((x, y) => x - y);
      return partition.every((t, j) => j === 0 || inside([
        p[0] + (end[0] - p[0]) * (t + partition[j - 1]) / 2,
        p[1] + (end[1] - p[1]) * (t + partition[j - 1]) / 2,
      ], polygon));
    });
  }

  function readNodes() {
    if (!svg.isConnected) return [];
    const result = [], ids = new Set();
    for (const element of svg.querySelectorAll(".fm-node[data-id]")) {
      if (element.closest("defs, symbol, clipPath, mask")) continue;
      const id = element.getAttribute("data-id");
      if (ids.has(id)) throw new Error(`Ambiguous SVG selection identity: ${JSON.stringify(id)}.`);
      ids.add(id);
      let hidden = false;
      for (let parent = element; parent; parent = parent.parentElement) {
        const style = window.getComputedStyle(parent);
        if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse"
          || Number(style.opacity) === 0 || parent.getAttribute("aria-hidden") === "true") {
          hidden = true; break;
        }
      }
      if (hidden) continue;
      let bounds, matrix, inverse;
      try {
        bounds = element.getBBox(); matrix = element.getScreenCTM();
        if (!matrixFinite(matrix)) continue;
        inverse = matrix.inverse();
      } catch { continue; } // Detached/non-rendering SVG elements need not expose geometry.
      if (!matrixFinite(inverse) || ![bounds.x, bounds.y, bounds.width, bounds.height].every(Number.isFinite)
        || bounds.width <= 0 || bounds.height <= 0) continue;
      const { x, y, width, height } = bounds;
      const points = [[x, y], [x + width, y], [x + width, y + height], [x, y + height]]
        .map(([px, py]) => transform(matrix, px, py));
      if (!points.flat().every(Number.isFinite)) continue;
      result.push({ id, element, bounds, inverse, points,
        center: transform(matrix, x + width / 2, y + height / 2) });
    }
    return result;
  }
  function reconcile() {
    nodes = readNodes();
    for (const [element, original] of tabIndices) {
      if (svg.contains(element)) continue;
      if (original === null && element.getAttribute("tabindex") === "-1") element.removeAttribute("tabindex");
      tabIndices.delete(element);
    }
    const ids = new Set(nodes.map(node => node.id));
    selected = new Set([...selected].filter(id => ids.has(id)));
    if (!selected.has(primary)) primary = nodes.find(node => selected.has(node.id))?.id ?? null;
  }
  function selectionState() {
    return Object.freeze({ selectedNodeIds: Object.freeze(nodes.filter(node => selected.has(node.id)).map(node => node.id)),
      primaryNodeId: primary, activeGesture: active?.token ?? null, frameAvailable: available() && svg.isConnected });
  }
  function notify() {
    if (disposed) return;
    const state = selectionState(), key = JSON.stringify(state);
    if (previous === key) return;
    previous = key;
    try { onSelectionChange(state); } catch (error) { report(error); }
  }
  function draw() {
    if (layer) layer.replaceChildren();
    if (!available() || (selected.size === 0 && !active?.dragging)) return;
    const matrix = svg.getScreenCTM();
    if (!matrixFinite(matrix)) return;
    const inverse = matrix.inverse();
    if (!matrixFinite(inverse)) return;
    if (!layer) {
      layer = document.createElementNS(ns, "g"); owned.add(layer);
      layer.setAttribute("data-fm-selection-overlay", "");
      layer.setAttribute("aria-hidden", "true");
      layer.style.pointerEvents = "none";
    }
    if (layer.parentNode !== svg) svg.appendChild(layer);
    for (const node of nodes) {
      if (!selected.has(node.id)) continue;
      const outline = document.createElementNS(ns, "polygon"); owned.add(outline);
      outline.setAttribute("data-node-id", node.id);
      outline.setAttribute("points", node.points.map(([x, y]) => transform(inverse, x, y).join(",")).join(" "));
      outline.setAttribute("fill", "var(--fm-selection-color, #2563eb)");
      outline.setAttribute("fill-opacity", "0.12");
      outline.setAttribute("stroke", "var(--fm-selection-color, #2563eb)");
      outline.setAttribute("stroke-width", node.id === primary ? "2.5" : "1.5");
      outline.setAttribute("vector-effect", "non-scaling-stroke");
      layer.appendChild(outline);
    }
    if (active?.dragging) {
      const polygon = region(active) ?? active.points;
      const outline = document.createElementNS(ns, "polygon"); owned.add(outline);
      outline.setAttribute("data-fm-selection-region", "");
      outline.setAttribute("points", polygon.map(([x, y]) => transform(inverse, x, y).join(",")).join(" "));
      outline.setAttribute("fill", "var(--fm-selection-color, #2563eb)");
      outline.setAttribute("fill-opacity", "0.08");
      outline.setAttribute("fill-rule", "evenodd");
      outline.setAttribute("stroke", "var(--fm-selection-color, #2563eb)");
      outline.setAttribute("stroke-width", "1.5");
      outline.setAttribute("stroke-dasharray", "4 3");
      outline.setAttribute("vector-effect", "non-scaling-stroke");
      layer.appendChild(outline);
    }
  }
  function refresh() {
    if (disposed) return false;
    cancelGesture(false); suspended = false;
    reconcile(); draw(); notify();
    return true;
  }
  function update(hits, mode) {
    checkMode(mode);
    const before = JSON.stringify(selectionState());
    if (mode === "replace") selected = new Set(hits);
    else for (const id of hits) {
      if (mode === "remove" || (mode === "toggle" && selected.has(id))) selected.delete(id);
      else selected.add(id);
    }
    primary = [...hits].reverse().find(id => selected.has(id))
      ?? (selected.has(primary) ? primary : nodes.find(node => selected.has(node.id))?.id ?? null);
    draw();
    const changed = before !== JSON.stringify(selectionState());
    notify();
    return changed;
  }
  function selectNodes(ids, mode = "replace") {
    checkMode(mode);
    if (!Array.isArray(ids) || ids.some(id => typeof id !== "string")) throw new TypeError("Node IDs must be an array of strings.");
    if (!available()) return false;
    cancelGesture(false); reconcile();
    const valid = new Set(nodes.map(node => node.id));
    // Reject the whole command, rather than partially applying a misspelled selection.
    if (ids.some(id => !valid.has(id))) { draw(); notify(); return false; }
    return update([...new Set(ids)], mode);
  }
  function pick(x, y) {
    return [...nodes].reverse().find(node => {
      const [px, py] = transform(node.inverse, x, y), b = node.bounds;
      return px >= b.x && py >= b.y && px < b.x + b.width && py < b.y + b.height;
    });
  }
  function selectAt(x, y, mode = "replace") {
    checkMode(mode);
    if (!available() || !Number.isFinite(x) || !Number.isFinite(y)) return false;
    cancelGesture(false); reconcile();
    const hit = pick(x, y);
    return update(hit ? [hit.id] : [], mode);
  }
  function focusPrimary() {
    if (!available()) return;
    const element = nodes.find(node => node.id === primary)?.element ?? svg;
    if (!element.hasAttribute("tabindex")) {
      if (!tabIndices.has(element)) tabIndices.set(element, null);
      element.setAttribute("tabindex", "-1");
    }
    element.focus({ preventScroll: true });
  }
  function navigate(direction, extend = false) {
    if (!["left", "right", "up", "down"].includes(direction)) throw new RangeError("Invalid navigation direction.");
    if (!available()) return false;
    cancelGesture(false); reconcile();
    const from = nodes.find(node => node.id === primary);
    let target = nodes[0];
    if (from) {
      const horizontal = direction === "left" || direction === "right";
      const sign = direction === "left" || direction === "up" ? -1 : 1;
      let best = Infinity;
      target = null;
      for (const candidate of nodes) {
        const dx = candidate.center[0] - from.center[0], dy = candidate.center[1] - from.center[1];
        const along = sign * (horizontal ? dx : dy), across = Math.abs(horizontal ? dy : dx);
        if (along <= 0) continue;
        // Prefer the closest node in the intended direction; DOM order breaks exact ties.
        const score = Math.hypot(along, across) + across * 2;
        if (score < best) { best = score; target = candidate; }
      }
    }
    if (!target) { draw(); notify(); return false; }
    const changed = update([target.id], extend ? "add" : "replace");
    focusPrimary();
    return changed;
  }
  const editable = target => target?.closest?.("input, textarea, select, button, [contenteditable]:not([contenteditable='false'])");
  const targetNode = target => nodes.find(node => node.element === target?.closest?.(".fm-node[data-id]"));
  function click(event) {
    if (!available() || event.defaultPrevented || event.button !== 0 || editable(event.target)) return;
    if (suppressedPointer !== null && event.detail !== 0
      && (event.pointerId === undefined || event.pointerId === suppressedPointer)) {
      suppressedPointer = null; event.preventDefault(); event.stopImmediatePropagation(); return;
    }
    cancelGesture(false); reconcile();
    const node = event.detail === 0 ? targetNode(event.target) : pick(event.clientX, event.clientY);
    // Capture before authored click handlers and native <a> navigation.
    event.preventDefault(); event.stopImmediatePropagation();
    update(node ? [node.id] : [], modeOf(event)); focusPrimary();
  }
  function key(event) {
    if (!available() || event.defaultPrevented || editable(event.target)) return;
    const direction = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" }[event.key];
    if (event.key === "Escape") {
      event.preventDefault(); event.stopImmediatePropagation(); if (!cancelGesture()) selectNodes([]); return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a" && !event.altKey) {
      event.preventDefault(); event.stopImmediatePropagation(); cancelGesture(false); reconcile(); update(nodes.map(node => node.id), "replace"); return;
    }
    if (event.key === "Enter" || event.key === " ") {
      cancelGesture(false); reconcile();
      const node = targetNode(event.target);
      if (!node) return;
      event.preventDefault(); event.stopImmediatePropagation(); update([node.id], modeOf(event));
      return;
    }
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (direction) {
      event.preventDefault(); event.stopImmediatePropagation(); navigate(direction, event.shiftKey); return;
    }
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault(); event.stopImmediatePropagation(); cancelGesture(false); reconcile();
      const node = event.key === "Home" ? nodes[0] : nodes.at(-1);
      if (node) { update([node.id], event.shiftKey ? "add" : "replace"); focusPrimary(); }
    }
  }
  function detachGesture(job) {
    if (active !== job) return;
    active = null; // Revoke before release dispatches lostpointercapture.
    if (gestureFrame !== null) window.cancelAnimationFrame(gestureFrame);
    gestureFrame = null;
    suppressedPointer = job.pointerId;
    try { if (svg.hasPointerCapture(job.pointerId)) svg.releasePointerCapture(job.pointerId); }
    catch { /* Capture may have been revoked by the browser or source replacement. */ }
  }
  function cancelGesture(emit = true) {
    const job = active;
    if (!job) return false;
    detachGesture(job);
    selected = new Set(job.baseline); primary = job.primary;
    try {
      if (available()) reconcile();
      draw(); if (emit) notify();
    } catch (error) { layer?.replaceChildren(); report(error); }
    return true;
  }
  function currentGesture(job) {
    if (active !== job || !available()) return false;
    const matrix = svg.getScreenCTM(), current = readNodes();
    if (!matrixFinite(matrix) || !job.matrix.every((n, i) => n === [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f][i])
      || current.length !== job.nodes.length || current.some((node, i) => {
        const old = job.nodes[i];
        return node.id !== old.id || node.element !== old.element
          || node.points.some((point, j) => point.some((value, k) => value !== old.points[j][k]));
      })) { cancelGesture(); return false; }
    return true;
  }
  function flushGesture(job) {
    if (gestureFrame !== null) window.cancelAnimationFrame(gestureFrame);
    gestureFrame = null;
    if (!currentGesture(job) || !job.dragging) return;
    const polygon = region(job);
    selected = new Set(job.baseline); primary = job.primary;
    // Every preview starts from the original snapshot, never the prior preview.
    if (polygon) update(nodes.filter(node => regionContainsNode(polygon, node, job.containment)).map(node => node.id), job.mode);
    else { draw(); notify(); }
  }
  function appendPoint(job, event) {
    const point = [event.clientX, event.clientY];
    if (!point.every(Number.isFinite)) return false;
    job.end = point;
    if (job.tool === "lasso") {
      const previous = job.points.at(-1);
      if (point[0] !== previous[0] || point[1] !== previous[1]) {
        if (job.points.length >= MAX_SELECTION_POINTS) throw new RangeError("Lasso exceeds 4096 points; selection was rolled back.");
        job.points.push(point);
      }
    }
    return true;
  }
  function startDragging(job, event) {
    const distance = Math.hypot(event.clientX - job.start[0], event.clientY - job.start[1]);
    if (!Number.isFinite(distance)) return false;
    if (!job.dragging && distance > 0 && distance >= dragThreshold) {
      job.dragging = true; job.token = String(++gestureSequence);
    }
    return job.dragging;
  }
  function pointerDown(event) {
    if (!available() || event.defaultPrevented || editable(event.target)) return;
    if (active) {
      if (event.pointerType === "touch" && event.pointerId !== active.pointerId) cancelGesture();
      return;
    }
    if (event.button !== 0 || event.isPrimary === false || ![event.clientX, event.clientY].every(Number.isFinite)) return;
    reconcile();
    const m = svg.getScreenCTM();
    if (!matrixFinite(m)) return;
    svg.setPointerCapture(event.pointerId);
    active = { pointerId: event.pointerId, start: [event.clientX, event.clientY], end: [event.clientX, event.clientY],
      points: [[event.clientX, event.clientY]], baseline: new Set(selected), primary, nodes: nodes.slice(),
      matrix: [m.a, m.b, m.c, m.d, m.e, m.f], dragging: false, token: null, tool, containment, mode: modeOf(event) };
    suppressedPointer = null;
    event.preventDefault(); event.stopImmediatePropagation();
    svg.focus({ preventScroll: true });
  }
  function pointerMove(event) {
    const job = active;
    if (!job || event.pointerId !== job.pointerId || !currentGesture(job)) return;
    if (event.buttons === 0) { cancelGesture(); return; }
    if (!startDragging(job, event)) return;
    const samples = job.tool === "lasso" ? event.getCoalescedEvents?.() ?? [] : [];
    if (samples.length > MAX_SELECTION_POINTS) throw new RangeError("Lasso input batch exceeds 4096 points.");
    for (const sample of samples) if (sample.pointerId === undefined || sample.pointerId === job.pointerId) appendPoint(job, sample);
    if (!appendPoint(job, event)) { cancelGesture(); return; }
    event.preventDefault(); event.stopImmediatePropagation();
    if (gestureFrame !== null) return;
    gestureFrame = window.requestAnimationFrame(() => {
      gestureFrame = null;
      try { flushGesture(job); } catch (error) { cancelGesture(); report(error); }
    });
  }
  function pointerUp(event) {
    const job = active;
    if (!job || event.pointerId !== job.pointerId || !currentGesture(job)) return;
    event.preventDefault(); event.stopImmediatePropagation();
    if (!appendPoint(job, event)) { cancelGesture(); return; }
    if (startDragging(job, event)) {
      flushGesture(job); // Include the final sample even if RAF has not run yet.
      if (!currentGesture(job)) return;
      if (!region(job)) { cancelGesture(); return; }
      detachGesture(job); draw(); notify(); focusPrimary();
    } else {
      detachGesture(job);
      const hit = pick(event.clientX, event.clientY);
      update(hit ? [hit.id] : [], job.mode); focusPrimary();
    }
  }

  function listen(target, name, handler, options) {
    const guarded = event => {
      if (disposed) return;
      try { handler(event); } catch (error) {
        // Malformed/replaced geometry must not turn a selection click into link activation.
        event.preventDefault?.(); event.stopImmediatePropagation?.();
        cancelGesture(); report(error);
      }
    };
    target.addEventListener(name, guarded, options);
    listeners.push(() => target.removeEventListener(name, guarded, options));
  }
  function schedule() {
    if (!available() || frame !== null) return;
    frame = window.requestAnimationFrame(() => {
      frame = null;
      if (!available()) return;
      try {
        if (active && !currentGesture(active)) return;
        reconcile(); draw(); notify();
      } catch (error) { cancelGesture(); layer?.replaceChildren(); report(error); }
    });
  }
  function dispose() {
    if (disposed) return;
    disposed = true; cancelGesture(false);
    if (frame !== null) window.cancelAnimationFrame(frame);
    for (const remove of listeners) remove();
    observer?.disconnect(); resizeObserver?.disconnect(); layer?.remove();
    for (const [element, original] of tabIndices) {
      if (original === null && element.getAttribute("tabindex") === "-1") element.removeAttribute("tabindex");
    }
    if (originalTabIndex === null && svg.getAttribute("tabindex") === "0") svg.removeAttribute("tabindex");
    if (svg.style.touchAction === "none" && svg.style.getPropertyPriority("touch-action") === "important") {
      svg.style.setProperty("touch-action", originalTouchAction, originalTouchPriority);
    }
    if (svgOwners.get(svg) === owner) svgOwners.delete(svg);
    nodes = []; selected.clear(); primary = null;
  }
  svgOwners.set(svg, owner);
  try {
    if (originalTabIndex === null) svg.setAttribute("tabindex", "0");
    svg.style.setProperty("touch-action", "none", "important");
    listen(svg, "pointerdown", pointerDown, true); listen(svg, "pointermove", pointerMove, true);
    listen(svg, "pointerup", pointerUp, true);
    for (const name of ["pointercancel", "lostpointercapture"]) listen(svg, name, event => {
      if (active?.pointerId === event.pointerId) cancelGesture();
    });
    listen(window, "blur", () => cancelGesture());
    listen(document, "visibilitychange", () => { if (document.hidden) cancelGesture(); });
    listen(svg, "click", click, true); listen(svg, "keydown", key, true);
    listen(window, "resize", schedule); listen(window, "scroll", schedule, true);
    observer = new window.MutationObserver(records => {
      if (records.some(record => !owned.has(record.target)
        && !(record.type === "childList" && [...record.addedNodes, ...record.removedNodes].every(node => owned.has(node))))) schedule();
    });
    observer.observe(svg, { subtree: true, childList: true, characterData: true, attributes: true,
      attributeFilter: ["transform", "viewBox", "width", "height", "x", "y", "class", "style", "data-id", "aria-hidden"] });
    if (typeof window.ResizeObserver === "function") {
      resizeObserver = new window.ResizeObserver(schedule); resizeObserver.observe(svg);
    }
    reconcile(); draw();
  } catch (error) { dispose(); throw error; }
  return Object.freeze({ selectionState,
    selectNode: (id, mode = "replace") => selectNodes([id], mode), selectNodes, selectAt, navigate,
    clear: () => selectNodes([]), redraw: () => { if (!available()) return false; return refresh(); },
    beforeRender: () => { if (!disposed) { suspended = true; cancelGesture(false); layer?.replaceChildren(); } },
    cancel: () => disposed ? false : cancelGesture(),
    setTool: value => { checkTool(value); if (disposed || value === tool) return; cancelGesture(); tool = value; },
    refresh, dispose,
  });
}
