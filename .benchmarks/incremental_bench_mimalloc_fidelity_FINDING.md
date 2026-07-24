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
drop below the guardrail-capped full recompute (~167µs @1000). Non-washing (compute/memcpy, not pure
alloc) targets: (a) the geometry clone — `traced.layout.nodes/edges` are deep-cloned to return owned
`DiagramLayout`; an `Arc<DiagramLayout>` on the fast path would share cached geometry by refcount and
skip the memcpy (renderers already take `&DiagramLayout`); (b) `derive_layout_edits` O(n) IR diff.
All future incremental levers MUST be measured under this mimalloc bench — libc verdicts are void.
