# Perf win: FxHashMap for parser lookup maps (parse +6–14%)

**Crate:** `fm-parser` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — parse +5.7% (large) / +13.6% (medium), output-identical.

## What changed

`IrBuilder`'s uniqueness lookups were `BTreeMap`s keyed by `String` /
`(String, Vec<IrLabelSegment>)`:
`node_index_by_id`, `cluster_index_by_key`, `subgraph_index_by_key`,
`label_index_by_text`. Every edge endpoint resolves a node by id (O(log n)
string comparisons), and every node label is interned (lookup + insert with a
costly composite key). A probe (skip label dedup) showed interning alone is ~4–6%
of parse; node-id resolution adds more.

Switched all four to `rustc_hash::FxHashMap` (already a workspace dep). They are
**read by key only — never iterated** — so IR output order is unchanged
(it comes from the `ir.*` vectors), making this fully deterministic.

## Correctness

All **402 fm-parser tests pass** (parse output unchanged). Conformance GREEN;
clippy clean.

## Measurement — same-worker A/B (stash-swap, measurement-time 3)

| bench | FxHashMap faster by | p |
|-------|---------------------|---|
| `parse/flowchart/medium_100` | **+13.6%** | <0.05 |
| `parse/flowchart/small_10`   | **+13.5%** | <0.05 |
| `parse/flowchart/large_1000` | **+5.7%** | <0.05 |
| `full_pipeline/large_500`    | +3.8% (borderline) | 0.05 |

The classic swiss-tables lever: `BTreeMap`'s O(log n) per-lookup string comparisons
on the hot node-id-resolution and label-interning paths were a real cost; FxHashMap's
O(1) hashing removes most of it. Parse is a smaller share of the full pipeline after
the layout/render wins, so the pipeline-level effect is diluted but still positive.
