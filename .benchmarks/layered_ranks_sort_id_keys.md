# Perf win: cache-friendly id-key extraction for the Sugiyama `layered_ranks` sorts

**Crate:** `fm-layout` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — cyclic/Sugiyama layout ~7–13% faster; byte-identical. Same lever as
`0c370f3` (tree path), applied to the second layout path.

## What changed

`layered_ranks` (Sugiyama rank assignment, exercised by cyclic graphs and general-graph
flowcharts that don't route to the tidy-tree path) sorts each node's neighbor list and the full
node list by node id via `compare_node_indices(ir, l, r)` = `ir.nodes[l].id.cmp(&ir.nodes[r].id)`
— a random-index deref into the large `Vec<IrNode>` struct array (a cache miss per comparison).
Pre-extracted the keys into a contiguous `Vec<&str>` and sorted with a `cmp_by_id` closure over
that, exactly as in `build_tree_layout_structure`.

## Correctness

Byte-identical: `ids[i]` is exactly `ir.nodes[i].id`, so both sorts produce the identical order.
428 fm-layout unit tests + doc tests + `frankentui_conformance_test` pass.

## Measurement

Same-worker both-order stash-swap A/B, fresh dir `mermaid-bt7`, `layout/cyclic`, mt=4.

| bench | OPT abs | ORIG abs | ORDER_A (ORIG vs opt) | geo-mean OPT/ORIG |
|---|---:|---:|---:|---:|
| `layout/cyclic/cyclic_10`  | 23.9 µs  | 29.3 µs  | +36.8% (p=0.00) | ~0.87 (**~13% faster**) |
| `layout/cyclic/cyclic_50`  | 110.2 µs | 139.4 µs | +20.2% (p=0.00) | ~0.89 (**~11% faster**) |
| `layout/cyclic/cyclic_200` | 227.0 µs | 237.8 µs | +12.9% (p=0.00) | ~0.93 (**~7% faster**) |

Direction-consistent OPT-faster on the absolute within-phase medians (~5–21%) and ORDER_A (all
sizes, p=0.00). The lone ORDER_B cyclic_10 +3.5% is load drift on a ~24 µs bench (later runs were
slower as the shared box loaded up). The win shrinks with size because `crossing_minimization`
takes a larger share of Sugiyama layout as graphs grow, so `layered_ranks` is a smaller fraction
at cyclic_200.

## Mermaid.js head-to-head

Layout-stage win on the Sugiyama path (cyclic graphs / non-tidy-tree flowcharts), complementing
the five wide-flowchart Tree-path wins. The wide head-to-head uses the Tree path; this covers the
cyclic workload (`layout/cyclic` mermaid-js comparisons in the ledger).

## Pattern (re-applied)

Confirms the `0c370f3` hunt pattern: a `sort_by` comparator that derefs a field of a
`Vec<BigStruct>` via random index should pre-extract keys into a flat `Vec<&K>`. `layered_ranks`
was the second hot occurrence; the remaining `compare_node_indices` sorts live in non-flowchart
diagram layouts (timeline/gantt/sankey/kanban/grid) not on the benched hot paths.
