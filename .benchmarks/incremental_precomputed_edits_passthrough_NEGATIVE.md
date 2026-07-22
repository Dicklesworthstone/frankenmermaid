# REJECTED (wash): pass precomputed edits into track_dependency_graph_query (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `346c790d`
Lane: bd-12e / bd-9rq7 — redundant derive elision

## Lever tested

`track_dependency_graph_query(ir, precomputed_edits: Option<&[LayoutEdit]>)`: the engine passed its
already-derived edit set (same Arc'd IR pair) plus the probe-known topology verdict, eliding one
`derive_layout_edits` + one `dependency_topology_equal` inside `dirty_nodes_for_edits` per pass
(via a factored `dirty_node_indexes_from_edits` core). Provably identical outputs; 439 tests green
while measured.

## A/B (C/O/O/C interleaved, sample-size 40, measurement-time 6)

- single_node_label_edit/incremental/1000: CAND 366.4 µs vs ORIG 368.1 µs → −0.5% (wash).
- five_node_cluster_edit/incremental/1000: raw +3.3% "slower", but CAND's OWN null rows ran ~3%
  elevated (188.7/183.1 vs 178.8/180.6 µs) → null-adjusted flat.

Predicted ~2-4% from the profile (`derive_layout_edits` 2.34% + `dependency_topology_equal` 3.59%,
of which the elided second calls are only a share); measured ≈0 — under the ~5% wall-noise floor
and under the 3% keep gate.

## Verdict and retry condition

**REJECTED (wash), reverted** (git checkout of the sole modified file; verified only this lever's
edits were present). Do not re-measure this elision standalone on the current incremental_layout
rows. Retry only as a RIDE-ALONG when a future lever needs the same parameterization anyway (the
region-granularity work will want dirty computation driven by the engine's edit set) — land it
then for cleanliness, not for speed, and attribute no perf claim to it.

Consecutive-REJECT count for the cycling protocol: 1.
