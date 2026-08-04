# bd-1buv.2 flowchart line-table streaming — REJECT (2026-07-22)

## Profile and prior-ledger boundary

A same-day symbolized `flowchart 5000 1800 parse` profile put
`parse_flowchart_document_items` at **9.88% self** and `ByteLines::next` at **5.37% self**.
The corresponding full `wide 1024 3000 pipeline` profile put them at **3.24%** and **1.23%**
self. Before editing, the negative ledger and recent parser history were searched for line-table,
`ByteLines`, collection, and streaming variants. The prior line-`Vec` presize reject was closed
because its extra newline-count scan cost more than realloc growth; its explicit exception was when
the count is already known. That predicate now held because the top-level parser already computes
`input_lines` once.

The candidate therefore tested a distinct stronger primitive: remove the transient
`Vec<(usize, &str)>` completely, stream the existing `ByteLines` iterator through recursive
subgraph parsing, and reuse the already-known line count to retain the landed exact item-vector
capacity. It did not change token parsing, lowering, warning order, IR ordering, or layout/rendering.

## Isomorphism and conformance

- The line number remained `enumerate() + 1`, and the recursive parser consumed the same iterator
  in the same depth-first order; `next_index` advanced once per consumed line exactly as before.
- The `FlowDocumentItem` stream, warning insertion order, subgraph termination, and capacity-only
  item reservation were unchanged.
- Across every `A/B/null/B/A` arm, exact SVG SHA-256 was
  `408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21` for
  `flowchart_large_500` and
  `30d79510dbc4590b6346742560acc6d2af20b2439f166adc58a93d2529681fce` for
  `wide_16x32`; default/lean sizes were respectively 343,946/232,778 and
  534,365/370,609 bytes.
- The strict-remote release parser suite executed 416 tests: **415 passed**. The sole failure,
  `flowchart_parses_chained_edges_left_to_right`, is pre-existing byte-for-byte at `HEAD`: it
  incorrectly requires ordinary flowchart nodes to carry `sankey-node`. The candidate does not
  touch node classes, and this peer-owned baseline defect was not modified.
- After measurement the candidate was manually removed; parser source SHA-256
  `1f5576af8d1533dc778851f54ea64a1cd10c340514a9e0324d19cc19b5d78153` matches `HEAD` exactly.

## Interleaved pinned A/B/null result

Both release binaries were compiled fail-closed through RCH; CAND was built on `ovh-a`.
ORIG SHA-256 was
`3e8badb7cac03a44cc030b7732c82f5b715c414835262de0b8279da30c33aac4`; CAND was
`9b8229e2d67c0d626d6c7a45c188b5f6a9f15a04b95e6e1f2b52c8ea81b807d1`.
The pinned `scripts/headtohead/run.mjs` corpus ran on the same host and CPU0 at 10x Rust
repetitions in `A/B/null/B/A` order.

| item / arm | p50 ns | min ns | CV | MAD |
|---|---:|---:|---:|---:|
| flow A1 | 345,828 | 333,530 | 431.07% | 0.98% |
| flow B1 | 347,302 | 342,527 | 457.94% | 0.36% |
| flow null (A) | 339,414 | 332,795 | 889.19% | 1.44% |
| flow B2 | 350,610 | 343,232 | 717.62% | 0.97% |
| flow A2 | 373,347 | 331,247 | 566.56% | 6.63% |
| wide A1 | 589,574 | 564,693 | 238.43% | 3.15% |
| wide B1 | 588,458 | 575,671 | 400.01% | 1.44% |
| wide null (A) | 573,637 | 563,060 | 298.38% | 1.19% |
| wide B2 | 587,850 | 578,329 | 212.34% | 0.77% |
| wide A2 | 576,529 | 564,386 | 12.32% | 1.20% |

The baseline/null median p50 is 345,828 ns for flow and 576,529 ns for wide. Candidate midpoint
p50 is 348,956 ns and 588,154 ns respectively: **+0.90% slower** on flow and **+2.02% slower**
on wide. Thus the direction is unfavorable on both target shapes and cannot approach the 3% keep
gate. Extreme one-sided outliers also make every arm fail the mandatory CV-under-5% KEEP gate;
the low MADs show why the robust p50 remains useful for this negative verdict, but no positive claim
is inferred from this run.

## Verdict and retry predicate

**REJECT.** Removing only the transient flowchart line table is closed: even with the prior
count-scan objection eliminated, iterator state and recursive generic code offset the saved allocation
and traversal. Do not retry line-table-only streaming. Reopen only as part of a materially different
single-pass parse-and-lower design that also eliminates `Vec<FlowDocumentItem>` and only after a fresh
full-pipeline profile attributes at least **8% self** to document materialization. Before scoring, require
a same-host null arm with CV below 5%; then require direction-consistent at least 3% improvement on both
pinned large-flowchart shapes plus exact IR/SVG hashes and a green baseline/candidate conformance suite.
