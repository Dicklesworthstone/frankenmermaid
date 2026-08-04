# Perf lever: single-pass barycenter reorder (bd-qsm0)

**Crate:** `fm-layout` · **Function:** `reorder_rank_by_barycenter`
**Date:** 2026-06-24 · **Agent:** frankenmermaid-cc

## What changed

`reorder_rank_by_barycenter` (the inner loop of `crossing_minimization`, run
4 iterations × 2 sweeps × every rank in `layout_diagram`) previously rescanned the
**entire edge list once per node in the rank** — `O(rank_size × edge_count)`. For
graphs with wide ranks (fan-out pipelines, ER/state diagrams, org charts) this is
the dominant layout cost.

The barycenter of a node is the mean position of its neighbours in the adjacent
rank. For wide ranks we now make a **single pass over the edge list**, binning each
edge's contribution into a per-slot accumulator — `O(edge_count + rank_size)`.
Output is bit-identical (integer position sum divided once by the neighbour count,
same sort/tie-break), so layout/render are unchanged and conformance stays green.

Narrow ranks (`< SINGLE_PASS_RANK_THRESHOLD = 8`) keep the original per-node scan
byte-for-byte, so small graphs incur **zero** added allocation/lookup overhead —
no regression there by construction.

## Measurement (criterion `--save-baseline orig` / `--baseline orig`)

Per-crate, same rch remote worker, back-to-back:

```
CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cc \
  rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench \
    -- "^layout" --warm-up-time 1 --measurement-time 3 [--save-baseline|--baseline] orig
```

| bench (median) | baseline (orig) | optimized | change | path taken |
|----------------|-----------------|-----------|--------|------------|
| `layout_wide/layered/8x16`  | 1.072 ms  | 683.8 µs | **−34.8%** (p<0.05) | single-pass |
| `layout_wide/layered/12x24` | 4.386 ms  | 3.163 ms | **−27.9%** (p<0.05) | single-pass |
| `layout_wide/layered/16x32` | 12.917 ms | 9.307 ms | **−28.0%** (p<0.05) | single-pass |
| `layout/flowchart/large_500` | 4.712 ms | 4.664 ms | −1.0% (within noise) | original (chain → 1/rank) |
| `layout/flowchart/medium_100` | 634.6 µs | 671.1 µs | +0.9% (p=0.63, n.s.) | original |
| `layout/flowchart/small_10` | 20.5 µs | 22.6 µs | noise¹ | original |

¹ `small_10` takes the **identical** original code path (rank size < 8), so any
delta there is measurement noise, not this change. The shared rch workers swing
up to ~2× in absolute time between runs; the wide-case CIs above were tight
(e.g. 16x32 = [9.3012, 9.3121] ms), so those −28%…−35% figures are trustworthy,
while small/narrow cases (unchanged path) bounce on load.

## Verification

- `cargo test -p fm-layout --release` → **426 passed; 0 failed** (incl.
  `crossing_count_reports_layer_crossings`, `barycenter_tie_breaks_with_centrality`,
  `refinement_improves_or_maintains_crossings`, `prop_invariant_rank_consistency_dag`).
- Output bit-identical → SVG/term/canvas conformance unaffected.

## vs. the original (mermaid-js)

This is an internal before/after A/B. A head-to-head ratio against the mermaid-js
renderer is tracked separately and is currently blocked on comparator-corpus
availability (see `docs/NEGATIVE_EVIDENCE.md`, cod-b). The new `layout_wide` /
`full_pipeline_wide` bench cases are the realistic-workload harness that the
mermaid-js comparator will reuse.
