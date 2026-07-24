# bd-1buv.2 — current large-flowchart parse-layout-SVG frontier — NEGATIVE

Date: 2026-07-23
Agent: MagentaGull
Base: `b96ae4f94403`
Verdict: **BLOCKER / REJECT — no unmined measured-frontier micro-lever is admitted**

## Scope and stopping condition

This is the measured-frontier lane only. The incremental-computation and public-API/architectural
lanes remain owned by cc and were not touched. The requested stopping condition permits a ledgered
blocker; this row records that blocker from a fresh current-head full-pipeline profile rather than
manufacturing a fourth variant from a family with a standing do-not-retry result.

No production source was edited.

## Ledger and history scan before profiling

The current Git and benchmark history already closes the obvious families:

- `.benchmarks/bd_1buv_2_flowchart_parse_layout_floor_ANALYSIS.md` classifies every large-flowchart
  parser/layout top frame as mined, load-bearing, or covered by the three numeric-index rejects.
- `.benchmarks/render_capacity_preshaping_NEGATIVE.md` classifies large-flowchart SVG as
  number-format/escape/output-byte bound and rejects capacity tuning.
- `.benchmarks/render_write_fixed2_pairs_lut_NEGATIVE.md`,
  `.benchmarks/render_write_int_itoa_NEGATIVE.md`, and the 2026-07-11
  `write_uint_into` cold-split/inline reject close alternate integer/fixed-point writers.
- The 2026-07-11 escape-loop reject and `.benchmarks/write_escaped_text_short_clean_fast_path.md`
  close the short clean-label escape family: the profitable bulk-copy path is already landed and
  the remaining scalar fallback is required.
- Raw-part body fusion (`+22.65%`), between-child guards (flat / `+0.85%`), output-capacity
  tightening, and further node/edge `Element` streaming have dated no-retry rows.

Recent Git history through `b96ae4f9` contains only incremental benchmark/docs work after the
last flowchart parser keep; there is no unledgered parse-layout-SVG candidate in the worktree.

## Fresh current-head profile

The existing `profharness` `flow 500 ... full` generator is byte-for-byte the pinned
`scripts/headtohead/corpus.mjs` `flowchart_large_500` generator: 500 nodes, 499 edges, 15,060 input
bytes. A release-optimized profiling binary was built fail-closed through RCH on `vmi1227854`, with
symbols/frame pointers and LTO disabled only for attribution:

```text
RCH_REQUIRE_REMOTE=1 RCH_FORCE_REMOTE=1 RCH_WORKER=vmi1227854 \
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
  rch exec -- cargo \
    --config 'profile.release.strip=false' \
    --config 'profile.release.debug=true' \
    --config 'profile.release.lto=false' \
    --config 'build.rustflags=["-C","force-frame-pointers=yes"]' \
    build -p frankenmermaid-cli --example profharness --profile release
```

`taskset -c 3 perf record -F 999 --call-graph fp` captured 5,642 cycle samples with zero
lost samples across 12,000 complete parse-layout-SVG iterations. The ranked self profile was:

| Self | Frame | Classification |
|---:|---|---|
| 6.95% | `attributes::write_uint_into<String>` | digit-table/de-recursed fast path landed; cold-split/inlining and itoa variants rejected |
| 4.27% | `attributes::write_fixed2<String>` | exact two-decimal writer landed; pair-LUT variant rejected |
| 3.44% | `layout::build_edge_paths_with_orientation` | mined routing family |
| 3.02% | `render_nodes_serial` | direct streaming path; remaining work is output emission |
| 2.93% | `attributes::write_escaped_attr<String>` | required escaping; clean bulk-copy path already landed |
| 2.82% | `parse_fast_simple_flowchart_edge_parts` | mined fast parser; scan-fusion REJECT |
| 2.77% | `parse_flowchart_document_items` | byte-line/document streaming family closed |
| 2.72% | `IrBuilder::intern_node_auto_normalized` | numeric representation family closed by three REJECTs |
| 2.60% | `attributes::write_escaped_text<String>` | short-clean bulk path landed; scalar restructure REJECT |
| 2.59% | `NodeIdIndex::get_with_hash` | temporal/dense/numeric index family closed |
| 2.57% | `layout_diagram_tree_traced` | load-bearing O(n) passes; allocation-only variants wash under mimalloc |
| 2.52% | `parse_fast_simple_flowchart_node_borrowed` | mined fast parser |
| 2.43% | `render_edges_serial` | direct streaming path; remaining work is output emission |
| 2.33% | `write_common_node_fragment_into<true>` | annotated inherent literal/number/escape byte production |
| 2.20% | `build_tree_layout_structure` | sort/dedup/CSR family mined |
| 2.12% | `lower_flow_document_item` | mined |
| 2.05% | `ObstacleSpatialIndex::query_segment` | adaptive index/gating family mined |

Seven pinned-core `perf stat` repeats independently showed stable work counts despite wall-clock
preemption: instructions CV `0.10%`, branches CV `0.07%`, cycles CV `1.80%`, and cache-misses CV
`2.06%`. There is no new self frame at or above 8%, and every frame above 2% belongs to a landed,
load-bearing, or dated-REJECT family.

## Pinned corpus and dominance

The admissible same-day `scripts/headtohead` release row remains:

- input SHA-256 `7012902b9fdaa3ff2d7a2d0c327eaaea543b347b51155521b86daf7aacd9ec83`;
- frankenmermaid `665,115 ns` p50, CV `3.25%`;
- mermaid-js 11.15.0 `1.3231 s` p50;
- **1,989x** p50 dominance;
- SVG 343,946 bytes.

A freshly RCH-built production-release `headtohead` binary at `b96ae4f9` reproduced the exact
343,946-byte SVG and SHA-256
`408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21`.
Its new `vmi1227854` timing row was deliberately rejected: `445,167 ns` p50, CV `28.57%`, MAD
`7.00%`. It proves current output/path identity, not performance. No timing claim uses that row.

The requested literal Cargo form with `cargo bench ... --release` remains a command-level blocker:
this toolchain rejects `--release` before timing. The supported equivalent is `--profile release`,
which was used for both current builds.

## Why no source lever is admitted

The current maximum SVG frame is 6.95%, and the only plausible alternatives for it have already
been tested and rejected. Combining distinct formatter/escape/streaming frames would be a
multi-lever rewrite and would violate the one-lever and negative-evidence rules. The remaining
large wins require either fewer output bytes/changed numeric fidelity or incremental ownership/API
work; both are contract/architectural changes outside this lane.

Therefore this sweep stops on a **measured, ledgered blocker**, not on an unmeasured hunch and not
because a noisy candidate was mistaken for a result.

## Retry predicate

Reopen `bd-1buv.2` micro-lever work only when at least one of these is true:

1. A fresh pinned `flowchart_large_500` full-pipeline profile on a production-equivalent build
   exposes an **unledgered single frame >=8% self** or a single contained call-chain >=10%.
2. A source change materially alters one of the closed hot families; the new profile must show
   why the dated do-not-retry mechanism no longer applies.
3. The lane owner explicitly admits an output-contract change (fewer bytes/different coordinate
   fidelity) or transfers an architectural/incremental task into this lane.

Any retried candidate still requires a same-binary, same-worker A/B plus reference/reference null,
every scored arm CV `<5%`, null median delta `<1%`, a null-adjusted realistic-case win `>=3%`,
exact IR/SVG identity where the contract is unchanged, and conformance.
