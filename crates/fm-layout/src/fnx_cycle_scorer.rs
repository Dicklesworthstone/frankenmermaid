//! FNX-powered edge criticality scoring for cycle removal.
//!
//! This module provides scoring functions that use graph-theoretic analysis
//! to rank candidate edge reversals by their structural criticality.
//!
//! Higher scores indicate edges that should be preserved (not reversed):
//! - Bridge edges: Removing them disconnects the graph
//! - Edges with articulation point endpoints: Critical for connectivity
//! - High-centrality edges: Important for overall graph structure
//!
//! The scoring is deterministic and integrates with existing cycle removal
//! strategies as a penalty modifier.

use std::collections::BTreeMap;

use fm_core::{IrEndpoint, IrPort, MermaidDiagramIr};
use fnx_algorithms::{
    ArticulationPointsResult, BridgesResult, DegreeCentralityResult, articulation_points, bridges,
    degree_centrality,
};

use crate::fnx_adapter::{ProjectionTable, ir_to_graph};

// ============================================================================
// Scoring Configuration
// ============================================================================

/// Configuration for edge criticality scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct CriticalityScoringConfig {
    /// Weight for bridge penalty (0.0 to 1.0).
    pub bridge_weight: f64,
    /// Weight for articulation point endpoint penalty (0.0 to 1.0).
    pub articulation_weight: f64,
    /// Weight for degree centrality penalty (0.0 to 1.0).
    pub centrality_weight: f64,
    /// Base penalty to add to all edges (ensures minimum score > 0).
    pub base_penalty: f64,
}

impl Default for CriticalityScoringConfig {
    fn default() -> Self {
        Self {
            bridge_weight: 0.5,
            articulation_weight: 0.3,
            centrality_weight: 0.2,
            base_penalty: 0.1,
        }
    }
}

// ============================================================================
// Criticality Score
// ============================================================================

/// Computed criticality score for an edge.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeCriticalityScore {
    /// Edge index in the IR.
    pub edge_index: usize,
    /// Whether this edge is a bridge.
    pub is_bridge: bool,
    /// Whether the source node is an articulation point.
    pub source_is_articulation: bool,
    /// Whether the target node is an articulation point.
    pub target_is_articulation: bool,
    /// Source node degree centrality (0.0 to 1.0).
    pub source_centrality: f64,
    /// Target node degree centrality (0.0 to 1.0).
    pub target_centrality: f64,
    /// Final composite score (higher = more critical, avoid reversing).
    pub score: f64,
}

// ============================================================================
// Scoring Results
// ============================================================================

/// Results from computing edge criticality scores.
#[derive(Debug, Clone, Default)]
pub struct CriticalityScoringResults {
    /// Scores indexed by IR edge index.
    pub scores: BTreeMap<usize, EdgeCriticalityScore>,
    /// Whether FNX analysis was available.
    pub fnx_available: bool,
    /// Number of bridges detected.
    pub bridge_count: usize,
    /// Number of articulation points detected.
    pub articulation_count: usize,
}

impl CriticalityScoringResults {
    /// Get the criticality score for an edge, or a default low score if unavailable.
    #[must_use]
    pub fn get_score(&self, edge_index: usize) -> f64 {
        self.scores.get(&edge_index).map(|s| s.score).unwrap_or(0.0)
    }

    /// Sort edges by criticality (ascending = reverse least critical first).
    #[must_use]
    pub fn edges_by_criticality_ascending(&self) -> Vec<usize> {
        let mut edges: Vec<_> = self.scores.keys().copied().collect();
        edges.sort_by(|a, b| {
            let score_a = self.get_score(*a);
            let score_b = self.get_score(*b);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b)) // Deterministic tie-break by index
        });
        edges
    }
}

// ============================================================================
// Scoring Function
// ============================================================================

/// Compute edge criticality scores using FNX graph analysis.
///
/// This function analyzes the graph structure and assigns each edge a score
/// indicating how critical it is to preserve during cycle removal.
#[must_use]
pub fn compute_criticality_scores(
    ir: &MermaidDiagramIr,
    config: &CriticalityScoringConfig,
) -> CriticalityScoringResults {
    if ir.nodes.is_empty() || ir.edges.is_empty() {
        return CriticalityScoringResults::default();
    }

    let (graph, table) = ir_to_graph(ir);

    // Run FNX analyses
    let bridges_result = bridges(&graph);
    let articulation_result = articulation_points(&graph);
    let centrality_result = degree_centrality(&graph);

    let mut results = CriticalityScoringResults {
        fnx_available: true,
        bridge_count: 0,
        articulation_count: articulation_result.nodes.len(),
        ..Default::default()
    };

    // Build lookup sets for fast checking
    let bridge_set = build_bridge_set(&bridges_result, &table);
    let articulation_set = build_articulation_set(&articulation_result, &table);
    let centrality_map = build_centrality_map(&centrality_result, &table);
    let pair_counts = build_edge_pair_counts(ir);

    // Score each edge
    for (edge_idx, edge) in ir.edges.iter().enumerate() {
        let source_idx = endpoint_node_index(edge.from, &ir.ports);
        let target_idx = endpoint_node_index(edge.to, &ir.ports);

        let (source_idx, target_idx) = match (source_idx, target_idx) {
            (Some(s), Some(t)) => (s, t),
            _ => continue, // Skip unresolved edges
        };

        let pair = (source_idx.min(target_idx), source_idx.max(target_idx));
        let multiplicity = pair_counts.get(&pair).copied().unwrap_or(1);
        let is_bridge = source_idx != target_idx && multiplicity == 1 && bridge_set.contains(&pair);
        let source_is_articulation = articulation_set.contains(&source_idx);
        let target_is_articulation = articulation_set.contains(&target_idx);
        let source_centrality = centrality_map.get(&source_idx).copied().unwrap_or(0.0);
        let target_centrality = centrality_map.get(&target_idx).copied().unwrap_or(0.0);

        // Compute composite score
        let bridge_penalty = if is_bridge { 1.0 } else { 0.0 };
        let articulation_penalty = match (source_is_articulation, target_is_articulation) {
            (true, true) => 1.0,
            (true, false) | (false, true) => 0.5,
            (false, false) => 0.0,
        };
        let centrality_penalty = (source_centrality + target_centrality) / 2.0;

        let score = config.base_penalty
            + config.bridge_weight * bridge_penalty
            + config.articulation_weight * articulation_penalty
            + config.centrality_weight * centrality_penalty;

        if is_bridge {
            results.bridge_count += 1;
        }

        results.scores.insert(
            edge_idx,
            EdgeCriticalityScore {
                edge_index: edge_idx,
                is_bridge,
                source_is_articulation,
                target_is_articulation,
                source_centrality,
                target_centrality,
                score,
            },
        );
    }

    results
}

fn endpoint_node_index(endpoint: IrEndpoint, ports: &[IrPort]) -> Option<usize> {
    endpoint.resolved_node_id(ports).map(|id| id.0)
}

fn build_bridge_set(
    result: &BridgesResult,
    table: &ProjectionTable,
) -> std::collections::BTreeSet<(usize, usize)> {
    result
        .edges
        .iter()
        .filter_map(|(source_fnx, target_fnx)| {
            let source_idx = table.get_ir_node_index(source_fnx)?;
            let target_idx = table.get_ir_node_index(target_fnx)?;
            Some((source_idx.min(target_idx), source_idx.max(target_idx)))
        })
        .collect()
}

fn build_articulation_set(
    result: &ArticulationPointsResult,
    table: &ProjectionTable,
) -> std::collections::BTreeSet<usize> {
    result
        .nodes
        .iter()
        .filter_map(|fnx_id| table.get_ir_node_index(fnx_id))
        .collect()
}

fn build_centrality_map(
    result: &DegreeCentralityResult,
    table: &ProjectionTable,
) -> BTreeMap<usize, f64> {
    result
        .scores
        .iter()
        .filter_map(|score| {
            let ir_idx = table.get_ir_node_index(&score.node)?;
            Some((ir_idx, score.score))
        })
        .collect()
}

fn build_edge_pair_counts(ir: &MermaidDiagramIr) -> BTreeMap<(usize, usize), usize> {
    let mut counts = BTreeMap::new();
    for edge in &ir.edges {
        let Some(source_idx) = endpoint_node_index(edge.from, &ir.ports) else {
            continue;
        };
        let Some(target_idx) = endpoint_node_index(edge.to, &ir.ports) else {
            continue;
        };
        let pair = (source_idx.min(target_idx), source_idx.max(target_idx));
        *counts.entry(pair).or_insert(0) += 1;
    }
    counts
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{IrEdge, IrNode, IrNodeId, IrPort, IrPortId, NodeShape};

    fn make_chain_ir() -> MermaidDiagramIr {
        // A -> B -> C (chain, both edges are bridges)
        MermaidDiagramIr {
            nodes: vec![
                IrNode {
                    id: "A".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "B".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "C".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
            ],
            edges: vec![
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(1)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(1)),
                    to: IrEndpoint::Node(IrNodeId(2)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn make_triangle_ir() -> MermaidDiagramIr {
        // A -> B -> C -> A (triangle cycle, no bridges)
        MermaidDiagramIr {
            nodes: vec![
                IrNode {
                    id: "A".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "B".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "C".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
            ],
            edges: vec![
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(1)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(1)),
                    to: IrEndpoint::Node(IrNodeId(2)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(2)),
                    to: IrEndpoint::Node(IrNodeId(0)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn make_parallel_edges_ir() -> MermaidDiagramIr {
        MermaidDiagramIr {
            nodes: vec![
                IrNode {
                    id: "A".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "B".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
            ],
            edges: vec![
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(1)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(1)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn make_port_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr {
            nodes: vec![
                IrNode {
                    id: "A".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "B".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        ir.ports = vec![
            IrPort {
                node: IrNodeId(0),
                name: "p0".to_string(),
                ..Default::default()
            },
            IrPort {
                node: IrNodeId(1),
                name: "p1".to_string(),
                ..Default::default()
            },
            IrPort {
                node: IrNodeId(0),
                name: "p2".to_string(),
                ..Default::default()
            },
        ];

        ir.edges = vec![IrEdge {
            from: IrEndpoint::Port(IrPortId(2)),
            to: IrEndpoint::Port(IrPortId(1)),
            ..Default::default()
        }];

        ir
    }

    #[test]
    fn empty_graph_returns_empty_scores() {
        let ir = MermaidDiagramIr::default();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        assert!(results.scores.is_empty());
        assert!(!results.fnx_available);
    }

    #[test]
    fn chain_edges_are_bridges() {
        let ir = make_chain_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        assert!(results.fnx_available);
        assert_eq!(results.bridge_count, 2);

        // Both edges should be marked as bridges
        for edge_idx in 0..2 {
            let score = results.scores.get(&edge_idx).expect("edge score");
            assert!(score.is_bridge, "edge {edge_idx} should be a bridge");
        }
    }

    #[test]
    fn port_endpoints_resolve_to_parent_nodes() {
        let ir = make_port_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        let score = results.scores.get(&0).expect("edge score");
        assert!(score.is_bridge, "edge between ports should be a bridge");
    }

    #[test]
    fn triangle_has_no_bridges() {
        let ir = make_triangle_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        assert!(results.fnx_available);
        assert_eq!(results.bridge_count, 0);

        // No edges should be bridges
        for edge_idx in 0..3 {
            let score = results.scores.get(&edge_idx).expect("edge score");
            assert!(!score.is_bridge, "edge {edge_idx} should not be a bridge");
        }
    }

    #[test]
    fn parallel_edges_are_not_bridges() {
        let ir = make_parallel_edges_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        assert_eq!(
            results.bridge_count, 0,
            "parallel edges should not count as bridges"
        );
        for edge_idx in 0..2 {
            let score = results.scores.get(&edge_idx).expect("edge score");
            assert!(
                !score.is_bridge,
                "parallel edge {edge_idx} should not be a bridge"
            );
        }
    }

    #[test]
    fn chain_middle_node_is_articulation_point() {
        let ir = make_chain_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        // B (index 1) is an articulation point
        assert_eq!(results.articulation_count, 1);

        // Edge 0 (A->B) has B as target
        let score_0 = results.scores.get(&0).expect("edge 0 score");
        assert!(score_0.target_is_articulation);

        // Edge 1 (B->C) has B as source
        let score_1 = results.scores.get(&1).expect("edge 1 score");
        assert!(score_1.source_is_articulation);
    }

    #[test]
    fn bridge_edges_have_higher_scores() {
        let ir = make_chain_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        // All edges in chain are bridges and should have higher scores
        for edge_idx in 0..2 {
            let score = results.get_score(edge_idx);
            // Score should be > base penalty due to bridge weight
            assert!(
                score > config.base_penalty,
                "bridge edge should have score > base"
            );
        }
    }

    #[test]
    fn scoring_is_deterministic() {
        let ir = make_triangle_ir();
        let config = CriticalityScoringConfig::default();

        // Run 5 times and verify scores are identical
        let results: Vec<_> = (0..5)
            .map(|_| compute_criticality_scores(&ir, &config))
            .collect();

        for i in 1..results.len() {
            for edge_idx in 0..3 {
                let score_0 = results[0].get_score(edge_idx);
                let score_i = results[i].get_score(edge_idx);
                assert!(
                    (score_0 - score_i).abs() < 1e-10,
                    "score mismatch at edge {edge_idx}, iteration {i}"
                );
            }
        }
    }

    #[test]
    fn edges_by_criticality_ascending_stable_ordering() {
        let ir = make_chain_ir();
        let config = CriticalityScoringConfig::default();
        let results = compute_criticality_scores(&ir, &config);

        let sorted = results.edges_by_criticality_ascending();

        // Verify ordering is deterministic
        for _ in 0..5 {
            let sorted_again = results.edges_by_criticality_ascending();
            assert_eq!(sorted, sorted_again);
        }
    }

    #[test]
    fn config_weights_affect_scoring() {
        let ir = make_chain_ir();

        // High bridge weight
        let config_high_bridge = CriticalityScoringConfig {
            bridge_weight: 1.0,
            articulation_weight: 0.0,
            centrality_weight: 0.0,
            base_penalty: 0.0,
        };
        let results_high = compute_criticality_scores(&ir, &config_high_bridge);

        // Low bridge weight
        let config_low_bridge = CriticalityScoringConfig {
            bridge_weight: 0.1,
            articulation_weight: 0.0,
            centrality_weight: 0.0,
            base_penalty: 0.0,
        };
        let results_low = compute_criticality_scores(&ir, &config_low_bridge);

        // Bridge edges should have higher scores with high bridge weight
        for edge_idx in 0..2 {
            let score_high = results_high.get_score(edge_idx);
            let score_low = results_low.get_score(edge_idx);
            assert!(
                score_high > score_low,
                "high bridge weight should produce higher scores"
            );
        }
    }
}
