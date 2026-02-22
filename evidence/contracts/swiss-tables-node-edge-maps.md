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

- Baseline + profiling: TBD (estimate: 1 session)
- Implementation: TBD (estimate: 1 session, mechanical replacement)
- Evaluation: TBD (estimate: 1 session)
- Decision: TBD

## Reviewers

Project maintainer (ubuntu)

## Notes

- hashbrown is already used internally by Rust's std HashMap (but with RandomState)
- FxHasher is fastest for integer keys but has poor distribution for some patterns
- ahash provides hardware-accelerated hashing with WASM support
- Key insight: profile first. If BTreeMap is < 5% of layout time, reject early
- Alternative: keep BTreeMap everywhere but add allocation caching (arena allocator)
- Rust 2024 edition's std HashMap already uses hashbrown internally; the gain here is from using a deterministic, non-random hasher
