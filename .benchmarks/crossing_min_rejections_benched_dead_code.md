# DIG: four crossing-minimization rejections were benched on a workload where the code never runs

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `6082890` · **Status:** ANALYSIS ONLY, no code changed.
**Headline:** `reorder_rank_by_barycenter` is **47.64% of the entire parse+layout+render pipeline** on
`cyclic_scc_100` — the single largest frame anywhere in this repo — and **0.000%** on every input the
`layout_wide` bench uses.

---

## Ledger-grep first

`docs/NEGATIVE_EVIDENCE.md` — *"Barycenter sweep precomputed edge adjacency — REJECTED (2026-06-26)"*:

> **Do-not-retry note:** this is the **4th** data-structure rewrite of the crossing-minimization / ordering area
> to fail — see `flat-array-total-crossings-position-edge-tables`, `dense-crossing-count-position-maps`, and the
> stashed "local-delta `crossing_refinement` ~0 gain". Stop guessing at this stage: do **not** attempt further
> container/adjacency rewrites of barycenter or crossing counting **without a CPU profile (e.g. `perf`/`samply`
> on `layout_wide/16x32`) that names the actual dominant function** — the live candidates are Brandes-Köpf
> coordinate assignment and edge routing, not the ordering scans.

That retry-condition is a profile. This document supplies it — and the profile overturns the premise the
rejections rest on.

Also relevant, and *already in the ledger since 2026-06-29* (three days **after** the barycenter rejection):

> Phase-bisect (env-gated probes, load-immune): the Sugiyama crossing-minimization barycenter path contributes
> **ZERO** (wide flowcharts route to the Tree/fallback path, **NOT** Sugiyama — confirmed the routing fact).

Nobody connected that routing fact back to the rejections it invalidates.

## The confound, measured

`bench_layout_wide` (`crates/fm-cli/benches/pipeline_bench.rs:306`) builds its inputs with `gen_wide(layers,
width)` and registers them under `BenchmarkId::new("layered", label)`. The name says *layered*. The
auto-selector picks **Tree**.

`perf record -F 2500 --call-graph=dwarf` on the existing symbolized binary, self-time as a share of the whole
pipeline:

| input | `fm_layout` total | `reorder_rank_by_barycenter` + `total_crossings` | `…sugiyama…` | `build_tree_layout_structure` |
|---|---:|---:|---:|---:|
| `wide_8x16` *(= `layout_wide/8x16`)* | 21.80% | **0.000%** | 0.00% | 2.83% |
| `wide_16x32` *(= `layout_wide/16x32`)* | 14.16% | **0.000%** | 0.00% | 3.66% |
| `wide_40x80` | 10.41% | **0.000%** | 0.00% | 3.27% |
| **`cyclic_scc_100`** | **70.76%** | **48.450%** | 11.17% | 0.00% |

So all four rejected rewrites of the barycenter / crossing-counting area were A/B'd on
`layout_wide/{8x16,12x24,16x32}` — three inputs on which **the code under test executes zero instructions**.
The recorded outcomes are exactly what that predicts:

- `8x16` +1.40% "No change", `12x24` +1.70% "No change" — the lever's savings are structurally zero.
- `16x32` **+5.84% regressed** — the adjacency *build cost* was added to a path that never reads it.

The reject's own root-cause paragraph ("the fixed build cost cancels or exceeds whatever scanning it saves")
is correct *for a Tree-path flowchart* and says nothing about Sugiyama. **These are not four pieces of evidence
that the lever fails. They are one piece of evidence that the bench was wrong, repeated four times.**

## Where the time actually goes (Sugiyama path)

`cyclic_scc_100`, 100 nodes / 196 edges, default profile, `fm_layout` = 70.76% of pipeline:

| frame | self-time (pipeline) |
|---|---:|
| `fm_layout::reorder_rank_by_barycenter` | **47.64%** |
| `fm_layout::layout_diagram_sugiyama_traced_with_config` | 11.17% |
| `fm_layout::find_obstacle_nudge_y` | 5.24% |
| `fm_layout::find_obstacle_nudge_x` | 2.21% |
| `fm_layout::total_crossings` | 0.81% |
| `fm_layout::detect_cycle_components` | 0.80% |
| `fm_layout::bk_horizontal_compaction::place_block` | 0.74% |

Note **Brandes-Köpf is 0.74%** — the reject note's "live candidate" is cold. Edge routing
(`find_obstacle_nudge_*`, 7.45%) is warm but an order of magnitude behind the ordering scan.

For contrast, on the DAG-shaped `dense_dag_200` (which routes to Tree) `fm_layout` is 26.44% and
`find_obstacle_nudge_x` alone is **19.36%** — a completely different bottleneck. **The layout bottleneck is
diagram-shape-dependent, and no single bench can stand in for the crate.**

## Mechanism (read from the source, `fm-layout/src/lib.rs:11572`)

`reorder_rank_by_barycenter` is called `~rounds(4) × 2 × ranks` times per layout. Each call:

1. `ordering_by_rank.get(&rank).cloned()` — clones a `Vec<usize>` (**one allocation per call**);
2. builds `adjacent_position: BTreeMap<usize, usize>` from the adjacent rank (**allocation + O(k log k)**);
3. then, for ranks narrower than `SINGLE_PASS_RANK_THRESHOLD = 8` (the common case: ~100 nodes spread over
   ~25 ranks ⇒ ~4 nodes/rank), **for every node it rescans the entire `ir.edges` list**, and for every edge it
   does a `ranks.get(&node)` lookup into a `BTreeMap<usize, usize>`.

So the narrow path is `O(rank_size × |E| × log|V|)` per call, with a B-tree probe in the innermost loop. At
240 calls × ~4 nodes × 196 edges that is ~188k B-tree probes per layout — which is where the 47.64% lives.

## The lever this profile licenses (NOT attempted here — see Blocker)

The rejected shape was a `Vec<Vec<usize>>` adjacency: `~2·node_count` outer + `node_count` inner allocations.
That is the wrong primitive, and its build cost is precisely what sank it. The profile points at something
cheaper and different in kind:

1. **Dense rank array.** Replace the `ranks: &BTreeMap<usize, usize>` probe in the innermost loop with a
   `Vec<u32>` indexed by node index, built **once per `crossing_minimization`** — one allocation, O(1) lookup.
   This removes the `log|V|` factor from ~188k inner-loop probes without building any adjacency at all.
2. **Dense position arrays.** `adjacent_position` and `local_slot` are per-call `BTreeMap`s over a permutation;
   both are `Vec<u32>` scatter tables. Reuse one scratch buffer across calls instead of allocating per call.
3. **CSR incidence, only if 1–2 do not suffice.** Flat `offsets`/`targets` arrays (two allocations total, built
   once) — *not* `Vec<Vec<usize>>`. This is the "work-proportional-to-incidence" primitive the reject tried,
   with the allocation profile that sank it removed.

Do 1 first. It is the smallest change, it targets the measured innermost cost, and it is
**output-identical by construction**: `node_rank[n]` must equal `ranks.get(&n).copied().unwrap_or(0)`, and the
barycenter (integer position sum ÷ neighbor count) and the downstream stable sort are untouched.

## Blocker: there is no bench that exercises this function

- `pipeline_bench::layout_wide` — Tree path. **0.000%** barycenter. Useless for this lever (and the reason four
  rejections are void).
- `fm-layout/benches/crossing_minimization.rs` — benches `crossing_min/{sparse_dag,dense_dag,bipartite}` over
  synthetic `LayerOrdering` / `LayerEdges` structures. It exercises a **different** crossing minimizer, not
  `reorder_rank_by_barycenter` (which takes `&MermaidDiagramIr`). Also useless for this lever.

**Before any code changes, a bench must exist whose input routes to Sugiyama.** `cyclic_scc_100` from
`scripts/headtohead/corpus.mjs` is a known-good generator (cyclic SCC ⇒ `detect_cycle_components` ⇒ Sugiyama).

**Substrate rule for the A/B** (per the `franken_networkx` `br-r37-c1-839yx` addendum, and the correction in
`docs/NEGATIVE_EVIDENCE.md`): `rch exec` has no worker-pinning flag and picks workers non-deterministically, so
ORIG and CAND **must be benched as two arms of one alternating criterion group inside one binary, in a single
`rch exec` invocation** — keep the current `reorder_rank_by_barycenter` as a bench-only reference fn.

## Verdict

**The crossing-minimization vein is NOT closed.** It was never opened: every prior attempt was measured on dead
code. The retry-condition in the do-not-retry note ("a CPU profile that names the actual dominant function") is
now satisfied, and it names `reorder_rank_by_barycenter` at **47.64%** — not Brandes-Köpf (0.74%), not the
ordering *containers*, but the `BTreeMap` probe in the innermost edge rescan.

Filed as a bead. Parked rather than attempted this turn because the honest first step is a correct bench, and
shipping a rushed rewrite of the deterministic ordering path — with no bench that can see it — is exactly the
mistake this document is about.
