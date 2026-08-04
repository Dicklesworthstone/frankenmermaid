# ~~NEGATIVE result: local-delta crossing_refinement = ~0 gain~~ — **MEASUREMENT INVALID** (invalidated 2026-07-10)

> 🚫 **This is the 4th of the four crossing-minimization rejections whose bench never ran the code under test.**
> It was measured on the `layout_wide` / default `layout_diagram` flowchart path, whose inputs (`gen_wide()`)
> route to the **Tree** layout. On those inputs the whole Sugiyama ordering subtree — `reorder_rank_by_barycenter`,
> `total_crossings` — has **0.000%** `perf` self-time.
>
> **Caveat, stated honestly:** `crossing_refinement` itself shows 0.000% *self*-time on every input profiled,
> including the real Sugiyama `cyclic_scc_100`. That is consistent with it being **inlined into
> `layout_diagram_sugiyama_traced_with_config` (11.17%)**, so 0.000% self-time here does NOT prove it never
> runs — unlike the barycenter case, where the callee's own self-time is the direct witness. So: the *bench* was
> wrong, but the headroom is **unquantified**, and the sibling counter `total_crossings` is only 0.810% on the
> Sugiyama input. Treat this as "unmeasured", not "reopened".
>
> The one frame with proven headroom is `reorder_rank_by_barycenter` at **47.640%** self-time on
> `cyclic_scc_100` (`bd-9w78`). Full evidence: `.benchmarks/crossing_min_rejections_benched_dead_code.md`.

**Crate:** `fm-layout` · **Function:** `crossing_refinement` · **Date:** 2026-06-24
**Agent:** frankenmermaid-cc · **Verdict:** ~0 gain (within noise) → **reverted** (stash@{0}).

## The lever

`crossing_refinement` (transpose + sifting, on the default `layout_diagram` path)
recomputes the **whole-graph** crossing count via `total_crossings` for every probe,
even though a single swap/move only changes the two layer pairs around the modified
rank. Replaced the per-probe full recount with a **local-delta** evaluation
(`edges_by_consecutive_pair` index + `crossings_touching_rank`), keeping the running
`best_crossings` in sync by subtracting the local delta. Provably output-identical
(total = local + invariant rest); **426 fm-layout tests pass**.

## Measurement — same-worker A/B (the technique that finally worked)

rch scatters runs across workers (~1.3–2× speed spread) and its criterion baselines
**don't sync between workers**, nor does it reliably sync the 7.7 MB bench binary back
to a reused target dir — so cross-run comparison of sub-5% effects is invalid (an
earlier cross-worker pair even *suggested* a 15–19% win that evaporated under control).
What works: run **both versions inside one `rch exec` session** so they share a worker
and a criterion target dir, swapping code with `git stash` between them:

```
rch exec -- bash -c '
  cargo bench ... --save-baseline xopt          # OPT (working-tree change)
  git stash push -- crates/fm-layout/src/lib.rs # -> orig
  cargo bench ... --baseline xopt               # ORIG vs xopt, same worker
'
```

| `layout_wide` | OPT (median) | ORIG vs OPT change | p |
|---------------|--------------|--------------------|---|
| 8x16  | 820.8 µs | −1.8% (orig faster) | 0.26 (n.s.) |
| 12x24 | 3.947 ms | −1.7% (orig faster) | 0.28 (n.s.) |
| 16x32 | 11.64 ms | +0.1% | 0.95 (n.s.) |

No significant difference; if anything ORIG is marginally faster. Below the ≥3% keep
threshold → reverted.

## Why ~0

`crossing_refinement` is **lightly exercised** on the bench corpus: barycenter +
e-graph ordering already drive crossings low, so `total_crossings`'s per-probe cost
isn't a meaningful share of `layout_diagram`, and the local helper's per-probe BTreeMap
position rebuilds offset what little it saves. The lever would only pay off on inputs
that leave many residual crossings after barycenter (dense, adversarial layered graphs)
— which the current benches don't contain.

## Recommended next target

The dominant per-node cost for large inputs is **parsing** (`parse/flowchart/large_1000`
≈ 7–10 ms — more than layout or render). Node interning already uses BTreeMap (not a
linear scan), so the cost is in tokenization/allocation in the per-line parse loop, not
name resolution. That, not lightly-exercised layout sub-phases, is where the next real
win likely is. See also `.benchmarks/render_cow_capacity_NEGATIVE.md` (allocation cuts
to render were also ~0 — the lesson is to target the actually-dominant work, measured
same-worker).
