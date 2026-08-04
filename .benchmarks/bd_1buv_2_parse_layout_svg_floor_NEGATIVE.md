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

## Revalidation after explicit continuation (2026-07-24)

The user explicitly reopened the measured-frontier cycle after the three node-metadata REJECTs.
Current `main` was `8c2cd60b`; the only intervening production-source commit since the original
profile was cc's incremental `Arc<DiagramLayout>` change. Production source had no worktree diff.

The symbolized `profharness` binary was rebuilt fail-closed through RCH on `vmi1149989` with the
same release optimization, frame-pointer, debug-symbol, and no-LTO attribution settings used above.
Its SHA-256 was
`a09ee7ad30c9e30412110bae3c7474e30bb8ba9f1cb453383a9c26bca7565a58`.
`taskset -c 3 perf record -F 999 --call-graph fp` captured 4,407 cycle samples with zero lost
across 12,000 complete `flowchart_large_500` parse-layout-SVG iterations. The median was 365,372 ns.

The ranked current self profile still did not admit a lever:

| Self | Frame | Existing classification |
|---:|---|---|
| 5.86% | `attributes::write_uint_into<String>` | landed digit writer; alternate inlining/itoa shapes rejected |
| 5.21% | `attributes::write_fixed2<String>` | landed exact writer; pair-LUT rejected |
| 4.45% | `build_edge_paths_with_orientation` | mined routing family |
| 3.67% | `render_nodes_serial` | direct output streaming |
| 3.59% | `write_common_node_fragment_into<true>` | output-byte production; fresh metadata siblings rejected |
| 3.58% | `attributes::write_escaped_text<String>` | clean bulk path landed; scalar alternatives rejected |
| 3.05% | `IrBuilder::intern_node_auto_normalized` | numeric-index family closed |
| 2.90% | `layout_diagram_tree_traced` | load-bearing O(n) passes; incremental ownership is cc's lane |
| 2.79% | `ObstacleSpatialIndex::query_segment` | mined obstacle-index family |

No unledgered frame reached the 8% self admission gate and no single contained call-chain reached
10%. Combining the formatter, escape, routing, and node-streaming rows would be a multi-lever
rewrite and would violate the measured-frontier and negative-evidence constraints.

A separate production `headtohead` binary was built remotely on `hz1`; SHA-256
`899bf24ef887d4cf0c13b3e3809dcf1507b61b1424a82937f46cfe59e31df236`.
The final doubled-repetition pinned row used the exact 15,060-byte corpus input and passed the
user's stricter dual-CV gate:

- frankenmermaid: 351,977 ns p50, 347,526 ns min, CV 2.34%, MAD 0.53%, 100 samples;
- mermaid-js 11.15.0: 1.1896 s p50, 1.1482 s min, CV 4.95%, MAD 2.09%, 14 samples;
- dominance: **3,379.77x by p50**, **3,303.93x by min**;
- exact frankenmermaid SVG: 343,946 bytes, SHA-256
  `408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21`.

The first two same-binary attempts had frankenmermaid CV 5.64% and 5.44% and were rejected as
inadmissible under the explicit CV `<5%` rule. The final row, not either retry, supports the current
dominance statement.

**Verdict: BLOCKER / NEGATIVE.** No production source lever is admitted and no source change
ships. Resume only when one of the existing retry predicates above becomes true. In particular,
ordinary timing drift or another sub-8% microframe is not sufficient to retry a dated family.
