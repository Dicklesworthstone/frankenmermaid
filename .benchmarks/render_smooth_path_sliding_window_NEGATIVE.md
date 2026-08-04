# REJECT: sliding-window in build_smooth_path_by_into (bd-1buv, 2026-07-24)

Agent: CopperCliff (cc), Opus 4.8. Base: `d770a5e9`. Lane: bd-1buv measured-frontier (dense/cyclic edge-heavy).

## Lever

`build_smooth_path_by_into` (2.89% self of the dense-DAG full pipeline; a "new-vs-linear-flowchart"
frame that cod's `flowchart_large_500` floor did not cover) calls its `point_at` closure FOUR times per
segment (p_prev, p_cur, p_next, p_next2). `point_at` is `FnMut`, so the compiler cannot prove the
redundant cross-iteration re-fetches equal and elide them. Candidate slid a 4-point window forward,
fetching only the new lead point (~4x fewer `point_at` lookups + offset adds), byte-identically.

## Correctness — held

256 fm-render-svg tests pass incl. the `build_smooth_path` into-variant byte-identity assertion. Full
SVG dumps byte-identical across densedag 200/60, flow 500 (SHA `408ecdcc…`, the pinned corpus hash),
and subg 100.

## Measured — WASH / slight regression → REJECT

profharness `render` phase, densedag 200, `taskset -c 3`, interleaved base-vs-cand, min ns over 20k
iters × 5 runs (load ~2.3):

- base mins: 192394 188497 192415 193436 185712 → **overall min 185,712**
- cand mins: 191533 192745 192024 202384 191654 → **overall min 191,533**

The candidate's overall min is 3% ABOVE base's (a real win sits below). No direction-consistent win;
does not meet the ≥3% null-adjusted bar. Reverted.

## Why it washed

Expected effect ~1% of render (densedag edges are short — few spline points, so the 4x redundancy is
small in absolute terms) — below the ~4% measurement noise. Either LLVM already handles the redundant
fetches adequately after inlining the pure closure, or the saved lookups are outweighed by the window's
shift bookkeeping (branch + 8 float moves/iter). 

## Do not retry

Do not re-attempt the sliding window unless a profile shows `build_smooth_path_by_into` self ≥8% on a
workload with genuinely LONG edges (many spline points per edge — obstacle-routed graphs), AND a
same-binary A/B shows ≥3% direction-consistent render win with CV<5%.
