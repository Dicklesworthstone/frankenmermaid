# Perf win: flat CSR bucket grid for the edge-routing obstacle index

**Crate:** `fm-layout` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — wide-graph layout ~12–13% faster; byte-identical; stacks on the dense
obstacle-index (3d81eca) + FxHashMap pair-tracker (2c09a38) wins.

## What changed

`ObstacleSpatialIndex` was a `FxHashMap<(i32,i32), Vec<usize>>` cell grid: the build did one
heap `Vec` allocation per occupied cell plus per-insert hashing (~200 allocs for a 16x32 wide
graph), and each query hashed every cell it touched. Replaced with a flat **CSR
(compressed-sparse-row) bucket grid**:
- `new` computes the obstacle cell-bbox, counts obstacles per cell into `offsets[i+1]`,
  prefix-sums to row pointers, then scatters indices into one flat `Vec<u32>` — two flat
  arrays, **3 allocations total**, no hashing.
- `query_segment` indexes `offsets[ci]..offsets[ci+1]` directly (clamped to the grid) instead
  of hashing each cell, then dedups via the existing generation-stamp `seen` array and
  `sort_unstable`s — same as before.
- `new` now returns `Option`: a layout so spread out that its cell bbox exceeds
  `MAX_CELLS_PER_OBSTACLE`×obstacles is not worth a dense grid, so it returns `None` and the
  caller falls back to the per-edge linear AABB scan (byte-identical, just slower — and real
  diagrams pack obstacles tightly, so this never trips).

## Correctness

Byte-identical. The same `cell_of` mapping fills the same cells with the same obstacle
indices; `query_segment` returns the deduped candidate set `sort_unstable`-ordered, so
within-cell order never escapes the final sort (the hash grid's within-cell order didn't
either). Query cells outside the grid hold no obstacles — exactly the hash grid's missing
entries. The two index↔linear-scan equivalence tests
(`obstacle_index_*`) plus the full suite confirm it: **428 fm-layout unit tests + doc tests
pass; `frankentui_conformance_test` (whole-corpus identity) passes; clippy clean.**

## Measurement

Same-worker, both-order stash-swap A/B in one `rch exec` (controls hardware + brackets the
second-phase thermal penalty). Fresh per-role target dir `/data/projects/.rch-targets/mermaid-bt2`
(see infra note), package `frankenmermaid-cli`, bench `pipeline_bench`, group
`wide_stages/layout` (pure `layout_diagram`), criterion `--measurement-time 4`.

| bench | ORIG (hash grid) | OPT (CSR) | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | net |
|---|---:|---:|---:|---:|---:|
| `wide_stages/layout/8x16`  | 107.70 µs | 108.99 µs | −1.8% (p=0.36, NS) | −5.6% (p=0.00) | neutral¹ |
| `wide_stages/layout/12x24` | 250.64 µs | 220.34 µs | +22.2% (p=0.00) | −20.8% (p=0.00) | **~12% faster** |
| `wide_stages/layout/16x32` | 449.81 µs | 391.25 µs | +19.9% (p=0.00) | −20.7% (p=0.00) | **~13% faster** |

¹ 8x16 has 128 obstacles, below the `DENSE_INDEX_OBSTACLES` (256) floor, so it is unindexed in
both ORIG and OPT — the identical no-index linear-scan path. Its readings straddle zero
(sign-flips with order) = noise, as expected. The indexed sizes (12x24, 16x32) are
direction-consistent OPT-faster in both orders, p=0.00.

## Mermaid.js head-to-head

Third stacking layout-stage win (after 3d81eca dense indexing + 2c09a38 pair-tracker). At
16x32 the wide layout stage drops ~13% (≈450→391 µs); full-pipeline `full_pipeline_wide/16x32`
was ~4.32 ms vs the pinned Mermaid `11.12.0` denominator 2879.185 ms (≈666× faster), so the
~59 µs layout cut nudges the full-pipeline ratio to ≈675×. Modest at the pipeline level (layout
is ~14% of the wide pipeline) but a clean, free, byte-identical reduction of the dominant layout
phase (`build_edge_paths`).

## Infra note

rch retrieves worker-specific build artifacts back into the shared local `CARGO_TARGET_DIR`, so
a dir reused across workers with heterogeneous rustc versions throws spurious
`error[E0514]: incompatible rustc version` (silently hidden when piping `cargo bench | grep
time:`). A **brand-new per-attempt dir** (`mermaid-bt2`) syncs empty and lets the worker build
clean. When an rch bench "build fails" but a standalone `cargo build -p <crate>` succeeds,
suspect target-dir artifact pollution, not the crate.
