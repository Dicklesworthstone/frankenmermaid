# Retained SVG selection

`bindSvgSelection` adds node selection to an already-rendered FrankenMermaid SVG.
It is exported from the existing `web/mermaid.mjs` browser entry, alongside
`bindCanvasSelection`. It does not parse, lay out, start a worker, load WASM, mutate
diagram geometry, or invoke authored links/callbacks. Selection is opt-in: merely
importing the entry does not install this controller.

```js
import { bindSvgSelection } from "./web/mermaid.mjs";

const svg = document.querySelector("#diagram-host svg");
const selection = bindSvgSelection({
  svg,
  tool: "box",                 // "box" or "lasso"
  containment: "intersects",    // "intersects" or "contains"
  dragThreshold: 4,             // CSS pixels, from 0 through 64
  onSelectionChange(state) {
    console.log(state.selectedNodeIds, state.primaryNodeId);
    // Update the host inspector, toolbar, or accessible live announcement here.
  },
  onError(error) {
    console.error("SVG selection failed", error);
  },
});

selection.selectNode("build");
selection.selectNodes(["build", "test"], "add");
selection.navigate("right", true); // Extend selection and move keyboard focus.
selection.setTool("lasso");
```

## Input and selection semantics

Click selects the topmost node box; clicking empty space clears a replacement
selection. Shift adds, Control/Command toggles, and Alt removes. These modifiers
also apply to dragging. Every drag preview starts from the selection captured at
pointer-down, so moving repeatedly across a node does not toggle it repeatedly.
Pointer-up includes the final position and commits. Escape, pointer cancellation,
lost capture, window blur, a second touch, tool changes, source replacement, and
changed geometry cancel the gesture and restore the original selection.

Arrow keys move in screen-space directions. Shift+Arrow extends the selected set.
Home/End select the first/last usable node in DOM order. Control/Command+A selects
all usable nodes. Escape cancels an active gesture; otherwise it clears selection.
Enter/Space select the focused node rather than activating its authored callback
or URL. Embedded HTML form controls and editable content retain their own input.

This is a **selection mode**, not an additional activation handler. While bound,
the controller captures diagram clicks and selection keys before descendant
handlers. Do not attach a competing gesture/activation controller to the same SVG
or an earlier ancestor capture listener. After `dispose()`, normal host activation
and the original focus/touch-action attributes are restored; later host edits to
those attributes are not overwritten.

## Identity, coordinates, and region geometry

Nodes are identified by `.fm-node[data-id]`. The authored `data-id` remains stable
when DOM IDs are scoped or nodes are reordered. Duplicate authored IDs are rejected
rather than choosing an arbitrary representation. Node-free specialized charts
produce an empty selection surface; this API does not invent chart-element IDs.

`selectAt(clientX, clientY, mode)` takes browser **client coordinates in CSS pixels**,
exactly like a PointerEvent. Do not multiply by devicePixelRatio or apply the SVG
viewBox transform yourself. The controller uses current `getBBox()` and
`getScreenCTM()` geometry, including nested SVG and two-dimensional CSS transforms.
Picking uses label-inclusive node boxes, not shape contours, clip paths or masks.
Hidden, empty, detached and singular/non-finite node geometry is skipped.

Box selection is axis-aligned in screen space. Lasso uses even-odd fill, including
its boundary, and is limited to 4,096 samples. Oversized input cancels and reports an
error instead of leaving a partial selection. Degenerate regions restore the
baseline. `intersects` includes boundary contact and polygon-edge crossings;
`contains` requires the whole node box, not just its center or corners. Containment
conservatively rejects lasso boundary traces inside a box (including retraced
seams), preventing self-crossing inner loops from falsely containing a node.

Selection outlines are a separate, pointer-transparent SVG overlay. They do not
change a node's bounding box, style, children or authored handlers. Customize their
color with `--fm-selection-color` on the SVG or an ancestor.

## Lifecycle and API

The controller exposes immutable `selectionState()` snapshots with
`selectedNodeIds`, `primaryNodeId`, `activeGesture` (a decimal string or null), and
`frameAvailable`. IDs are returned in current DOM order; primary focus is retained
by authored ID. The callback fires on changes, including gesture begin/end, not on
every redundant pointer sample. Callback failures are reported without breaking
selection input.

`selectNode(id, mode)`, `selectNodes(ids, mode)`, `selectAt(x, y, mode)`, `navigate`,
and `clear()` return whether selection changed. Modes are `replace` (default),
`add`, `toggle`, and `remove`. Invalid modes/directions throw. Unknown IDs reject
the entire command instead of partially applying it. Programmatic selection
commands cancel any pending pointer gesture before applying their own selection.
`cancel()` returns whether a gesture was cancelled. `setTool()` cancels an active
gesture when changing tools.

For source replacement that keeps the same SVG root:

```js
selection.beforeRender();
try {
  // Replace this root's authored contents using the host's existing safe render path.
  await renderIntoExistingSvgRoot(svg, nextSource);
} finally {
  selection.refresh();
}
```

`beforeRender()` rolls back active input and suspends selection without discarding
stable IDs. `refresh()` reconciles IDs with the new geometry; removed nodes are
forgotten. DOM geometry/source changes inside the SVG, resize, and scrolling also
schedule reconciliation. Call `redraw()` after changing an ancestor's transform
or visibility. A root replacement requires disposing the old controller and
binding the new root; `refresh()` does not silently retarget a detached root.

```js
const retainedIds = selection.selectionState().selectedNodeIds;
selection.dispose();
// Replace the root through the host renderer, then bind its replacement.
const next = bindSvgSelection({ svg: replacementSvg });
const availableIds = new Set([...replacementSvg.querySelectorAll(".fm-node[data-id]")]
  .map(node => node.getAttribute("data-id")));
next.selectNodes(retainedIds.filter(id => availableIds.has(id)));
```

Call `dispose()` when unmounting. It is idempotent, releases capture, cancels pending
animation frames, removes observers/listeners/overlays and restores owned focus
and touch-action changes. Only one controller may own a given SVG at a time.

## Validation

Run `node --check web/mermaid.mjs` and `python scripts/svg_selection_test.py`.
The latter requires Python Playwright and Chromium (`CHROMIUM` can name its binary).
It loads the production entry in a real Chromium document with an inert worker
factory to stay offline. Selection geometry, pointer capture, focus, browser input
and DOM observers are real; the suite does not validate a rebuilt WASM package,
mermaid-js equivalence, deployment, or other browser engines.
