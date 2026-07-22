# REJECTED (wash): dirty-member-scoped node sizing in the incremental path (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `79908acc`
Lane: bd-12e / bd-9rq7

## Lever tested

`compute_node_sizes_for_members`: the incremental subgraph path sized only the union of the dirty
regions' expanded local member sets (`incremental_region_members` output, precomputed per region
before the relayout loop) instead of all nodes; placeholder slots elsewhere (verified unread —
`build_subgraph_local_layout` indexes member slots only). Cache discipline preserved via a shared
`cached_node_size` helper. 439 tests green while measured.

## A/B (O/C/C/O interleaved, sample-size 40, measurement-time 6, quietest run of the session)

All CIs ≤ ±0.7%, nulls matched (~178-183 µs both arms):
- single_node_label_edit/incremental/1000: ORIG 363.1 µs vs CAND 360.8 µs → −0.6% (wash).
- five_node_cluster_edit/incremental/1000: ORIG 366.2 µs vs CAND 374.6 µs → +2.3% (slightly worse).

## Why it washed

With the FxHashMap node-size cache already landed (72ffc1f) the per-node cost on clean nodes is a
cheap probe; the frame's 4.24% self time is mostly the irreducible per-pass iteration + summary
machinery + the genuinely-dirty node's recompute. The lever's own bookkeeping (BTreeSet union,
placeholder Vec fill, per-region member precompute) gave the savings back.

## Verdict, retry condition, and VEIN ANALYSIS (2 consecutive REJECTs)

**REJECTED (wash), reverted** (backup of the candidate file kept in session scratchpad). Do not
retry member-scoped sizing while the node-size cache exists; retry only if the cache is ever
removed or a workload makes >5% of nodes dirty per pass.

With this and `incremental_precomputed_edits_passthrough_NEGATIVE.md`, the SAFE micro-vein on the
incremental rows is measured-exhausted: remaining frames are (a) `build_edge_paths` + routing
(~12.7%) — stale-path reuse breaks byte-identity when dirty nodes move; (b) region-granular
relayout (~9.3% tree frames) — changes incremental geometry, needs the incremental-vs-full parity
contract pinned down first; (c) the last pass snapshot clone (~4.9%) — irreducible without an
edit-session API (true Adapton interface: caller declares edits, engine owns the IR); (d) the
~21% allocator block — candidate vein per the alien graveyard is arena/pool allocation for layout
temporaries (no-unsafe constraint applies) AND aligning the bench harness onto mimalloc like the
CLI (substrate change — needs peer coordination since all fm-layout benches share it).
Next session should pick ONE of (b) or (d) as the new vein; (b) is the true Adapton depth.
