//! Directed graph algorithms for Phase-2 FNX integration (bd-ml2r.7.1).
//!
//! This module provides directed algorithm APIs with deterministic output contracts
//! for layout integration:
//! - Strongly Connected Components (SCC)
//! - Weakly Connected Components
//! - Directed cycle detection
//! - Reachability analysis
//!
//! All outputs are sorted/ordered deterministically for reproducible layout decisions.

use fm_core::{IrEndpoint, MermaidDiagramIr};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Resolve both node and port endpoints through the shared IR contract.
/// Invalid references and unresolved endpoints do not contribute graph edges.
#[inline]
fn endpoint_node_index(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> Option<usize> {
    ir.resolve_endpoint_node(endpoint)
        .map(|node| node.0)
        .filter(|&index| index < ir.nodes.len())
}

// ============================================================================
// Strongly Connected Components (Tarjan's Algorithm)
// ============================================================================

/// A strongly connected component in a directed graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StronglyConnectedComponent {
    /// Component index (0-based, sorted by discovery order).
    pub index: usize,
    /// Node indices in this SCC, sorted for determinism.
    pub nodes: Vec<usize>,
    /// Whether this is a trivial SCC (single node, no self-loop).
    pub is_trivial: bool,
}

/// Result of SCC decomposition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SccResult {
    /// All SCCs, sorted by first node index for determinism.
    pub components: Vec<StronglyConnectedComponent>,
    /// Total number of non-trivial SCCs.
    pub non_trivial_count: usize,
    /// Maximum SCC size.
    pub max_component_size: usize,
    /// Node index to component index mapping.
    pub node_to_component: BTreeMap<usize, usize>,
}

/// Compute Strongly Connected Components using Tarjan's algorithm.
///
/// Output ordering contract:
/// - Components are sorted by their minimum node index
/// - Nodes within each component are sorted by index
/// - This ensures deterministic output regardless of iteration order
#[must_use]
pub fn compute_scc(ir: &MermaidDiagramIr) -> SccResult {
    let n = ir.nodes.len();
    if n == 0 {
        return SccResult {
            components: Vec::new(),
            non_trivial_count: 0,
            max_component_size: 0,
            node_to_component: BTreeMap::new(),
        };
    }

    // Build adjacency list (sorted for determinism)
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    let mut has_self_loop: Vec<bool> = vec![false; n];

    for edge in &ir.edges {
        let (Some(from_idx), Some(to_idx)) = (
            endpoint_node_index(ir, edge.from),
            endpoint_node_index(ir, edge.to),
        ) else {
            continue;
        };
        if from_idx < n && to_idx < n {
            if from_idx == to_idx {
                has_self_loop[from_idx] = true;
            } else {
                adj[from_idx].insert(to_idx);
            }
        }
    }

    // Tarjan's algorithm state
    let mut index_counter = 0;
    let mut indices: Vec<Option<usize>> = vec![None; n];
    let mut lowlinks: Vec<usize> = vec![0; n];
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    // Keep DFS frames on the heap. A valid long chain must not abort the process by
    // exhausting a native thread's stack (or the smaller stack available in WASM).
    // Each frame retains its sorted successor iterator, exactly as recursive Tarjan
    // would, so discovery order and lowlink propagation remain deterministic.
    let mut traversal = Vec::new();
    for root in 0..n {
        if indices[root].is_some() {
            continue;
        }
        indices[root] = Some(index_counter);
        lowlinks[root] = index_counter;
        index_counter += 1;
        stack.push(root);
        on_stack[root] = true;
        traversal.push((root, adj[root].iter()));

        while let Some((v, successors)) = traversal.last_mut() {
            let v = *v;
            if let Some(&w) = successors.next() {
                if let Some(index) = indices[w] {
                    if on_stack[w] {
                        lowlinks[v] = lowlinks[v].min(index);
                    }
                } else {
                    indices[w] = Some(index_counter);
                    lowlinks[w] = index_counter;
                    index_counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    traversal.push((w, adj[w].iter()));
                }
                continue;
            }

            traversal.pop();
            if indices[v] == Some(lowlinks[v]) {
                let mut scc = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                scc.sort_unstable();
                sccs.push(scc);
            }
            if let Some((parent, _)) = traversal.last() {
                lowlinks[*parent] = lowlinks[*parent].min(lowlinks[v]);
            }
        }
    }

    // Sort SCCs by minimum node index for deterministic output
    sccs.sort_by_key(|scc| scc.first().copied().unwrap_or(usize::MAX));

    // Build result
    let mut node_to_component = BTreeMap::new();
    let mut components = Vec::with_capacity(sccs.len());
    let mut non_trivial_count = 0;
    let mut max_component_size = 0;

    for (idx, nodes) in sccs.into_iter().enumerate() {
        let is_trivial = nodes.len() == 1 && !has_self_loop[nodes[0]];
        if !is_trivial {
            non_trivial_count += 1;
        }
        max_component_size = max_component_size.max(nodes.len());

        for &node in &nodes {
            node_to_component.insert(node, idx);
        }

        components.push(StronglyConnectedComponent {
            index: idx,
            nodes,
            is_trivial,
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    crate::fnx_budget::record_budget_work((n + ir.edges.len()).max(1) as u64);

    SccResult {
        components,
        non_trivial_count,
        max_component_size,
        node_to_component,
    }
}

/// Compute Strongly Connected Components under an explicit [`crate::fnx_budget::AnalysisBudget`].
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn compute_scc_with_budget(
    ir: &MermaidDiagramIr,
    budget: crate::fnx_budget::AnalysisBudget,
) -> crate::fnx_budget::BudgetResult<SccResult> {
    let mut executor = crate::fnx_budget::BudgetExecutor::new(budget);
    executor.execute_with_progress(|exec| {
        let work_units = (ir.nodes.len() + ir.edges.len()).max(1) as u64;
        exec.record_work(work_units);
        compute_scc(ir)
    })
}

// ============================================================================
// Weakly Connected Components
// ============================================================================

/// A weakly connected component (connected ignoring edge direction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaklyConnectedComponent {
    /// Component index (0-based).
    pub index: usize,
    /// Node indices in this component, sorted for determinism.
    pub nodes: Vec<usize>,
}

/// Result of WCC decomposition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WccResult {
    /// All WCCs, sorted by first node index for determinism.
    pub components: Vec<WeaklyConnectedComponent>,
    /// Whether the graph is weakly connected (single WCC).
    pub is_connected: bool,
    /// Maximum WCC size.
    pub max_component_size: usize,
    /// Node index to component index mapping.
    pub node_to_component: BTreeMap<usize, usize>,
}

/// Compute Weakly Connected Components using BFS.
///
/// Output ordering contract:
/// - Components are sorted by their minimum node index
/// - Nodes within each component are sorted by index
#[must_use]
pub fn compute_wcc(ir: &MermaidDiagramIr) -> WccResult {
    let n = ir.nodes.len();
    if n == 0 {
        return WccResult {
            components: Vec::new(),
            is_connected: true,
            max_component_size: 0,
            node_to_component: BTreeMap::new(),
        };
    }

    // Build undirected adjacency list (sorted for determinism)
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];

    for edge in &ir.edges {
        let (Some(from_idx), Some(to_idx)) = (
            endpoint_node_index(ir, edge.from),
            endpoint_node_index(ir, edge.to),
        ) else {
            continue;
        };
        if from_idx < n && to_idx < n && from_idx != to_idx {
            adj[from_idx].insert(to_idx);
            adj[to_idx].insert(from_idx);
        }
    }

    // BFS to find components
    let mut visited: Vec<bool> = vec![false; n];
    let mut wccs: Vec<Vec<usize>> = Vec::new();

    for start in 0..n {
        if visited[start] {
            continue;
        }

        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited[start] = true;

        while let Some(v) = queue.pop_front() {
            component.push(v);
            for &w in &adj[v] {
                if !visited[w] {
                    visited[w] = true;
                    queue.push_back(w);
                }
            }
        }

        component.sort_unstable();
        wccs.push(component);
    }

    // Sort WCCs by minimum node index
    wccs.sort_by_key(|wcc| wcc.first().copied().unwrap_or(usize::MAX));

    // Build result
    let mut node_to_component = BTreeMap::new();
    let mut components = Vec::with_capacity(wccs.len());
    let mut max_component_size = 0;

    for (idx, nodes) in wccs.into_iter().enumerate() {
        max_component_size = max_component_size.max(nodes.len());
        for &node in &nodes {
            node_to_component.insert(node, idx);
        }
        components.push(WeaklyConnectedComponent { index: idx, nodes });
    }

    let is_connected = components.len() <= 1;

    WccResult {
        components,
        is_connected,
        max_component_size,
        node_to_component,
    }
}

// ============================================================================
// Directed Cycle Detection
// ============================================================================

/// A directed cycle found in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectedCycle {
    /// Node indices forming the cycle, in cycle order.
    /// The cycle goes from nodes[0] -> nodes[1] -> ... -> nodes[n-1] -> nodes[0].
    pub nodes: Vec<usize>,
    /// Edge indices in cycle order: `edges[i]` connects `nodes[i]` to the next
    /// node, with the final edge returning to `nodes[0]`.
    pub edges: Vec<usize>,
}

/// Result of directed cycle detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectedCycleResult {
    /// Whether the graph has any directed cycles.
    pub has_cycles: bool,
    /// All detected cycles, sorted by minimum node index.
    pub cycles: Vec<DirectedCycle>,
    /// Nodes that participate in at least one cycle.
    pub cyclic_nodes: BTreeSet<usize>,
    /// Edges that participate in at least one cycle (indices).
    pub cyclic_edges: BTreeSet<usize>,
}

/// Detect directed cycles in the graph using DFS.
///
/// This finds cycles by detecting back edges during DFS traversal.
/// Note: This may not find ALL cycles in complex graphs, but will detect
/// if cycles exist and return a representative set.
///
/// Output ordering contract:
/// - Cycles are sorted by their minimum node index
#[must_use]
pub fn detect_directed_cycles(ir: &MermaidDiagramIr) -> DirectedCycleResult {
    let n = ir.nodes.len();
    if n == 0 {
        return DirectedCycleResult {
            has_cycles: false,
            cycles: Vec::new(),
            cyclic_nodes: BTreeSet::new(),
            cyclic_edges: BTreeSet::new(),
        };
    }

    // Build adjacency list with edge indices
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n]; // (target, edge_idx)

    for (edge_idx, edge) in ir.edges.iter().enumerate() {
        let (Some(from_idx), Some(to_idx)) = (
            endpoint_node_index(ir, edge.from),
            endpoint_node_index(ir, edge.to),
        ) else {
            continue;
        };
        if from_idx < n && to_idx < n {
            adj[from_idx].push((to_idx, edge_idx));
        }
    }

    // Sort adjacency lists for determinism
    for neighbors in &mut adj {
        neighbors.sort_by_key(|&(target, _)| target);
    }

    // DFS state
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: Vec<Color> = vec![Color::White; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut parent_edge: Vec<Option<usize>> = vec![None; n];
    let mut cycles: Vec<DirectedCycle> = Vec::new();
    let mut cyclic_nodes = BTreeSet::new();
    let mut cyclic_edges = BTreeSet::new();

    // Explicit DFS frames preserve the recursive traversal's successor order without
    // tying the largest supported graph to the caller's thread-stack limit.
    let mut traversal = Vec::new();
    for root in 0..n {
        if color[root] != Color::White {
            continue;
        }
        color[root] = Color::Gray;
        traversal.push((root, 0));

        while let Some((v, next_neighbor)) = traversal.last_mut() {
            let v = *v;
            let Some(&(w, edge_idx)) = adj[v].get(*next_neighbor) else {
                color[v] = Color::Black;
                traversal.pop();
                continue;
            };
            *next_neighbor += 1;
            if color[w] == Color::Gray {
                // A gray successor is an ancestor on this traversal. Walk back to it
                // and reverse both lists so edges[i] connects nodes[i] to the next
                // node, including the closing back edge.
                let mut cycle_nodes = vec![v];
                let mut cycle_edges = vec![edge_idx];
                let mut curr = v;

                while curr != w {
                    if let Some(pe) = parent_edge[curr] {
                        cycle_edges.push(pe);
                    }
                    curr = parent[curr].unwrap_or(w);
                    cycle_nodes.push(curr);
                }

                cycle_nodes.reverse();
                cycle_edges.reverse();

                for &node in &cycle_nodes {
                    cyclic_nodes.insert(node);
                }
                for &edge in &cycle_edges {
                    cyclic_edges.insert(edge);
                }

                cycles.push(DirectedCycle {
                    nodes: cycle_nodes,
                    edges: cycle_edges,
                });
            } else if color[w] == Color::White {
                parent[w] = Some(v);
                parent_edge[w] = Some(edge_idx);
                color[w] = Color::Gray;
                traversal.push((w, 0));
            }
        }
    }

    // Sort cycles by minimum node index
    cycles.sort_by_key(|c| c.nodes.iter().min().copied().unwrap_or(usize::MAX));

    DirectedCycleResult {
        has_cycles: !cycles.is_empty(),
        cycles,
        cyclic_nodes,
        cyclic_edges,
    }
}

// ============================================================================
// Reachability Analysis
// ============================================================================

/// Reachability matrix for directed graphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachabilityResult {
    /// For each node, the set of nodes reachable from it.
    pub reachable_from: BTreeMap<usize, BTreeSet<usize>>,
    /// For each node, the set of nodes that can reach it.
    pub reaches_to: BTreeMap<usize, BTreeSet<usize>>,
    /// Source nodes (no incoming edges from other nodes).
    pub sources: BTreeSet<usize>,
    /// Sink nodes (no outgoing edges to other nodes).
    pub sinks: BTreeSet<usize>,
}

/// Compute reachability information for the directed graph.
///
/// Output ordering contract:
/// - All sets are BTreeSet for deterministic iteration order
#[must_use]
pub fn compute_reachability(ir: &MermaidDiagramIr) -> ReachabilityResult {
    let n = ir.nodes.len();
    if n == 0 {
        return ReachabilityResult {
            reachable_from: BTreeMap::new(),
            reaches_to: BTreeMap::new(),
            sources: BTreeSet::new(),
            sinks: BTreeSet::new(),
        };
    }

    // Build adjacency lists
    let mut out_adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    let mut in_adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];

    for edge in &ir.edges {
        let (Some(from_idx), Some(to_idx)) = (
            endpoint_node_index(ir, edge.from),
            endpoint_node_index(ir, edge.to),
        ) else {
            continue;
        };
        if from_idx < n && to_idx < n && from_idx != to_idx {
            out_adj[from_idx].insert(to_idx);
            in_adj[to_idx].insert(from_idx);
        }
    }

    // Compute reachability via BFS from each node
    let mut reachable_from = BTreeMap::new();
    let mut reaches_to = BTreeMap::new();

    for start in 0..n {
        let mut reachable = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);

        while let Some(v) = queue.pop_front() {
            for &w in &out_adj[v] {
                if w != start && !reachable.contains(&w) {
                    reachable.insert(w);
                    queue.push_back(w);
                }
            }
        }

        // Build reverse mapping
        for &target in &reachable {
            reaches_to
                .entry(target)
                .or_insert_with(BTreeSet::new)
                .insert(start);
        }

        reachable_from.insert(start, reachable);
    }

    // Ensure all nodes have entries
    for v in 0..n {
        reachable_from.entry(v).or_insert_with(BTreeSet::new);
        reaches_to.entry(v).or_insert_with(BTreeSet::new);
    }

    // Identify sources and sinks
    let mut sources = BTreeSet::new();
    let mut sinks = BTreeSet::new();

    for v in 0..n {
        if in_adj[v].is_empty() {
            sources.insert(v);
        }
        if out_adj[v].is_empty() {
            sinks.insert(v);
        }
    }

    ReachabilityResult {
        reachable_from,
        reaches_to,
        sources,
        sinks,
    }
}

/// Complete directed topology summary combining SCC, WCC, directed cycle detection,
/// and reachability analysis (`bd-l93qp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectedTopologySummary {
    /// Strongly connected components (Tarjan's algorithm).
    pub scc: SccResult,
    /// Weakly connected components (undirected BFS).
    pub wcc: WccResult,
    /// Detected directed cycles and participating nodes/edges.
    pub cycles: DirectedCycleResult,
    /// Forward/backward reachability sets and source/sink nodes.
    pub reachability: ReachabilityResult,
}

/// Compute a complete [`DirectedTopologySummary`] for a diagram (`bd-l93qp`).
#[must_use]
pub fn analyze_directed_topology(ir: &MermaidDiagramIr) -> DirectedTopologySummary {
    DirectedTopologySummary {
        scc: compute_scc(ir),
        wcc: compute_wcc(ir),
        cycles: detect_directed_cycles(ir),
        reachability: compute_reachability(ir),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, IrEdge, IrNode, IrNodeId, IrPort, IrPortId};

    fn make_test_ir_with_edges(edges: &[(usize, usize)]) -> MermaidDiagramIr {
        let max_node = edges.iter().map(|&(a, b)| a.max(b)).max().unwrap_or(0);
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);

        for i in 0..=max_node {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                label: None, // Labels stored in separate labels vec, use None for tests
                ..Default::default()
            });
        }

        for &(from, to) in edges {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(from)),
                to: IrEndpoint::Node(IrNodeId(to)),
                ..Default::default()
            });
        }

        ir
    }

    // SCC Tests

    #[test]
    fn scc_empty_graph() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), 0);
        assert_eq!(result.non_trivial_count, 0);
    }

    #[test]
    fn scc_single_node() {
        // Single node with no edges
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode::default());
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), 1);
        assert!(result.components[0].is_trivial);
    }

    #[test]
    fn scc_linear_chain() {
        // A -> B -> C (no cycles, 3 trivial SCCs)
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2)]);
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), 3);
        assert_eq!(result.non_trivial_count, 0);
        for comp in &result.components {
            assert!(comp.is_trivial);
            assert_eq!(comp.nodes.len(), 1);
        }
    }

    #[test]
    fn scc_simple_cycle() {
        // A -> B -> C -> A (one SCC with all nodes)
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2), (2, 0)]);
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), 1);
        assert_eq!(result.non_trivial_count, 1);
        assert!(!result.components[0].is_trivial);
        assert_eq!(result.components[0].nodes, vec![0, 1, 2]);
    }

    #[test]
    fn scc_two_components() {
        // A -> B -> A, C -> D (two SCCs)
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 0), (2, 3)]);
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), 3); // {0,1}, {2}, {3}
        assert_eq!(result.non_trivial_count, 1);
    }

    #[test]
    fn scc_deterministic_ordering() {
        let ir = make_test_ir_with_edges(&[(2, 3), (3, 2), (0, 1), (1, 0)]);
        let result1 = compute_scc(&ir);
        let result2 = compute_scc(&ir);
        assert_eq!(result1, result2, "SCC results must be deterministic");

        // First SCC should contain nodes {0,1} (lower indices)
        assert!(result1.components[0].nodes.contains(&0));
    }

    // WCC Tests

    #[test]
    fn wcc_empty_graph() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let result = compute_wcc(&ir);
        assert_eq!(result.components.len(), 0);
        assert!(result.is_connected);
    }

    #[test]
    fn wcc_connected_graph() {
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2)]);
        let result = compute_wcc(&ir);
        assert_eq!(result.components.len(), 1);
        assert!(result.is_connected);
        assert_eq!(result.components[0].nodes, vec![0, 1, 2]);
    }

    #[test]
    fn wcc_disconnected_graph() {
        // Two separate components: {0,1} and {2,3}
        let ir = make_test_ir_with_edges(&[(0, 1), (2, 3)]);
        let result = compute_wcc(&ir);
        assert_eq!(result.components.len(), 2);
        assert!(!result.is_connected);
    }

    // Directed Cycle Tests

    #[test]
    fn cycles_no_cycles() {
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2)]);
        let result = detect_directed_cycles(&ir);
        assert!(!result.has_cycles);
        assert!(result.cycles.is_empty());
    }

    #[test]
    fn cycles_simple_cycle() {
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2), (2, 0)]);
        let result = detect_directed_cycles(&ir);
        assert!(result.has_cycles);
        assert!(!result.cycles.is_empty());
        assert!(result.cyclic_nodes.contains(&0));
        assert!(result.cyclic_nodes.contains(&1));
        assert!(result.cyclic_nodes.contains(&2));
    }

    #[test]
    fn cycles_self_loop() {
        let ir = make_test_ir_with_edges(&[(0, 0)]);
        let result = detect_directed_cycles(&ir);
        assert!(result.has_cycles);
        assert!(result.cyclic_nodes.contains(&0));
    }

    // Reachability Tests

    #[test]
    fn reachability_linear() {
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2)]);
        let result = compute_reachability(&ir);

        // Node 0 can reach 1 and 2
        assert!(result.reachable_from[&0].contains(&1));
        assert!(result.reachable_from[&0].contains(&2));

        // Node 2 cannot reach anything
        assert!(result.reachable_from[&2].is_empty());

        // Sources and sinks
        assert!(result.sources.contains(&0));
        assert!(result.sinks.contains(&2));
    }

    #[test]
    fn reachability_cycle() {
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 0)]);
        let result = compute_reachability(&ir);

        // Both nodes can reach each other
        assert!(result.reachable_from[&0].contains(&1));
        assert!(result.reachable_from[&1].contains(&0));

        // No sources or sinks in a pure cycle
        assert!(result.sources.is_empty());
        assert!(result.sinks.is_empty());
    }

    // Edge case tests

    #[test]
    fn scc_with_isolated_nodes() {
        // Create graph with nodes 0,1,2 but only edges between 0 and 1
        // Node 2 should be a trivial SCC by itself
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for i in 0..3 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                label: None,
                ..Default::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(0)),
            ..Default::default()
        });

        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), 2); // {0,1} and {2}
        assert_eq!(result.non_trivial_count, 1);

        // Node 2 should be in its own trivial component
        let node2_comp = result.node_to_component[&2];
        assert!(result.components[node2_comp].is_trivial);
        assert_eq!(result.components[node2_comp].nodes, vec![2]);
    }

    #[test]
    fn wcc_isolated_node_forms_own_component() {
        // Create graph with nodes 0,1,2 but only edge 0->1
        // Node 2 should be its own WCC
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for i in 0..3 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                label: None,
                ..Default::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            ..Default::default()
        });

        let result = compute_wcc(&ir);
        assert_eq!(result.components.len(), 2); // {0,1} and {2}
        assert!(!result.is_connected);

        // Verify node 2 is in its own component
        let node2_comp = result.node_to_component[&2];
        assert_eq!(result.components[node2_comp].nodes, vec![2]);
    }

    #[test]
    fn wcc_self_loop_only_node_is_isolated() {
        // Node with only a self-loop should be isolated
        let ir = make_test_ir_with_edges(&[(0, 0), (1, 2)]);
        let result = compute_wcc(&ir);

        // Node 0 (self-loop only) should be in its own component
        // Nodes 1,2 should be together
        assert_eq!(result.components.len(), 2);
        assert!(!result.is_connected);
    }

    #[test]
    fn reachability_isolated_nodes() {
        // Create graph where node 2 has no connections
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for i in 0..3 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                label: None,
                ..Default::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            ..Default::default()
        });

        let result = compute_reachability(&ir);

        // Node 2 should be both a source and a sink
        assert!(result.sources.contains(&2));
        assert!(result.sinks.contains(&2));

        // Node 2 cannot reach anything and nothing can reach it
        assert!(result.reachable_from[&2].is_empty());
        assert!(result.reaches_to[&2].is_empty());
    }

    #[test]
    fn cycles_determinism() {
        // Run cycle detection multiple times and verify same result
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2), (2, 0)]);
        let result1 = detect_directed_cycles(&ir);
        let result2 = detect_directed_cycles(&ir);
        assert_eq!(result1, result2, "Cycle detection must be deterministic");
    }

    #[test]
    fn directed_analysis_preserves_port_connectivity_and_ignores_invalid_endpoints() {
        // Two SCCs, a source, a sink, an isolated self-loop and a parallel edge.
        let nodes = make_test_ir_with_edges(&[
            (0, 1),
            (1, 2),
            (2, 1),
            (2, 3),
            (3, 4),
            (4, 3),
            (4, 5),
            (6, 6),
            (0, 1),
        ]);
        let expected = analyze_directed_topology(&nodes);
        assert_eq!(expected.scc.non_trivial_count, 3);
        assert_eq!(expected.wcc.components.len(), 2);
        assert_eq!(
            expected.reachability.reachable_from[&0],
            BTreeSet::from([1, 2, 3, 4, 5])
        );

        let mut ports = nodes.clone();
        ports.ports = (0..ports.nodes.len())
            .map(|node| IrPort {
                node: IrNodeId(node),
                name: format!("port-{node}"),
                ..Default::default()
            })
            .collect();
        for (index, edge) in ports.edges.iter_mut().enumerate() {
            let IrEndpoint::Node(from) = edge.from else {
                unreachable!()
            };
            let IrEndpoint::Node(to) = edge.to else {
                unreachable!()
            };
            edge.from = IrEndpoint::Port(IrPortId(from.0));
            // Exercise mixed node/port edges as well as port/port edges.
            if index % 2 == 0 {
                edge.to = IrEndpoint::Port(IrPortId(to.0));
            }
        }
        assert_eq!(analyze_directed_topology(&ports), expected);

        let invalid_port = ports.ports.len();
        ports.ports.push(IrPort {
            node: IrNodeId(usize::MAX),
            ..Default::default()
        });
        for invalid in [
            IrEndpoint::Unresolved,
            IrEndpoint::Node(IrNodeId(usize::MAX)),
            IrEndpoint::Port(IrPortId(usize::MAX)),
            IrEndpoint::Port(IrPortId(invalid_port)),
        ] {
            ports.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(5)),
                to: invalid,
                ..Default::default()
            });
            ports.edges.push(IrEdge {
                from: invalid,
                to: IrEndpoint::Node(IrNodeId(0)),
                ..Default::default()
            });
        }
        assert_eq!(analyze_directed_topology(&ports), expected);
    }

    #[test]
    fn directed_components_and_cycle_witnesses_match_transitive_closure() {
        // Exhaust every directed graph on three vertices, including self-loops.
        // Floyd-Warshall is an independent oracle for the DFS/lowlink algorithms.
        const N: usize = 3;
        for mask in 0_u16..(1 << (N * N)) {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            ir.nodes = (0..N)
                .map(|node| IrNode {
                    id: format!("N{node}"),
                    ..Default::default()
                })
                .collect();
            let mut reachable = [[false; N]; N];
            for (from, row) in reachable.iter_mut().enumerate() {
                for (to, connected) in row.iter_mut().enumerate() {
                    if mask & (1 << (from * N + to)) != 0 {
                        *connected = true;
                        ir.edges.push(IrEdge {
                            from: IrEndpoint::Node(IrNodeId(from)),
                            to: IrEndpoint::Node(IrNodeId(to)),
                            ..Default::default()
                        });
                    }
                }
            }
            for intermediate in 0..N {
                for from in 0..N {
                    for to in 0..N {
                        reachable[from][to] |=
                            reachable[from][intermediate] && reachable[intermediate][to];
                    }
                }
            }

            let scc = compute_scc(&ir);
            for from in 0..N {
                for to in 0..N {
                    assert_eq!(
                        scc.node_to_component[&from] == scc.node_to_component[&to],
                        from == to || (reachable[from][to] && reachable[to][from]),
                        "component mismatch for mask {mask}, {from} and {to}"
                    );
                }
                let component = &scc.components[scc.node_to_component[&from]];
                assert_eq!(component.is_trivial, !reachable[from][from]);
            }

            let cycles = detect_directed_cycles(&ir);
            assert_eq!(cycles.has_cycles, (0..N).any(|node| reachable[node][node]));
            for cycle in &cycles.cycles {
                assert_eq!(cycle.edges.len(), cycle.nodes.len());
                assert!(!cycle.nodes.is_empty());
                for (index, &edge_index) in cycle.edges.iter().enumerate() {
                    let edge = &ir.edges[edge_index];
                    assert_eq!(edge.from, IrEndpoint::Node(IrNodeId(cycle.nodes[index])));
                    assert_eq!(
                        edge.to,
                        IrEndpoint::Node(IrNodeId(cycle.nodes[(index + 1) % cycle.nodes.len()]))
                    );
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn directed_depth_is_not_limited_by_the_thread_stack() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                const N: usize = 20_000;
                let edges: Vec<_> = (0..N - 1).map(|node| (node, node + 1)).collect();
                let mut ir = make_test_ir_with_edges(&edges);
                let chain = compute_scc(&ir);
                assert_eq!(chain.components.len(), N);
                assert_eq!(chain.non_trivial_count, 0);
                assert!(!detect_directed_cycles(&ir).has_cycles);

                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(N - 1)),
                    to: IrEndpoint::Node(IrNodeId(0)),
                    ..Default::default()
                });
                let ring = compute_scc(&ir);
                assert_eq!(ring.components.len(), 1);
                assert_eq!(ring.components[0].nodes.len(), N);
                assert_eq!(ring.non_trivial_count, 1);
                let cycles = detect_directed_cycles(&ir);
                assert_eq!(cycles.cycles.len(), 1);
                assert_eq!(cycles.cycles[0].nodes.len(), N);
                assert_eq!(cycles.cycles[0].edges.len(), N);
            })
            .expect("start a thread with a deliberately small stack")
            .join()
            .expect("directed analysis must complete without recursion");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn compute_scc_with_budget_enforces_work_limit() {
        let ir = make_test_ir_with_edges(&[(0, 1), (1, 2), (2, 0)]);
        let tight = crate::fnx_budget::AnalysisBudget::default().with_work_budget(1);
        let result = compute_scc_with_budget(&ir, tight);
        assert!(result.is_cancelled());
        assert_eq!(result.into_value().unwrap().non_trivial_count, 1);
    }
}
