# Perf win: index obstacles for *large dense* graphs, not just sparse ones

**Crate:** `fm-layout` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — wide layered flowcharts route ~25–42% faster in `build_edge_paths`; small
dense and all sparse graphs are byte-identical and unchanged.

## What changed

`build_edge_paths_with_orientation` builds an `ObstacleSpatialIndex` grid whenever the
obstacle count clears `MIN_INDEXED_OBSTACLES` (64) **and** the graph is either sparse
(`edges <= 1.5*nodes`, the original gate) **or** a *large* dense graph
(`obstacles >= DENSE_INDEX_OBSTACLES`, new = 256). The previous gate indexed only sparse
graphs, leaving every wide layered graph on the per-edge linear AABB scan over all node
obstacles.

### Why the density gate was leaving a win on the table

A fresh phase profile of `layout_diagram` on the canonical wide bench (`gen_wide`, which the
auto-selector routes to the **Tree** algorithm) showed `build_edge_paths` is the dominant
layout cost — ≈1.0 ms of ≈1.3 ms total at 16x32 (960 edges, 512 node obstacles) — while every
other phase is tens of µs. With the index disabled, each edge's mid-segment nudge linearly
scans all 512 obstacles (~491k AABB tests). But each axis-aligned segment spans only one rank
gap / one column, i.e. a couple of 128px grid cells, so the index's localized candidate query
collapses that to a handful of tests per edge. Edge *density* (the old gate) is the wrong
proxy: it does not predict segment length, and wide fan-out-2 graphs are dense by edge count
yet local by geometry.

### Why dense indexing is floored at 256 obstacles

For *small* dense graphs the index's build + candidate-sort overhead loses to the already-cheap
scan: at 8x16 (128 obstacles) the unconditional index measured ~+5% slower. The 256 floor keeps
those graphs on the scan (provably the identical no-index code path) while capturing the
crossover — 8x16 (128, scan faster) is excluded, 12x24 (288, index ~25% faster) is included.

## Correctness

Byte-identical. The grid query is the same conservative superset of the AABB scan already
validated for the sparse path (`obstacle` overlaps `segment + margin` ⇔ the old
`obstacle + margin` overlaps `segment`); candidates are sorted ascending, preserving the old
"first intersecting obstacle in slice order" tie-break; the nudge reads the live
`obstacle_bounds[idx]`, so endpoints parked at the far-away sentinel still AABB-reject. This
change only *widens when* that identical path runs. Validation: `fm-layout` 428 unit tests +
doc tests pass; `frankentui_conformance_test` (whole-corpus AST/layout identity) passes; clippy
clean.

## Measurement

Controlled, single-worker A/B via a temporary `FM_DENSE_IDX` env toggle (removed before commit)
so baseline (dense indexing off) and candidate (on) run from the **same binary on the same
rch worker** — zero compile or hardware variance. Both run inside one `rch exec`. Per-crate
target dir `/data/projects/.rch-targets/mermaid-cc`, package `frankenmermaid-cli`, bench
`pipeline_bench`, criterion `--warm-up-time 1 --measurement-time 4` (100 samples each).

Pure-layout group `layout_wide/layered` (absolute criterion medians; both-order run to cancel
the ~9% second-phase thermal penalty):

| bench | baseline (idx off) | candidate (idx on) | Δ (median) | base-first change |
|---|---:|---:|---:|---:|
| `layout_wide/layered/8x16`  | 106.52 µs | 102.36 µs | ~neutral | −0.33% (p=0.84, NS) |
| `layout_wide/layered/12x24` | 386.92 µs | 290.67 µs | **−24.9%** | −30.3% (p=0.00) |
| `layout_wide/layered/16x32` | 898.43 µs | 517.18 µs | **−42.4%** | −38.9% (p=0.00) |

8x16 is below the 256 floor → identical no-index path; its sign flips between runs (noise about
zero, NS in the fair base-first order). The base-first column is conservative (candidate runs
2nd = penalized), so the true wins are if anything larger.

Full-pipeline group `full_pipeline_wide/parse_layout_svg` (candidate runs 2nd = penalized):

| bench | baseline | candidate | change |
|---|---:|---:|---:|
| `…/8x16`  | 877.66 µs | 901.49 µs | +2.0% (p=0.22, NS) |
| `…/12x24` | 2.2989 ms | 2.0688 ms | **−10.0%** (p=0.00) |
| `…/16x32` | 4.8749 ms | 4.3216 ms | **−11.4%** (p=0.00) |

## Mermaid.js head-to-head

Pinned comparator: Mermaid `11.12.0` ESM bundle, full render-to-SVG, wide denominators
`315.14 ms` (8x16), `981.73 ms` (12x24), `2879.185 ms` (16x32) — the same denominators recorded
in `edge_routing_obstacle_spatial_index.md`.

| wide case | frankenmermaid (before → after) | mermaid.js | ratio after | mermaid.js slower |
|---|---:|---:|---:|---:|
| `12x24` | 2.2989 ms → 2.0688 ms | 981.73 ms | 0.002107x | 427x → **475x** |
| `16x32` | 4.8749 ms → 4.3216 ms | 2879.185 ms | 0.001501x | 591x → **666x** |

## Commands

```bash
# single-worker controlled A/B (FM_DENSE_IDX gate is temporary; final code is compile-time)
CARGO_TARGET_DIR=/data/projects/.rch-targets/mermaid-cc rch exec -- bash -lc '
  FM_DENSE_IDX=0 cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- layout_wide/layered --warm-up-time 1 --measurement-time 4 --save-baseline b0
  FM_DENSE_IDX=1 cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- layout_wide/layered --warm-up-time 1 --measurement-time 4 --baseline b0'
```

## Supersedes

Revises the design note in `edge_routing_obstacle_spatial_index.md` ("Dense wide layered graphs
are deliberately not indexed because the extra candidate/sort work did not beat the already-cheap
AABB scan there"). That holds only for *small* dense graphs; above the 256-obstacle floor the
localized query wins decisively, and `build_edge_paths` had since become the layout frontier.
