# Decision Contract: Swiss Tables for Node/Edge Maps (Section 7.7)

## Graveyard Score: 3.0 / Tier: B

## Hypothesis

Replacing the 41 `BTreeMap`/`BTreeSet` instances in the layout engine with Swiss Table-backed hash maps (via `hashbrown` or `ahash`) will improve lookup throughput by 20%+ and yield a measurable 5%+ end-to-end layout speedup, particularly in the hot-path barycenter reordering loop where position maps are recomputed per rank per sweep.

## Current Baseline

**File:** `crates/fm-layout/src/lib.rs`
**Hot paths:**
- Barycenter reordering (line ~1091): recomputes `adjacent_position: BTreeMap<usize, usize>` for every rank in every sweep
- Crossing count (line ~1166): recomputes `positions_by_rank` and `edges_by_layer_pair` per call
- Cycle cluster mapping: `BTreeMap<usize, Vec<usize>>` for cluster_members

**Data structure profile:**
- 41 uses of BTreeMap/BTreeSet across the layout engine
- All keys are `usize` (node/rank indices)
- BTreeMap provides O(log n) lookups with stable iteration order
- Maps are short-lived: created per function call, not persistent

**Current performance characteristics:**
- BTreeMap<usize, T>: O(log n) lookup, O(n log n) construction
- Determinism guaranteed by BTreeMap's sorted iteration
- No allocation reuse (maps created fresh each call)

**No current benchmark instrumentation** (no criterion crate in dev-dependencies).

## Acceptance Criteria (Adopt)

- [ ] Isolated lookup throughput improved by >= 20% (microbenchmark)
- [ ] End-to-end layout time improved by >= 5% on medium graphs (50-200 nodes)
- [ ] No crossing count or edge length metric regressed (exact same outputs)
- [ ] Determinism preserved via fixed-seed hasher (not RandomState)
- [ ] WASM target compilation verified (`wasm32-unknown-unknown`)
- [ ] Implementation <= 100 LOC of changes (mechanical BTreeMap -> HashMap swap)
- [ ] All existing tests pass with identical LayoutStats outputs

## Rejection Criteria (Reject)

- [ ] End-to-end improvement < 5% (overhead of change not justified)
- [ ] Any LayoutStats field differs between BTreeMap and Swiss Table runs
- [ ] Swiss Table crate introduces `unsafe` in user-facing API
- [ ] WASM compilation fails or produces > 10% larger binary
- [ ] Determinism broken: different outputs with same inputs across runs
- [ ] Memory usage increases by > 20% (hash maps over-allocate)

## Evaluation Protocol

1. **Baseline measurement**
   - Add criterion benchmarks for layout_diagram() on benchmark corpus
   - Record wall-clock time, peak memory, and all LayoutStats fields
   - Profile with `flamegraph` to confirm BTreeMap is in hot path
   - If BTreeMap is NOT in hot path: **early reject** (not worth changing)

2. **Implementation**
   - Replace `BTreeMap<usize, T>` with `HashMap<usize, T, FixedSeedHasher>` in hot paths
   - Use `hashbrown` crate with `ahash` or fixed-seed `FxHasher`
   - Preserve BTreeMap where iteration order matters for determinism
   - Where sorted iteration is needed, collect into Vec and sort

3. **Post-measurement**
   - Same benchmark corpus, same machine, same conditions
   - Assert all LayoutStats fields identical (metric preservation)
   - Record timing improvements

4. **Statistical comparison**
   - Report timing improvement with 95% CI across 100 runs
   - Report memory usage delta
   - Compare WASM binary size before/after

5. **Decision**
   - Adopt if >= 5% end-to-end speedup with identical outputs
   - Reject if profiling shows BTreeMap not in hot path (early exit)
   - Defer if improvement only visible on large graphs (> 500 nodes)

## Benchmark Corpus

- **Small:** 10 nodes, 12 edges (should show negligible difference)
- **Medium:** 100 nodes, 200 edges (target improvement range)
- **Large:** 500 nodes, 1500 edges (maximum expected benefit)
- **Wide:** 20 ranks x 50 nodes per rank (stress barycenter reordering)
- **Deep:** 100 ranks x 5 nodes per rank (stress rank assignment lookups)

## Timeline

- Baseline + profiling: Completed (`bd-2gr9.1`, `bd-2gr9.2`)
- Implementation: Completed (`72ffc1f`, `152d936`, 41 `FxHashMap`/`FxHashSet` sites in `fm-layout`, 20 in `fm-parser`)
- Evaluation: Completed (`bd-6g0v`, `NEGATIVE_EVIDENCE.md:3373/3341/3261/11791`)
- Decision: **RATIFIED — Selective Adoption (Items 1 & 2 Adopted; Items 3 & 4 Rejected on Measured Negative Evidence)** (2026-10-14)

## Ratified Decision Contract

1. **Adopted (Items 1 & 2):**
   - Deterministic `FxHashMap`/`FxHashSet` (`fm_core::FxHashMap`) adopted across lookup-only hot paths (`41` sites in `crates/fm-layout/src/lib.rs`, `20` sites in `crates/fm-parser/src/ir_builder.rs`, `bd-2gr9.1`).
   - Map pre-sizing (`with_capacity` / `with_capacity_and_hasher` / `reserve`) adopted across `33` sites in `fm-layout` and `11` sites in `ir_builder` (`bd-2gr9.2`), eliminating the ~5% `RawTable::reserve_rehash` cost.
   - `TreeChildren` CSR flat storage adopted for adjacency lists in `fm-layout`.
2. **Preserved `BTreeMap` Where Iteration Order Is Load-Bearing (`bd-6g0v`):**
   - `LayoutDependencyGraph.regions` and `DirtySet.regions` use `BTreeMap` as a deterministic ordering mechanism (verified by `lib.rs:15716`); swapping them to hash maps would violate layout determinism.
3. **Rejected (Items 3 & 4 — Direct `hashbrown::RawTable` & Hybrid Flat-Array Index):**
   - Rejected based on measured negative evidence (`docs/NEGATIVE_EVIDENCE.md` rows `3373`, `3341`, `3261`, `11791`), where dense/flat-array rewrites of crossing counters and post-presize constant-factor container micro-opts regressed or measured ~0 gain.

## Reviewers

Project maintainer (ubuntu)

