# KEPT: incremental engine per-pass cache install clones elided (mem::take + Arc IR) (2026-07-22)

Agent: cc (CopperCliff)
Base (ORIG): `4de7ffc0`
Lane: bd-12e / bd-9rq7 — `IncrementalLayoutEngine` per-pass state install

## Ledger-first routing

- bd-9rq7 frame map, item (2): the engine clones `node_size_cache` (FxHashMap with owned String
  keys) AND `dependency_graph_cache` (containing a FULL `MermaidDiagramIr`) into the thread-local
  `IncrementalCacheState` on EVERY cache-miss pass, then moves both back at pass end.
- NOT the rejected topology-stable shape (`incremental_topology_stable_cache_hit_patch_NEGATIVE.md`
  covers: skipping `track_dependency_graph_query`, synthetic hit recording, cached-IR in-place
  patching — none touched here). Not the rejected span packing.
- July-10 profile context: `MermaidDiagramIr::clone` 4.93% + allocator family ~22% with the hash
  family still present; after 211c5ced removed the 31% hash block those proportions grew.

## Lever (one shape: eliminate the engine-boundary install copies)

1. `node_size_cache: self.node_size_cache.clone()` → `std::mem::take(&mut self.node_size_cache)`.
   Safe: all in-pass access goes through the installed thread-local (`compute_node_sizes` reads
   `state.node_size_cache`), nothing reads `self.node_size_cache` mid-pass, and every exit path
   writes the state's map back onto `self`.
2. `CachedDependencyGraph.ir: MermaidDiagramIr` → `Arc<MermaidDiagramIr>`. The per-pass
   `self.dependency_graph_cache.clone()` collapses from a deep IR copy to refcount bumps. The two
   snapshot-store sites deep-clone into a fresh `Arc::new(ir.clone())` exactly as before
   (unchanged cost there); every reader coerces through `Deref`. Same values, shared ownership —
   behavior-identical by construction.

## Fresh profile-first evidence (non-LTO, debug symbols, fp call-graph)

Base (4de7ffc0) `single_node_label_edit/incremental/1000` top frames: `_int_malloc` 10.60%
(2.69% of it under `MermaidDiagramIr::clone`), `dependency_graph_cache_key` 9.62%,
`MermaidDiagramIr::clone` 7.74%, `malloc_consolidate` 5.90%, `hash_endpoint_value` 5.35%,
`graph_metrics_cache_key` 3.98%, `build_edge_paths_with_orientation` 3.76%.

Post-lever same row: `dependency_graph_cache_key` 11.87%, `_int_malloc` 8.98%,
`hash_endpoint_value` 6.28%, `MermaidDiagramIr::clone` 5.96% (the REMAINING snapshot-store
clones), `build_edge_paths_with_orientation` 4.77%, `malloc_consolidate` 4.67%. The clone/alloc
block shrank in absolute terms (ratios of the smaller total); the frame-diff matches the lever's
mechanism exactly.

## A/B (C/O/O/C interleaved, canonical release profile, same host, null rows)

ORIG = `git archive 4de7ffc0`, CAND = working tree; identical flags
(`--sample-size 30 --measurement-time 5 --warm-up-time 1 --noplot --discard-baseline`).

| Row | ORIG means (O1/O2) | CAND means (C1/C2) | Delta |
| --- | ---: | ---: | ---: |
| single_node_label_edit/incremental/1000 | 793.99 / 791.65 µs | 613.56 / 643.29 µs | **−20.7%** |
| five_node_cluster_edit/incremental/1000 | 787.03 / 787.74 µs | 656.43 / 657.74 µs | **−16.6%** |
| single_node_label_edit/full_recompute/1000 (NULL) | 188.61 / 186.57 µs | 186.63 / 186.48 µs | −0.5% (flat) |
| five_node_cluster_edit/full_recompute/1000 (NULL) | 193.76 / 187.31 µs | 185.96 / 187.26 µs | −2.1% (flat) |

Arm-to-arm point-estimate spreads: ORIG ≤0.3%, CAND single 4.7% (worst; still under the 5% gate),
CAND five 0.2%. Cross-session note: this ORIG (792.8 µs avg) differs from the previous session's
post-211c5ced CAND (764.2 µs) by ~4% — fresh target dirs / binary layout / load; that is exactly
the known wall-noise floor and why only same-pair interleaved deltas are scored.

## Allocator caveat

The bench harness has no mimalloc global allocator, so allocation-elision is overstated vs the
mimalloc CLI (~2.4x per the A/B substrate rules). The production consumer of this engine is
fm-wasm (dlmalloc, libc-like), where the elision is representative. Wall gate applied with this
bias acknowledged.

## Verdict

**KEPT.** −20.7% / −16.6% on the decision rows, null-controlled, ~6x above the 3% keep gate even
before any allocator-bias deflation. 439 fm-layout tests green; workspace fmt + clippy green.

## Next (measured)

The fresh profile makes `dependency_graph_cache_key` + `hash_endpoint_value` (~18% combined) the
top admissible block — the per-pass topology re-hash at `track_dependency_graph_query`. The
topology-stable rejection's OWN retry predicate now holds ("query/clone path top-2 after edge path
construction and node-size recomputation are separately controlled"): edge paths sit at 4.77% and
node-size recompute has left the top frames. The admissible next shape is NOT the rejected
skip/synthetic-hit/IR-patch triple — it is eliding the redundant key recompute when the caller's
already-computed edit set proves topology stability, keeping the query and its recording intact.
