const root = document.getElementById('preview');
const { wasmModule, wasmBinary } = document.body.dataset;
const vscode = acquireVsCodeApi();

if (!root) {
  throw new Error('FrankenMermaid preview root is missing');
}

let renderSvg;
let activeObjectUrl;

function showText(text) {
  root.textContent = text;
}

async function start() {
  try {
    const wasm = await import(wasmModule);
    await wasm.default(wasmBinary);
    renderSvg = wasm.renderSvg;
    showText('Open a Mermaid document to render it here.');
    vscode.postMessage({ type: 'ready' });
  } catch (error) {
    showText(`Unable to initialize FrankenMermaid: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function render(message) {
  if (!renderSvg) {
    return;
  }
  try {
    const svg = renderSvg(message.source);
    if (activeObjectUrl) {
      URL.revokeObjectURL(activeObjectUrl);
    }
    activeObjectUrl = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }));
    const image = new Image();
    image.alt = `FrankenMermaid preview: ${message.title}`;
    image.src = activeObjectUrl;
    root.replaceChildren(image);
  } catch (error) {
    showText(`Unable to render Mermaid: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function onMessage(event) {
  if (event.origin !== window.location.origin || event.source !== window) {
    return;
  }
  if (event.data?.type === 'render') {
    render(event.data);
  }
}

function dispose() {
  window.removeEventListener('message', onMessage);
  window.removeEventListener('pagehide', dispose);
  if (activeObjectUrl) {
    URL.revokeObjectURL(activeObjectUrl);
  }
}

window.addEventListener('message', onMessage);
window.addEventListener('pagehide', dispose, { once: true });

void start();
