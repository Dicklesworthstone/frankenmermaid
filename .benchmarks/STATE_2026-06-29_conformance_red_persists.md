# State snapshot — golden_svg RED persists 6+ turns; render WIP stale; perf lanes at floor

**Date:** 2026-06-29 — **Agent:** BlackThrush — **HEAD:** 3a9e822

## Conformance health: `golden_svg_test` has been RED on main for 6+ turns

`svg_golden_snapshots_are_stable` FAILs on `dense_flowchart_stress` (FNV mismatch), reproduced on a
clean worktree at HEAD — first flagged at 90446ae, still unaddressed. `BLESS=1` regenerates **37**
goldens: 36 are the intentional byte-reduction drift (font-family→root, dropped `data-fm-node-id`,
gated CSS — all landed perf commits) and SHOULD be re-blessed; **1 is anomalous** — the first/common
labelled edge renders as a bare `<path>` with no `<g>`/`<title>` (the edge loop renders all edges
uniformly via `render_edge`, so this is inside `render_edge`/serialization, not the loop). A render
owner needs to: re-bless the 36 intentional ones AND confirm-or-fix the first-edge `<title>` drop
(intentional → re-bless; bug → restore the per-edge title). The RED guard is masking this every CI run.

## Render WIP appears abandoned

`crates/fm-render-svg/src/lib.rs` has carried an uncommitted +144-line `build_common_node_fragment`
(full-node direct-byte, the ~0 21203f3 approach) for ~10 turns; mtime is **~8h stale**. It is neither
landed nor reverted. Whoever owns it should land (if measured) or `git checkout` it — it has blocked
clean render A/Bs and contaminated working-tree standings all session.

## Perf frontier (my uncontested lanes — parse + fm-layout)

All at the constraint-bound byte-identical floor, confirmed with fresh profiles this session:
- **parse**: doc_parse + lower at the IR-ownership-alloc/interning floor; detect already guarded. 2
  wins landed (parse_label ~4-9% bbaf088, edge right-contains ~5-8% 6a8d164).
- **layout (Tree path)**: edge_paths CSR-indexed; tree+spans' only lever (Vec<Vec>→CSR) is byte-identical
  but ~0 (hot-recycled) + unmeasurable on the small bench; node_sizes LUT'd. cyclic/Sugiyama is a dead
  end (FxHashMap ~0, ranks too small; not a gap-vs-mermaid).
- **render** = the peer's `lib.rs` (off-limits) + byte-writing-floored micro-levers.

## The only ≥3% headroom left: render output reduction (~7-8% pipeline), both owner-decisions

1. **a11y ~12%** (`<title>`/role/aria/tabindex, unconditional — NOT the `accessible` flag) — cod-b's
   Mermaid comparator (strong prior `<title>Node: X, shape</title>` is non-Mermaid).
2. **`data-*` ~7%** — functionally unconsumed (FrankenTUI/demo/in-repo verified), gated only on an
   owner API decision; no comparator needed → the simpler win.

Methodology: bench on the largest-magnitude bench; null-A/B gate (±8% false-sig under fleet load;
small benches can't resolve sub-7% even at moderate load).
