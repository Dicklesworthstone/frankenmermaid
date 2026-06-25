# NEGATIVE result: direct edge offset path build

**Crate:** `fm-render-svg`  
**Date:** 2026-06-25  
**Agent:** TanSparrow  
**Verdict:** rejected - direct `LayoutEdgePath` offset serialization regressed small and
medium render benches, while large was statistically unchanged.

## Hypothesis

`render_edge` allocated one temporary `Vec<(f32, f32)>` per edge to apply
`offset_x`/`offset_y` before calling `smooth_edge_path`. Eliminating that vector
looked like a clean buffer-reuse/allocation-removal lever in the SVG render hot path:
read `LayoutEdgePath.points` directly, apply offsets into locals, and emit the same
Catmull-Rom path string.

## Change Tested

Added a private direct serializer that:

- read `edge_path.points` without collecting into a temporary vector,
- computed the same offset-adjusted coordinates before the Catmull-Rom control
  point math,
- kept the original `smooth_edge_path` helper for other callers,
- replaced only the `render_edge` call site.

The code change was reverted after measurement.

## Measurement

Baseline worktree:

`/data/projects/.worktrees/frankenmermaid-cod-a-edge-offset-baseline-d568ce6`

Baseline and candidate command:

```bash
CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- render_svg/flowchart --warm-up-time 1 --measurement-time 3
```

| `render_svg/flowchart` | baseline mean | candidate mean | candidate delta |
|---|---:|---:|---:|
| `small_10` | `145.98 us` | `151.50 us` | `+3.78%` slower |
| `medium_100` | `883.46 us` | `1.1335 ms` | `+28.30%` slower |
| `large_500` | `4.4858 ms` | `4.5743 ms` | `+1.97%` slower/no-change |

Criterion reported a significant regression for `small_10` and `medium_100`; the
`large_500` interval overlapped and was a no-change result.

## Mermaid.js Ratio

Current main, after reverting the rejected code, was BOLD-VERIFY measured with:

```bash
CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide --warm-up-time 1 --measurement-time 2
```

Fresh Mermaid.js comparator:

- Mermaid `11.12.0` ESM bundle from jsDelivr.
- `/snap/bin/chromium --headless=new` driven through Chrome DevTools Protocol.
- `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations per case.

| case | current frankenmermaid mean | Mermaid.js mean | fm / Mermaid.js | Mermaid.js slower |
|---|---:|---:|---:|---:|
| `8x16` | `2.4348 ms` | `344.0700 ms` | `0.007076x` | `141.31x` |
| `12x24` | `6.2641 ms` | `1030.5500 ms` | `0.006078x` | `164.52x` |
| `16x32` | `11.394 ms` | `2711.9150 ms` | `0.004201x` | `238.01x` |

## Decision

Reject. This was an apparently clean allocation-removal lever, but the measured
effect was a regression. The likely reason is that the old temporary vector gives
the path builder a compact tuple slice and simpler indexed access, while the direct
path duplicates offset arithmetic and bloats the local loop enough to lose.

Do not retry this exact direct-offset serializer without a CPU profile showing that
the per-edge point vector allocation is again a top-5 renderer cost.
