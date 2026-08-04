# FINDING + FIX: incremental_layout bench now measures under mimalloc (bd-12e, 2026-07-24)

Agent: CopperCliff (cc). Base: `40e07b5`.

## The fix

`crates/fm-layout/benches/incremental_layout.rs` now installs `#[global_allocator] mimalloc::MiMalloc`
(dev-dep added to `fm-layout/Cargo.toml`), matching `fm-cli` / `pipeline_bench` and the native
pipeline. Previously this alloc-heavy bench ran under the **libc** allocator, which overstates
malloc/free ~2.4× and inflated the per-pass IR/geometry clones — the exact substrate trap in
`project_ab_substrate_rules_v2` ("harness needs `#[global_allocator] mimalloc` else libc OVERSTATES
alloc wins ~2.4×").

## What it revealed (the numbers change a LOT)

`single_node_label_edit`, incremental vs full_recompute, per size:

| size | LIBC incr | LIBC full | **mimalloc incr** | **mimalloc full** | mimalloc verdict |
|---|---|---|---|---|---|
| 100  | — | — | **18.3µs** | **78.3µs** | incremental **4.3× faster** |
| 500  | — | — | **86µs** | **85µs** | tie |
| 1000 | ~240µs | ~185µs | **169µs** | **167µs** | tie |

Two consequences:

1. **The incremental engine's true value profile is size-dependent.** For SMALL graphs (≤~100 nodes)
   where a full recompute runs full Sugiyama, incremental serves cached geometry and wins ~4×. For
   LARGE graphs (≥500) the full recompute is guardrail-capped (Sugiyama blocked → cheaper fallback),
   so its cost stays flat while the incremental per-pass overhead (`derive_layout_edits` +
   IR clone + geometry clone) scales O(n) and CATCHES UP — no advantage at /500-/1000.

2. **The prior libc bench overstated the large-graph advantage.** The landed size-stable fast-path
   win (`567c86ef`, "−34.5% incremental/1000") was measured under libc where the full-recompute
   fallback's allocs were expensive; under production mimalloc that gap is only **~2.3%** at /1000.
   The fast path is still correct and still a large win at small sizes — but the −34.5% headline was
   substantially a libc-allocator artifact. (Same mimalloc-wash mechanism as the edit-session REJECT.)

## Direction for bd-12e (re-baselined)

To make incremental beat full recompute on LARGE graphs under production, the per-pass overhead must
drop below the guardrail-capped full recompute (~167µs @1000). All future incremental levers MUST be
measured under this mimalloc bench — libc verdicts are void.

### True fast-path frontier UNDER MIMALLOC (non-LTO profile, incremental/1000)

The libc profile showed `_int_malloc`/`_int_free` at ~37% (overstated). Under mimalloc those vanish;
the real cost is **compute + clone MEMCPY** (genuine work, not alloc):
- `MermaidDiagramIr::clone` **12.2%** self (+ its `Vec<IrNode>` to_vec 7.3%, `String` clone 6.6%) — the
  per-pass IR snapshot memcpy.
- `derive_layout_edits` **11.3%** self — the O(n) IR diff (compute; inherent to diff-based incremental).
- geometry clone: `Vec<LayoutEdgePath>` **8.7%** + `LayoutEdgePath` 7.0% — the fast path deep-clones
  cached edges/nodes to return owned `DiagramLayout`.
- `dependency_topology_equal` 5.8% (compute; do-not-retry topology-recheck, and NOT top-2).
- raw `memcpy`/`memmove` ~12% — the physical copies behind the clones above.

### Two now-justified Arc levers (the reject was libc-based)

The top-2 frames (IR clone, geometry clone) are **memcpy**, which Arc-sharing eliminates — real work
under mimalloc, NOT the alloc-wash the libc bench suggested:
1. **edit-session Arc-input** (IR snapshot) — REJECTED earlier as mimalloc-wash, but that verdict came
   from the LIBC bench; its retry predicate ("IR clone top-2 wall frame") is now SATISFIED under
   mimalloc. Needs re-measurement with the fresh-IR bench under this allocator.
2. **`Arc<DiagramLayout>`** on `TracedLayout.layout` — the fast path shares cached geometry by refcount
   instead of deep-cloning (renderers take `&DiagramLayout`, unchanged via Deref). Wide (~53
   construction sites wrap in `Arc::new`, cheap for freshly-built layouts) but mechanical.
Both are focused architectural efforts (wide edits + a fresh-IR bench), warranting careful A/B under
this mimalloc bench rather than a cycle-tail rush.

### `Arc<DiagramLayout>` — confirmed-feasible plan (2026-07-24)

Feasibility audit (clean): **0** functions take `DiagramLayout` by value; **0** post-construction
mutations of `traced.layout.*` in fm-layout or any consumer; all renderers take `&DiagramLayout`
(unchanged via `Arc` Deref). Steps:
1. `TracedLayout.layout: DiagramLayout` → `Arc<DiagramLayout>`.
2. ~53 `TracedLayout { layout: DiagramLayout {…} }` sites → `layout: Arc::new(DiagramLayout {…})`
   (compiler-guided; mechanical). Fresh-built layouts ⇒ `Arc::new` is one cheap alloc each.
3. Size-stable fast path: return `Arc::clone(&cached.traced.layout)` (+ refreshed trace) instead of
   deep-cloning nodes/edges/clusters — node spans don't change on a label edit, so the refresh loop
   is a no-op and can be dropped. THIS is the win (~15-20% of the fast path = the geometry memcpy).
4. Non-traced `layout_diagram*` (~10 fns returning owned `DiagramLayout`): extract via
   `Arc::try_unwrap(traced.layout).unwrap_or_else(|a| (*a).clone())` — refcount 1 ⇒ moves out, no
   clone; no regression for the full-layout path.
5. BONUS: memo-cache store `traced.clone()` becomes an Arc refcount bump (cheaper), and the top-level
   memo-hit return likewise.
Safety net: 440 fm-layout tests + workspace goldens catch any drift. Measure the fast path A/B under
THIS mimalloc bench (expect incremental/1000 to drop below full_recompute, turning the /500-/1000 tie
into a win). Best done with reliable Agent-Mail coordination (was degraded) and a quiet machine
(load was fluctuating 4→140 this session) — hence scoped here rather than rushed.
