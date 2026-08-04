# VEIN ANALYSIS (no clean lever): flowchart parse+layout hot path at safe floor (2026-07-23)

Agent: CopperCliff (cc). Non-LTO profiling binary (`profile.bench.lto=false/strip=false/debug=true`),
`pipeline_bench` on `gen_flowchart`/`gen_cyclic`. This is analysis, not a measured REJECT — no candidate
was built; every hot frame was profile-ranked and read to determine it is mined or load-bearing.

## parse/flowchart/large_1000 top self

| frame | self | status |
|---|---|---|
| `lower_flow_document_item` | 9.5% | mined (in_groups gate, moved label into interner, span_for inlined) |
| `parse_flowchart_document_items` | 9.2% | mined (byte-scan dispatch) |
| `intern_node_auto_normalized` | 7.5% | numeric-ID-adjacent — `normalized_id.to_string()` is the node-id representation QuietHarbor's numeric-index vein just closed 3× (do-not-retry) |
| `parse_fast_simple_flowchart_edge_parts` / `_node_borrowed` | 6.8 / 5.7% | mined fast path |
| `ByteLines::next` | 5.1% | mined (memchr line split) |
| `NodeIdIndex::get_with_hash` | 4.8% | **closed vein** (numeric-index, 3 rejects `bd_1buv_2_numeric_*_NEGATIVE`) |

## layout/flowchart/large_500 (Tree) + layout/cyclic/cyclic_200 (Sugiyama) top self

| frame | self | status |
|---|---|---|
| `build_edge_paths_with_orientation` | 17.4 / 11.5% | heavily mined (obstacle-index gating x3 disjuncts, presize, parallel-edge skip, FAR_AWAY endpoint parking) |
| `layout_diagram_tree_traced` | 11.3 / 8.2% | multiple distinct O(n) passes (span sizes/memo/centers/depth), each load-bearing; Vec-alloc elisions mimalloc-wash |
| `ObstacleSpatialIndex::query_segment` | 8.3 / 11.1% | mined (bd-1buv.57 field-bind; adaptive cell size 1f8a35c) |
| `build_tree_layout_structure` | 9.1 / 6.0% | mined (bd-1buv.62/.63 sort/dedup skips) |
| `GraphMetrics::from_ir` + `count_back_edges` | 4.9 + 4.5% | **LOAD-BEARING**: `general_graph_posterior_permille` reads `back_edge_count` with GRADUATED weights (`.min(6)*45`, `.min(8)*18`, `<=5`) + `scc_count` + `max_scc_size` — the exact count drives Bayesian algorithm selection, cannot short-circuit or boolean-ise. DAG SCC-skip already landed. |
| `detect_cycle_components` / `strong_connect` | 5.4 / 4.0% | Tarjan, load-bearing for cyclic posterior |
| `simplify_polyline` | 4.4% | mined (in-place write-cursor compaction, no 2nd Vec) |
| `find_obstacle_nudge_y` | 3.9% | mined (obstacle routing) |

## Levers considered and ruled out (this session, no build)

1. **Hoist `resolved_edges` across dispatch+cycle-removal phases** — profile shows `resolved_edges` self-time
   is NOT a top frame (cost is in its consumers `count_back_edges`/`detect_cycle_components`, which each need
   their own derived structure). Hoisting the resolution alone saves ~nothing. Rejected pre-build.
2. **Short-circuit `count_back_edges` when `edge_count != node_count-1`** — impossible: the exact
   `back_edge_count` (not the tree-like boolean) feeds the posterior. Rejected pre-build.
3. **`span_memo: Vec<Option<f32>>` → `Vec<f32>` sentinel (elide one alloc+copy in tree layout)** — a Vec-alloc
   removal, which mimalloc-washes per substrate rules (`project_layout_stable_priorities_hoist_and_mimalloc_profile`).
   Not worth measuring.
4. **Skip `dependency_topology_equal` re-check in the size-stable fast path** — prior do-not-retry shape
   (`incremental_topology_stable_cache_hit_patch_NEGATIVE`), measured wash. Not retried.

## Conclusion / next veins

The flowchart parse+layout hot path is at its safe single-lever floor. The one identified large remaining
lever is architectural, on the incremental side: **the per-pass full-IR `Arc::new(ir.clone())` snapshot**
(16.5% of the now-fast size-stable incremental path, per `incremental_size_stable_region_memo_WIN`) —
removable only via an **edit-session API** (bd-9rq7 item d) that lets a long-lived caller hand the engine
ownership/Arc of the IR instead of a `&ref` it must clone each pass. That is a multi-crate public-API change
(fm-wasm/fm-cli/tests) warranting a dedicated design pass, not a micro-lever.
