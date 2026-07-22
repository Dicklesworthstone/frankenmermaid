# KEPT: one shared per-pass IR snapshot for all engine cache stores (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `bcf8c44d` (saved pre-edit binary, byte-identical source)
Lane: bd-12e / bd-9rq7 — remaining per-pass snapshot-store clones

## Ledger-first routing

Fresh post-bcf8c44d profile (non-LTO, symbols) on `single_node_label_edit/incremental/1000`:
allocator family ~25% (`_int_malloc` 9.74%, `malloc_consolidate` 5.05%, `_int_free_chunk` 2.91%,
`cfree` 2.88%, `__libc_malloc2` 2.38%, `unlink_chunk` 2.31%), `MermaidDiagramIr::clone` 7.23% +
`String::clone` 3.16%, `build_edge_paths_with_orientation` 6.03%, `graph_metrics_cache_key` 5.77%,
`dependency_topology_equal` 3.05% (the new probe — cheap as designed). The two remaining per-pass
deep clones (dep-cache store + memo store) were the scoped bd-9rq7 item; not adjacent to any
closed shape.

## Lever

`IncrementalCacheState.pass_snapshot: Option<Arc<MermaidDiagramIr>>` — created lazily ONCE per
engine pass (`get_or_insert_with` in `track_dependency_graph_query`, before the hit/miss split),
shared by the dependency-cache hit store (`cached.ir = snapshot`), the miss store, and — via
`pass_snapshot.take()` after `state_guard.finish()` — the memo store (`CachedTracedLayout.ir` is
now `Arc<MermaidDiagramIr>`). Paths that never reach the query (small-graph bypass, non-flowchart)
fall back to their own `Arc::new(ir.clone())`. Every store still snapshots the SAME `ir` the pass
received (immutable borrow throughout), so cache semantics are value-identical; the memo and dep
caches now share one allocation instead of holding two equal copies (RSS down too).

## A/B (C/O/O/C interleaved, canonical release profile, quiet host, nulls matched)

| Row | ORIG means (O1/O2) | CAND means (C1/C2) | Delta |
| --- | ---: | ---: | ---: |
| single_node_label_edit/incremental/1000 | 506.06 / 531.67 µs | 400.52 / 396.70 µs | **−23.2%** |
| five_node_cluster_edit/incremental/1000 | 520.12 / 534.32 µs | 424.82 / 398.11 µs | **−22.0%** |
| single full_recompute (NULL) | 184.32 / 185.19 µs | 184.01 / 187.69 µs | flat |
| five full_recompute (NULL) | 186.12 / 189.46 µs | 187.07 / 183.46 µs | flat |

Arm spreads ≤4.9%; all CIs ≤ ±2%. The delta exceeds the naive one-clone estimate because halving
IR-clone traffic also halves its malloc/free/consolidate churn (~25% profile block). Allocator
caveat as before: no mimalloc in the bench harness, so this is overstated vs the mimalloc CLI
(~2.4x rule); fm-wasm (dlmalloc) is the representative consumer.

## Verdict

**KEPT.** −23.2% / −22.0%, null-controlled. 439 fm-layout tests green; workspace fmt+clippy green.
Cumulative single/1000 across four bd-12e levers: ~905 → ~399 µs (−56%).

## Next (fresh-profile order)

`build_edge_paths_with_orientation` 6.03% + `query_segment` 3.26% (full edge rebuild per pass);
`graph_metrics_cache_key` 5.77% (third FNV walk — same hash→equality family, but needs a snapshot
to compare against on paths where the dep cache is absent; the shared pass_snapshot now exists to
serve that); region granularity (1-node edit still relayouts a 500-node region; incremental beats
full recompute only at /100 so far).
