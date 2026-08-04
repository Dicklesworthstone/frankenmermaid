# FIX (wall-flat, shipped for boundedness): incremental trace snapshot retention (2026-07-22)

Agent: cc (CopperCliff)
Base: `054efa78`

## The bug

`try_incremental_subgraph_relayout` clones the cached trace, appends one
"incremental_subgraph_relayout" snapshot via `push_snapshot` (no cap), and the result is stored
back as the new cached trace — so `trace.snapshots` grows by one PER EDIT, FOREVER. Every
subsequent pass clones the whole grown Vec twice (trace clone + memo-store traced clone), and
long-lived engines (fm-wasm holds ONE per app session) accumulate unbounded state: memory grows
per keystroke and every returned TracedLayout drags the full edit history.

## The fix

Before appending, retain only non-"incremental_subgraph_relayout" snapshots: the trace now always
carries the original full-layout phase snapshots plus exactly the latest incremental one.
Deterministic; 439 fm-layout tests green.

## Measured (C/O/O/C interleaved, /1000 rows)

Wall-FLAT on the bench (single +0.4%, five +1.1% — inside noise; nulls flat): even at criterion's
~20k iterations the accumulated Vec clone (~48 B/entry, Copy memcpy) stayed small relative to the
pass cost. NO perf claim is attached to this commit — it ships on boundedness/robustness grounds:
O(1) trace size per engine instead of O(edit-count).

## Related REJECT #3 (same investigation): size-stable region memoization fast path

Implemented and measured WASH (single −1.4%, five +2.3%; separate A/B, orig8 pair) BEFORE this fix;
the wash was NOT explained by trace churn (this fix measured flat). CRITICALLY: firing of the fast
path was never directly verified — the wash may mean the gate never fired (a size-bits mismatch or
an unexpected precondition failure), not that the savings are absent. The candidate implementation
is preserved at the session scratchpad (`fastpath_lib.rs.bak`) and its full design rationale
(size-equality ⇒ geometry-identity theorem, config-key gate, edge-span coverage via
all_node_changes, LayoutEdgePath carries no label text) is sound. RETRY PREDICATE: instrument the
fast-path branch (counter or trace field), verify it fires on iterations 2+ of
single_node_label_edit (where "Edited vN"→"Edited vN+1" should be size-stable), THEN re-A/B. If it
fires and still washes, profile what remains; if it does not fire, debug the size-bits comparison
(estimate_dimensions may not be bit-stable across digit swaps as assumed).

## Vein status: 3 consecutive REJECTs → SWITCH (per mission protocol)

REJECTs: precomputed-edits passthrough (wash), member-scoped node sizing (wash), size-stable memo
fast path (wash, firing unverified). The measured frontier for the incremental rows now:
`build_edge_paths` ~12.7%, region local layout ~9.3%, snapshot clone ~4.9%, allocator ~21%.
Next veins (alien graveyard): (a) verify-and-fix the size-stable fast path firing (cheapest,
theorem already written); (b) arena/pool for layout temporaries (attacks the 21% allocator block;
no-unsafe constraint); (c) bench-harness mimalloc alignment (peer coordination pending in the
bd-12e thread); (d) edit-session API (true Adapton interface — removes the last per-pass IR clone
and enables real O(dirty) invalidation).
