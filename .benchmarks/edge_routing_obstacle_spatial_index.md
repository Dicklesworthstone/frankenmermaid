# Perf win: sparse obstacle grid for edge routing

**Crate:** `fm-layout` - **Date:** 2026-06-25 - **Agent:** TanSparrow
**Verdict:** kept - sparse flowchart layout wins, wide layered graphs stay on the old scan path.

## What changed

`build_edge_paths_with_orientation` still builds node obstacle bounds once and still parks
an edge's own endpoints at the far-away sentinel before routing. For sparse/tree-like
flowcharts with at least 64 node obstacles and edge count at most 1.5x node count, it now
also builds an `ObstacleSpatialIndex` grid once.

Each nudge query expands the axis-aligned segment by the existing 8px margin, gathers all
obstacle indices from overlapping grid cells, de-duplicates them with a generation stamp,
sorts indices ascending, and then calls the same CGA nudge test on those candidates. Dense
wide layered graphs are deliberately not indexed because the extra candidate/sort work did
not beat the already-cheap AABB scan there.

## Correctness

The grid query is a conservative superset for the existing AABB overlap test: `obstacle`
overlaps `segment + margin` exactly covers the old `obstacle + margin` overlaps `segment`
predicate. Sorting candidates preserves the old "first intersecting obstacle in slice
order" behavior. The nudge function reads the current `obstacle_bounds[idx]`, so endpoints
parked after index construction still AABB-reject as the far-away sentinel.

Focused tests cover both ordering and parked-endpoint behavior. Full `fm-layout` validation:
428 unit tests passed, plus doc tests.

## Measurement

Baseline worktree: `/data/projects/.worktrees/frankenmermaid-cod-b-next-main-28b271d`
at `28b271d`. Candidate: main checkout with sparse obstacle grid. Both runs used
`CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a`, package
`frankenmermaid-cli`, bench `pipeline_bench`, and separate metadata tags.

| bench | baseline | candidate | delta |
|---|---:|---:|---:|
| `layout/flowchart/medium_100` | 247.84 us | 234.58 us | -5.35% |
| `layout/flowchart/large_500` | 736.49 us | 558.35 us | -24.19% |
| `full_pipeline/parse_layout_svg/large_500` | 6.9929 ms | 6.6015 ms | -5.60% directionally, Criterion no-change |
| `full_pipeline_wide/8x16` | 2.4635 ms | 2.3002 ms | -6.63%, no-change |
| `full_pipeline_wide/12x24` | 5.6366 ms | 5.5850 ms | -0.92%, no-change |
| `full_pipeline_wide/16x32` | 11.011 ms | 10.023 ms | -8.97% |

Live Mermaid.js comparator: Mermaid `11.12.0` ESM bundle via Node `v24.14.0` and
`/snap/bin/chromium --headless=new` over Chrome DevTools Protocol, 3 warmups and
20 timed render-to-SVG iterations. Mean denominators were `315.14 ms`, `981.73 ms`,
and `2879.185 ms` for wide `8x16`, `12x24`, and `16x32`.

Candidate frankenmermaid/Mermaid.js ratios:

| wide case | frankenmermaid | mermaid.js | ratio | mermaid.js slower |
|---|---:|---:|---:|---:|
| `8x16` | 2.3002 ms | 315.14 ms | 0.007299x | 137.01x |
| `12x24` | 5.5850 ms | 981.73 ms | 0.005689x | 175.78x |
| `16x32` | 10.023 ms | 2879.185 ms | 0.003481x | 287.26x |

## Commands

```bash
RUSTFLAGS='-C metadata=codaspidxbaseline' CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- layout/flowchart --warm-up-time 1 --measurement-time 2
RUSTFLAGS='-C metadata=codaspidxcandidate2' CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- layout/flowchart --warm-up-time 1 --measurement-time 2
RUSTFLAGS='-C metadata=codaspidxbaseline' CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide --warm-up-time 1 --measurement-time 3
RUSTFLAGS='-C metadata=codaspidxcandidate2' CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide --warm-up-time 1 --measurement-time 3
```
