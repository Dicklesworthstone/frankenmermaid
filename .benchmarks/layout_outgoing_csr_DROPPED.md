# Dropped: layout `build_tree_layout_structure` outgoing-CSR — byte-identical but ~0-impact + unmeasurable

**Crate:** `fm-layout` — **Date:** 2026-06-29 — **Agent:** BlackThrush
**Verdict:** dropped (not landed). Byte-identical, but ~0.4% pipeline and not measurable on this fleet.

## What it was

Replace the `outgoing` adjacency `Vec<Vec<usize>>` in `build_tree_layout_structure` with a flat CSR
(out_offsets / out_flat + per-node deduped out_len), avoiding ~512 inner Vec allocations. Validated
**byte-identical** in a clean worktree (golden_layout + frankentui_conformance GREEN; `indegree` only
used for the `== 0` root test; `cmp_by_id` total order ⇒ identical sorted+deduped buckets).

## Why dropped

1. **~0 impact.** The inner Vecs are hot-free-list recycled across bench iterations; the CSR saves
   ~512 alloc/free fast-paths ≈ ~10 µs on a ~530 µs layout stage ≈ **~0.4% of the pipeline** — below
   the worth-landing bar even if it measured cleanly.
2. **Unmeasurable on this fleet.** `wide_stages/layout` is ~100-530 µs; a null A/B (identical code)
   reads ±5-14% false-significance even when the 1-min load momentarily dips to ~4.8 — because the
   load **fluctuates faster than a bench run completes** (4.8 → 14 mid-run). No quiet window holds
   long enough. (Refines d121ee1: it's not just steady load — intra-run drift kills the small bench.)

Per REVERT-~0-gain, a byte-identical change with ~0.4%-pipeline upside that cannot even be measured is
not worth landing or the churn. Closed.

## Context this turn

- The peer **landed** the render node direct-byte fast path (66ff940), resolving the +144-line
  uncommitted WIP that sat in `lib.rs` all session — the working tree is clean again.
- The highs-sys/cmake worker outage is back and **widespread** (3 fresh dirs all failed), blocking all
  fm-cli benches/tests (render, layout, full_pipeline, conformance) this turn; only fm-parser builds.
- Open items unchanged: golden_svg RED (render owner) and the render output reduction — a11y ~12%
  (cod-b comparator) + data-* ~7% (owner API call). These hold all remaining ≥3% headroom.
