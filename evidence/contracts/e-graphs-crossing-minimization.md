# Decision Contract: E-Graphs for Crossing Minimization (Section 6.6)

## Graveyard Score: 3.0 / Tier: B

## Hypothesis

Replacing the fixed 4-iteration barycenter heuristic with an e-graph-based equality saturation approach will find better node orderings per rank, reducing edge crossings by 15% or more on dense graphs. E-graphs can explore the space of all valid node orderings simultaneously, discovering global optima that greedy barycenter misses.

## Current Baseline

**File:** `crates/fm-layout/src/lib.rs`
**Function:** `crossing_minimization()` (line ~943)
**Algorithm:** Deterministic barycenter heuristic
- 4 fixed top-down/bottom-up sweeps (hardcoded iteration count)
- Barycenter = average position of neighbors in adjacent rank
- Tie-breaking by node ID (stable, deterministic)
- Inversion-counting via merge-sort for crossing detection: O(m log m)
- No convergence monitoring; always runs exactly 4 sweeps

**Data structures:**
- `BTreeMap<usize, Vec<usize>>` for ordering_by_rank
- `BTreeMap<usize, usize>` for rank-to-position lookups
- Recomputed from scratch each sweep iteration

**Metrics available:**
- `LayoutStats.crossing_count`: total edge crossings in final layout
- `LayoutStats.phase_iterations`: total layout phase iterations

**Current performance:** O(|ranks| x |edges| log |edges|) per sweep, 4 sweeps fixed.

## Acceptance Criteria (Adopt)

- [ ] Crossing count reduced by >= 15% on dense benchmark graphs (50+ nodes, edge density > 2x node count)
- [ ] No regression on sparse graphs (crossing count within 5% of baseline)
- [ ] Layout time within 3x of current barycenter for graphs under 200 nodes
- [ ] Fallback to barycenter if e-graph saturation exceeds 100ms
- [ ] Implementation <= 500 LOC (excluding tests and the `egg` crate itself)
- [ ] Determinism preserved: identical inputs produce identical crossing counts
- [ ] All 27 existing fm-layout tests pass
- [ ] Evidence ledger entry with before/after crossing counts on benchmark corpus

## Rejection Criteria (Reject)

- [ ] Crossing count improvement < 5% on benchmark corpus (not worth the complexity)
- [ ] Layout time > 5x baseline for any graph under 200 nodes
- [ ] Fallback triggers on > 30% of benchmark corpus (instability)
- [ ] `egg` crate does not compile for `wasm32-unknown-unknown`
- [ ] Determinism broken: different crossing counts across runs with same input
- [ ] Implementation exceeds 800 LOC

## Evaluation Protocol

1. **Baseline measurement**
   - Run benchmark corpus through current `crossing_minimization()`
   - Record `crossing_count` for each input graph
   - Record wall-clock time for crossing minimization phase only
   - Measure for 10 runs, report mean and stddev

2. **Implementation**
   - Add `CycleStrategy::EGraph` variant or separate `CrossingStrategy` enum
   - Gate behind `LayoutConfig` option (default: barycenter, opt-in: e-graph)
   - Represent node orderings as e-class terms; define rewrite rules for swaps
   - Cost function = crossing count (computed via existing inversion counting)
   - Saturation budget: 100ms or 1000 e-nodes, whichever comes first

3. **Post-measurement**
   - Same corpus, same conditions, record crossing_count and time
   - Compare per-graph and aggregate improvements

4. **Statistical comparison**
   - Report crossing count delta (absolute and percentage) per graph
   - Report layout time ratio (e-graph / barycenter) per graph
   - Flag any graph where e-graph produces more crossings than barycenter

5. **Decision**
   - Adopt if median crossing reduction >= 15% and max time ratio <= 3x
   - Hybrid if only large/dense graphs benefit (gate by node count threshold)

## Benchmark Corpus

- **Small:** 5-node diamond graph (existing test fixture)
- **Medium-sparse:** 50 nodes, 60 edges, tree-like structure
- **Medium-dense:** 50 nodes, 150 edges, multiple back-edges
- **Large-sparse:** 200 nodes, 250 edges, layered DAG
- **Large-dense:** 200 nodes, 600 edges, many crossings
- **Pathological:** Complete bipartite K(10,10) graph (maximum crossings)
- **Regression:** Already-optimal linear chain (should not degrade)

## Timeline

- Baseline: TBD
- Implementation: TBD (estimate: 3-5 sessions)
- Evaluation: TBD
- Decision: TBD

## Reviewers

Project maintainer (ubuntu)

## Notes

- The `egg` crate (equality saturation) is the primary candidate library
- Alternative: `egglog` for Datalog-flavored e-graphs
- Key risk: e-graph term representation for node orderings may blow up combinatorially
- Consider hybrid approach: use e-graphs only for ranks with > N crossings
- Reference: "Equality Saturation: A New Approach to Optimization" (Tate et al., 2009)
