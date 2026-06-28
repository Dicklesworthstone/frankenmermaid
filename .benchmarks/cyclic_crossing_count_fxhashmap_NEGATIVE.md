# Negative: BTreeMap→FxHashMap in egraph crossing_count — ~0/slightly-worse, REVERTED

**Crate:** `fm-layout` (`egraph_ordering.rs`) — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** ~0 (likely slightly slower); the maps are too small to benefit. Do not retry.

## The lever (a DIFFERENT primitive on the uncontested cyclic/Sugiyama lane)

The cyclic-graph layout uses e-graph crossing minimization. `crossing_count` (called per sweep per
layer-pair via `local_crossing_count` in the median/transpose ordering) builds **two `BTreeMap<usize,
usize>` position maps every call** (node→position), used only for `.get()`. Swapped both to
`FxHashMap` (non-iterated → byte-identical; the merge-sort inversion count is unchanged).

## Why ~0 (mechanistic — do not retry)

`crossing_count` is only reached for **cyclic** graphs (wide DAGs take the Tree path, never
Sugiyama). Cyclic graphs layer into **small ranks** (a few nodes each), so the position maps are
tiny (n ≈ 2–10). For n that small, `FxHashMap`'s hashing overhead does not beat `BTreeMap`'s handful
of comparisons — the swap is a wash or slightly *worse*. FxHashMap pays off only for large maps
(e.g. the wide-graph edge-pair tracker, 2c09a38), which this code never sees.

## Measurement (also: the cyclic benches are too small to measure under load)

`layout/cyclic` both-order A/B (mermaid-xc1), fleet load ~16. The benches are tiny — cyclic_10
~27 µs, cyclic_50 ~105 µs, cyclic_200 ~few-hundred µs — so the result was **contradictory**
(noise-dominated, per the null-A/B / small-bench calibration d121ee1): cyclic_10 ORDER_A −11.0%
(p=0.00) vs ORDER_B +8.1% (p=0.00); cyclic_50 ORDER_A −8.8% vs ORDER_B +12.3%; cyclic_200 ±neutral.
Absolute medians put OPT slightly *above* ORIG. No direction-consistent signal; mechanistically ~0.
Reverted.

## Takeaway

The cyclic/Sugiyama lane is not a productive target: its benches are too small to measure under any
fleet load, and the obvious data-structure lever (FxHashMap) is null because the per-rank maps are
tiny. The head-to-head vs Mermaid is wide-flowchart-only anyway (`[ratios]` in
mermaid-js-head-to-head.toml) — cyclic is not a gap-vs-mermaid. Parse + layout (Tree path) remain at
their byte-identical floors; the one big remaining win is the render a11y/`data-*` output reduction.
