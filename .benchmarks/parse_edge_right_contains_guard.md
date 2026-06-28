# Perf win: guard the right-side chained-operator scan in the edge fast path (parse ~5-8%)

**Crate:** `fm-parser` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — flowchart/wide parse ~5-8% faster; byte-identical. Stacks on the parse_label
fast path (bbaf088).

## What changed

`parse_fast_simple_flowchart_edge_parts` rejects a chained right side (e.g. `a-->b-->c`, where the
right endpoint itself holds a second operator) by testing `right.contains(op)` for all six
`FAST_OPERATORS` — six substring-search calls on **every** edge (960 at 16x32), even though the
common single-operator edge's right endpoint contains no operator at all. Guarded the six
`contains` behind a single byte scan: every fast operator starts with `-` or `=`, so if `right`
has neither byte, none can match.

```rust
if right.bytes().any(|b| matches!(b, b'-' | b'=')) && FAST_OPERATORS.iter().any(|(op, _)| right.contains(op)) {
    return None;
}
```

## Correctness

Byte-identical: when `right` has no `-`/`=` byte, every `contains` would have returned false anyway
(all six needles begin with `-`/`=`), so the short-circuit changes nothing; chained edges (right
has `-`/`=`) still run the full check. All gating bytes are ASCII. 405 fm-parser tests pass.

## Measurement

Same-worker both-order stash-swap A/B, fresh dir `mermaid-pp4`, `cargo bench -p fm-parser --bench
parse_bench`, mt=4 (fm-parser is highs-sys-free → benches reliably during the cmake/highs-sys pool
outage). ORIG is the committed parse_label fast path (HEAD); OPT adds this guard.

| bench | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | geo-mean OPT/ORIG |
|---|---:|---:|---:|
| `parse/flowchart/medium_100` | +3.5% (p=0.01) | −13.5% (p=0.00) | ~0.91 (**~8.6% faster**) |
| `parse/flowchart/large_1000` | +27%¹ (p=0.00) | −5.7% (p=0.01) | ~0.86 (**~5.7% faster**) |
| `parse/wide/16x32` | +14.8% (p=0.00) | −3.3% (p=0.04) | ~0.92 (**~8% faster**) |
| `parse/wide/8x16`  | +5.9% (p=0.00) | +0.5% (NS) | ~0.97 (~2.6%) |
| `parse/wide/12x24` | −0.6% (NS) | −2.6% (NS) | ~0.98 (~2%) |

¹ large_1000 ORDER_A is a load artifact (CI [+11.5%, +48.6%]); ORDER_B −5.7% is the clean read.
Direction-consistent OPT-faster in both orders at the large sizes (16x32, medium_100, large_1000).
The six `contains` substring-search *calls* per edge (call setup + first-byte memchr ×6) were
costlier than the divide-loop estimate suggested.

## Mermaid.js head-to-head

Second parse win this session (with bbaf088). Parse is ~27% of the wide pipeline; ~8% parse ≈ ~2%
wide pipeline at 16x32, ~8.6% on the sparse `medium_100` flowchart. Byte-identical; full-pipeline
ratio vs pinned Mermaid `11.12.0` improves accordingly.
