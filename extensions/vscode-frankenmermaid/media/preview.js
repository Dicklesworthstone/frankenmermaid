const root = document.getElementById('preview');
const { wasmModule, wasmBinary } = document.body.dataset;
const vscode = acquireVsCodeApi();

if (!root) {
  throw new Error('FrankenMermaid preview root is missing');
}

let renderSvg;

function showText(text) {
  root.textContent = text;
}

async function start() {
  try {
    const wasm = await import(/* @vite-ignore */ `${wasmModule}`);
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
    const parser = new DOMParser();
    const doc = parser.parseFromString(svg, 'image/svg+xml');
    const svgElement = doc.documentElement;
    root.replaceChildren(svgElement);
  } catch (error) {
    showText(`Unable to render Mermaid: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function isRenderMessage(message) {
  return message !== null
    && typeof message === 'object'
    && message.type === 'render'
    && typeof message.source === 'string'
    && typeof message.title === 'string';
}

function onMessage(event) {
  if (!isRenderMessage(event.data)) {
    return;
  }
  render(event.data);
}

function dispose() {
  window.removeEventListener('message', onMessage);
  window.removeEventListener('pagehide', dispose);
}

window.addEventListener('message', onMessage);
window.addEventListener('pagehide', dispose, { once: true });

void start();
