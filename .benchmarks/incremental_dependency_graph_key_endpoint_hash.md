# Incremental dependency-graph endpoint hashing - KEPT 2026-07-10

Agent: cod_fm
Base: 5003c27
Lane: fm-layout incremental edit rerender (`bd-1buv.3`)

## Ledger check

This is not a retry of the prior `stable_layout_request_hash` Debug-string optimization. That entry covered
the whole-layout memo key. This lever targets `dependency_graph_cache_key`, where each edge endpoint was
still hashed via `format!("{:?}", endpoint)`.

Respected closed entries: acyclic SCC fast-path, thresholded barycenter accumulation, precomputed adjacency
rejection, dense crossing-count position-map/Fenwick rejection, flat-array crossing-count rejection, and
FxHashMap cyclic crossing-count rejection.

## Profile

Incremental `single_node_label_edit/incremental/1000` profile:

- `_int_malloc`: 12.93%
- `dependency_graph_cache_key`: 6.56%
- `String::write_str`: 6.14%
- `__memmove`: 4.11%
- `_int_free_chunk`: 3.60%
- `MermaidDiagramIr::clone`: 3.44%

Crossing `crossing_min/dense_dag/egraph/20` profile still points at the closed crossing-count family:

- `fm_layout::egraph_ordering::crossing_count`: 46.27%

## Lever

Replace per-edge endpoint `format!("{:?}", ...)` strings in `dependency_graph_cache_key` with the existing
structured `hash_endpoint_value` helper. This keeps the topology-only key and removes two temporary strings
per edge.

## Same-worker A/B

Command shape:

```bash
cargo bench -p fm-layout --bench incremental_layout --profile release -- \
  --warm-up-time 1 --measurement-time 5 --sample-size 20 --noplot --discard-baseline
```

ORIG was built from `git archive HEAD` at `5003c27`; candidate was built from the edited checkout.

| Row | ORIG median | Candidate median | Ratio | Delta |
| --- | ---: | ---: | ---: | ---: |
| `single_node_label_edit/incremental/1000` | 2.0537 ms | 1.1302 ms | 1.82x faster | -44.96% |
| `five_node_cluster_edit/incremental/1000` | 3.0204 ms | 1.0807 ms | 2.79x faster | -64.22% |

Candidate repeat:

- `single_node_label_edit/incremental/1000`: [1.1238, 1.1435, 1.1614] ms
- `five_node_cluster_edit/incremental/1000`: [1.0567, 1.0699, 1.0855] ms

Primary candidate sample CV: 4.42%.

## Behavior and gates

Added `dependency_graph_cache_key_tracks_topology_not_label_text`: label text changes keep the key stable;
edge rewiring changes the key.

Passed:

- `cargo fmt --check -p fm-layout`
- `rch exec -- cargo check -p fm-layout --all-targets --quiet`
- `cargo test -p fm-layout dependency_graph_cache_key_tracks_topology_not_label_text --quiet`
- `cargo clippy -p fm-layout --all-targets --quiet -- -D warnings`
- `ubs crates/fm-layout/src/lib.rs` was run; it exits 1 on the existing fm-layout baseline/heuristics while
  its cargo-backed formatting, clippy, cargo check, and test-build sections are clean.

Verdict: KEEP.
