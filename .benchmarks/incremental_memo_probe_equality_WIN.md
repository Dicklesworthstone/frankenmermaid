# KEPT: incremental memo probe by structural equality — full-IR FNV fingerprint scan eliminated (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `0feefe46`
Lane: bd-12e Incremental Computation (architectural) — `IncrementalLayoutEngine` memo probe

## Ledger-first routing

- The fingerprint family (`hash_span_value` 9.58%, `hash_endpoint_value` 8.77%,
  `dependency_graph_cache_key` 7.56%, `graph_metrics_cache_key` 2.82%, `hash_label_ref` 2.35% —
  31.08% combined self time on `single_node_label_edit/incremental/1000`) was the top admissible
  frame block per the 2026-07-10 profile in `incremental_packed_span_hash_NEGATIVE.md`.
- That rejection's retry predicate (b) names this exact route: "a new … primitive eliminates a
  whole fingerprint scan". The word-packed span hash (REJECTED −0.86%) and the topology-stable
  cache-hit patch (REJECTED flat) are NOT retried; neither shape is touched.
- The 2026-06-29 NEGATIVE_EVIDENCE scoping ("cache-hit is hash-bound (605us), NOT clone-bound
  (23us)") bounded the O(1) follow-up to eliminating the IR hash; this lever does that without the
  3-crate mutation-invalidation IR-carried fingerprint by probing with exact equality instead.

## Lever

`layout_memo_key` no longer hashes the IR (was: `stable_layout_request_hash` — hand-rolled
flowchart walk or serde byte-FNV serializer over the entire IR, on EVERY engine call). The memo
key is now config-only (algorithm, cycle strategy, edge routing, spacing bits, constraint solver,
font metrics bits, guardrails — the exact former `hash_request_tail` coverage lifted into explicit
`LayoutMemoKey` fields), and the probe compares `cached.ir` (an exact snapshot stored on
`CachedTracedLayout`) against the incoming IR via `memo_ir_equal` — a destructuring,
cheapest-first / most-frequently-edited-first field comparison that short-circuits at the first
difference.

- HIT: equality scan (memcmp-speed) replaces the byte-at-a-time serial FNV scan; no collision
  risk — a hit now proves the inputs identical (strictly more correct than hash equality).
- MISS (the per-edit path): label edits short-circuit at the labels compare, so miss detection is
  near-free; one `ir.clone()` per miss is added to store the new snapshot (allocation-heavy, so
  libc malloc overstates this added cost ~2.4x per the A/B substrate rules — bias is AGAINST the
  lever, making the measured win conservative).
- ~640 lines of now-dead machinery removed (StableHashSerializer + 9 serde impls +
  `flowchart_layout_request_hash` + helpers unique to them). `hash_u64`/`hash_str`/
  `hash_endpoint_value` retained for the dependency/metrics/node-size cache keys.

## Behavior isomorphism

- Probe is strictly stricter (exact equality vs hash equality); a hit returns the identical cached
  clone as before; a miss runs the identical compute path. All 439 fm-layout tests pass, including
  the incremental invalidation/reuse suite and the three `layout_memoized_reuse` assertions.
- `memo_ir_equal` destructures `MermaidDiagramIr`, so any future IR field fails compilation here
  until it is compared — no silent staleness hole.

## Honest same-host interleaved A/B (null-controlled)

ORIG = `git archive 0feefe46` snapshot, CAND = working tree; both built
`cargo bench -p fm-layout --bench incremental_layout --profile release --no-run --locked`
(fm-layout opt-level 3, identical binary hash suffix f68625f9343e3f22 both target dirs).
Order C O O C, identical flags
(`--sample-size 30 --measurement-time 5 --warm-up-time 1 --noplot --discard-baseline`).

| Row | ORIG means (O1/O2) | CAND means (C1/C2) | Delta (avg vs avg) |
| --- | ---: | ---: | ---: |
| single_node_label_edit/incremental/1000 | 897.28 / 913.58 µs | 752.67 / 775.69 µs | **−15.6%** |
| five_node_cluster_edit/incremental/1000 | 919.31 / 944.09 µs | 789.35 / 804.76 µs | **−14.4%** |
| single_node_label_edit/full_recompute/1000 (NULL) | 188.36 / 186.50 µs | 182.07 / 187.74 µs | −1.3% (flat) |
| five_node_cluster_edit/full_recompute/1000 (NULL) | 186.33 / 193.53 µs | 182.64 / 186.29 µs | −2.9% (flat) |

Arm-to-arm point-estimate spreads: CAND single 3.0%, ORIG single 1.8% — under the 5% gate. The
full_recompute NULL rows never touch the engine, and both sit inside the known ~5% wall-noise
floor.

perf stat instructions:u (3 reps/arm, primary row, measurement-time 3): CAND 60.53/60.68/60.61B,
ORIG 54.18/54.31/54.28B. Criterion runs fixed TIME, so the faster candidate executes ~18% more
iterations in-window; normalizing by the wall delta gives ≈ **−5% instructions per iteration**.
The wall win exceeds the instr win because the removed FNV byte-loop is a serial dependency chain
(low IPC) while the added memcmp-style equality is wide.

small_graph_bypass sanity (single-shot each, not interleaved): /10 CAND +5.0% (at the wall-noise
floor at 11.7 µs), /20 CAND −4.6%, /40 CAND −6.3% — no regression signal, trend improves with
size.

## Verdict

**KEPT.** −15.6% / −14.4% on the two decision rows, null-controlled, 5x above the 3% keep gate.

## Honest frontier note + follow-up map (measured, not speculative)

Even after this lever, `incremental/1000` (764 µs) is still ~4x SLOWER than `full_recompute/1000`
(185 µs) on this bench: the incremental subgraph path recomputes a whole 500-node region for a
1-node edit, rebuilds ALL edge paths, and clones caches per pass. Remaining per-edit O(n) frames:
`track_dependency_graph_query`'s per-pass `dependency_graph_cache_key` recompute + `cached.ir =
ir.clone()` on every dep-cache hit (lib.rs ~1355/1375), the engine's per-pass
`node_size_cache.clone()` + `dependency_graph_cache.clone()` into the thread-local (a
move/`mem::take` shape instead), `compute_node_sizes` over all nodes, full `build_edge_paths`, and
subgraph-granular dirty regions. The topology-stable rejection's retry predicate ("query/clone
path top-2 after edge-paths and node-size are separately controlled") should be re-evaluated
against a FRESH post-land profile.
