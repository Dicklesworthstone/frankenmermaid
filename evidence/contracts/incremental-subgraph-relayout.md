# Decision Contract: Incremental Subgraph Re-Layout (Section 6.1)

## Graveyard Score: 6.7 / Tier: A

## Hypothesis

Implementing incremental subgraph re-layout will reduce update latency from full O(n^2) re-layout to O(k^2) where k is the size of the modified subgraph, achieving < 10ms update time on 1000-node diagrams when only a small region changes. This is mandatory for interactive editing use cases (WASM live preview, CLI watch mode).

## Current Baseline

**File:** `crates/fm-layout/src/lib.rs`

**Current behavior: Full re-layout every time.**
- `layout_diagram()` runs the complete 6-phase Sugiyama pipeline for every call
- No caching of previous layout state
- No identification of changed regions
- No incremental computation

**Pipeline phases (all run from scratch):**
1. Cycle removal (line ~542): reverse edges to break cycles
2. Rank assignment (line ~791): longest-path rank computation
3. Crossing minimization (line ~943): 4-sweep barycenter
4. Coordinate assignment (line ~972): single-pass positioning
5. Edge routing (line ~1271): polyline generation
6. Cluster computation (line ~1433): bounding boxes from members

**Cluster/subgraph support:**
- `IrCluster { id, title, members, span }` parsed into IR
- `LayoutClusterBox { cluster_index, bounds }` computed post-hoc from member node positions
- `LayoutCycleCluster { head_node_index, member_node_indexes, bounds }` for SCC collapse
- Clusters are bounding boxes only; no layout-time cluster awareness

**Subgraph parsing status:**
- Subgraph syntax recognized but full parsing not yet implemented (documented in integration tests)
- Nodes and edges within subgraphs are parsed correctly

**No layout state caching. No change detection. No partial re-computation.**

## Acceptance Criteria (Adopt)

- [ ] Update time < 10ms for single-node edit on 1000-node diagram
- [ ] Update time < 50ms for cluster-level edit (add/remove node in subgraph) on 1000-node diagram
- [ ] Full re-layout produces identical output to incremental path (correctness)
- [ ] Nodes outside modified region remain at exactly the same coordinates
- [ ] Edge routing updated only for edges touching modified nodes
- [ ] Memory overhead for layout state caching < 2x full layout memory
- [ ] Implementation <= 600 LOC (incremental infrastructure + integration)
- [ ] Determinism preserved: same sequence of edits produces same final layout
- [ ] WASM target compilation succeeds

## Rejection Criteria (Reject)

- [ ] Update time > 50ms for single-node edit on 1000-node diagram
- [ ] Incremental layout produces different output than full re-layout (correctness bug)
- [ ] Memory overhead > 4x (caching too expensive)
- [ ] Non-deterministic: different edit sequences to same final state produce different layouts
- [ ] Implementation exceeds 1000 LOC
- [ ] Requires fundamental restructuring of all 6 pipeline phases (too invasive)

## Evaluation Protocol

1. **Baseline measurement**
   - Create 1000-node benchmark graph with 5 identifiable clusters
   - Record full layout time for this graph
   - Profile each pipeline phase separately (cycle, rank, crossing, coord, routing, cluster)
   - Identify which phases dominate layout time
   - Record memory usage for full layout

2. **Implementation approach**
   - **Layout state snapshot:** After full layout, persist intermediate state:
     - Rank assignments per node
     - Node orderings per rank
     - Coordinate positions
     - Edge routing polylines
   - **Change detection:** Compare new IR against cached IR
     - Identify added/removed/modified nodes and edges
     - Compute affected cluster(s) from change set
   - **Boundary nodes:** Nodes on cluster perimeter that connect to external graph
     - These nodes anchor the incremental region to the full layout
     - Their positions are fixed during incremental re-layout
   - **Partial pipeline re-run:**
     - If only labels changed: skip all phases, update text positions only
     - If node added/removed in cluster: re-run rank + crossing + coord for cluster only
     - If edge added/removed: re-run crossing + coord for affected ranks only
     - If structural change across clusters: fall back to full re-layout
   - Gate behind `LayoutConfig` option

3. **Post-measurement**
   - Apply single-node edit to 1000-node graph, measure incremental time
   - Apply cluster-level edit, measure incremental time
   - Compare incremental output against full re-layout output (must be identical)
   - Record memory usage with caching

4. **Statistical comparison**
   - Report speedup ratio (full time / incremental time) per edit type
   - Report correctness: diff between incremental and full outputs (must be zero)
   - Report memory overhead ratio
   - Report fallback rate: % of edits that trigger full re-layout

5. **Decision**
   - Adopt if < 10ms for node edits and correctness verified
   - Hybrid if only label/style edits incremental (skip layout phases entirely)
   - Defer if cluster-level incrementality too complex (implement label-only first)
   - Reject if correctness cannot be guaranteed

## Benchmark Corpus

- **1000-node clustered graph:** 5 clusters of ~200 nodes each, inter-cluster edges
- **Single node edit:** Change label of one node (trivial incremental)
- **Node addition:** Add node to cluster of 200 nodes
- **Node deletion:** Remove node from cluster, reconnect edges
- **Edge addition:** Add cross-cluster edge (tests boundary node handling)
- **Cluster restructure:** Move node between clusters
- **Full restructure:** Add new cluster (should fall back to full re-layout)
- **Rapid edits:** 100 sequential single-node edits (throughput test)

## Timeline

- Baseline profiling + phase timing: TBD (estimate: 1 session)
- Layout state snapshot: TBD (estimate: 2 sessions)
- Change detection + boundary nodes: TBD (estimate: 2-3 sessions)
- Partial pipeline integration: TBD (estimate: 2-3 sessions)
- Evaluation: TBD
- Decision: TBD

## Reviewers

Project maintainer (ubuntu)

## Notes

- **Tier A (mandatory adoption)** per Alien Graveyard scoring
- This is the highest-scored concept at 6.7; it directly enables interactive editing
- Key challenge: rank assignment is global (longest-path across entire graph)
  - Mitigation: cache ranks; only recompute if structural change affects longest path
  - If added node doesn't change any longest path, all ranks remain valid
- Crossing minimization is also global but can be scoped to affected rank pairs
- Coordinate assignment is the easiest to make incremental (reposition affected nodes only)
- Edge routing is inherently local (only touches source and target nodes)
- Consider "layout fingerprint" for change detection: hash of node IDs + edge list + constraint list
- For WASM live preview: even 50ms incremental would be massive improvement over ~200ms full
- Reference: North "Incremental Layout in DynaDAG" (1996)
- Reference: Gorczyca et al. "Incremental Graph Drawing" (2020)
