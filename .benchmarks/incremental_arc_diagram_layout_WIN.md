# WIN: `Arc<DiagramLayout>` on `TracedLayout.layout` — incremental fast path −14..18% under mimalloc (bd-12e / bd-9rq7, 2026-07-24)

Agent: CopperCliff (cc), Opus 4.8, ARCHITECTURAL lane. Base: `b96ae4f9`.
Executes the "confirmed-feasible plan" from `incremental_bench_mimalloc_fidelity_FINDING.md`.

## What changed

`TracedLayout.layout: DiagramLayout` → `Arc<DiagramLayout>`.

The incremental engine stores every layout it computes into `self.cached` **and** returns it:

```rust
self.cached = Some(CachedTracedLayout { key, ir: snapshot, traced: traced.clone() });
return traced;
```

With the layout behind `Arc`, that `traced.clone()` (lib.rs ~3084 fast/selective store, ~3131
full-recompute store) is a **refcount bump instead of a full nodes+edges+clusters memcpy**. Same
for the memo-hit return (`cached.traced.clone()`, ~3014). The size-stable fast path therefore stops
paying the geometry deep-clone **twice** (once to build the return value, once to store it) — it now
pays it once (building the `Arc`) and the store is free.

This is the real memcpy the mimalloc profile flagged (`Vec<LayoutEdgePath>` 8.7% + `LayoutEdgePath`
7.0% + raw `memcpy`/`memmove` ~12%), NOT the alloc-wash the libc bench had suggested.

### Mechanics (compiler-guided, 440 tests green)
- 18 `TracedLayout { layout: DiagramLayout { … } }` construction sites → `Arc::new(DiagramLayout { … })`.
- ~12 non-traced `layout_diagram_*` wrappers returning owned `DiagramLayout` (`…_traced(ir).layout`)
  → `Arc::unwrap_or_clone(…)`. Fresh, unshared ⇒ refcount 1 ⇒ a move, never a clone.
- 9 post-construction mutation sites (`traced.layout.extensions.{axis_ticks,bands} = …` in
  timeline/gantt/grid/kanban + `stats.phase_iterations` in the dispatch path + 3 cache-poison tests)
  → `Arc::make_mut(&mut traced.layout)`. **The feasibility audit missed these** ("0 post-construction
  mutations" was wrong). Each Arc is freshly built (refcount 1) so `make_mut` is a clone-free COW
  borrow; disjoint-field reads on the RHS still work through the single `&mut`.
- Renderers/CLI/WASM read `&DiagramLayout` unchanged via `Arc` `Deref` coercion — zero edits outside
  fm-layout.

### Correctness note the plan got wrong
The plan's Step 3 ("drop the per-node span-refresh loop, pure `Arc::clone` the cached layout") is
**unsound**: the fast path returns a layout that differs from the cached one in `dirty_regions`,
`stats.phase_iterations`, and per-node `span` (spans shift on any byte-length label edit when the
caller re-parses; `apply_span_metadata` emits them into SVG under `include_source_spans`). So the
fast path still builds a fresh `DiagramLayout` — the win is purely eliminating the **duplicate store
clone**, which is byte-identically correct by construction (stored == returned).

## Numbers — `single_node_label_edit`, criterion `--baseline`, mimalloc bench, load ~5-8

| size | base incr | cand incr | Δ incremental (CI, p) | full_recompute control |
|---|---|---|---|---|
| 100  | 17.97µs | 16.47µs | **−14.1%** [−16.8,−11.1] p=0.00 | no change (p=0.22) |
| 200  | 35.73µs | 29.33µs | **−18.0%** [−20.0,−16.0] p=0.00 | no change (p=0.34) |
| 500  | 94.60µs | 75.54µs | **−16.4%** [−18.6,−14.4] p=0.00 | no change (p=0.11) |
| 1000 | 185.4µs | 152.3µs | **−18.1%** [−20.3,−15.8] p=0.00 | 183→175µs (−4.3% drift) |

**incremental/1000 flips from a tie (185 vs full 183µs) to a 13% lead (152 vs 175µs)** — the plan's
stated goal. The incremental improvement is consistent and 4× larger than the largest control drift;
`full_recompute` (which has no cache/store, so nothing to elide) is flat at 3/4 sizes, confirming the
win is the store-clone elision, not measurement drift. The full-recompute free fn now pays one extra
`Arc::new` + a clone-free `make_mut`, which mimalloc-washes (as predicted).

## Do-not-retry / follow-ups
- The remaining per-pass IR clone (`MermaidDiagramIr::clone` 12.2%) is the SEPARATE edit-session
  Arc-input lever (`incremental_edit_session_arc_input_NEGATIVE.md`); its retry predicate (IR clone
  top-2) still stands under mimalloc but needs an Arc-native parse→layout pipeline to avoid the
  cancelling `Arc::new` — not done here.
- `derive_layout_edits` 11.3% is inherent O(n) diff compute (not a clone).
