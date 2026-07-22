# KEPT: graph-metrics probe by equality — third per-pass FNV walk eliminated (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `c4d79de3` (saved pre-edit binary, byte-identical source)
Lane: bd-12e / bd-9rq7 — `graph_metrics_cache_key`

## Ledger-first routing

Post-c4d79de3 profile had `graph_metrics_cache_key` at 5.77% — the last remaining per-pass FNV
topology walk (it also ALLOCATED a resolved_edges Vec per probe). Same hash→equality family as
211c5ced (memo) and bcf8c44d (dependency graph); no closed shape touched.

## Lever

`graph_metrics_cache: Option<(u64, GraphMetrics)>` → `Option<(Arc<MermaidDiagramIr>, GraphMetrics)>`
(state + engine, engine install now `mem::take`). Probe = `metrics_topology_equal`: node count, raw
edge endpoints, port→node assignments — exactly the inputs of `resolved_edges`, whose resolved
index pairs the hash covered. No allocation, short-circuits, no collision false-hits. Conservative:
an edit changing raw endpoints that resolve identically now misses and recomputes identical metrics.
Miss-store reuses the shared `pass_snapshot` when it matches the queried IR (always true for the
engine's own pass), so no extra clone in steady state; exotic callers pay a fresh clone.
Corrupted-state test's poison entry updated from `u64::MAX` key to an empty-IR anchor (same intent).

## A/B (two interleave sets, opposite orders: C/O/O/C then O/C/C/O; 8 samples/arm-row total)

| Row | ORIG pooled mean | CAND pooled mean | Delta | Null check |
| --- | ---: | ---: | ---: | --- |
| single_node_label_edit/incremental/1000 | 400.9 µs | 375.8 µs | **−6.3%** | nulls matched (~183 µs both) |
| five_node_cluster_edit/incremental/1000 | 405.1 µs | 370.4 µs | **−8.6%** (≈−6% null-adjusted) | one ORIG null spiked (200 µs), conservatively adjusted |

First-set C2 single sample had a ±6.8% CI (load blip) — retained in the pooled means rather than
cherry-picked out; the confirmation set's CIs were all ≤±1.7%.

## Verdict

**KEPT.** Consistently above the 3% gate across both orderings after conservative null adjustment.
439 fm-layout tests green; workspace fmt+clippy green. Cumulative single/1000 across five bd-12e
levers: ~905 → ~375 µs (−59%).

## Next (fresh-profile order)

All three per-pass FNV topology walks are now gone. Remaining: `build_edge_paths_with_orientation`
(~6%) + `query_segment` (full edge rebuild per pass), `dirty_nodes_for_edits` re-deriving edits the
engine already computed (~2%), region granularity (the asymptotic one: a 1-node edit still
relayouts a 500-node region; incremental beats full recompute only at /100).
