// Image exports render an immutable source snapshot, never the possibly stale live preview.
// This adapter does not modify source, source history, or the source-download baseline.

const SVG_NS = "http://www.w3.org/2000/svg";
const MAX_SVG_UNITS = 16 * 1024 * 1024;

function aborted() {
  const error = new Error("Image export cancelled; no new download was requested.");
  error.name = "AbortError";
  return error;
}

export function imageFilename(name, format) {
  if (format !== "svg") throw new RangeError("Unsupported image format.");
  const leaf = String(name || "diagram").split(/[\\/]/).at(-1)
    .replace(/[\u0000-\u001f\u007f]/g, "").trim();
  const stem = (leaf.replace(/\.[^.]+$/, "") || "diagram").replace(/^\.+$/, "diagram");
  return `${Array.from(stem).slice(0, 180).join("")}.${format}`;
}

/** Validate and serialize the renderer's SVG, without mounting it in the live document. */
export function svgArtifact(svg, { filename = "diagram" } = {}, host = globalThis) {
  if (typeof svg !== "string" || !svg || svg.length > MAX_SVG_UNITS) {
    throw new Error("Renderer returned an empty or oversized SVG; no download was created.");
  }
  const parsed = new host.DOMParser().parseFromString(svg, "image/svg+xml");
  const root = parsed.documentElement;
  if (parsed.doctype || !root || root.localName !== "svg" || root.namespaceURI !== SVG_NS ||
      parsed.getElementsByTagName("parsererror").length) {
    throw new Error("Renderer returned malformed SVG; no download was created.");
  }
  // Export the renderer's document, not editor selection outlines or viewport transforms.
  return Object.freeze({ text: new host.XMLSerializer().serializeToString(root),
    mime: "image/svg+xml;charset=utf-8", filename: imageFilename(filename, "svg") });
}

/** Latest-request-only export transaction. Recheck freshness after every asynchronous boundary. */
export function createDiagramExporter({ getSource, renderSource, cancelRender = () => {},
  makeArtifact = (svg, options) => svgArtifact(svg, options), saveArtifact }) {
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
  const cancel = make("button", "image-export-cancel", "Cancel image export");
  svg.type = cancel.type = "button";
  const status = make("p", "image-export-status", "Image exports render the current source. Download source separately to preserve editable text.");
  status.setAttribute("role", "status");
  function download(artifact) {
    const url = host.URL.createObjectURL(new host.Blob([artifact.text], { type: artifact.mime }));
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
    makeArtifact: (text, options) => svgArtifact(text, options, host),
    saveArtifact: saveArtifact || download,
  });
  function controls() {
    svg.disabled = disposed || exporter.busy;
    cancel.disabled = disposed || !exporter.busy;
  }
  function invalidate(message) {
    const busy = exporter.busy;
    operation += 1;
    exporter.cancel();
    controls();
    if (!disposed && busy) status.textContent = message;
  }
  async function exportImage() {
    if (disposed) return false;
    const current = ++operation;
    const pending = exporter.export({ filename: filename.value });
    status.textContent = "Rendering a fresh source snapshot for SVG export…";
    controls();
    try {
      const artifact = await pending;
      if (disposed || current !== operation) return false;
      status.textContent = `Download requested for ${artifact.filename} (${backend.target || "renderer"}). Editable source is unchanged.`;
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
  listen(svg, "click", () => { void exportImage(); });
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
