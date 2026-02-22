# Decision Contract: Constraint Programming for Layout (Section 9.7)

## Graveyard Score: 3.5 / Tier: B

## Hypothesis

Integrating a constraint solver into the Sugiyama layout pipeline will enforce user-specified layout constraints (SameRank, MinLength, Pin, OrderInRank) that are currently parsed into the IR but completely ignored. This will make the layout engine constraint-aware, producing layouts that respect user intent while maintaining acceptable solve times (< 1s for 200-node graphs).

## Current Baseline

**File:** `crates/fm-core/src/lib.rs` (lines ~373-395)

**Constraint types defined:**
```rust
pub enum IrConstraint {
    SameRank { node_ids: Vec<String>, span: Span },
    MinLength { from_id: String, to_id: String, min_len: usize, span: Span },
    Pin { node_id: String, x: f64, y: f64, span: Span },
    OrderInRank { node_ids: Vec<String>, span: Span },
}
```

**Current enforcement: 0%** - Constraints are parsed into `MermaidDiagramIr.constraints` (line ~855 in fm-core) but the layout engine (`fm-layout/src/lib.rs`) never reads them.

**Layout pipeline phases that would be affected:**
1. **Rank assignment** (line ~791): computes ranks via longest-path; SameRank and MinLength would add equality/inequality constraints
2. **Crossing minimization** (line ~943): reorders nodes per rank; OrderInRank would restrict reordering freedom
3. **Coordinate assignment** (line ~972): positions nodes; Pin would fix absolute coordinates

**No constraint-related tests exist in the codebase.**

## Acceptance Criteria (Adopt)

- [ ] All 4 constraint types enforced: SameRank, MinLength, Pin, OrderInRank
- [ ] SameRank: constrained nodes placed on identical rank (100% satisfaction)
- [ ] MinLength: edge spans at least min_len ranks (100% satisfaction)
- [ ] Pin: node placed within 1px of specified (x, y) coordinates
- [ ] OrderInRank: constrained nodes appear in specified order within their rank
- [ ] Solve time < 1 second for graphs with 200 nodes and up to 50 constraints
- [ ] Solve time < 100ms for graphs with 50 nodes and up to 10 constraints
- [ ] Unconstrained graphs produce identical output to current engine (no regression)
- [ ] Implementation <= 600 LOC (constraint integration, excluding solver library)
- [ ] WASM target compilation succeeds

## Rejection Criteria (Reject)

- [ ] Solve time > 5 seconds for 100-node graph with 20 constraints
- [ ] Any constraint type cannot be expressed in the solver's constraint language
- [ ] Solver library does not compile for `wasm32-unknown-unknown`
- [ ] Unconstrained graph outputs differ from current baseline (regression)
- [ ] Solver introduces non-determinism (different solutions across runs)
- [ ] Implementation exceeds 1000 LOC
- [ ] Conflicting constraints cause crashes instead of graceful degradation

## Evaluation Protocol

1. **Baseline measurement**
   - Record layout output for benchmark corpus (unconstrained)
   - Create constraint test cases for each of the 4 constraint types
   - Verify constraints are correctly parsed into IR (they should be already)
   - Document constraint interaction edge cases (conflicting constraints)

2. **Implementation approach options**
   - **Option A:** Lightweight custom solver
     - Topological sort with SameRank groups for rank assignment
     - Prefix constraints for OrderInRank in crossing minimization
     - Coordinate pinning as post-processing step
     - Pro: No external dependency. Con: May not handle complex constraint interactions
   - **Option B:** LP/MIP solver integration
     - Use `good_lp` crate (pure Rust LP solver) or `minilp`
     - Express rank assignment as LP with constraint equations
     - Pro: Handles complex interactions. Con: Solver dependency, WASM compatibility risk
   - **Option C:** Incremental constraint propagation
     - Arc-consistency propagation for constraint domains
     - Pro: Fast for simple constraints. Con: May not converge for complex interactions

3. **Post-measurement**
   - Verify 100% constraint satisfaction on test cases
   - Record solve time for increasing graph sizes
   - Verify unconstrained outputs identical to baseline

4. **Statistical comparison**
   - Report constraint satisfaction rate per type
   - Report solve time scaling curve (nodes vs time)
   - Report quality delta: crossing count, edge length on constrained vs unconstrained

5. **Decision**
   - Adopt if all constraints enforceable and solve time acceptable
   - Hybrid if only lightweight constraints (SameRank, OrderInRank) implemented first
   - Reject if solver dependency blocks WASM or introduces unacceptable latency

## Benchmark Corpus

- **No-constraint baseline:** Standard benchmark corpus (verify no regression)
- **SameRank test:** 20-node graph with 3 groups of SameRank nodes
- **MinLength test:** 15-node graph with 5 MinLength constraints of varying lengths
- **Pin test:** 10-node graph with 3 pinned nodes at specific coordinates
- **OrderInRank test:** 30-node graph with 2 ordered groups of 5 nodes each
- **Mixed constraints:** 50-node graph with all 4 constraint types simultaneously
- **Conflicting constraints:** Graph with mutually exclusive constraints (graceful failure test)
- **Stress test:** 200-node graph with 50 mixed constraints

## Timeline

- Baseline + constraint parsing verification: TBD
- Solver selection and prototype: TBD (estimate: 2-3 sessions)
- Full integration and testing: TBD (estimate: 2-3 sessions)
- Evaluation: TBD
- Decision: TBD

## Reviewers

Project maintainer (ubuntu)

## Notes

- The constraint infrastructure already exists in fm-core (parsed, stored in IR)
- The gap is purely in fm-layout: no code reads `ir.constraints`
- Recommend starting with Option A (lightweight custom solver) for SameRank and OrderInRank
- Pin constraints may conflict with rank-based layout; may need to relax rank assignment
- MinLength can be expressed as rank difference inequality during longest-path computation
- MermaidConfig.layout_iterations (default: 200) provides existing iteration budget
- Conflicting constraints should produce a warning + best-effort layout, not a crash
- Reference: Gansner et al. "A Technique for Drawing Directed Graphs" (1993) - constraint extensions
