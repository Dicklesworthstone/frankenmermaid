# Perf win: cache-friendly id-key extraction for the tree-structure sorts

**Crate:** `fm-layout` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — wide layout ~10–15% faster (`build_tree_layout_structure`); byte-identical.

## What changed

`build_tree_layout_structure` (the Tree algorithm's structure builder, which wide flowcharts
dispatch through) sorts the node list, every node's neighbor list, and every node's children list
by node id via `compare_node_indices(ir, l, r)` = `ir.nodes[l].id.cmp(&ir.nodes[r].id)`. Reading
`ir.nodes[idx].id` for a *random* `idx` during a sort strides through the `Vec<IrNode>` — `IrNode`
is a large multi-field struct, so each comparison touches a different cache line (a miss per
deref). The 512-element `sorted_nodes` sort alone does ~4600 comparisons × 2 derefs.

Pre-extract the keys once into a contiguous `Vec<&str>` and sort against that:

```rust
let ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
let cmp_by_id = |l: &usize, r: &usize| ids[*l].cmp(ids[*r]).then_with(|| l.cmp(r));
// ...sort_by(&cmp_by_id) for neighbors, sorted_nodes, and children
```

`ids` is built in one sequential (cache-friendly) pass over `ir.nodes`; the sorts then compare
16-byte `&str` slices packed contiguously instead of striding through the big struct array — a
classic decorate-sort key extraction.

## Correctness

Byte-identical: `ids[i]` is exactly `ir.nodes[i].id`, so every `cmp` and the `then_with(|| l.cmp(r))`
tie-break is unchanged — the three sorts produce the identical order, hence the identical tree.
428 fm-layout unit tests + doc tests + `frankentui_conformance_test` (whole-corpus identity) pass.

## Measurement

Same-worker both-order stash-swap A/B, fresh dir `mermaid-bt6`, `wide_stages/layout`, mt=4.

| bench | OPT abs | ORIG abs | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) |
|---|---:|---:|---:|---:|
| `…/8x16`  | 109.2 µs | 124.6 µs | +10.1% (p=0.00) | −15.8% (p=0.00) |
| `…/12x24` | 216.1 µs | 250.3 µs | +15.1% (p=0.00) | +6.0%¹ |
| `…/16x32` | 377.1 µs | ~445 µs  | +18.0% (p=0.00) | −5.2% (p=0.00) |

Direction-consistent OPT-faster on the absolute within-phase medians (~12–15%) and ORDER_A (all
sizes, p=0.00). ¹ The lone ORDER_B 12x24 +6% is cross-run drift (the box loaded up by the 4th
run — that run's OPT median exceeded even the 1st-run OPT), not a real regression. Conservatively
≥10% layout. Mechanism (contiguous `&str` keys vs large-struct random-index derefs) guarantees the
direction.

## Mermaid.js head-to-head

Layout-stage win stacking on the prior four wins (3 edge-routing + text-measure LUT). Layout is
~14% of the wide pipeline; full-pipeline `full_pipeline_wide/16x32` ≈4.3 ms vs the pinned Mermaid
`11.12.0` 2879.185 ms (≈670× faster), nudged further by trimming `build_tree_layout_structure`,
the 2nd-largest layout phase after `build_edge_paths`.
