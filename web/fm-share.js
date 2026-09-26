// Editable source sharing, not a rendered-image cache. The versioned fragment carries only
// source + filename: never the recovery baseline, storage keys, SVG, or executable settings.
// No server, WASM, or third-party codec is involved. Limits apply BEFORE parsing/decompression.
export const MAX_SHARE_HASH_CHARS = 64 * 1024;
export const MAX_SHARE_SOURCE_BYTES = 256 * 1024;
const MAX_PAYLOAD_BYTES = MAX_SHARE_SOURCE_BYTES * 6 + 4096; // Worst-case JSON escaping.
const PREFIX = "#fm:";

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

/** Ordinary page anchors are not documents. Malformed/unknown fm: versions fail explicitly. */
export async function decodeShareHash(hash, { Decompression = globalThis.DecompressionStream } = {}) {
  if (typeof hash !== "string") throw new TypeError("Share fragment must be text.");
  if (!hash.startsWith(PREFIX)) return null;
  if (hash.length > MAX_SHARE_HASH_CHARS) throw new Error("Share link exceeds the encoded size limit.");
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
  function make(tag, text, id) {
    const element = document.createElement(tag);
    element.id = id;
    element.textContent = text;
    panelEl.append(element);
    return element;
  }
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
  function clearLink() {
    currentUrl = "";
    link.value = "";
    link.hidden = label.hidden = true;
    copy.disabled = true;
  }
  clearLink();
  function sourceChanged() {
    if (disposed || revision === null || workspace.sourceSnapshot().revision === revision) return;
    operation += 1;
    revision = null;
    clearLink();
    generate.disabled = false;
    status.textContent = "Source changed. Create a new link to share the current document.";
  }
  async function createLink() {
    if (disposed) return false;
    const ticket = ++operation;
    clearLink();
    generate.disabled = true;
    status.textContent = "Creating share link…";
    try {
      const snapshot = workspace.sourceSnapshot();
      revision = snapshot.revision;
      const hash = await encodeShareHash(snapshot);
      if (disposed || ticket !== operation) return false;
      sourceChanged();
      if (ticket !== operation) return false;
      currentUrl = shareUrl(host.location.href, hash);
      link.value = currentUrl;
      link.hidden = label.hidden = false;
      copy.disabled = false;
      status.textContent = "Link ready. Anyone with it can read the source. Longer links may exceed the limits of messaging services; source download is the alternative.";
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
  async function restoreInitialLink() {
    const hash = host.location.hash;
    if (!hash.startsWith(PREFIX)) return false;
    status.textContent = "Opening shared source…";
    const isCurrent = () => !disposed && host.location.hash === hash;
    const opened = await workspace.openSharedSource(() => decodeShareHash(hash), { isCurrent });
    if (isCurrent()) status.textContent = opened ? "Shared source opened. Edit, present, or download it using the normal document controls." :
      "Shared source was not opened; the current document is retained. See the source-file status for details.";
    return opened;
  }
  const onGenerate = () => { void createLink(); };
  const onCopy = () => { void copyLink(); };
  generate.addEventListener("click", onGenerate);
  copy.addEventListener("click", onCopy);
  const ready = restoreInitialLink();
  return {
    ready, createLink, copyLink, sourceChanged,
    dispose() {
      if (disposed) return;
      disposed = true;
      operation += 1;
      generate.removeEventListener("click", onGenerate);
      copy.removeEventListener("click", onCopy);
      clearLink();
      generate.disabled = true;
    },
  };
}
