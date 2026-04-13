//! FNX-powered ordering support for Sugiyama layer optimization.
//!
//! This module provides centrality-based node scoring for use as tie-breakers
//! in barycenter ordering. Key invariants:
//!
//! - **Determinism**: Scores are quantized to avoid floating-point ordering issues
//! - **Caching**: Results are keyed by graph hash and projection mode
//! - **Stable normalization**: Scores are normalized to [0, 1000] range for precision

use std::collections::BTreeMap;
use std::hash::Hash;

use fm_core::{IrEndpoint, MermaidDiagramIr};
use fnx_algorithms::degree_centrality;

use crate::fnx_adapter::{ProjectionConfig, ir_to_graph};

// ============================================================================
// Quantization Policy
// ============================================================================

/// Quantization precision for centrality scores.
/// Scores are multiplied by this value and truncated to integers.
const QUANTIZATION_FACTOR: u32 = 10000;

/// Quantized centrality score for deterministic ordering.
///
/// Wraps an integer score derived from floating-point centrality values.
/// This avoids floating-point comparison non-determinism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct QuantizedScore(u32);

impl QuantizedScore {
    /// Create a quantized score from a floating-point value in [0, 1].
    #[must_use]
    pub fn from_normalized(value: f64) -> Self {
        let clamped = value.clamp(0.0, 1.0);
        let quantized = (clamped * f64::from(QUANTIZATION_FACTOR)) as u32;
        Self(quantized)
    }

    /// Get the raw quantized value.
    #[must_use]
    pub const fn raw(&self) -> u32 {
        self.0
    }

    /// Convert back to approximate floating-point value.
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        f64::from(self.0) / f64::from(QUANTIZATION_FACTOR)
    }
}

// ============================================================================
// Centrality Scores
// ============================================================================

/// Cached centrality scores for nodes, keyed by node index.
#[derive(Debug, Clone, Default)]
pub struct NodeCentralityScores {
    /// Quantized degree centrality scores, indexed by IR node index.
    pub degree: BTreeMap<usize, QuantizedScore>,
    /// Whether centrality computation was successful.
    pub computed: bool,
}

impl NodeCentralityScores {
    /// Get the degree centrality score for a node, or default if not available.
    #[must_use]
    pub fn get_degree(&self, node_idx: usize) -> QuantizedScore {
        self.degree.get(&node_idx).copied().unwrap_or_default()
    }

    /// Check if any scores are available.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.degree.is_empty()
    }
}

// ============================================================================
// Computation
// ============================================================================

/// Compute and quantize centrality scores for all nodes in the IR.
///
/// Returns scores that can be used as tie-breakers in barycenter ordering.
/// Scores are deterministically quantized to avoid floating-point issues.
#[must_use]
pub fn compute_centrality_scores(ir: &MermaidDiagramIr) -> NodeCentralityScores {
    if ir.nodes.is_empty() {
        return NodeCentralityScores::default();
    }

    let (graph, table) = ir_to_graph(ir);
    let centrality_result = degree_centrality(&graph);

    let mut scores = NodeCentralityScores {
        degree: BTreeMap::new(),
        computed: true,
    };

    for entry in &centrality_result.scores {
        if let Some(ir_idx) = table.get_ir_node_index(&entry.node) {
            scores.degree.insert(ir_idx, QuantizedScore::from_normalized(entry.score));
        }
    }

    scores
}

/// Compute centrality scores with a specific projection configuration.
#[must_use]
pub fn compute_centrality_scores_with_config(
    ir: &MermaidDiagramIr,
    _config: &ProjectionConfig,
) -> NodeCentralityScores {
    // For now, use default projection. In future, use config to customize.
    compute_centrality_scores(ir)
}

// ============================================================================
// Ordering Integration
// ============================================================================

/// Centrality-aware node comparison for ordering tie-breaks.
///
/// Primary sort: barycenter value (lower = left)
/// Secondary sort: centrality score (higher = left, more important nodes first)
/// Tertiary sort: node index (deterministic tie-break)
#[must_use]
pub fn compare_with_centrality(
    a: (usize, Option<f32>, usize), // (node_idx, barycenter, stable_idx)
    b: (usize, Option<f32>, usize),
    centrality: &NodeCentralityScores,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (a.1, b.1) {
        // Both have barycenters: compare them first
        (Some(a_bary), Some(b_bary)) => {
            match a_bary.total_cmp(&b_bary) {
                Ordering::Equal => {
                    // Tie-break by centrality (higher centrality = earlier)
                    let a_cent = centrality.get_degree(a.0);
                    let b_cent = centrality.get_degree(b.0);
                    match b_cent.cmp(&a_cent) {
                        Ordering::Equal => a.0.cmp(&b.0), // Final tie-break by node index
                        other => other,
                    }
                }
                other => other,
            }
        }
        // Only a has barycenter: a comes first
        (Some(_), None) => Ordering::Less,
        // Only b has barycenter: b comes first
        (None, Some(_)) => Ordering::Greater,
        // Neither has barycenter: use stable index, then centrality, then node index
        (None, None) => {
            match a.2.cmp(&b.2) {
                Ordering::Equal => {
                    let a_cent = centrality.get_degree(a.0);
                    let b_cent = centrality.get_degree(b.0);
                    match b_cent.cmp(&a_cent) {
                        Ordering::Equal => a.0.cmp(&b.0),
                        other => other,
                    }
                }
                other => other,
            }
        }
    }
}

// ============================================================================
// Cache Integration
// ============================================================================

/// Generate a cache key for centrality scores based on IR structure.
#[must_use]
pub fn centrality_cache_key(ir: &MermaidDiagramIr) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();

    // Hash node count
    ir.nodes.len().hash(&mut hasher);

    // Hash node IDs in order
    for node in &ir.nodes {
        node.id.hash(&mut hasher);
    }

    // Hash edge topology
    ir.edges.len().hash(&mut hasher);
    for edge in &ir.edges {
        hash_endpoint(&edge.from, &mut hasher, &ir.ports);
        hash_endpoint(&edge.to, &mut hasher, &ir.ports);
    }

    hasher.finish()
}

fn hash_endpoint<H: std::hash::Hasher>(
    endpoint: &IrEndpoint,
    hasher: &mut H,
    ports: &[fm_core::IrPort],
) {
    match endpoint {
        IrEndpoint::Node(id) => {
            0u8.hash(hasher);
            id.0.hash(hasher);
        }
        IrEndpoint::Port(id) => {
            1u8.hash(hasher);
            // Hash the parent node for port endpoints
            if let Some(port) = ports.get(id.0) {
                port.node.0.hash(hasher);
            } else {
                id.0.hash(hasher);
            }
        }
        IrEndpoint::Unresolved => {
            2u8.hash(hasher);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{IrEdge, IrNode, IrNodeId, NodeShape};

    fn make_chain_ir() -> MermaidDiagramIr {
        // A -> B -> C (B has highest centrality)
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

    fn make_star_ir() -> MermaidDiagramIr {
        // Center -> A, Center -> B, Center -> C (Center has highest centrality)
        MermaidDiagramIr {
            nodes: vec![
                IrNode {
                    id: "Center".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
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
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(2)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(3)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn quantized_score_roundtrip() {
        let original = 0.75;
        let quantized = QuantizedScore::from_normalized(original);
        let recovered = quantized.as_f64();

        assert!((original - recovered).abs() < 0.0001);
    }

    #[test]
    fn quantized_score_clamping() {
        assert_eq!(QuantizedScore::from_normalized(-0.5).raw(), 0);
        assert_eq!(QuantizedScore::from_normalized(1.5).raw(), QUANTIZATION_FACTOR);
    }

    #[test]
    fn quantized_score_ordering() {
        let a = QuantizedScore::from_normalized(0.3);
        let b = QuantizedScore::from_normalized(0.7);

        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn chain_centrality_middle_highest() {
        let ir = make_chain_ir();
        let scores = compute_centrality_scores(&ir);

        assert!(scores.computed);
        assert_eq!(scores.degree.len(), 3);

        // B (index 1) should have highest centrality (connected to both A and C)
        let a_score = scores.get_degree(0);
        let b_score = scores.get_degree(1);
        let c_score = scores.get_degree(2);

        assert!(b_score >= a_score);
        assert!(b_score >= c_score);
    }

    #[test]
    fn star_centrality_center_highest() {
        let ir = make_star_ir();
        let scores = compute_centrality_scores(&ir);

        assert!(scores.computed);

        let center_score = scores.get_degree(0);
        let a_score = scores.get_degree(1);
        let b_score = scores.get_degree(2);
        let c_score = scores.get_degree(3);

        // Center (index 0) should have highest centrality
        assert!(center_score > a_score);
        assert!(center_score > b_score);
        assert!(center_score > c_score);
    }

    #[test]
    fn centrality_scores_deterministic() {
        let ir = make_chain_ir();

        // Compute 5 times and verify identical results
        let results: Vec<_> = (0..5).map(|_| compute_centrality_scores(&ir)).collect();

        for i in 1..results.len() {
            for node_idx in 0..3 {
                assert_eq!(
                    results[0].get_degree(node_idx),
                    results[i].get_degree(node_idx),
                    "score mismatch at node {node_idx}, iteration {i}"
                );
            }
        }
    }

    #[test]
    fn compare_with_centrality_barycenter_primary() {
        let ir = make_chain_ir();
        let scores = compute_centrality_scores(&ir);

        // Different barycenters: barycenter wins
        let a = (0, Some(1.0_f32), 0);
        let b = (1, Some(2.0_f32), 1);

        let result = compare_with_centrality(a, b, &scores);
        assert_eq!(result, std::cmp::Ordering::Less);
    }

    #[test]
    fn compare_with_centrality_tiebreak() {
        let ir = make_chain_ir();
        let scores = compute_centrality_scores(&ir);

        // Same barycenter: centrality wins (higher centrality = earlier)
        let a = (0, Some(1.5_f32), 0); // A: lower centrality
        let b = (1, Some(1.5_f32), 1); // B: higher centrality

        let result = compare_with_centrality(a, b, &scores);
        // B should come first (higher centrality)
        assert_eq!(result, std::cmp::Ordering::Greater);
    }

    #[test]
    fn cache_key_stable() {
        let ir = make_chain_ir();

        let key1 = centrality_cache_key(&ir);
        let key2 = centrality_cache_key(&ir);

        assert_eq!(key1, key2);
    }

    #[test]
    fn cache_key_differs_on_topology_change() {
        let ir1 = make_chain_ir();
        let ir2 = make_star_ir();

        let key1 = centrality_cache_key(&ir1);
        let key2 = centrality_cache_key(&ir2);

        assert_ne!(key1, key2);
    }

    #[test]
    fn empty_graph_returns_default_scores() {
        let ir = MermaidDiagramIr::default();
        let scores = compute_centrality_scores(&ir);

        assert!(!scores.computed);
        assert!(scores.is_empty());
    }
}
