// Image exports render an immutable source snapshot, never the possibly stale live preview.
// This adapter does not modify source, source history, or the source-download baseline.

const SVG_NS = "http://www.w3.org/2000/svg";
const MAX_SVG_UNITS = 16 * 1024 * 1024;
const MAX_PDF_BYTES = 64 * 1024 * 1024;
const RASTER_BACKGROUNDS = { transparent: null, white: "#ffffff", dark: "#111827" };

function aborted() {
  const error = new Error("Image export cancelled or superseded.");
  error.name = "AbortError";
  return error;
}

export function imageFilename(name, format) {
  if (!["svg", "png", "pdf"].includes(format)) throw new RangeError("Unsupported image format.");
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
      throw new Error("SVG has no finite positive viewBox; download SVG instead of a raster export.");
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

function rasterSettings(background, timeoutMs, format) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60000) throw new RangeError(`Invalid ${format} timeout.`);
  if (!Object.hasOwn(RASTER_BACKGROUNDS, background)) throw new RangeError(`Unsupported ${format} background.`);
}

// Both formats decode the engine SVG through the same bounded, cancellable browser path.
// PDF passes a print-sized viewport so even a very large diagram can be fitted to one page.
async function withRasterCanvas(root, box, dimensions, background, timeoutMs, host, signal, format, encode) {
  if (signal?.aborted) throw aborted();
  // SVG-as-image cannot reliably load remote dependencies. Do not silently omit an icon.
  for (const element of root.querySelectorAll("image, use")) {
    const href = (element.getAttribute("href") || element.getAttributeNS("http://www.w3.org/1999/xlink", "href") || "").trim();
    if (href && !href.startsWith("#") && !/^data:image\//i.test(href)) {
      throw new Error(`${format} cannot include external image references reliably. Download SVG instead.`);
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
    }, signal, timeoutMs, host, `${format} image decoding`);
    if (signal?.aborted) throw aborted();
    canvas = host.document.createElement("canvas");
    canvas.width = dimensions.width;
    canvas.height = dimensions.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas2D is unavailable. Download SVG instead.");
    if (RASTER_BACKGROUNDS[background]) {
      context.fillStyle = RASTER_BACKGROUNDS[background];
      context.fillRect(0, 0, canvas.width, canvas.height);
    }
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const result = await encode(canvas, context);
    if (signal?.aborted) throw aborted();
    return result;
  } finally {
    if (image) { image.onload = image.onerror = null; image.removeAttribute("src"); }
    if (url !== null) host.URL.revokeObjectURL(url);
    if (canvas) { canvas.width = 0; canvas.height = 0; }
  }
}

/** Rasterize the full SVG bounds, not its current on-screen viewport. */
export async function pngArtifact(svg, { filename = "diagram", scale = 2, background = "transparent",
  timeoutMs = 15000 } = {}, host = globalThis, signal) {
  if (signal?.aborted) throw aborted();
  rasterSettings(background, timeoutMs, "PNG");
  const root = parseSvgRoot(svg, host);
  const box = intrinsicBox(root);
  const dimensions = pngDimensions(box[2], box[3], scale);
  const blob = await withRasterCanvas(root, box, dimensions, background, timeoutMs, host, signal, "PNG",
    canvas => boundedStep((resolve, reject) => canvas.toBlob(value => {
      if (!value || value.type !== "image/png" || value.size === 0) {
        reject(new Error("Browser could not encode PNG. Download SVG instead."));
      } else resolve(value);
    }, "image/png"), signal, timeoutMs, host, "PNG encoding"));
  return Object.freeze({ blob, mime: "image/png", filename: imageFilename(filename, "png"), ...dimensions });
}

/** Paper dimensions are PDF points; image resolution is 96 * scale DPI, not source size.
 * This is deliberately a raster PDF. Download SVG for vector geometry/selectable text.
 */
export function pdfPageLayout(width, height, { paper = "a4", orientation = "auto", margin = 24, scale = 2 } = {}) {
  if (![width, height].every(value => typeof value === "number" && Number.isFinite(value) && value > 0) ||
      ![1, 2, 3, 4].includes(scale)) throw new RangeError("PDF needs positive finite dimensions and a scale of 1, 2, 3, or 4.");
  const papers = { a4: [595.28, 841.89], letter: [612, 792] };
  if (!Object.hasOwn(papers, paper) || !["auto", "portrait", "landscape"].includes(orientation)) {
    throw new RangeError("PDF paper must be A4 or Letter, with auto, portrait, or landscape orientation.");
  }
  if (typeof margin !== "number" || !Number.isFinite(margin) || margin < 0 || margin >= papers[paper][0] / 2) {
    throw new RangeError("PDF margin must leave a positive printable area.");
  }
  const [short, long] = papers[paper];
  const fit = (w, h) => Math.min((w - margin * 2) / width, (h - margin * 2) / height);
  if (orientation === "auto") orientation = fit(long, short) > fit(short, long) ? "landscape" : "portrait";
  const [pageWidth, pageHeight] = orientation === "portrait" ? [short, long] : [long, short];
  const zoom = fit(pageWidth, pageHeight);
  const imageWidth = width * zoom, imageHeight = height * zoom;
  if (![imageWidth, imageHeight].every(value => Number.isFinite(value) && value >= 0.00001)) {
    throw new RangeError("PDF aspect ratio is outside the representable range. Download SVG instead.");
  }
  const pixels = pngDimensions(imageWidth * 4 / 3, imageHeight * 4 / 3, scale);
  return Object.freeze({ paper, orientation, width: pageWidth, height: pageHeight,
    pages: Object.freeze([Object.freeze({ x: 0, y: 0, width, height,
      imageX: (pageWidth - imageWidth) / 2, imageY: (pageHeight - imageHeight) / 2,
      imageWidth, imageHeight, pixels })]) });
}

async function deflateRaster(bytes, host, signal, timeoutMs) {
  if (signal?.aborted) throw aborted();
  const reader = new host.Blob([bytes]).stream().pipeThrough(new host.CompressionStream("deflate")).getReader();
  const collect = async () => {
    const parts = [];
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (signal?.aborted) throw aborted();
        if (done) return new host.Blob(parts);
        parts.push(value);
      }
    } finally { reader.releaseLock(); }
  };
  try {
    return await boundedStep((resolve, reject) => { collect().then(resolve, reject); },
      signal, timeoutMs, host, "PDF compression");
  } finally {
    // Cancel a pending reader on abort/timeout; a completed reader is already unlocked.
    void reader.cancel().catch(() => {});
  }
}

async function pdfRaster(canvas, context, host, signal, timeoutMs) {
  const count = canvas.width * canvas.height;
  const rgba = context.getImageData(0, 0, canvas.width, canvas.height).data;
  const rgb = new Uint8Array(count * 3), alpha = new Uint8Array(count);
  let transparent = false;
  for (let start = 0; start < count; start += 262144) {
    if (signal?.aborted) throw aborted();
    const end = Math.min(count, start + 262144);
    for (let pixel = start; pixel < end; pixel += 1) {
      rgb[pixel * 3] = rgba[pixel * 4];
      rgb[pixel * 3 + 1] = rgba[pixel * 4 + 1];
      rgb[pixel * 3 + 2] = rgba[pixel * 4 + 2];
      alpha[pixel] = rgba[pixel * 4 + 3];
      if (alpha[pixel] !== 255) transparent = true;
    }
    // Give source edits and Cancel a chance to invalidate this export during pixel packing.
    if (end < count) await new Promise(resolve => host.setTimeout(resolve, 0));
  }
  return { width: canvas.width, height: canvas.height,
    rgb: await deflateRaster(rgb, host, signal, timeoutMs),
    alpha: transparent ? await deflateRaster(alpha, host, signal, timeoutMs) : null };
}

function pdfText(text) {
  // PDF 1.4 text strings use UTF-16BE, not UTF-8. Hex encoding also prevents PDF injection.
  let hex = "feff";
  for (let index = 0; index < text.length; index += 1) hex += text.charCodeAt(index).toString(16).padStart(4, "0");
  return `<${hex}>`;
}

function pdfDocument(layout, images, title, host) {
  const objects = [];
  const object = value => { objects.push(value); return objects.length; };
  const number = value => String(Number(value.toFixed(6)));
  const stream = (dictionary, data) => new host.Blob([`<< ${dictionary} /Length ${data.size} >>\nstream\n`, data, "\nendstream"]);
  const catalog = object("<< /Type /Catalog /Pages 2 0 R >>");
  const pageTree = object(null);
  const pageIds = [];
  for (let index = 0; index < images.length; index += 1) {
    const image = images[index], page = layout.pages[index];
    const imageDictionary = `/Type /XObject /Subtype /Image /Width ${image.width} /Height ${image.height} /BitsPerComponent 8 /Filter /FlateDecode`;
    const maskId = image.alpha ? object(stream(`${imageDictionary} /ColorSpace /DeviceGray`, image.alpha)) : null;
    const imageId = object(stream(`${imageDictionary} /ColorSpace /DeviceRGB${maskId ? ` /SMask ${maskId} 0 R` : ""}`, image.rgb));
    const content = new host.Blob([`q\n${number(page.imageWidth)} 0 0 ${number(page.imageHeight)} ${number(page.imageX)} ${number(layout.height - page.imageY - page.imageHeight)} cm\n/Im0 Do\nQ\n`]);
    const contentId = object(stream("", content));
    pageIds.push(object(`<< /Type /Page /Parent ${pageTree} 0 R /MediaBox [0 0 ${number(layout.width)} ${number(layout.height)}] /Resources << /XObject << /Im0 ${imageId} 0 R >> >> /Contents ${contentId} 0 R >>`));
  }
  objects[pageTree - 1] = `<< /Type /Pages /Count ${pageIds.length} /Kids [${pageIds.map(id => `${id} 0 R`).join(" ")}] >>`;
  const info = object(`<< /Title ${pdfText(title)} /Producer (FrankenMermaid) /Subject (Lossless raster diagram; download SVG for vector geometry and source separately for editing.) >>`);
  const parts = [new host.Blob(["%PDF-1.4\n%", new Uint8Array([0xe2, 0xe3, 0xcf, 0xd3]), "\n"])];
  let position = parts[0].size;
  const offsets = ["0000000000 65535 f \n"];
  objects.forEach((value, index) => {
    offsets.push(`${String(position).padStart(10, "0")} 00000 n \n`);
    const part = new host.Blob([`${index + 1} 0 obj\n`, value, "\nendobj\n"]);
    parts.push(part);
    position += part.size;
  });
  parts.push(`xref\n0 ${objects.length + 1}\n${offsets.join("")}trailer\n<< /Size ${objects.length + 1} /Root ${catalog} 0 R /Info ${info} 0 R >>\nstartxref\n${position}\n%%EOF\n`);
  const blob = new host.Blob(parts, { type: "application/pdf" });
  if (blob.size > MAX_PDF_BYTES) throw new RangeError("PDF exceeds the 64 MiB output limit. Lower the scale or download SVG instead.");
  return blob;
}

/** A real downloadable, losslessly compressed raster PDF; no print dialog, CDN, or server.
 * Page images contain no JavaScript, network references, or automatically embedded source.
 */
export async function pdfArtifact(svg, { filename = "diagram", scale = 2, background = "white",
  timeoutMs = 15000, paper = "a4", orientation = "auto", margin = 24 } = {}, host = globalThis, signal) {
  if (signal?.aborted) throw aborted();
  rasterSettings(background, timeoutMs, "PDF");
  const root = parseSvgRoot(svg, host);
  const box = intrinsicBox(root);
  const layout = pdfPageLayout(box[2], box[3], { paper, orientation, margin, scale });
  if (typeof host.CompressionStream !== "function") throw new Error("PDF compression is unavailable in this browser. Download PNG or SVG instead.");
  const images = [];
  for (const page of layout.pages) {
    images.push(await withRasterCanvas(root, [box[0] + page.x, box[1] + page.y, page.width, page.height],
      page.pixels, background, timeoutMs, host, signal, "PDF",
      (canvas, context) => pdfRaster(canvas, context, host, signal, timeoutMs)));
  }
  if (signal?.aborted) throw aborted();
  return Object.freeze({ blob: pdfDocument(layout, images, imageFilename(filename, "pdf"), host),
    mime: "application/pdf", filename: imageFilename(filename, "pdf"), pageCount: images.length,
    paper: layout.paper, orientation: layout.orientation, rasterDpi: scale * 96 });
}

export function imageArtifact(svg, options = {}, host = globalThis, signal) {
  const format = options.format || "svg";
  if (format === "svg") return svgArtifact(svg, options, host);
  if (format === "png") return pngArtifact(svg, options, host, signal);
  if (format === "pdf") return pdfArtifact(svg, options, host, signal);
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
  const pdf = make("button", "image-export-pdf", "Download diagram PDF");
  const scaleLabel = make("label", "image-export-scale-label", " PNG/PDF raster scale ");
  const scale = make("select", "image-export-scale");
  scaleLabel.htmlFor = scale.id;
  for (const value of [1, 2, 3, 4]) {
    const option = document.createElement("option");
    option.value = String(value);
    option.textContent = `${value}×`;
    scale.append(option);
  }
  scale.value = "2";
  const backgroundLabel = make("label", "image-export-background-label", " PNG/PDF background ");
  const background = make("select", "image-export-background");
  backgroundLabel.htmlFor = background.id;
  for (const [value, label] of [["transparent", "No added background"], ["white", "White"], ["dark", "Dark"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    background.append(option);
  }
  const paperLabel = make("label", "image-export-paper-label", " PDF paper ");
  const paper = make("select", "image-export-paper");
  paperLabel.htmlFor = paper.id;
  for (const [value, label] of [["a4", "A4"], ["letter", "US Letter"]]) {
    const option = document.createElement("option");
    option.value = value; option.textContent = label; paper.append(option);
  }
  make("p", "image-export-pdf-help", "PDF fits the full diagram on one page using lossless raster images (96 DPI × scale). Download SVG for vector output or source for editable text.");
  const cancel = make("button", "image-export-cancel", "Cancel image export");
  svg.type = png.type = pdf.type = cancel.type = "button";
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
    for (const control of [svg, png, pdf, filename, scale, background, paper]) control.disabled = disposed || exporter.busy;
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
      scale: Number(scale.value), background: background.value, paper: paper.value });
    status.textContent = `Rendering a fresh source snapshot for ${format.toUpperCase()} export…`;
    controls();
    try {
      const artifact = await pending;
      if (disposed || current !== operation) return false;
      const detail = artifact.pageCount ? ` — ${artifact.pageCount} page(s), ${artifact.paper.toUpperCase()}, ${artifact.rasterDpi} DPI raster`
        : artifact.width ? ` — ${artifact.width} × ${artifact.height} pixels` : "";
      status.textContent = `Download requested for ${artifact.filename}${detail} (${backend.target || "renderer"}). Editable source is unchanged.`;
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
  listen(pdf, "click", () => { void exportImage("pdf"); });
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
