# LEDGERED BLOCKER: incremental size-stable fast-path micro-lever vein EXHAUSTED (bd-12e / bd-9rq7, 2026-07-24)

Agent: CopperCliff (cc), Opus 4.8. Base: `2dc23108` (after the Arc<DiagramLayout> store-clone win `317844e2`).

## Context

After `317844e2` (Arc<DiagramLayout> eliminated the duplicate cache-store clone, incremental −14..18%),
the size-stable fast path (`single_node_label_edit`/`five_node_cluster_edit`, the only paths the benches
exercise — both use same-width label edits) was re-profiled fresh under mimalloc (non-LTO, `/1000`).
Every remaining reducible frame has now been individually run to ground. **No confidently-measurable
micro clone/compute win remains.** Stop condition per lane: ledgered blocker.

## Frontier frames and why each is closed

| frame | self% (perf non-LTO) | verdict |
|---|---|---|
| `MermaidDiagramIr::clone` | 15% | **mimalloc-wall-wash.** perf-record counts the clone's instructions/allocs, which mimalloc services cheaply on the WALL when uncontended. The rigorous `fresh_ir_edit` re-parse A/B (`incremental_edit_session_arc_input_NEGATIVE.md`) already measured owning-vs-borrowing at representative low load = **~0% wash** (the −23% was a load-34 contention artifact). Retry ONLY if the whole parse→layout pipeline is `Arc<IR>`-native so the caller pays no extra `Arc::new` — a fm-parser+fm-core+fm-wasm+fm-cli change that, per the reject, would likely STILL wall-wash. Not a micro-lever. |
| `derive_layout_edits` | 13% | **inherent O(n) diff compute.** Read in full: compares id(String)/label/subgraphs/label-text per node. The label-text compare is required even when label-IDs match (the text at that index is what changed). No redundant work; precomputing a changed-label-index set is ≈ the same memcmp count when labels≈nodes. |
| `dependency_topology_equal` | 8% | **REJECTED this session** (`incremental_topology_recheck_skip_mimalloc_NEGATIVE.md`): an 8% non-LTO frame that washed to /1000 −1.5% (p=0.34) under LTO. ⭐non-LTO self% ≠ LTO wall%. |
| `Vec<LayoutEdgePath>::clone` (edges build-clone) | 5.75% | **blast radius prohibitive.** Would need `DiagramLayout.edges: Vec → Arc<…>` (public render-input struct). Empirically verified: `for x in &l.edges` does NOT compile for `Arc<Vec<T>>` NOR `Arc<[T]>` (for-loops don't deref-coerce the iterable), so this breaks EVERY `for edge_path in &layout.edges` site across fm-render-{svg,term,canvas}, fm-cli, fm-wasm (dozens) plus ~49 construction sites. Not worth ~5.75% (which may itself partly wash — alloc/memcpy). `bd-69bs` remains filed but is re-scoped as NOT a micro-lever. |

## ⭐ Meta-lessons (this investigation)

- ⭐⭐⭐ **perf-record self% is an INSTRUCTION/alloc sample, NOT wall%.** Under uncontended mimalloc, clone/alloc frames that dominate the instruction profile (IR-clone 15%) wall-wash. Confirm every clone-elimination lever on a LOW-load interleaved wall A/B; do NOT trust the perf-record %.
- ⭐⭐ The Arc<DiagramLayout> store-clone WON (−14..18% wall) because it eliminated a **duplicate** full-layout memcpy with **no added alloc** (the Arc already wrapped the built layout). IR-clone-elision WASHES because it trades one clone for one `Arc::new`. Lever shape that wins: eliminate a duplicate/redundant copy adding no alloc; NOT trade-a-clone-for-an-alloc.
- ⭐ `for x in &collection` does not apply deref coercion to the iterable — wrapping a Vec field in Arc/Box breaks all such sites. Blast-radius check any container-type change with this.

## What remains for bd-12e (epic-level, NOT micro-levers)

1. **Incremental structural edits** — node add/remove/edge-change currently fall to full recompute (`all_node_changes==false ⇒ try_incremental returns None`). Handling them incrementally is the epic's core value, a feature not a lever.
2. **Incremental re-render (bd-12e.3)** — `DiagramLayout.dirty_regions` is populated but has ZERO readers; renderers don't skip unchanged regions. A feature.
3. **Arc-native parse→layout pipeline** — the only path to the IR-clone frame, but likely wall-washes per the reject; large multi-crate architectural change.

Micro-optimization of the label-edit fast path is DONE. Further incremental work is feature/architecture, warranting dedicated scoping.
