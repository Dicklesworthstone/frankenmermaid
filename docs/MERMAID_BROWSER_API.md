# Mermaid-shaped browser API

`web/mermaid.mjs` provides a browser-facing `initialize`, `render`, `run`, and `parse` API on top of the existing Rust WASM engine. The raw `pkg/frankenmermaid.js` API is unchanged. Serve the checkout over HTTP with its built `pkg/` assets:

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

`render(id, source, container?)` returns a promise with `{ svg, diagramType, diagnostics, warnings, bindFunctions? }`. The optional binding function activates registered node callbacks after insertion, as described below. The optional element supplies its owning document; it is never cleared or populated by `render`. Render IDs must start with an ASCII letter or underscore and contain at most 128 letters, digits, underscores, or hyphens. Use different IDs for diagrams that coexist. Source is limited to 2 MiB of UTF-8; returned SVG is limited to 16 MiB.

## Render documents automatically or explicitly

The default browser entry scans `.mermaid` elements once after DOM readiness. Its startup is deferred so the importing script can first call `initialize({ startOnLoad: false })`. That disables automatic startup, not explicit rendering:

```html
<pre class="mermaid">flowchart LR
Order--&gt;Payment</pre>
<script type="module">
  import mermaid from './web/mermaid.mjs';
  mermaid.initialize({ startOnLoad: false, theme: 'dark' });
  await mermaid.run({
    querySelector: '.mermaid',
    postRenderCallback: async (svgId) => {
      console.log(document.getElementById(svgId));
    },
  });
</script>
```

`run({ nodes })` accepts an array-like collection of HTML elements and overrides `querySelector`. Duplicate targets are deduplicated. A call is limited to 64 targets and 16 MiB of aggregate source text, with the existing 2 MiB per-diagram limit. Nested targets are rejected before any rendering, because replacing a parent would destroy the selected child. An empty selection does not load WASM.

Each successful node receives `data-processed="true"`; later runs skip it rather than parse the generated SVG as source. To render a changed diagram, replace the element's text with its new Mermaid source, remove `data-processed`, and call `run` again. The element itself and its existing attributes are retained; only its children are replaced after validation succeeds.

The adapter snapshots all selected sources before waiting for the engine. Source edits, replaced child nodes, attachment changes, disposal, or a competing render cannot authorize a stale replacement. Overlapping runs with the same instance/configuration coalesce in-flight work. Each newly inserted diagram invokes its owning run's callback once, after insertion; callbacks may call `run` again without deadlocking. A callback failure does not roll back an already inserted diagram.

Failures do not erase their source or mark it processed, and independent siblings continue rendering. The returned promise rejects with an `AggregateError` containing each failure. `run({ suppressErrors: true })` instead sends failures to the instance's error reporter and resolves after processing the remaining diagrams. `parseError` receives unsuppressed per-diagram failures. Native recoveries remain warnings, not strict Mermaid.js syntax validation.

`createMermaid` instances are manual by default. Pass `autoStart: true` to opt a custom instance into the same one-shot startup, or call `contentLoaded()` explicitly. `dispose()` removes pending startup listeners/timers and prevents in-flight results from replacing source. It does not destroy a shared engine or interrupt synchronous Rust WASM execution.

## Multiple diagrams and safety

The adapter namespaces the engine's SVG IDs, local paint/marker/use references, accessibility references, CSS selectors, and keyframe names. This allows multiple differently themed diagrams in the same light DOM without marker, style, or animation collisions. Text, geometry, and source remain engine-owned. Authored links and SVG tooltips are retained; new-tab links receive `noopener noreferrer`.

SVG processing requires a browser `Document`, XML DOM, and constructable CSS stylesheets. It rejects active markup, event-handler attributes, document-global CSS rules, escaped CSS, unresolved internal references, and remote resource loads. It is a constrained adapter for native SVG output, not a general arbitrary-SVG sanitizer. The raw API remains available for applications that intentionally own a broader SVG policy.

## Parsing and errors

The native Venn, Wardley, and Event Modeling families are accepted by this adapter as
`venn`, `wardley`, and `eventmodeling`, respectively, rather than rejected as unknown types.

```js
mermaid.parseError = (error, { diagnostics }) => {
  console.error(error.message, diagnostics);
};
const result = await mermaid.parse(source, { suppressErrors: true });
if (result !== false) console.log(result.diagramType, result.warnings);
```

**Parsing uses FrankenMermaid's best-effort native parser, not Mermaid.js's strict grammar.** Error-level native diagnostics and unknown diagram types reject; recovered warnings remain visible in the result. `suppressErrors: true` returns `false` and does not invoke `parseError`. A render failure rejects and invokes the hook. Configuration errors carry the native field diagnostics.

The adapter does not implement Mermaid plugins, automatic lookup of page-global callback functions,
`mermaidAPI` internals, the deprecated callback form of `render`, or arbitrary Mermaid.js
configuration. It does not claim complete Mermaid.js API or syntax equivalence.

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

## Interactive node callbacks

The engine emits callback names and authored node IDs as SVG metadata. A browser must bind
those actions after inserting the SVG. `render()` now provides Mermaid's `bindFunctions`
pattern; `run()` performs that binding automatically, before `postRenderCallback` runs.

Callbacks require **both explicit host registration and host-level `securityLevel: 'loose'`**.
Register names before starting `render()` or `run()`. Registration is an instance-local
FrankenMermaid extension, separate from the JSON initialization configuration:

```html
<div id="interactive-graph"></div>
<p id="selected-node" role="status"></p>
<script type="module">
  import mermaid from './web/mermaid.mjs';
  mermaid.initialize({ startOnLoad: false, securityLevel: 'loose' });
  const unregister = mermaid.registerCallback('showDetails', (nodeId) => {
    // Authored IDs are untrusted text, not markup or executable code.
    document.getElementById('selected-node').textContent = `Selected: ${nodeId}`;
  });
  const { svg, bindFunctions } = await mermaid.render('orders-interactive',
    'flowchart LR\nOrder-->Payment\nclick Order showDetails "Open order details"');
  const host = document.getElementById('interactive-graph');
  host.innerHTML = svg;
  const unbind = bindFunctions?.(host);
  // Retain unbind/unregister for the component's teardown, rather than calling them here.
</script>
```

`bindFunctions(container)` accepts the SVG itself or a containing element, including an
element inside a shadow root. Insert the returned SVG unchanged, bind it, then apply any
host-specific DOM decoration. A missing, duplicated, or changed SVG is rejected before
listeners are installed. Calling the same binding function on the same live SVG is
idempotent and returns the same cleanup function. Call that cleanup before replacing the
diagram or rebinding; it removes listeners and restores only accessibility attributes
which the binding added and which the host has not subsequently changed.

Registered handlers receive the **authored node ID**, not its rewritten DOM ID. Pointer
clicks, Tab focus, Enter, and Space are supported; Space activates on release and cancels
when focus leaves. Modified/composing keyboard input, key repeats, disabled targets, and stale events do not invoke
callbacks. Native hyperlinks keep their browser behavior, and existing tooltip metadata
is preserved. Unregistered callback names remain inert and are reported through
`createMermaid`'s `reportError` hook (the console by default). Handler exceptions and rejected
promises use that same reporter, not `parseError`.

There is no `eval`, `window[name]`, dotted-property traversal, or interpretation of authored
callback arguments. Names such as `app.showDetails` are literal registry keys. Unsafe
prototype names and non-functions are rejected, and each instance permits at most 1,024
registered names. The exported SVG contains metadata, not function bodies or inline
event handlers; a downloaded SVG does not embed the application's registered code.

Each rendering operation snapshots its callback registry; `run()` uses one snapshot for
the entire selected batch. A registration made during an in-flight render cannot grant
authority to that result. Replacing or unregistering a handler revokes its old bindings;
an old unregister function cannot remove a newer registration of the same name. Render
again to bind a replacement handler. Independent instances never share registrations.

Initialization replaces the complete configuration. Switching to strict/antiscript or
omitting `securityLevel: 'loose'` removes existing interaction bindings and revokes pending
ones. A source-level initialization directive cannot weaken this host policy. Returning
to loose mode requires a new render; it does not resurrect old binding functions.
`dispose()` removes all surviving bindings without removing their diagrams, clears the
registry, and prevents future binding. Lifecycle tracking uses weak references so detached
diagram trees need not remain alive until their owning widget is disposed.

## Validation

```sh
node --test web/mermaid-compat.test.mjs
python web/mermaid-compat.browser.test.py
python web/mermaid-interactions.browser.test.py
```

The browser tests execute production DOM/CSS handling and real pointer/keyboard input in
Chromium. The interaction suite covers binding, registry and security revocation, disposal,
concurrent runs, malformed metadata, and reporter reentrancy at the publication boundary.
The engine is an explicit boundary fixture; these tests do not claim execution of the
packaged Rust WASM binary or strict Mermaid.js equivalence. Playwright and an existing
Chromium are required; `CHROMIUM_PATH` can select the binary. The tests load local production
modules with blob/data URLs and make no network requests.
