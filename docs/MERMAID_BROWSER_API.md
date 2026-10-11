# Mermaid-shaped browser API

`web/mermaid.mjs` provides a browser-facing `initialize`, `render`, `run`, and `parse` API on top of the existing Rust WASM engine. Its default instance executes WASM in an owned module worker; the raw `pkg/frankenmermaid.js` API is unchanged. Serve the checkout over HTTP with its built `pkg/` assets:

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

## Worker isolation, stop, and recovery

The default source-checkout entry uses `createWorkerMermaid`. Configuration validation,
parsing, layout, and SVG generation execute in one lazy module worker per instance. SVG
validation, ID/CSS scoping, DOM insertion, and registered callbacks remain on the UI thread.
An empty `run({ nodes: [] })` does not start a worker. The worker entry is the existing
`mermaid-compat.mjs` module with an internal opt-in fragment; no additional runtime asset is
needed. Hosts must permit module workers and the engine's resources under their content
security policy. A blocked or unavailable worker rejects explicitly; failed work is never
silently replayed through synchronous WASM on the UI thread.

Independent diagrams use a first-in, first-out queue, not the playground editor's
latest-keystroke supersession policy. The queue admits at most 128 active/pending engine
calls and 32 MiB of serialized UTF-8 arguments per instance. Overload rejects the new call
with `code: 'worker-queue-full'` without evicting admitted work. Worker replies are bounded
at 16 MiB before being posted to the host; the parser reply carries only its native diagram
type, diagnostics, and warnings, not a second copy of the graph IR.

For independently owned widgets, custom engine URLs, or a different execution deadline:

```js
import { createWorkerMermaid } from './web/mermaid.mjs';
const widget = createWorkerMermaid({
  engineUrl: new URL('./pkg/frankenmermaid.js', document.baseURI),
  requestTimeoutMs: 30_000,
  document,
});
widget.initialize({ startOnLoad: false, theme: 'neutral' });

// A Stop button can call this without destroying the widget or changing its source.
// Always handle the promises returned by parse/render/run, including cancellation.
const pending = widget.render('preview', 'flowchart LR\nA-->B');
widget.cancelPending();
try {
  await pending;
} catch (error) {
  if (error.code !== 'cancelled') throw error;
}

// Configuration and registered callbacks survive Stop. A new call starts fresh work.
const corrected = await widget.render('preview', 'flowchart LR\nA-->C');
// At component teardown, release the worker and interaction bindings permanently.
widget.dispose();
```

`requestTimeoutMs` defaults to 30 seconds and must be a positive integer within the browser
32-bit timer range. It bounds initialization and each dispatched engine call, not queue
waiting or the entire multi-diagram batch. A timeout terminates the active worker and
rejects its request with `code: 'worker-timeout'`; crashes and WebAssembly traps also retire
the instance. Independent queued work may start in a fresh worker, but the failed request
is never automatically retried. Initialization failure rejects the waiting group together;
a later explicit call may retry with the same configuration. Idle workers retire after
60 seconds and reload lazily when needed. These are resource/recovery rules, not a measured
performance claim or a hard real-time scheduling guarantee.

`cancelPending()` stops all operations already submitted to that API instance, including
queued calls, a non-yielding WASM invocation, and the remaining diagrams of an active
`run()`. It rejects `parse`/`render` with an error named `AbortError` and `code: 'cancelled'`.
`run()` retains its normal `AggregateError` contract, containing the cancellation error;
`suppressErrors` follows the existing reporting behavior. A stopped batch does not mark
unrendered sources processed or start its next diagram in the replacement worker. Immediate
retry on the same element is supported, including when older runs had coalesced their work.
Already inserted diagrams, configuration, callback registrations, and existing bindings
are retained. Other API instances are unaffected. Cancellation does not undo host callback
side effects or stop host JavaScript; it stops waiting for pending callbacks and prevents
the old operation from continuing the library's pipeline or publishing a late result.

`engineUrl` is trusted host configuration, never a diagram/source directive. HTTP(S) and
host-owned blob/data module URLs are accepted. `createWorkerMermaid` does not accept a
`loadEngine` function or `wasmInput`; use the explicit custom-loader API below when the host
intentionally owns execution outside this worker path. This change does not rebuild the
vendored WASM binary or publish a new package distribution.

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

Failures do not erase their source or mark it processed, and independent siblings continue rendering. The returned promise rejects with an `AggregateError` containing each failure. `run({ suppressErrors: true })` instead sends failures to the instance's error reporter and resolves after processing the remaining diagrams. `parseError` receives unsuppressed per-diagram failures. Native recoveries remain warnings, not strict Mermaid.js syntax validation. Explicit cancellation is different from a single-diagram failure: it stops the remaining batch rather than continuing siblings.

Factory-created instances are manual by default. Pass `autoStart: true` to opt a custom instance into the same one-shot startup, or call `contentLoaded()` explicitly. `dispose()` removes pending startup listeners/timers, settles pending operations, and prevents in-flight results from replacing source. A worker-backed instance also terminates its owned worker. A direct `createMermaid` instance does not destroy a shared engine or interrupt synchronous Rust WASM execution supplied by its host.

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

For a custom host-owned WASM loader, explicitly choose `createMermaid`. This example imports
and executes WASM on the calling thread; it does not provide worker timeout/preemption:

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

The direct adapter also supports logical `cancelPending()` invalidation of pending async
operations. It cannot stop synchronous host code; use `createWorkerMermaid` to preempt WASM.

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
node --test web/mermaid-compat.test.mjs web/mermaid-worker.test.mjs
python web/mermaid-compat.browser.test.py
python web/mermaid-interactions.browser.test.py
python web/mermaid-worker.browser.test.py
```

The browser tests execute production DOM/CSS handling and real pointer/keyboard input in
Chromium. The interaction suite covers binding, registry and security revocation, disposal,
concurrent runs, malformed metadata, and reporter reentrancy at the publication boundary.
The engine is an explicit boundary fixture; these tests do not claim execution of the
packaged Rust WASM binary or strict Mermaid.js equivalence. Playwright and an existing
Chromium are required; `CHROMIUM_PATH` can select the binary. The tests load local production
modules with blob/data URLs and make no network requests.

The worker browser suite runs the production protocol and scheduler in real dedicated
workers. Its engine fixture includes a real non-yielding WebAssembly loop to exercise
initialization, validation, parse/render deadlines, explicit stop, and disposal. It checks
UI responsiveness, no failed-input replay, queue recovery, source retention, immediate
same-element retry, callbacks, and oversized reply rejection. Only static module URL
bindings are substituted for offline loading; the WASM fixture is not the Rust renderer.
