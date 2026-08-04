# Perf win: AABB pre-filter for edge-routing obstacle checks (layout ~5× faster)

**Crate:** `fm-layout` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — layout up to ~5× faster on large flowcharts, output byte-identical.

## What changed

A probe (`build_edge_paths` → empty) showed **edge routing is ~94% of layout** for
`large_500` (the per-edge work blew layout up ~16×). The cause: for every edge,
`build_edge_paths` collects all node bounds as obstacles and `route_edge_points_with_obstacles`
calls `find_horizontal/vertical_segment_nudge`, which runs a **heavy CGA
`intersect_segment` test on every obstacle** (returning on the first hit, in node order)
— O(edges × nodes) conformal-geometric-algebra ops. For a clean layout most edges hit
no obstacle, so ~all of those CGA ops are wasted.

Added a **cheap conservative AABB rejection** in both nudge functions: compute the
segment's axis-aligned bounding box once, and `continue` past any obstacle whose
margin-expanded box doesn't overlap it. The segment lies within its AABB (and `start`
within it), so a non-overlapping AABB **guarantees** the CGA test would report no
intersection/containment — skipping it cannot change the result. The expensive CGA now
runs only on the handful of obstacles that could actually be hit.

## Correctness — output-identical

The CGA test remains the authoritative check; the AABB filter is a strict superset
(inclusive comparisons), so the first CGA-intersecting obstacle in node order is
unchanged. All **426 fm-layout tests pass**, including every `cga_routing` collision
test (`segment_hits_obstacle`, `segment_misses_obstacle`, `segment_misses_with_margin`,
`*_inside_obstacle_*`) and the layout snapshot tests → routing output is byte-identical.
Conformance GREEN; clippy clean.

## Measurement — same-worker A/B (stash-swap, measurement-time 3)

| bench | AABB faster by | p |
|-------|----------------|---|
| `layout/flowchart/large_500`  | **+398%** (orig ~5× slower; layout −80%) | <0.05 |
| `layout/flowchart/medium_100` | +28% | <0.05 |
| `layout/flowchart/small_10`   | +7% | <0.05 |
| `full_pipeline/large_500`     | **+62%** (full pipeline −38%) | <0.05 |
| `full_pipeline/medium_100`    | +27% | <0.05 |
| `full_pipeline/cyclic_50`     | +12% | <0.05 |

The single biggest lever of the session: edge routing was the dominant layout cost
(an accidental O(edges × nodes) CGA loop), and a constant-factor AABB guard removes
the wasted work without touching output. A spatial index could prune the remaining
O(edges × nodes) cheap AABB checks to O(edges × log nodes), but the CGA was the cost,
not the cheap comparisons.
