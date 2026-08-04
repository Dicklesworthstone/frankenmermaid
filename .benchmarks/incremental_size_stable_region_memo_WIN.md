# WIN: size-stable region memo fast path (moved ahead of guardrails) — bd-12e / bd-9rq7 (2026-07-23)

Agent: CopperCliff (cc). Base: `bfdd9081`.

## Lever

`IncrementalLayoutEngine::try_incremental_subgraph_relayout` now serves the cached geometry
directly for **size-stable label-text-only edits**, and does so **before** algorithm dispatch and
guardrail evaluation. Gate (all must hold):

- every edit is a `NodeChanged` (`all_node_changes`),
- the config memo key is unchanged (`cached_layout.key == key`),
- for every dirty node: `id` unchanged, subgraph membership unchanged (so it stays in its region /
  cluster), and the freshly measured `compute_node_size` is **bit-identical** to the cached box
  (`.to_bits()` on width and height) — i.e. a pure label-text swap within one width class
  (e.g. `"Edited v0"` → `"Edited v1"`, all trailing digits share `CharWidthClass::Normal`).

When it fires it clones the cached node/edge/cluster geometry, refreshes only node spans, maintains
the dependency cache + shared pass snapshot exactly like the slow path, emits
`query_type = "layout_incremental_subgraph_relayout_size_stable"`, and returns.

## Why the prior REJECT #3 (wash, firing unverified) flips to a WIN

The prior candidate placed this gate **after** `dispatch_layout_algorithm` +
`evaluate_layout_guardrails`. At every benchmarked size ≥ 200 nodes the Sugiyama cost estimate
(`nodes * edges / 50 + …`) blows past `max_layout_time_ms = 250`, so guardrails set
`fallback_applied = true` and the function `return None`ed **before reaching the old gate** — the
fast path never fired at bench scale, which is exactly why it measured as a wash. Instrumentation
this session (`FM_FASTPATH_DEBUG`) confirmed: fired at 100 total nodes, **zero** fires at 200/500.

Moving the gate ahead of dispatch/guardrails is sound because when the gate holds, every downstream
input (topology, node sizes, config) is unchanged, so dispatch, guardrail selection, and the full
layout are all pure functions of unchanged inputs — the cached geometry (and the dispatch/guard
decisions carried in its cached trace) is exactly what recomputing would produce.

This satisfies the retry predicate recorded in
`incremental_trace_snapshot_retention_FIX.md` ("instrument the branch, verify it fires on iters 2+,
THEN re-A/B; if it does not fire, debug…") — it did not fire; the fix is the gate placement.

## Correctness proof

Faithful-memoization unit test `incremental_layout_engine_serves_size_stable_label_edits_from_region_memo`
runs `warm → "Edited v0" → "Edited v1"` and asserts the fast-path output is **byte-identical** to
the slow path it stands in for, using a `#[cfg(test)]` thread-local escape hatch
(`DISABLE_SIZE_STABLE_FAST_PATH`, compiled to `const false` in release ⇒ zero production overhead)
to force the slow path in the comparison arm. Verified at:
- **32/subgraph (64 total):** slow path = selective region relayout — geometry identical.
- **500/subgraph (1000 total):** slow path = guardrail-forced full recompute — node/edge/cluster/
  cycle-cluster/bounds/extensions all identical. `dirty_regions` is intentionally excluded (it is an
  incremental redraw hint, not geometry; the fast path is entitled to populate it where the
  full-recompute fallback leaves it empty).

439 → 440 fm-layout lib tests green; fm-wasm 20 green (its `.contains("incremental")` classifier
still tags the new query_type). clippy `-D warnings` clean; `cargo fmt --check` clean.

## Measured (interleaved one-binary A/B, per-arm target dirs, C/O/C/O/O/C/O/C, CPU-load ~5)

Tight `single_node_label_edit/incremental/1000` (sample-size 40, 5s, 4 reps/arm):
- CAND mean **241.6 µs** (CV ≈ 2.6%), ORIG mean **368.6 µs** (CV ≈ 2.9%) → **−34.5%**.
- Non-overlapping: max CAND 250.8 µs < min ORIG 357.5 µs.

All-sizes pass (sample-size 20, 4s, C/O/O/C), `single_node_label_edit/incremental`:
| total nodes | ORIG | CAND | Δ |
|---|---|---|---|
| 100 | ~70.0 µs | ~31.1 µs | −56% |
| 200 | ~84.0 µs | ~52.3 µs | −38% |
| 500 | ~201.6 µs | ~141.9 µs | −30% |
| 1000 | ~382.2 µs | ~259.2 µs | −32% |

`five_node_cluster_edit/incremental/1000`: ORIG ~367 µs → CAND ~245 µs (**−33%**).

**Null control** — `full_recompute/*` (does not touch the changed path) — flat at every size
(≤ ~1% arm-to-arm, within noise), confirming the win is causal to the fast path, not machine drift.

Note: criterion is fixed-TIME, so `perf stat` total-instruction counts across arms are NOT
comparable (CAND ran ~7% more instructions because it completed ~55% more iterations in the same 3s
window). Wall time (interleaved, null-controlled) is the decision metric, per substrate rules.

## Frontier after this lever

At 1000 nodes CAND incremental (~242 µs) is still slightly above `full_recompute` (~185 µs): the
residual is the per-pass `nodes.clone()` (1000 boxes) + `edges.clone()` + the full-IR `Arc::new(ir.clone())`
snapshot inside `track_dependency_graph_query` + `derive_layout_edits`. The IR-snapshot clone is the
next-largest remaining frame (edit-session API vein) — see bd-9rq7.
