# WIN: Brandes-Köpf dense node-rank/position lookup — Sugiyama layout ~1.10×, byte-identical

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `aa3903d` · **File:** `crates/fm-layout/src/lib.rs`
(Brandes-Köpf region, peer-free — cod on rendering). **Verdict: KEEP.**

## Profile-first

`perf --call-graph=dwarf` on `cyclic_scc_100` (Sugiyama-routed): `layout_diagram_sugiyama_traced_with_config`
= **11.17% self-time**, unusually high for an orchestrator. The folded chain resolved it to inlined
`brandes_kopf_secondary_coords` → `bk_vertical_alignment` → `bk_upper_neighbours`, with
**`BTreeMap<usize,usize>::get` ≈ 3.76% of the whole pipeline** in the alignment inner loop.

## Root cause

Brandes-Köpf vertical alignment runs **4 passes**, each iterating every rank × every node. Per node,
`bk_vertical_alignment` probed `ranks: &BTreeMap<usize,usize>`, and `bk_upper_neighbours` probed *both*
`ranks` and a per-rank `pos_map: &BTreeMap<usize,usize>` — **two B-tree lookups per adjacency edge, ×4
directions**, i.e. `O(edges · 4 · log|V|)` with the probe in the hot path. The exact anti-pattern the
certified barycenter dense-rank win (3.591×) removed, on a sibling function.

## The lever (one)

Build two node-indexed dense arrays **once** in `brandes_kopf_secondary_coords`, reused by all 4 passes:
- `dense_node_rank: Vec<usize>` — reproduces `ranks.get(&v).copied().unwrap_or(0)`.
- `pos_of: Vec<usize>` — reproduces each node's position in its rank's `pos_map`, with `BK_POS_ABSENT`
  (`usize::MAX`) where that map had no entry.

`bk_vertical_alignment` and `bk_upper_neighbours` now index these O(1) instead of probing the B-trees. The
per-rank `rank_pos_maps: BTreeMap<usize, BTreeMap<usize,usize>>` is **eliminated** (its build removed too);
the `adjacent_rank` existence guard becomes `ordering_by_rank.contains_key(&adjacent_rank)` (same key set).

## Byte-identical — by construction, verified by the layout goldens

The dense arrays hold exactly the values the B-trees returned. The one subtlety — the old
`pos_map.get(&n)` `Some`-guard also filtered the `adjacent_rank == 0` + unranked-node case — is preserved by
the `pos_of[n] != BK_POS_ABSENT` sentinel check (a node absent from every rank ordering keeps the sentinel).
BK coordinates feed straight into node positions, so any change would move the goldens:
- `golden_layout_test` **2/2**, `frankentui_conformance_test` green, `cargo test -p fm-layout` **439 passed**.
- `cargo fmt --check` clean; `ubs` 160→161 (+1 = the `!=`/`==` "secret compared with" false positive).

## Measurement — same-worker A/B, gate on median

New `layout_sugiyama/scc` bench (fm-cli): `layout_diagram` on cyclic-SCC graphs (route to Sugiyama ⇒ BK runs).
cand (worktree) vs base (`git show HEAD:lib.rs > lib.rs`, lever reverted, bench kept), via
`RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec`. `rch` can't pin a worker, so I retried until **both
arms landed on the same worker** and read the ratio directly (no cross-worker confound). Two independent
same-worker pairs on `hz2`:

| pair | cand p50 | base p50 | ratio | CIs |
|---|---:|---:|---:|---|
| 1 | 196.85 µs [191.64, 203.88] | 220.35 µs [215.20, 225.66] | **0.893× (1.12×)** | non-overlapping |
| 2 | 198.96 µs [195.12, 202.50] | 214.37 µs [211.07, 218.20] | **0.928× (1.08×)** | non-overlapping |

Both matched pairs agree (geomean ~**1.10× faster** on `scc_100` layout), CIs non-overlapping in both — well
above the ~1% harness floor calibrated earlier. (A first cross-worker run read 0.73× because `ovh-a` is faster
than `hz2`; discarded — same-worker matching is the honest read.)

## Scope

Helps every Sugiyama-routed diagram (cyclic graphs, graphs with back-edges) — Brandes-Köpf runs on all of them.
Neutral on Tree-routed flowcharts (BK not invoked). Same proven primitive as the barycenter dense-rank win,
now applied to coordinate assignment; the B-tree-probe family in the ordering/coordinate hot path is a step
closer to harvested.
