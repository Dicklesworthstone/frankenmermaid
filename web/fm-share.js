// Editable source sharing, not a rendered-image cache. The versioned fragment carries only
// source + filename: never the recovery baseline, storage keys, SVG, or executable settings.
// No server, WASM, or third-party codec is involved. Limits apply BEFORE parsing/decompression.
export const MAX_SHARE_HASH_CHARS = 64 * 1024;
export const MAX_SHARE_SOURCE_BYTES = 256 * 1024;
const MAX_PAYLOAD_BYTES = MAX_SHARE_SOURCE_BYTES * 6 + 4096; // Worst-case JSON escaping.
const MAX_SHARE_INPUT_CHARS = MAX_SHARE_HASH_CHARS + 4096;
const PREFIX = "#fm:";

function isShareHash(hash) {
  return typeof hash === "string" &&
    (hash.startsWith(PREFIX) || hash.startsWith("#pako:") || hash.startsWith("#base64:"));
}

function validUnicode(text) {
  for (const character of text) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw new Error("Repair unpaired Unicode surrogates before sharing.");
  }
}
function checkedDocument(value) {
  if (!value || Array.isArray(value) || Object.keys(value).sort().join(",") !== "name,source" ||
      typeof value.source !== "string" || typeof value.name !== "string") {
    throw new Error("Invalid shared document: expected source text and a filename.");
  }
  const { source, name } = value;
  if (!name || name.length > 240 || name.trim() !== name || name === "." || name === ".." ||
      /[\\/\u0000-\u001f\u007f]/.test(name)) throw new Error("Invalid shared filename.");
  if (source.length > MAX_SHARE_SOURCE_BYTES) throw new Error("Source is too large for a share link; download it instead.");
  validUnicode(source);
  validUnicode(name);
  if (source.includes("\0")) throw new Error("Shared source cannot contain NUL bytes.");
  if (new TextEncoder().encode(source).byteLength > MAX_SHARE_SOURCE_BYTES) {
    throw new Error("Source is too large for a share link; download it instead.");
  }
  return Object.freeze({ source, name });
}
function base64url(bytes) {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 16384) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 16384));
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function unbase64url(text) {
  if (!/^[A-Za-z0-9_-]+$/.test(text) || text.length % 4 === 1) throw new Error("Malformed share-link encoding.");
  const binary = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = Uint8Array.from(binary, character => character.charCodeAt(0));
  if (base64url(bytes) !== text) throw new Error("Non-canonical share-link encoding.");
  return bytes;
}
function unbase64Live(text) {
  // Mermaid Live emits unpadded base64url. Also accept correctly padded standard
  // base64 from older producers, but not ignored whitespace or nonzero pad bits.
  if (!/^[A-Za-z0-9+/_-]+={0,2}$/.test(text) ||
      (text.includes("=") && text.length % 4 !== 0)) {
    throw new Error("Malformed Mermaid Live link encoding.");
  }
  return unbase64url(text.replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_"));
}
async function boundedBytes(stream, limit) {
  const reader = stream.getReader(), chunks = [];
  let size = 0, complete = false;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) { complete = true; break; }
      size += value.byteLength;
      if (size > limit) throw new Error("Shared document exceeds the decoded size limit.");
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    return bytes;
  } finally {
    if (!complete) await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/** Encode an immutable copy; extra caller fields (including dirty baselines) are excluded. */
export async function encodeShareHash(document, { Compression = globalThis.CompressionStream } = {}) {
  const checked = checkedDocument({ source: document?.source, name: document?.name });
  let bytes = new TextEncoder().encode(JSON.stringify(checked)), encoding = "raw";
  if (typeof Compression === "function") {
    try {
      const compressed = await boundedBytes(new Blob([bytes]).stream().pipeThrough(new Compression("gzip")), MAX_PAYLOAD_BYTES + 1024);
      if (compressed.length + 1 < bytes.length) { bytes = compressed; encoding = "gzip"; }
    } catch {
      // Small documents remain shareable in browsers without a working compression codec.
    }
  }
  const hash = `${PREFIX}v1:${encoding}:${base64url(bytes)}`;
  if (hash.length > MAX_SHARE_HASH_CHARS) throw new Error("Share link is too long; download the source instead.");
  return hash;
}

/** Export only source plus fixed editor defaults, never host configuration or recovery data. */
export async function encodeMermaidLiveHash(source, { Compression = globalThis.CompressionStream } = {}) {
  const checked = checkedDocument({ source, name: "diagram.mmd" });
  // Mermaid Live State uses a JSON string for mermaid, not a configuration object.
  const state = { code: checked.source, mermaid: "{}", updateDiagram: true, rough: false, editorMode: "code" };
  let bytes = new TextEncoder().encode(JSON.stringify(state)), encoding = "base64";
  if (typeof Compression === "function") {
    try {
      const compressed = await boundedBytes(new Blob([bytes]).stream().pipeThrough(new Compression("deflate")), MAX_PAYLOAD_BYTES + 1024);
      if (compressed.length < bytes.length) { bytes = compressed; encoding = "pako"; }
    } catch {
      // The upstream legacy base64 format needs no compression codec.
    }
  }
  const hash = `#${encoding}:${base64url(bytes)}`;
  if (hash.length > MAX_SHARE_HASH_CHARS) throw new Error("Share link is too long; download the source instead.");
  return hash;
}

/** Decode native or Mermaid Live fragments. Import source only, never foreign settings. */
export async function decodeShareHash(hash, { Decompression = globalThis.DecompressionStream } = {}) {
  if (typeof hash !== "string") throw new TypeError("Share fragment must be text.");
  if (!isShareHash(hash)) return null;
  if (hash.length > MAX_SHARE_HASH_CHARS) throw new Error("Share link exceeds the encoded size limit.");
  const live = /^#(pako|base64):(.+)$/.exec(hash);
  if (live) {
    let bytes = unbase64Live(live[2]);
    if (live[1] === "pako") {
      // pako.deflate uses a zlib wrapper, not gzip and not raw DEFLATE.
      if (typeof Decompression !== "function") {
        throw new Error("This browser cannot decompress the Mermaid Live link. Use a browser with deflate stream support.");
      }
      bytes = await boundedBytes(new Blob([bytes]).stream().pipeThrough(new Decompression("deflate")), MAX_PAYLOAD_BYTES);
    }
    const state = JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes));
    if (!state || typeof state !== "object" || Array.isArray(state) ||
        !Object.hasOwn(state, "code") || typeof state.code !== "string") {
      throw new Error("Invalid Mermaid Live document: expected code text.");
    }
    // Discard Mermaid config, callbacks, external resource URLs, and editor state.
    // The host's security policy and the normal source validation remain authoritative.
    return checkedDocument({ source: state.code, name: "diagram.mmd" });
  }
  const match = /^#fm:v([0-9]+):(raw|gzip):([^:]+)$/.exec(hash);
  if (!match) throw new Error("Malformed or unsupported share-link format.");
  if (match[1] !== "1") throw new Error("Unsupported share-link version.");
  let bytes = unbase64url(match[3]);
  if (match[2] === "gzip") {
    if (typeof Decompression !== "function") throw new Error("This browser cannot decompress the share link. Open it in a browser with gzip stream support.");
    bytes = await boundedBytes(new Blob([bytes]).stream().pipeThrough(new Decompression("gzip")), MAX_PAYLOAD_BYTES);
  }
  const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  return checkedDocument(JSON.parse(text));
}

/** Read a pasted link locally. Never fetch its host, path, query, or embedded resources. */
export async function decodeShareLink(input, options) {
  if (typeof input !== "string") throw new TypeError("Share link must be text.");
  if (input.length > MAX_SHARE_INPUT_CHARS) throw new Error("Share link exceeds the input size limit.");
  const text = input.trim();
  if (!text || /[\u0000-\u0020\u007f]/.test(text)) {
    throw new Error("Paste one complete share URL or share fragment without internal whitespace.");
  }
  let hash = text;
  if (!text.startsWith("#")) {
    let url;
    try { url = new URL(text); }
    catch { throw new Error("Paste a full HTTP or HTTPS share URL, or a supported share fragment."); }
    if (url.protocol !== "https:" && url.protocol !== "http:") {
      throw new Error("Only HTTP or HTTPS share URLs can be imported.");
    }
    if (url.username || url.password) throw new Error("Remove credentials from the share URL before importing.");
    hash = url.hash;
  }
  if (!isShareHash(hash)) throw new Error("The link has no supported source fragment (#fm:, #pako:, or #base64:).");
  return decodeShareHash(hash, options);
}

export function shareUrl(base, hash) {
  const url = new URL(base);
  if (url.protocol !== "https:" && url.protocol !== "http:") throw new Error("Serve the playground over HTTP or HTTPS to share it.");
  if (!hash.startsWith(PREFIX) || hash.length > MAX_SHARE_HASH_CHARS) throw new Error("Invalid share fragment.");
  // Do not accidentally share unrelated query-string credentials or HTTP URL credentials.
  url.username = "";
  url.password = "";
  url.search = "";
  url.hash = hash;
  return url.href;
}

/** Source-only sharing remains usable when every renderer is unavailable. */
export function mountShareControls({ panelEl, workspace }) {
  const document = panelEl.ownerDocument, host = document.defaultView;
  let disposed = false, operation = 0, revision = null, currentUrl = "";
  let navigation = 0, observedHash = null, opening = false, openedRevision = null;
  let importOperation = 0, importing = false;
  function make(tag, text, id) {
    const element = document.createElement(tag);
    element.id = id;
    element.textContent = text;
    panelEl.append(element);
    return element;
  }
  const formatLabel = make("label", "Share link destination", "share-format-label");
  const format = make("select", "", "share-format");
  formatLabel.htmlFor = format.id;
  for (const [value, text] of [["frankenmermaid", "FrankenMermaid (this playground)"], ["mermaid-live", "Mermaid Live (source only)"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = text;
    format.append(option);
  }
  format.value = "frankenmermaid";
  const formatNotice = make("p", "FrankenMermaid links keep the current filename and source in this playground.", "share-format-notice");
  formatNotice.setAttribute("role", "status");
  format.setAttribute("aria-describedby", formatNotice.id);
  const generate = make("button", "Create share link", "share-create");
  const label = make("label", "Editable diagram link", "share-label");
  const link = make("input", "", "share-url");
  link.type = "text";
  link.readOnly = true;
  link.autocomplete = "off";
  link.spellcheck = false;
  label.htmlFor = link.id;
  const copy = make("button", "Copy link", "share-copy");
  generate.type = copy.type = "button";
  const status = make("p", "Links contain your source, not just an image. Anyone with a link can read it. Creating a link does not save a file.", "share-status");
  status.setAttribute("role", "status");
  link.setAttribute("aria-describedby", status.id);
  const reopen = make("button", "Open address-bar source", "share-open");
  reopen.type = "button";
  reopen.hidden = true;
  const navigationStatus = make("p", "", "share-navigation-status");
  navigationStatus.setAttribute("role", "status");
  navigationStatus.hidden = true;
  const importLabel = make("label", "Import a FrankenMermaid or Mermaid Live link", "share-import-label");
  const importInput = make("input", "", "share-import-url");
  importInput.type = "text";
  importInput.autocomplete = "off";
  importInput.spellcheck = false;
  importInput.maxLength = MAX_SHARE_INPUT_CHARS;
  importInput.placeholder = "Paste a share URL or #pako: / #base64: / #fm: fragment";
  importLabel.htmlFor = importInput.id;
  const importButton = make("button", "Import linked source", "share-import");
  importButton.type = "button";
  importButton.disabled = true;
  const importStatus = make("p", "Links are decoded locally without visiting their websites. Only source is imported; embedded settings are not applied.", "share-import-status");
  importStatus.setAttribute("role", "status");
  importInput.setAttribute("aria-describedby", importStatus.id);
  function cancelImport(message) {
    importOperation += 1;
    if (importing) importStatus.textContent = message;
    importing = false;
    importButton.disabled = disposed || !importInput.value.trim();
  }
  async function importLink() {
    if (disposed || importing) return false;
    const value = importInput.value, ticket = ++navigation, importTicket = ++importOperation;
    importing = true;
    importButton.disabled = true;
    // A pasted link supersedes pending address-bar imports without changing history.
    opening = false;
    openedRevision = null;
    reopen.disabled = false;
    importStatus.textContent = "Importing linked source…";
    const isCurrent = () => !disposed && ticket === navigation && importTicket === importOperation &&
      importInput.value === value;
    try {
      const opened = await workspace.openSharedSource(() => decodeShareLink(value), { isCurrent });
      if (!isCurrent()) return false;
      sourceChanged();
      if (!isCurrent()) return false;
      importStatus.textContent = opened ?
        "Linked source imported. Embedded settings were not applied. Download or save it to keep a local file; the address bar was not changed." :
        "Linked source was not imported; current document retained. See the source-file status for details.";
      if (opened && isShareHash(host.location.hash)) {
        navigationStatus.hidden = false;
        navigationStatus.textContent = "Current source came from the pasted link. The address bar still contains the previous shared source.";
      }
      return opened;
    } catch (error) {
      if (isCurrent()) importStatus.textContent = `Cannot import link: ${error.message || error}. Current document retained.`;
      return false;
    } finally {
      if (!disposed && importTicket === importOperation) {
        importing = false;
        importButton.disabled = !importInput.value.trim();
      }
    }
  }
  function clearLink() {
    currentUrl = "";
    link.value = "";
    link.hidden = label.hidden = true;
    copy.disabled = true;
  }
  clearLink();
  function sourceChanged() {
    if (disposed) return;
    const currentRevision = workspace.sourceSnapshot().revision;
    if (openedRevision !== null && currentRevision !== openedRevision) {
      openedRevision = null;
      navigationStatus.textContent = "Source changed. The address-bar link still contains the original shared source; create a new link to share your edits.";
    }
    if (revision === null || currentRevision === revision) return;
    operation += 1;
    revision = null;
    clearLink();
    generate.disabled = false;
    status.textContent = "Source changed. Create a new link to share the current document.";
  }
  async function createLink() {
    if (disposed) return false;
    const ticket = ++operation, destination = format.value;
    clearLink();
    generate.disabled = true;
    status.textContent = "Creating share link…";
    try {
      if (destination !== "frankenmermaid" && destination !== "mermaid-live") throw new Error("Choose a supported share destination.");
      const snapshot = workspace.sourceSnapshot();
      revision = snapshot.revision;
      const hash = await (destination === "mermaid-live" ? encodeMermaidLiveHash(snapshot.source) : encodeShareHash(snapshot));
      if (disposed || ticket !== operation || destination !== format.value) return false;
      sourceChanged();
      if (disposed || ticket !== operation || destination !== format.value) return false;
      currentUrl = destination === "mermaid-live" ? `https://mermaid.live/edit${hash}` : shareUrl(host.location.href, hash);
      link.value = currentUrl;
      link.hidden = label.hidden = false;
      copy.disabled = false;
      status.textContent = destination === "mermaid-live" ?
        "Mermaid Live link ready. Opening it sends you to Mermaid Live with this source. Anyone with the link can read it. FrankenMermaid-only syntax may not render there; filename and playground settings are not included." :
        "Link ready. Anyone with it can read the source. Longer links may exceed the limits of messaging services; source download is the alternative.";
      return true;
    } catch (error) {
      if (!disposed && ticket === operation) status.textContent = `Cannot share: ${error.message || error}`;
      return false;
    } finally {
      if (!disposed && ticket === operation) generate.disabled = false;
    }
  }
  async function copyLink() {
    if (disposed) return false;
    sourceChanged();
    if (!currentUrl) return false;
    const ticket = operation;
    try {
      // A separate click retains user activation even when compression took time.
      if (!host.navigator.clipboard?.writeText) throw new Error("Clipboard API unavailable");
      await host.navigator.clipboard.writeText(currentUrl);
      if (!disposed && ticket === operation) status.textContent = "Link copied. It contains your source; share it only with intended recipients.";
      return true;
    } catch {
      if (!disposed && ticket === operation) {
        link.focus();
        link.select();
        status.textContent = "Clipboard unavailable. The link is selected; copy it manually.";
      }
      return false;
    }
  }
  // URL navigation is separate from link generation: copying a snapshot never changes
  // history, and an old decoder/confirmation cannot commit into a later navigation.
  async function restoreLocation(force = false) {
    if (disposed) return false;
    const hash = host.location.hash;
    // Traversal can emit both popstate and hashchange. Never reset editor selection/undo
    // twice, or retry a declined replacement without another explicit user action.
    if (!force && hash === observedHash) return false;
    cancelImport("Address-bar navigation cancelled the pending link import. Current document retained.");
    const previousHash = observedHash;
    observedHash = hash;
    const ticket = ++navigation;
    openedRevision = null;
    opening = isShareHash(hash);
    reopen.hidden = !opening;
    reopen.disabled = opening;
    reopen.textContent = "Open address-bar source";
    if (!opening) {
      if (isShareHash(previousHash)) {
        navigationStatus.hidden = false;
        navigationStatus.textContent = "The address-bar share link was cleared. Current source retained.";
      }
      return false;
    }
    navigationStatus.hidden = false;
    navigationStatus.textContent = "Opening shared source…";
    const isCurrent = () => !disposed && ticket === navigation && host.location.hash === hash;
    try {
      const opened = await workspace.openSharedSource(() => decodeShareHash(hash), { isCurrent });
      if (!isCurrent()) return false;
      if (opened) {
        openedRevision = workspace.sourceSnapshot().revision;
        reopen.textContent = "Reopen address-bar source";
        navigationStatus.textContent = "Shared source opened. Edit, present, or download it using the normal document controls." +
          (hash.startsWith(PREFIX) ? "" : " Mermaid Live settings were not applied; only source was imported.");
      } else {
        navigationStatus.textContent = "Shared source was not opened; the current document is retained. See the source-file status for details. Use Open address-bar source to retry.";
      }
      return opened;
    } catch (error) {
      if (isCurrent()) navigationStatus.textContent = `Cannot open shared source: ${error.message || error}. Current document retained.`;
      return false;
    } finally {
      if (isCurrent()) { opening = false; reopen.disabled = false; }
    }
  }
  function onLocationChange(event) {
    if (disposed) return;
    // A rapid A -> B -> A can queue a B event after the location is already A. Its
    // intervening navigation must invalidate an A decode still in flight (ABA guard).
    // Completed opens are not repeated for these obsolete notifications.
    if (event.newURL && new URL(event.newURL).hash !== host.location.hash) {
      if (!opening && !importing) return;
      navigation += 1;
      observedHash = null;
    }
    void restoreLocation();
  }
  const openCurrentLink = () => restoreLocation(true);
  const onReopen = () => { void openCurrentLink(); };
  const onGenerate = () => { void createLink(); };
  const onCopy = () => { void copyLink(); };
  const onFormat = () => {
    if (disposed) return;
    operation += 1; // Even an A -> B -> A switch revokes pending compression results.
    revision = null;
    clearLink();
    generate.disabled = false;
    status.textContent = "Destination changed. Create a new link for the selected destination.";
    formatNotice.textContent = format.value === "mermaid-live" ?
      "Opening these links uses the external Mermaid Live editor. Only source is included; FrankenMermaid-only syntax may not render there." :
      "FrankenMermaid links keep the current filename and source in this playground.";
  };
  const onImport = () => { void importLink(); };
  const onImportInput = () => cancelImport("Link changed. Import again to open the new link; current document retained.");
  const onImportKey = event => {
    if (event.key !== "Enter" || event.defaultPrevented || event.repeat || event.isComposing ||
        event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    event.preventDefault();
    if (!importButton.disabled) void importLink();
  };
  generate.addEventListener("click", onGenerate);
  format.addEventListener("change", onFormat);
  copy.addEventListener("click", onCopy);
  reopen.addEventListener("click", onReopen);
  importButton.addEventListener("click", onImport);
  importInput.addEventListener("input", onImportInput);
  importInput.addEventListener("keydown", onImportKey);
  host.addEventListener("hashchange", onLocationChange);
  host.addEventListener("popstate", onLocationChange);
  const ready = restoreLocation();
  return {
    ready, createLink, copyLink, sourceChanged, openCurrentLink, importLink,
    dispose() {
      if (disposed) return;
      disposed = true;
      operation += 1;
      navigation += 1;
      cancelImport("Link import cancelled because the sharing controls were closed.");
      importInput.disabled = true;
      importButton.removeEventListener("click", onImport);
      importInput.removeEventListener("input", onImportInput);
      importInput.removeEventListener("keydown", onImportKey);
      host.removeEventListener("hashchange", onLocationChange);
      host.removeEventListener("popstate", onLocationChange);
      reopen.removeEventListener("click", onReopen);
      reopen.disabled = true;
      generate.removeEventListener("click", onGenerate);
      format.removeEventListener("change", onFormat);
      format.disabled = true;
      copy.removeEventListener("click", onCopy);
      clearLink();
      generate.disabled = true;
    },
  };
}
