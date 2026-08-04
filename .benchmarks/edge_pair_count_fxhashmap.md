# Perf win: FxHashMap + no-parallel skip for the edge-routing parallel-edge tracker

**Crate:** `fm-layout` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — wide-graph layout ~6–13% faster; byte-identical; stacks on the dense
obstacle-index win (3d81eca).

## What changed

`build_edge_paths_with_orientation` tracked parallel edges in a
`BTreeMap<(usize,usize),usize>` — built with one keyed insert per edge (960 at 16x32) and then
read once per edge in the hot routing closure (another 960). That is ~1920 O(log n) keyed
operations over a map that, for the common flowchart, is never iterated for output order.

Two changes:
1. Swap the map to `FxHashMap` (already re-exported from `fm-core` and used by the obstacle
   spatial index in this same function), with `reserve(edges.len())`. The map is read **by key
   only** — never iterated — so it is determinism-safe, and the keyed ops become O(1).
2. Compute `any_parallel = edge_pair_count.values().any(|&c| c > 1)` once after the build loop.
   When there are no parallel edges (the common case), the hot per-edge closure skips the map
   lookup entirely and uses `(pair_total, pair_idx) = (1, 0)` directly.

## Correctness

Byte-identical. `FxHashMap` and `BTreeMap` return the same value for a keyed `get`; the map is
never iterated, so hash order is irrelevant. When `!any_parallel`, every pair has count 1, so
`pair_total = 1`, `pair_idx = 0`, and `parallel_offset = 0` — exactly what the per-edge lookup
produced. Validation: `fm-layout` 428 unit tests + doc tests pass; `frankentui_conformance_test`
(whole-corpus identity) passes; clippy clean.

## Measurement

Same-worker, both-order stash-swap A/B inside one `rch exec` (controls hardware + brackets the
~9% second-phase thermal penalty). Per-crate target dir
`/data/projects/.rch-targets/mermaid-bt`, package `frankenmermaid-cli`, bench `pipeline_bench`,
group `wide_stages/layout` (pure `layout_diagram`), criterion `--measurement-time 4`.

| bench | ORIG (BTreeMap) | OPT (FxHashMap+skip) | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | net |
|---|---:|---:|---:|---:|---:|
| `wide_stages/layout/8x16`  | 119.74 µs | 111.38 µs | +3.4% (p=0.04) | −11.2% (p=0.00) | **~7% faster** |
| `wide_stages/layout/12x24` | 282.82 µs | 265.23 µs | +2.8% (p=0.04) | −13.1% (p=0.00) | **~6–8% faster** |
| `wide_stages/layout/16x32` | 552.89 µs | 480.52 µs | +14.2% (p=0.00) | −14.0% (p=0.00) | **~13% faster** |

Direction-consistent OPT-faster in **both** orders at all sizes (ORDER_B all p=0.00). The
absolute baseline is higher than the prior standing (552 vs 430 µs) only because this A/B landed
on a slower worker; the same-worker *relative* delta is the signal.

## Mermaid.js head-to-head

This is a layout-stage win stacking on the dense obstacle-index (3d81eca). At 16x32 the wide
layout stage drops ~13%; full-pipeline `full_pipeline_wide/16x32` was ~4.32 ms (post-3d81eca) vs
the pinned Mermaid `11.12.0` denominator 2879.185 ms (≈666× faster), so the ~55 µs layout cut
nudges the full-pipeline ratio to ≈674×. Modest at the pipeline level (layout is ~14% of the
wide pipeline) but a clean, free, byte-identical reduction.

## Infra note

The earlier `mermaid-cc` target dir had accumulated mixed-rustc artifacts (rch retrieves
artifacts from multiple workers into the shared dir), causing spurious `E0514: incompatible
rustc version` build failures. Using a fresh per-role dir (`mermaid-bt`) fixed it — prefer a
clean per-role `CARGO_TARGET_DIR` when cross-worker artifact pollution appears.
