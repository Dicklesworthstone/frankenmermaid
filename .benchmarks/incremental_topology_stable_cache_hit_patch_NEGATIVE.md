# REJECTED: topology-stable incremental dependency-cache hit patch (2026-07-10)

Agent: cod_fm
Base: `fbd150d`
Lane: `fm-layout` incremental edit rerender (`bd-1buv.3`)

## Ledger-first check

Prior closed entries were checked before coding:

- Crossing-count container family is closed for this pass: dense position maps/Fenwick, flat-array
  `total_crossings`, and cyclic FxHashMap crossing count.
- Thresholded barycenter accumulation is already kept; precomputed-adjacency barycenter is rejected.
- `dependency_graph_cache_key` endpoint hashing is already kept and is not retried here.
- Render double-copy shapes are mined and out of lane.

## Profile basis

Release-profile, symbol-preserving profiles routed the work away from crossing-container retries and toward
incremental cache/meta churn:

- `crossing_min/dense_dag/egraph/20`: `fm_layout::egraph_ordering::crossing_count` remained top at 47.51%.
- `single_node_label_edit/incremental/1000`: `hash_span_value` 10.30%,
  `IncrementalLayoutEngine::layout_diagram_traced_with_config_and_guardrails` 9.06%, `_int_malloc` 8.31%,
  `dependency_graph_cache_key` 7.81%, `hash_endpoint_value` 7.31%, `MermaidDiagramIr::clone` 4.54%,
  `malloc_consolidate` 4.16%, `compute_node_sizes` 4.15%.
- `perf stat` memory pressure:
  - `single_node_label_edit/incremental/1000`: 1.47B cache refs, 164.95M cache misses, 11.24% miss rate.
  - `five_node_cluster_edit/incremental/1000`: 1.42B cache refs, 159.05M cache misses, 11.18% miss rate.

## Lever Tested

For topology-stable `NodeChanged` edits:

- skip the generic `track_dependency_graph_query(ir)` hit path;
- derive the dirty node set directly from edits;
- record a synthetic dependency-graph cache hit;
- patch the cached IR in place for unchanged label IDs instead of cloning the whole `MermaidDiagramIr`.

The candidate had guard tests for label-text edits versus node-id relabels while measured. Code was reverted
after the A/B failed the keep gate.

## Same-worker A/B

ORIG was built from a fresh `git archive HEAD` snapshot at `fbd150d`; candidate was built from the edited
checkout. Both used:

```text
cargo bench -p fm-layout --bench incremental_layout --profile release --no-run
--warm-up-time 1 --measurement-time 10 --sample-size 30 --noplot --discard-baseline
```

Final in-place patch candidate:

| Row | ORIG mean | Candidate mean | Ratio | Verdict |
| --- | ---: | ---: | ---: | --- |
| `single_node_label_edit/incremental/1000` | 1.0340 ms | 1.0340 ms | 1.000x | flat |
| `five_node_cluster_edit/incremental/1000` | 1.0369 ms | 1.0459 ms | 0.991x | 0.87% slower |

Earlier skip-only variant:

| Row | ORIG mean | Candidate mean | Ratio | Verdict |
| --- | ---: | ---: | ---: | --- |
| `single_node_label_edit/incremental/1000` | 1.0245 ms | 1.0259 ms | 0.999x | 0.14% slower |
| `five_node_cluster_edit/incremental/1000` | 1.0447 ms | 1.0414 ms | 1.003x | 0.32% faster |

## Verdict

REJECTED. After the endpoint-hash keep, this dependency-query/cache-update bookkeeping is below the keep floor;
edge path construction, node-size recomputation, and general layout object churn dominate the 1000-node edit
rows.

## Do Not Retry

Do not retry this topology-stable dependency-cache hit patch shape (skip `track_dependency_graph_query`,
synthetic dependency-graph hit recording, or unchanged-label cached-IR patch) on the existing
`incremental_layout` 1000-node label-edit rows unless a future profile makes the query/clone path a top-2
incremental hotspot after edge path construction and node-size recomputation are separately controlled, or a
new workload changes per-rerender edit cardinality enough to predict a >3% direct same-worker A/B win.
