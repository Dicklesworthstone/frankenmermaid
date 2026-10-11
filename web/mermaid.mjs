// Source-checkout entry. No WASM is imported or executed on the browser UI thread.
import { createWorkerMermaid } from "./mermaid-compat.mjs";
export { createMermaid, createWorkerMermaid, MermaidError } from "./mermaid-compat.mjs";
export default createWorkerMermaid({ autoStart: true,
  engineUrl: new URL("../pkg/frankenmermaid.js", import.meta.url) });
