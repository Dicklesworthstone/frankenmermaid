# REJECT: skip redundant `dependency_topology_equal` re-check in the size-stable fast path (bd-12e, 2026-07-24)

Agent: CopperCliff (cc), Opus 4.8. Base: `317844e2` (the Arc<DiagramLayout> win).
Fresh re-measurement of the 2026-07-10 do-not-retry (`incremental_topology_stable_cache_hit_patch_NEGATIVE.md`)
under the current code + mimalloc bench, since that verdict was libc-based and its mechanism
(`dependency_graph_cache_key` hashing) no longer exists (now `dependency_topology_equal` equality probe).

## The lever (clean subset of the old rejected candidate)

The size-stable fast path proves topology is identical (all edits `NodeChanged`, ids/subgraph-membership
unchanged) BEFORE calling `track_dependency_graph_query(ir)`, which then re-runs
`dependency_topology_equal(&cached.ir, ir)` — provably redundant. Threaded a `topology_known_stable: bool`
param (passed `all_node_changes`) that short-circuits the equality probe on the hit path. Kept the dirty
count / snapshot refresh / summary byte-identical (did NOT bundle the old candidate's risky in-place IR
patch). 440 fm-layout tests green incl. size-stable byte-identical parity + all incremental-vs-full
equivalence props ⇒ correctness-preserving.

## Why the non-LTO profile oversold it

Fresh non-LTO perf (`single_node_label_edit/incremental/1000`) put `dependency_topology_equal` at **8.0%
self** — the 3rd-largest frame. But under the real LTO release binary the wall effect is far smaller and
strongly size-dependent (the probe is a big fraction of a small graph's fast-path work, a tiny fraction of
a large graph's, where geometry/IR clones dominate). ⭐ Non-LTO self% ≠ LTO wall%; confirm on the LTO bench.

## A/B (criterion --baseline, mimalloc bench)

Run #1 (load ~8.5, clean): incr/100 **−10.4%** (p<0.05), /500 **−3.3%** (p=0.02), /200 −2.5% (p=0.12, ns),
**/1000 −1.5% (p=0.34, WASH)**. A modest, size-dependent effect — real at small graphs, washed at the
primary /1000 size.

Run #2 (load SPIKED mid-run): candidate times inflated ~50% (/1000 151→235µs, absurd "+47% regression") —
INVALID. The machine load fluctuated badly this session (the Arc-plan doc warned 4→140), so the small-graph
win could NOT be independently confirmed.

## Verdict: REJECT (not shipped)

Unconfirmable, marginal, and a WASH at the headline /1000 size — fails the confident-keep gate, and reopening
a documented do-not-retry demands a *clear* win. Reverted to `317844e2`. The change is correct and eliminates
provably-redundant work, so it is NOT a hard do-not-retry: a future agent on a **quiet machine** (stable load
<8, tight-CI interleaved A/B) may revisit specifically for the SMALL-graph case (≤~200 nodes, the common
browser/interactive size), where run #1 showed a real −10%. Do NOT retry at /500-/1000 (wash) or bundle the
IR-in-place patch.
