# bd-1buv.68 — self-time profiles of the new workload classes

**Question.** The standing `bd-1buv.2` frontier blocker says "no unledgered frame reaches 8% self and
no single contained call-chain reaches 10%". That was measured on `flowchart_large_500`. Is it a
statement about frankenmermaid, or about that one workload?

**Instrument.** `crates/fm-cli/examples/headtohead` built through
`RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo build --release -p frankenmermaid-cli
--example headtohead --config profile.release.lto=false --config profile.release.strip=false
--config profile.release.debug=true` (worker `vmi1149989`, 200.0 s). Non-LTO with symbols because
fat LTO misattributes frames in this workspace — see
`docs/NEGATIVE_EVIDENCE.md` on profiling `lto=false/strip=false/debug=true`. That means these
percentages are for **frame discovery**, not for deciding a lever: a non-LTO self-time is not an LTO
wall-time share.

`perf record -F 999`, single pinned core (`taskset -c 40`), one corpus item per run, rep counts
chosen so each run spans several seconds. `sha2::sha256::*` frames are excluded throughout: they are
the harness hashing its own 63 MB debug ELF for the campaign §2.1 self-reported provenance, which
happens once, before any measurement, and is 8× smaller in the shipped 7 MB LTO binary.

## Top self-time frames, by workload

`flowchart_large_500` — the workload every existing ledger percentage was measured on:

| % self | frame |
|---:|---|
| 6.13 | `fm_render_svg::attributes::write_uint_into` |
| 5.37 | `fm_render_svg::attributes::write_fixed2` |
| 4.62 | `fm_layout::build_edge_paths_with_orientation` |
| 3.47 | `IrBuilder::intern_node_auto_normalized` |
| 3.02 | `fm_render_svg::render_nodes_serial` |
| 2.74 | `parse_fast_simple_flowchart_node_borrowed` |
| 2.72 | `write_escaped_attr` |
| 2.70 | `parse_flowchart_document_items` |
| 2.65 | `fm_layout::layout_diagram_tree_traced` |
| 2.63 | `lower_flow_document_item` |

Nothing reaches 8%. The blocker is correct here.

`flowchart_xl_5000` — same shape, ten times the nodes:

| % self | frame | at n=500 |
|---:|---|---|
| **8.72** | `lower_flow_document_item` | 2.63% — **3.3× larger share** |
| 5.49 | `parse_fast_simple_flowchart_edge_parts` | 2.49% |
| 4.32 | `fm_layout::layout_diagram_tree_traced` | 2.65% |
| 4.01 | `IrBuilder::intern_node_auto_normalized` | 3.47% |
| 3.87 | `write_uint_into` | 6.13% — **shrinks** |
| 3.82 | `build_edge_paths_with_orientation` | 4.62% |
| 3.38 | `build_tree_layout_structure` | 2.48% |

The distribution rotates with scale: render number-formatting recedes, parse lowering advances past
the admission threshold. Same code, same diagram type — different workload, different frontier.

`arch_100x50` — 5,000 nodes in 100 subgraphs:

| % self | frame | at n=500 |
|---:|---|---|
| **7.19 / 8.73** | `FxHashSet<(usize, IrNodeId)>::insert` | **absent** |
| 5.53 | `build_edge_paths_with_orientation` | 4.62% |
| 4.26 | `parse_flowchart_document_items` | 2.70% |
| 4.06 | `lower_flow_document_item` | 2.63% |
| 3.57 | `write_uint_into` | 6.13% |

The top frame does not appear in any flowchart profile at any scale. It is the dedup set behind
`add_node_to_cluster` / `add_node_to_subgraph`, and **no benchmark in this repo has ever routed
through it at scale** — the only subgraph fixture, `flowchart_subgraph.mmd`, has four nodes. Lever:
`bd-1buv.69`.

`er_schema_1000x6` — 1,000 entities with attribute blocks:

| % self | frame |
|---:|---|
| **9.37** | `parse_mermaid_with_detection_and_config` |
| 8.31 | `write_uint_into` |
| 4.69 | `_mi_memcpy` |
| 4.53 | `write_fixed2` |
| 4.00 | `fm_render_svg::write_er_entity_into` |
| 3.13 | `IrBuilder::add_entity_attribute` |

`doc_build_40` — 40 small diagrams across five types, one batch:

| % self | frame |
|---:|---|
| **20.02** | `fm_render_svg::render_svg_with_layout` |
| **9.00** | `memchr…packedpair::Finder::find_impl` (theme-CSS post-pass) |
| 4.83 | `__memmove_avx_unaligned_erms` |
| 2.29 | `layout_diagram_sugiyama_traced_with_config` |

This is the per-render **fixed** cost: the theme `<style>` strip + minify pass, paid once per
diagram regardless of diagram size, which a 40-diagram batch pays 40 times. Together with the
`memmove` it is roughly a third of the profile. The ledger already identifies this as "the top
unmined frame … ~20% of SMALL non-flowchart renders" and `bd-dh1c` already proposes memoizing the
strip decision. What was missing was never the idea — it was a workload in which small
non-flowchart renders are what gets measured.

## Result

Four workloads, five frames at or above the 8% admission threshold, four of which are invisible in
the baseline profile. The blocker's claim is true of `flowchart_large_500` and false of
frankenmermaid. Per `docs/LEDGER_RESURRECTION.md` §5 this is the third scope of the same void
predicate the repo has now hit three times: the benchmark did not exercise the code under test —
first wrong algorithm, then wrong input property, now wrong workload scale.

**Caveat, stated plainly.** These are non-LTO discovery profiles on a box at load ~12. They admit a
frame for investigation; they do not decide a lever. Any lever built on one of these rows still owes
byte-identity, an A/A null control, and a same-worker A/B under the campaign §2 contract.
