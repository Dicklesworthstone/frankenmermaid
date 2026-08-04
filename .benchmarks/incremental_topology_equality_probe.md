# KEPT: dependency-graph topology probe by equality — per-pass topology FNV hash eliminated (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `26e540a8`
Lane: bd-12e / bd-9rq7 item (1) — `track_dependency_graph_query` topology key

## Ledger-first routing — the do-not-retry predicate FLIPPED

- `incremental_topology_stable_cache_hit_patch_NEGATIVE.md` closes three shapes: skipping
  `track_dependency_graph_query`, synthetic dependency-graph hit recording, and cached-IR in-place
  patching. Its retry predicate: "unless a future profile makes the query/clone path a top-2
  incremental hotspot after edge path construction and node-size recomputation are separately
  controlled". After d0af276c the fresh non-LTO profile shows `dependency_graph_cache_key` 11.87%
  + `hash_endpoint_value` 6.28% as the TOP block, edge paths at 4.77%, node-size out of the top
  frames — the predicate HOLDS.
- This lever retries NONE of the three closed shapes. The query still runs on every pass and still
  records its summary; only the probe primitive changes: the O(n+m) byte-FNV `dependency_graph_cache_key`
  recompute (plus the stored u64 key) is replaced by `dependency_topology_equal` — direct field
  equality against the cached IR snapshot, covering EXACTLY the hash's field set (node/edge/subgraph
  counts, node ids, per-node subgraph memberships, edge endpoints, subgraph id/parent/members).
- Same hash→equality shape as the 211c5ced memo lever. Also fixes a latent correctness hole: the
  u64 key could false-HIT on a hash collision and reuse a stale dependency graph for a different
  topology; equality cannot.

## Sites converted (all four users of the hash)

1. `track_dependency_graph_query` hit test (the per-pass 18% block).
2. `dirty_node_indexes_for_edits` `same_topology` (two hash computations → one equality).
3. Engine fast-path graph-reuse test (`all_node_changes ||` short-circuits it on the bench rows).
4. `CachedDependencyGraph.key` field removed; `hash_endpoint_value` now dead and deleted.
   `dependency_graph_cache_key_tracks_topology_not_label_text` rewritten as
   `dependency_topology_equal_tracks_topology_not_label_text` (same invariants + a node-id-rename
   negative case).

## A/B (two independent interleave sets, canonical release profile, same host, null rows)

ORIG = saved HEAD-state binary (byte-identical source to `26e540a8`), CAND = working tree.
Set 1 (C/O/O/C, /1000 rows): ORIG2's arm was hit by a host load burst — identified by its OWN
null row (full_recompute 191→281 µs) and excluded; the clean five-node pair with matched nulls
(191.0 vs 191.6 µs) gave −16.5% (652.4→544.7 µs).
Set 2 (O/C/O/C, all single_node sizes; contaminated O1 /1000 again excluded via its wide CI):

| single_node_label_edit/incremental | ORIG mean | CAND mean | Delta |
| --- | ---: | ---: | ---: |
| /100 | 99.8 µs | 87.3 µs | **−12.5%** |
| /200 | 143.4 µs | 115.4 µs | **−19.5%** |
| /500 | 341.0 µs | 277.7 µs | **−18.6%** |
| /1000 (clean pair O2/C-avg) | 658.4 µs | 537.2 µs | **−18.4%** |

NULL full_recompute rows flat at every size on clean arms (e.g. /1000: 194.2/191.1 vs
195.5/189.8 µs). Clean-arm criterion CIs ≤ ±1%.

## Verdict

**KEPT.** −12.5%..−19.5% across all sizes, null-controlled, two independent interleave sets
agreeing. 439 fm-layout tests green (topology test rewritten with an added node-id-rename negative
case); workspace fmt + clippy green.

## Frontier shift (measured)

At /100 the incremental path (87.3 µs) now BEATS full recompute (116.6 µs) — the first size where
the engine is net-positive on this bench. /200: 115.4 vs 42.3 µs and /1000: 537.2 vs 195.5 µs
remain full-recompute-favored: the dominant remaining costs are the whole-subgraph region relayout
(a 1-node edit recomputes a 500-node region at /1000), the full `build_edge_paths` rebuild, and
the remaining per-pass snapshot-store clones (`cached.ir = Arc::new(ir.clone())` on dep-hit + the
memo store clone). Cumulative single/1000 across the three landed levers: ~905 → ~537 µs (−41%).
