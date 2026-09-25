// Image exports render an immutable source snapshot, never the possibly stale live preview.
// This adapter does not modify source, source history, or the source-download baseline.

const SVG_NS = "http://www.w3.org/2000/svg";
const MAX_SVG_UNITS = 16 * 1024 * 1024;

function aborted() {
  const error = new Error("Image export cancelled or superseded.");
  error.name = "AbortError";
  return error;
}

export function imageFilename(name, format) {
  if (format !== "svg" && format !== "png") throw new RangeError("Unsupported image format.");
  const leaf = String(name || "diagram").split(/[\\/]/).at(-1)
    .replace(/[\u0000-\u001f\u007f]/g, "").trim();
  const stem = (leaf.replace(/\.[^.]+$/, "") || "diagram").replace(/^\.+$/, "diagram");
  return `${Array.from(stem).slice(0, 180).join("")}.${format}`;
}

function parseSvgRoot(svg, host) {
  if (typeof svg !== "string" || !svg || svg.length > MAX_SVG_UNITS) {
    throw new Error("Renderer returned an empty or oversized SVG; no download was created.");
  }
  const parsed = new host.DOMParser().parseFromString(svg, "image/svg+xml");
  const root = parsed.documentElement;
  if (parsed.doctype || !root || root.localName !== "svg" || root.namespaceURI !== SVG_NS ||
      parsed.getElementsByTagName("parsererror").length) {
    throw new Error("Renderer returned malformed SVG; no download was created.");
  }
  return root;
}

/** Validate and serialize the renderer's SVG, without mounting it in the live document. */
export function svgArtifact(svg, { filename = "diagram" } = {}, host = globalThis) {
  const root = parseSvgRoot(svg, host);
  // Export the renderer's document, not editor selection outlines or viewport transforms.
  return Object.freeze({ text: new host.XMLSerializer().serializeToString(root),
    mime: "image/svg+xml;charset=utf-8", filename: imageFilename(filename, "svg") });
}

/** Bound raster memory before creating an image or allocating a canvas. SVG has no pixel cap. */
export function pngDimensions(width, height, scale = 2) {
  if (![width, height].every(value => typeof value === "number" && Number.isFinite(value) && value > 0) ||
      ![1, 2, 3, 4].includes(scale)) {
    throw new RangeError("PNG needs positive finite dimensions and a scale of 1, 2, 3, or 4.");
  }
  const pixelsWide = Math.ceil(width * scale), pixelsHigh = Math.ceil(height * scale);
  if (!Number.isSafeInteger(pixelsWide) || !Number.isSafeInteger(pixelsHigh) ||
      pixelsWide > 16384 || pixelsHigh > 16384 || pixelsWide * pixelsHigh > 32_000_000) {
    throw new RangeError("PNG exceeds the 16,384-pixel side or 32-megapixel allocation limit. Lower the scale or download SVG instead.");
  }
  return Object.freeze({ width: pixelsWide, height: pixelsHigh });
}

function intrinsicBox(root) {
  const raw = root.getAttribute("viewBox");
  if (raw !== null) {
    const values = raw.trim().split(/[\s,]+/).map(Number);
    if (values.length !== 4 || !values.every(Number.isFinite) || values[2] <= 0 || values[3] <= 0) {
      throw new Error("SVG has no finite positive viewBox; download SVG instead of PNG.");
    }
    return values;
  }
  const dimension = name => {
    const text = (root.getAttribute(name) || "").trim();
    return /^[+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?(?:px)?$/i.test(text)
      ? Number(text.replace(/px$/i, "")) : NaN;
  };
  return [0, 0, dimension("width"), dimension("height")];
}

function boundedStep(start, signal, timeoutMs, host, label) {
  return new Promise((resolve, reject) => {
    let settled = false, timer;
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      host.clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
      if (error) reject(error); else resolve(value);
    };
    const onAbort = () => finish(aborted());
    if (signal?.aborted) { onAbort(); return; }
    signal?.addEventListener("abort", onAbort, { once: true });
    timer = host.setTimeout(() => finish(new Error(`${label} timed out. Download SVG instead.`)), timeoutMs);
    try { start(value => finish(null, value), error => finish(error)); }
    catch (error) { finish(error); }
  });
}

/** Rasterize the full SVG bounds, not its current on-screen viewport. */
export async function pngArtifact(svg, { filename = "diagram", scale = 2, background = "transparent",
  timeoutMs = 15000 } = {}, host = globalThis, signal) {
  if (signal?.aborted) throw aborted();
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60000) throw new RangeError("Invalid PNG timeout.");
  const backgrounds = { transparent: null, white: "#ffffff", dark: "#111827" };
  if (!Object.hasOwn(backgrounds, background)) throw new RangeError("Unsupported PNG background.");
  const root = parseSvgRoot(svg, host);
  const box = intrinsicBox(root);
  const dimensions = pngDimensions(box[2], box[3], scale);
  // SVG-as-image cannot reliably load remote dependencies. Do not silently omit an icon.
  for (const element of root.querySelectorAll("image, use")) {
    const href = (element.getAttribute("href") || element.getAttributeNS("http://www.w3.org/1999/xlink", "href") || "").trim();
    if (href && !href.startsWith("#") && !/^data:image\//i.test(href)) {
      throw new Error("PNG cannot include external image references reliably. Download SVG instead.");
    }
  }
  root.setAttribute("viewBox", box.join(" "));
  root.setAttribute("width", String(dimensions.width));
  root.setAttribute("height", String(dimensions.height));
  // Responsive page sizing must not override intrinsic pixel dimensions in an image Blob.
  for (const name of ["width", "height", "max-width", "max-height", "min-width", "min-height"]) root.style.removeProperty(name);
  const text = new host.XMLSerializer().serializeToString(root);
  let url = null, image = null, canvas = null;
  try {
    url = host.URL.createObjectURL(new host.Blob([text], { type: "image/svg+xml;charset=utf-8" }));
    image = new host.Image();
    await boundedStep((resolve, reject) => {
      image.onload = resolve;
      image.onerror = () => reject(new Error("Browser could not decode the SVG image. Download SVG instead."));
      image.src = url;
    }, signal, timeoutMs, host, "PNG image decoding");
    if (signal?.aborted) throw aborted();
    canvas = host.document.createElement("canvas");
    canvas.width = dimensions.width;
    canvas.height = dimensions.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas2D is unavailable. Download SVG instead.");
    if (backgrounds[background]) {
      context.fillStyle = backgrounds[background];
      context.fillRect(0, 0, canvas.width, canvas.height);
    }
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const blob = await boundedStep((resolve, reject) => canvas.toBlob(value => {
      if (!value || value.type !== "image/png" || value.size === 0) {
        reject(new Error("Browser could not encode PNG. Download SVG instead."));
      } else resolve(value);
    }, "image/png"), signal, timeoutMs, host, "PNG encoding");
    if (signal?.aborted) throw aborted();
    return Object.freeze({ blob, mime: "image/png", filename: imageFilename(filename, "png"), ...dimensions });
  } finally {
    if (image) { image.onload = image.onerror = null; image.removeAttribute("src"); }
    if (url !== null) host.URL.revokeObjectURL(url);
    if (canvas) { canvas.width = 0; canvas.height = 0; }
  }
}

export function imageArtifact(svg, options = {}, host = globalThis, signal) {
  const format = options.format || "svg";
  if (format === "svg") return svgArtifact(svg, options, host);
  if (format === "png") return pngArtifact(svg, options, host, signal);
  throw new RangeError("Unsupported image format.");
}

/** Latest-request-only export transaction. Recheck freshness after every asynchronous boundary. */
export function createDiagramExporter({ getSource, renderSource, cancelRender = () => {},
  makeArtifact = (svg, options, signal) => imageArtifact(svg, options, globalThis, signal), saveArtifact }) {
  if (![getSource, renderSource, cancelRender, makeArtifact, saveArtifact].every(fn => typeof fn === "function")) {
    throw new TypeError("Image export requires source, rendering, artifact, and download functions.");
  }
  let generation = 0, controller = null, disposed = false;
  function cancel() {
    generation += 1;
    controller?.abort();
    controller = null;
    cancelRender();
  }
  return {
    get busy() { return controller !== null; },
    cancel,
    async export(options = {}) {
      if (disposed) throw aborted();
      cancel();
      const source = getSource();
      if (typeof source !== "string") throw new TypeError("Source must be text.");
      const version = generation;
      const request = new AbortController();
      controller = request;
      const settings = Object.freeze({ ...options });
      const check = () => {
        if (disposed || version !== generation || request.signal.aborted || getSource() !== source) throw aborted();
      };
      try {
        const result = await renderSource(source);
        check();
        const artifact = await makeArtifact(result?.svg, settings, request.signal);
        check();
        await saveArtifact(artifact);
        check();
        return artifact;
      } finally {
        if (controller === request) controller = null;
      }
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      cancel();
    },
  };
}

/** Export controls share source with the editor, but own an independent renderer/worker. */
export function mountImageExport({ sourceEl, panelEl, backend, saveArtifact }) {
  if (typeof backend?.renderSource !== "function" || typeof backend.cancel !== "function" ||
      typeof backend.dispose !== "function") throw new TypeError("An image-export rendering backend is required.");
  const document = panelEl.ownerDocument, host = document.defaultView;
  const urls = new Map(), listeners = [];
  let disposed = false, operation = 0;
  function make(tag, id, text) {
    const element = document.createElement(tag);
    element.id = id;
    if (text) element.textContent = text;
    panelEl.append(element);
    return element;
  }
  function listen(target, name, handler) {
    target.addEventListener(name, handler);
    listeners.push(() => target.removeEventListener(name, handler));
  }
  const nameLabel = make("label", "image-export-name-label", "Image filename ");
  const filename = make("input", "image-export-name");
  filename.value = "diagram";
  nameLabel.htmlFor = filename.id;
  const svg = make("button", "image-export-svg", "Download diagram SVG");
  const png = make("button", "image-export-png", "Download diagram PNG");
  const scaleLabel = make("label", "image-export-scale-label", " PNG scale ");
  const scale = make("select", "image-export-scale");
  scaleLabel.htmlFor = scale.id;
  for (const value of [1, 2, 3, 4]) {
    const option = document.createElement("option");
    option.value = String(value);
    option.textContent = `${value}×`;
    scale.append(option);
  }
  scale.value = "2";
  const backgroundLabel = make("label", "image-export-background-label", " PNG background ");
  const background = make("select", "image-export-background");
  backgroundLabel.htmlFor = background.id;
  for (const [value, label] of [["transparent", "No added background"], ["white", "White"], ["dark", "Dark"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    background.append(option);
  }
  const cancel = make("button", "image-export-cancel", "Cancel image export");
  svg.type = png.type = cancel.type = "button";
  const status = make("p", "image-export-status", "Image exports render the current source. Download source separately to preserve editable text.");
  status.setAttribute("role", "status");
  function download(artifact) {
    const url = host.URL.createObjectURL(artifact.blob || new host.Blob([artifact.text], { type: artifact.mime }));
    const link = document.createElement("a");
    link.href = url;
    link.download = artifact.filename;
    document.body.append(link);
    try { link.click(); }
    finally {
      link.remove();
      urls.set(url, host.setTimeout(() => { host.URL.revokeObjectURL(url); urls.delete(url); }, 1000));
    }
  }
  const exporter = createDiagramExporter({
    getSource: () => sourceEl.value,
    renderSource: source => backend.renderSource(source),
    cancelRender: () => backend.cancel(),
    makeArtifact: (text, options, signal) => imageArtifact(text, options, host, signal),
    saveArtifact: saveArtifact || download,
  });
  function controls() {
    for (const control of [svg, png, filename, scale, background]) control.disabled = disposed || exporter.busy;
    cancel.disabled = disposed || !exporter.busy;
  }
  function invalidate(message) {
    const busy = exporter.busy;
    operation += 1;
    exporter.cancel();
    controls();
    if (!disposed && busy) status.textContent = message;
  }
  async function exportImage(format = "svg") {
    if (disposed) return false;
    const current = ++operation;
    const pending = exporter.export({ filename: filename.value, format,
      scale: Number(scale.value), background: background.value });
    status.textContent = `Rendering a fresh source snapshot for ${format.toUpperCase()} export…`;
    controls();
    try {
      const artifact = await pending;
      if (disposed || current !== operation) return false;
      status.textContent = `Download requested for ${artifact.filename}${artifact.width ? ` — ${artifact.width} × ${artifact.height} pixels` : ""} (${backend.target || "renderer"}). Editable source is unchanged.`;
      return true;
    } catch (error) {
      if (!disposed && current === operation) status.textContent = error.name === "AbortError"
        ? "Image export cancelled because the source changed. Export again for the current source."
        : `Image export failed: ${error.message || error}. No replacement preview or source was installed.`;
      return false;
    } finally { if (!disposed && current === operation) controls(); }
  }
  const sourceChanged = () => invalidate("Image export cancelled because the source changed. Export again for the current source.");
  listen(sourceEl, "input", sourceChanged);
  listen(svg, "click", () => { void exportImage("svg"); });
  listen(png, "click", () => { void exportImage("png"); });
  listen(cancel, "click", () => invalidate("Image export cancelled."));
  controls();
  return {
    exportImage, sourceChanged,
    dispose() {
      if (disposed) return;
      disposed = true;
      operation += 1;
      exporter.dispose();
      backend.dispose();
      for (const remove of listeners) remove();
      for (const [url, timer] of urls) { host.clearTimeout(timer); host.URL.revokeObjectURL(url); }
      urls.clear();
      controls();
    },
  };
}
