# Mermaid-shaped browser API

`web/mermaid.mjs` provides a browser-facing `initialize`, `render`, and `parse` API on top of the existing Rust WASM engine. The raw `pkg/frankenmermaid.js` API is unchanged. Serve the checkout over HTTP with its built `pkg/` assets:

```html
<div id="graph"></div>
<script type="module">
  import mermaid from './web/mermaid.mjs';
  mermaid.initialize({ startOnLoad: false, theme: 'dark' });
  const { svg, diagnostics, warnings } = await mermaid.render(
    'orders-diagram',
    'flowchart LR\nOrder-->Payment\nPayment-->Shipping'
  );
  document.getElementById('graph').innerHTML = svg;
  console.log(diagnostics, warnings);
</script>
```

`initialize` is synchronous and replaces the instance's configuration. Each async operation captures its own deep snapshot: caller mutations and later initialization cannot change an already submitted render. The Rust `validateConfig` export validates each configuration before parsing/rendering; unsupported keys fail explicitly rather than becoming silent defaults. WASM loading is lazy, shared by concurrent calls, and retryable after initialization failure. No call mutates the raw engine's global `init` configuration.

`render(id, source, container?)` returns a promise with `{ svg, diagramType, diagnostics, warnings }`. The optional element supplies its owning document; it is never cleared or populated by `render`. Render IDs must start with an ASCII letter or underscore and contain at most 128 letters, digits, underscores, or hyphens. Use different IDs for diagrams that coexist. Source is limited to 2 MiB of UTF-8; returned SVG is limited to 16 MiB.

## Multiple diagrams and safety

The adapter namespaces the engine's SVG IDs, local paint/marker/use references, accessibility references, CSS selectors, and keyframe names. This allows multiple differently themed diagrams in the same light DOM without marker, style, or animation collisions. Text, geometry, and source remain engine-owned. Authored links and SVG tooltips are retained; new-tab links receive `noopener noreferrer`.

SVG processing requires a browser `Document`, XML DOM, and constructable CSS stylesheets. It rejects active markup, event-handler attributes, document-global CSS rules, escaped CSS, unresolved internal references, and remote resource loads. It is a constrained adapter for native SVG output, not a general arbitrary-SVG sanitizer. The raw API remains available for applications that intentionally own a broader SVG policy.

## Parsing and errors

```js
mermaid.parseError = (error, { diagnostics }) => {
  console.error(error.message, diagnostics);
};
const result = await mermaid.parse(source, { suppressErrors: true });
if (result !== false) console.log(result.diagramType, result.warnings);
```

**Parsing uses FrankenMermaid's best-effort native parser, not Mermaid.js's strict grammar.** Error-level native diagnostics and unknown diagram types reject; recovered warnings remain visible in the result. `suppressErrors: true` returns `false` and does not invoke `parseError`. A render failure rejects and invokes the hook. Configuration errors carry the native field diagnostics.

The adapter does not implement Mermaid plugins, callback registration, `mermaidAPI` internals, the deprecated callback form of `render`, or arbitrary Mermaid.js configuration. It does not claim complete Mermaid.js API or syntax equivalence.

For independent widgets or a custom WASM loader:

```js
import { createMermaid } from './web/mermaid.mjs';
const widget = createMermaid({
  loadEngine: () => import('./pkg/frankenmermaid.js'),
  document,
  // Optional: wasmInput: ArrayBuffer | WebAssembly.Module | URL
});
widget.initialize({ theme: 'neutral' });
// Dispose prevents pending operations from publishing and rejects future calls.
widget.dispose();
```

## Validation

```sh
node --test web/mermaid-compat.test.mjs
python web/mermaid-compat.browser.test.py
```

The browser tests execute production DOM/CSS handling in Chromium. The engine is an explicit boundary fixture; these tests do not claim execution of the packaged Rust WASM binary or strict Mermaid.js equivalence. Playwright and an existing Chromium are required; `CHROMIUM_PATH` can select the binary. The tests load local production modules with blob URLs and make no network requests.
