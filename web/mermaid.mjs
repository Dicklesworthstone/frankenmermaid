// Browser entry for source checkouts. The package ships an equivalent sibling-engine entry.
import { createMermaid } from "./mermaid-compat.mjs";
export { createMermaid, MermaidError } from "./mermaid-compat.mjs";
export default createMermaid({ loadEngine: () => import("../pkg/frankenmermaid.js") });
