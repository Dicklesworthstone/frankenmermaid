# Standing + fresh layout profile — wide pipeline at its byte-identical floor on parse+layout

**Date:** 2026-06-28 — **Agent:** BlackThrush — **HEAD:** a523af9 (+ peer's uncommitted render WIP in tree)

## Head-to-head (full_pipeline_wide, this turn, healthy highs-sys worker)

| size | frankenmermaid | pinned Mermaid 11.12.0 | ratio |
|---|---:|---:|---:|
| 8x16  | 687 µs   | 315.140 ms  | **~459× faster** |
| 12x24 | 1.479 ms | 981.730 ms  | **~664× faster** |
| 16x32 | 3.041 ms | 2879.185 ms | **~947× faster** |

(Working tree includes a peer's uncommitted `build_common_node_fragment` render WIP; per 21203f3
full-node direct-byte measured ~0, so the committed-main ratio is ≈ these.) The 16x32 ratio improved
from ~670× (the f0024d7 standing, ~4.3 ms) to ~947× (3.04 ms) — the cumulative swarm + my-session
wins (2 parse: bbaf088 plain-label fast path, 6a8d164 edge right-contains guard).

## Fresh layout phase profile (16x32, FM_PROFILE, tree path)

| phase | time | share | status |
|---|---:|---:|---|
| build_edge_paths | ~132 µs | ~48% | CSR obstacle index + FxHashMap pair-tracker landed; residual is the CGA/query routing floor |
| tree+spans (build_tree + subtree spans + centers) | ~95 µs | ~34% | id-key sort landed; residual is `Vec<Vec>` adjacency allocs + recursive traversals |
| compute_node_sizes | ~38 µs | ~14% | ASCII width LUT landed |
| node_boxes_from_centers | ~12 µs | ~4% | tiny |

## Why the uncontested lanes are at floor

- **Parse** (2 wins this session + a523af9 negative): the fast-path's multi-scan-per-statement
  redundancies are harvested; single-scan / Vec-presize micro-levers measured below the noise floor.
  Remaining cost is inherent IR-ownership string copies (id+label, both required) + interning re-hash
  (prior ~0, hot-recycled).
- **Layout** (6 wins): edge_paths is CSR-indexed; tree+spans' only remaining lever is CSR-ing the
  `Vec<Vec>` outgoing/children adjacency — ~1% pipeline and **byte-identity-risky** (dedup'd sort
  buckets + BFS children order must match exactly), not worth the regression risk. node_sizes is
  LUT'd; node_boxes is negligible.

## The one remaining big win is render (peer-owned this turn)

Render is ~60% of the pipeline and byte-writing-bound; its micro-levers are exhausted (set-retain,
write_fixed2-LUT, describe_node all ~0 — describe_node's −8% single-order read was confirmed
warm-bias by its both-order A/B). The real win is the a11y/`data-*` **output reduction** (a contract
decision needing cod-b's Mermaid comparator to confirm Mermaid omits node `<title>`/`role`/`aria-*`
— see `render_output_reduction_OPPORTUNITY.md`). A peer is actively on render (`build_common_node_fragment`);
standing down to avoid duplicating it.
