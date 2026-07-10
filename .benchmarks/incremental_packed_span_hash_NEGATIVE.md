# REJECTED: word-packed incremental span hashing (2026-07-10)

Agent: `cod_fm`

Base: `830d672` (the `fm-layout` source is identical to the ORIG bench binary's `fbd150d` base)

Lane: `fm-layout` crossing minimization and incremental edit rerender (`bd-1buv.3`)

## Ledger-first routing

The ledger was searched before the lever was designed. The dominant crossing frame belongs to the
already-closed crossing-count container family: the landed cyclic crossing-count keep and the rejected dense
position-map/Fenwick, flat-array `total_crossings`, and FxHashMap variants. Thresholded barycenter accumulation
is also already kept, and precomputed barycenter adjacency is rejected. The topology-stable dependency-cache
patch and endpoint Debug-string allocation are separately closed. Render double-copy, class dense insertion,
class relationship fusion, polygon streaming, and pie presizing are outside this lane and were not retried.

The next admissible profile frame was `hash_span_value`, so this attempt tested a different primitive:
injectively word-pack six `u32` span fields into three `u64` words before feeding the existing deterministic
hash mixer.

## Profile method

There is no `release-perf` profile in this repository. The canonical native profiling/benchmark profile is
`--profile release`; `Cargo.toml` gives `fm-layout` the required `opt-level = 3` override. Symbol-preserving
bench executables were built through RCH, then recorded with `perf record -e cycles:u` on the exact Criterion
rows. The tables below include every frame with at least 0.10% flat self time; there were zero lost samples.

### `crossing_min/dense_dag/egraph/20` ranked frames

| Self | Frame |
| ---: | --- |
| 51.37% | `egraph_ordering::crossing_count` |
| 5.98% | `egg::EGraph::add_uncanonical` |
| 5.56% | `egg::Machine::run` |
| 3.79% | `hashbrown::HashMap<OrderingLang, Id>::get` |
| 3.01% | `_int_malloc` |
| 2.64% | `egg::EGraph::perform_union` |
| 2.10% | `hashbrown::HashMap<OrderingLang, Id>::insert` |
| 1.57% | `egg::EGraph::rebuild_classes` |
| 1.54% | `cfree` |
| 1.53% | `egg::pattern::apply_pat` |
| 1.41% | `__libc_calloc` |
| 1.19% | `_int_free_merge_chunk` |
| 1.08% | `egg::EGraph::index` |
| 1.03% | `egg::EGraph::rebuild_classes::{closure#2}` |
| 0.97% | stable quicksort of candidate `LayerOrdering`s |
| 0.92% | `unlink_chunk` |
| 0.91% | `egraph_ordering::median_position` |
| 0.88% | `malloc` |
| 0.81% | `__memmove_avx_unaligned_erms` |
| 0.79% | `malloc_consolidate` |
| 0.61% | `_int_realloc` |
| 0.60% | `_int_free_chunk` |
| 0.58% | `__memset_avx2_unaligned_erms` |
| 0.56% | `realloc` |
| 0.56% | `egraph_crossing::saturate_layer` |
| 0.55% | `egg::Subst::get` |
| 0.54% | `RawTable<(OrderingLang, Id)>::reserve_rehash` |
| 0.53% | unresolved kernel frame `0xffffffffb0c1b2b7` |
| 0.51% | `egg::EGraph::process_unions` |
| 0.43% | `SmallVec<(Var, Id)>::deref` |
| 0.42% | `Pattern::apply_matches` |
| 0.40% | `OrderingLang` insertion sort |
| 0.38% | `RawVecInner::finish_grow` |
| 0.26% | `DefaultHasher::write` |
| 0.25% | e-class `RawTable::reserve_rehash` |
| 0.25% | `RawVecInner::grow_amortized` |
| 0.22% | `Pattern::search_with_limit` |
| 0.21% | `__libc_calloc2` |
| 0.20% | drop `Vec<SearchMatches>` |
| 0.19% | `__memcmp_avx2_movbe` |
| 0.19% | `Pattern::search_eclass_with_limit` |
| 0.18% | `RawVecInner::finish_grow` (second monomorphization) |
| 0.16% | `_int_free_maybe_consolidate` |
| 0.15% | stable `sort4` of candidate `LayerOrdering`s |
| 0.13% | `RawTable<(Id, ())>::reserve_rehash` |
| 0.12% | `BackoffScheduler::search_rewrite` |
| 0.11% | `RawVec<Subst>::grow_one` |
| 0.11% | `__libc_malloc2` |
| 0.11% | `RawVecInner::reserve::do_reserve_and_handle` |
| 0.10% | `egraph_ordering::optimize_layer_ordering` |

The 51.37% leader maps directly to the closed crossing-count family, so it was not retried. The next
non-closed mechanism came from the incremental profile.

### `single_node_label_edit/incremental/1000` ranked frames

| Self | Frame |
| ---: | --- |
| 9.58% | `hash_span_value` |
| 8.77% | `hash_endpoint_value` |
| 8.20% | `IncrementalLayoutEngine::layout_diagram_traced_with_config_and_guardrails` |
| 7.56% | `dependency_graph_cache_key` |
| 7.31% | `_int_malloc` |
| 5.48% | `compute_node_sizes` |
| 4.93% | `MermaidDiagramIr::clone` |
| 4.14% | `malloc_consolidate` |
| 4.06% | `__memcmp_avx2_movbe` |
| 3.35% | `build_edge_paths_with_orientation` |
| 2.82% | `graph_metrics_cache_key` |
| 2.70% | `cfree` |
| 2.59% | `unlink_chunk` |
| 2.35% | `hash_label_ref` |
| 2.12% | `ObstacleSpatialIndex::query_segment` |
| 2.05% | `compute_traced_layout_with_config_and_guardrails` |
| 1.98% | `malloc` |
| 1.73% | `build_tree_layout_structure` |
| 1.73% | `_int_free_chunk` |
| 1.68% | `__memmove_avx_unaligned_erms` |
| 0.98% | `derive_layout_edits` |
| 0.90% | `TracedLayout::clone` |
| 0.79% | `floorf` |
| 0.77% | `find_obstacle_nudge_x` |
| 0.71% | drop `Option<Box<IrInlineStyle>>` |
| 0.67% | `__libc_malloc2` |
| 0.66% | clone `BTreeMap<String, CachedNodeSize>` subtree |
| 0.50% | `simplify_polyline` |
| 0.44% | `RawVecInner::grow_amortized` |
| 0.44% | stable drift sort in `build_tree_layout_structure` |
| 0.42% | drop `IrNode` |
| 0.40% | `RawVecInner::finish_grow` |
| 0.37% | `rank_orders_from_key` |
| 0.37% | `node_boxes_from_centers` |
| 0.37% | `compute_bounds` |
| 0.36% | `resolved_edges` |
| 0.33% | rank sort closure in `build_tree_layout_structure` |
| 0.32% | `route_edge_points_with_obstacle_index` |
| 0.30% | `BTreeMap<String, CachedNodeSize>::IntoIter::dying_next` |
| 0.29% | drop `Option<Box<IrClassNodeMeta>>` |
| 0.29% | clone `Vec<LayoutNodeBox>` |
| 0.26% | clone `Vec<String>` |
| 0.24% | drop `Option<Box<IrRequirementNodeMeta>>` |
| 0.21% | `_int_free_merge_chunk` |
| 0.20% | drop `Option<Box<IrEdgeExtras>>` |
| 0.15% | drop `DiagramLayout` |
| 0.14% | drop `MermaidDiagramIr` |
| 0.13% | drop `TracedLayout` |
| 0.12% | `BTreeMap<SubgraphRegionId, SetValZST>::Iter::next` |
| 0.12% | drop `MermaidGraphIr` |
| 0.11% | rank sort closure in `rank_orders_from_key` |
| 0.11% | `BTreeSet<usize>::from_sorted_iter` |
| 0.10% | `alloc_perturb` |
| 0.10% | stable quicksort in `build_tree_layout_structure` |
| 0.10% | unresolved kernel frame `0xffffffffb0c1b2b7` |

### Memory-pressure check

Three-repeat `perf stat` totals on the profiled release binaries:

| Row | Instructions | Cycles | IPC | Cache refs | Cache misses | Miss rate | Elapsed CV |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `crossing_min/dense_dag/egraph/20` | 19.919B | 7.980B | 2.50 | 179.746M | 35.301M | 19.64% | 1.44% |
| `single_node_label_edit/incremental/1000` | 20.006B | 8.220B | 2.43 | 645.982M | 66.765M | 10.34% | 0.46% |
| `five_node_cluster_edit/incremental/1000` | 20.156B | 8.408B | 2.40 | 634.962M | 74.630M | 11.75% | 3.98% |

Allocation traffic is material, but it does not dominate incremental layout: explicitly named allocator
frames account for about 22% self time, while the structured fingerprint family (`hash_span_value`,
`hash_endpoint_value`, `dependency_graph_cache_key`, `graph_metrics_cache_key`, and `hash_label_ref`) accounts
for 31.08%. This is why the measured lever targeted the top admissible hash frame instead of assuming that an
arena alone would remove the bottleneck.

## One lever tested

`hash_span_value` originally mixed six zero-extended `u32` fields as six `u64` words. The candidate packed
the fields bijectively into three words:

1. `start.line | start.col << 32`
2. `start.byte | end.line << 32`
3. `end.col | end.byte << 32`

It then called the unchanged deterministic mixer three times. A focused guard mutated each of the six fields
independently and proved each mutation changed the fingerprint. That RCH test passed (1 passed, 433 filtered),
and `cargo fmt --check -p fm-layout` passed while the candidate existed.

## Honest same-worker A/B

ORIG and candidate were separate release executables, both built with the repository's canonical release
profile (`fm-layout` opt-level 3). ORIG was the exact pre-candidate `fm-layout` source at `fbd150d`; `git diff
fbd150d..830d672 -- crates/fm-layout` was empty. Both decision binaries ran on RCH worker `vmi1227854`.

The valid decision row used 100 Criterion samples and a 10-second measurement window:

| Row | ORIG raw mean | ORIG CV | Candidate raw mean | Candidate CV | Delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| `five_node_cluster_edit/incremental/1000` | 1.192554 ms | 4.7905% | 1.182294 ms | 4.6677% | -0.86% |

Both CVs are below 5%, but the 0.86% improvement fails the 3% keep-gate ratchet. Criterion's centers tell the
same story (ORIG 1.2065 ms, candidate 1.1839 ms: 1.87%, still below gate).

For completeness, every other measured attempt is retained rather than cherry-picked:

- Busy local host (load about 27), advisory C/O/O/C centers: candidate-1 single/five 1.1387/1.2418 ms;
  ORIG-1 1.1554/1.1550 ms; ORIG-2 1.2133/1.0989 ms; candidate-2 1.3635/1.1270 ms. Order drift made these
  invalid for scoring.
- Same `vmi1227854` 100-sample run, single-node row: candidate 1.157940 ms, CV 6.1119%; ORIG 1.127692 ms,
  CV 6.7890%. Both fail the dispersion gate.
- Same `vmi1227854`, 30-second single-node follow-ups: candidate 1.160579 ms, CV 4.9400%; ORIG 1.170494 ms,
  CV 6.3780%; second ORIG 1.191528 ms, CV 6.6430%. A later 30-sample candidate interval was
  [1.2586, 1.4316] ms. There is no pair with both sides under 5%, so these are not scored.
- Quiet alternate RCH worker `vmi1293453`, same CPU/core and C/O/C order: first candidate 1.227534 ms,
  CV 7.9427%; ORIG 1.189701 ms, CV 4.5856%; second candidate 1.104255 ms, CV 6.9648%. The large order drift
  and candidate CVs above 5% make this corroboration only. The preceding discard-baseline pass likewise
  produced candidate 1.2334 ms and ORIG 1.1560 ms.

## Verdict and retry condition

**REJECTED.** The code and focused test were manually restored before this evidence commit. Halving mixer
calls inside `hash_span_value` does not clear the keep gate; call/scan/allocation costs around the fingerprint
remain larger than this scalar packing win.

Landing validation: `git diff --check` passed. UBS was run on both changed evidence files; Markdown has no UBS
scanner. The required Rust fallback scan was therefore run on `crates/fm-layout/src/lib.rs`: its cargo-backed
formatting, clippy, check, test-build, audit, and deny gates all passed, while UBS exited 1 on its existing broad
heuristic baseline (for example treating ordinary layout equality as secret comparison). No source diff
remains from this rejected attempt.

Do not retry this three-word span packing (or a rearrangement of the same six fields) on the current
incremental 1000-node rows. A retry condition is either (a) a future profile puts `hash_span_value` above 15%
self time and an Amdahl estimate predicts more than 3%, or (b) a new bit-parallel primitive eliminates a whole
fingerprint scan by maintaining packed mutation/frontier hashes incrementally. The next dig should instead
target a different primitive such as dense arena/CSR dirty-frontier reuse around node sizing or edge routing;
this rejection is not a ceiling.
