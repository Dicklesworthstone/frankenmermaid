# Perf win: build edge-routing obstacle set once (layout ~2.1× faster on large graphs)

**Crate:** `fm-layout` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — layout ~2.1× faster on large_500, output byte-identical.

## What changed

After the AABB pre-filter (85b54d0) made the obstacle *check* cheap, a probe (per-edge
obstacle `Vec` → empty) showed the per-edge obstacle **build** is now ~69% of layout for
large_500 (+220% with it). `build_edge_paths` rebuilt a `Vec<LayoutRect>` of "all nodes
except this edge's two endpoints" for **every** edge — O(edges × nodes) allocations + copies.

Now the obstacle bounds are built **once** (`nodes.iter().map(|n| n.bounds).collect()`).
Each edge temporarily parks its own two endpoints at a far-away sentinel
(`{ x: 1e30, y: 1e30, w: 0, h: 0 }`) before routing and restores them after — the
router's AABB check always rejects the sentinels, so they're excluded exactly as before,
but at O(1) per edge instead of O(nodes). No router/test changes.

## Correctness — output-identical

The sentinel's margin-expanded box can't overlap any realistic segment bounding box, so
the AABB reject drops it; the remaining obstacles (all nodes except the two endpoints, in
node order) are exactly the previous per-edge set. All **426 fm-layout tests pass**
(routing + layout snapshots unchanged). Conformance GREEN; clippy clean.

## Measurement — same-worker A/B (stash-swap, measurement-time 3)

| bench | built-once faster by | p |
|-------|----------------------|---|
| `layout/flowchart/large_500`  | **+109%** (~2.1×; layout −52%) | <0.05 |
| `layout/flowchart/medium_100` | +6.4% | <0.05 |
| `full_pipeline/large_500`     | **+13.2%** | <0.05 |
| `layout/flowchart/small_10`   | ±noise | 0.07 (n.s.) |

Small graphs are within noise (the build is negligible there, and building once does
*fewer* copies than the per-edge filter anyway). Combined with the AABB pre-filter,
edge routing — which was ~94% of layout — is now a small fraction; layout/large_500 is
roughly **10× faster than before both levers**.
