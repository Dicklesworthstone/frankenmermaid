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

use fm_core::{IrEndpoint, IrNodeId, MermaidDiagramIr};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Extract node index from an IrEndpoint, if it's a Node variant.
/// Returns None for Unresolved or Port endpoints.
#[inline]
fn endpoint_node_index(endpoint: &IrEndpoint) -> Option<usize> {
    match endpoint {
        IrEndpoint::Node(IrNodeId(idx)) => Some(*idx),
        _ => None,
    }
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
            endpoint_node_index(&edge.from),
            endpoint_node_index(&edge.to),
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

    #[allow(clippy::too_many_arguments)] // Tarjan's algorithm state is necessarily coupled
    fn strongconnect(
        v: usize,
        adj: &[BTreeSet<usize>],
        index_counter: &mut usize,
        indices: &mut [Option<usize>],
        lowlinks: &mut [usize],
        on_stack: &mut [bool],
        stack: &mut Vec<usize>,
        sccs: &mut Vec<Vec<usize>>,
    ) {
        indices[v] = Some(*index_counter);
        lowlinks[v] = *index_counter;
        *index_counter += 1;
        stack.push(v);
        on_stack[v] = true;

        // Visit successors in sorted order (BTreeSet guarantees this)
        for &w in &adj[v] {
            if indices[w].is_none() {
                strongconnect(
                    w,
                    adj,
                    index_counter,
                    indices,
                    lowlinks,
                    on_stack,
                    stack,
                    sccs,
                );
                lowlinks[v] = lowlinks[v].min(lowlinks[w]);
            } else if on_stack[w] {
                lowlinks[v] = lowlinks[v].min(indices[w].unwrap());
            }
        }

        // If v is a root node, pop stack and generate SCC
        if lowlinks[v] == indices[v].unwrap() {
            let mut scc = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            scc.sort_unstable(); // Deterministic ordering
            sccs.push(scc);
        }
    }

    // Process nodes in sorted order for deterministic SCC discovery
    for v in 0..n {
        if indices[v].is_none() {
            strongconnect(
                v,
                &adj,
                &mut index_counter,
                &mut indices,
                &mut lowlinks,
                &mut on_stack,
                &mut stack,
                &mut sccs,
            );
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

    SccResult {
        components,
        non_trivial_count,
        max_component_size,
        node_to_component,
    }
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
            endpoint_node_index(&edge.from),
            endpoint_node_index(&edge.to),
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
    /// Edge indices participating in this cycle.
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
            endpoint_node_index(&edge.from),
            endpoint_node_index(&edge.to),
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

    #[allow(clippy::too_many_arguments)] // DFS state is necessarily coupled
    fn dfs_visit(
        v: usize,
        adj: &[Vec<(usize, usize)>],
        color: &mut [Color],
        parent: &mut [Option<usize>],
        parent_edge: &mut [Option<usize>],
        cycles: &mut Vec<DirectedCycle>,
        cyclic_nodes: &mut BTreeSet<usize>,
        cyclic_edges: &mut BTreeSet<usize>,
    ) {
        color[v] = Color::Gray;

        for &(w, edge_idx) in &adj[v] {
            if color[w] == Color::Gray {
                // Back edge found - extract cycle
                let mut cycle_nodes = vec![w];
                let mut cycle_edges = vec![edge_idx];
                let mut curr = v;

                while curr != w {
                    cycle_nodes.push(curr);
                    if let Some(pe) = parent_edge[curr] {
                        cycle_edges.push(pe);
                    }
                    curr = parent[curr].unwrap_or(w);
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
                dfs_visit(
                    w,
                    adj,
                    color,
                    parent,
                    parent_edge,
                    cycles,
                    cyclic_nodes,
                    cyclic_edges,
                );
            }
        }

        color[v] = Color::Black;
    }

    // Start DFS from each unvisited node (in sorted order)
    for v in 0..n {
        if color[v] == Color::White {
            dfs_visit(
                v,
                &adj,
                &mut color,
                &mut parent,
                &mut parent_edge,
                &mut cycles,
                &mut cyclic_nodes,
                &mut cyclic_edges,
            );
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
            endpoint_node_index(&edge.from),
            endpoint_node_index(&edge.to),
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, IrEdge, IrNode};

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
}
