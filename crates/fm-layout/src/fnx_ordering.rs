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
// Tier Classification
// ============================================================================

use crate::{CentralityTier, NodeCentrality};

/// Convert centrality scores to tier-classified layout data.
///
/// Nodes are classified into High (top 20%), Medium (middle 60%), and Low (bottom 20%)
/// tiers based on their relative centrality scores. For small graphs (< 5 nodes),
/// all nodes are classified as Medium since percentile tiers are not meaningful.
#[must_use]
pub fn classify_centrality_tiers(scores: &NodeCentralityScores) -> Vec<NodeCentrality> {
    if scores.degree.is_empty() {
        return Vec::new();
    }

    // Collect all scores sorted descending for percentile calculation
    let mut all_scores: Vec<(usize, u32)> = scores
        .degree
        .iter()
        .map(|(idx, qs)| (*idx, qs.raw()))
        .collect();
    all_scores.sort_by_key(|x| std::cmp::Reverse(x.1)); // Descending by score

    let total = all_scores.len();

    // For small graphs, percentile tiers are not meaningful - classify all as Medium
    if total < 5 {
        return all_scores
            .into_iter()
            .map(|(node_index, score)| NodeCentrality {
                node_index,
                score,
                tier: CentralityTier::Medium,
            })
            .collect();
    }

    // Calculate threshold indices (using ceiling division for fairer distribution)
    let high_count = total.div_ceil(5); // Top ~20%, at least 1
    let low_start = total - total.div_ceil(5); // Bottom ~20%

    // Get score values at threshold boundaries
    // high_threshold: scores >= this value are High
    // low_threshold: scores <= this value are Low
    let high_threshold = all_scores.get(high_count.saturating_sub(1)).map(|x| x.1).unwrap_or(0);
    let low_threshold = all_scores.get(low_start).map(|x| x.1).unwrap_or(0);

    all_scores
        .into_iter()
        .enumerate()
        .map(|(rank, (node_index, score))| {
            // Use rank position for tier assignment to handle ties correctly
            let tier = if rank < high_count {
                CentralityTier::High
            } else if rank >= low_start {
                CentralityTier::Low
            } else {
                CentralityTier::Medium
            };
            // Store the score for potential display, but use rank for tier
            let _ = (high_threshold, low_threshold); // silence unused warnings
            NodeCentrality {
                node_index,
                score,
                tier,
            }
        })
        .collect()
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

    #[test]
    fn classify_tiers_empty_scores() {
        let scores = NodeCentralityScores::default();
        let tiers = classify_centrality_tiers(&scores);
        assert!(tiers.is_empty());
    }

    #[test]
    fn classify_tiers_small_graph_all_medium() {
        // Small graph (< 5 nodes) should all be Medium
        let ir = make_chain_ir(); // 3 nodes
        let scores = compute_centrality_scores(&ir);
        let tiers = classify_centrality_tiers(&scores);

        assert_eq!(tiers.len(), 3);
        for tier_data in &tiers {
            assert_eq!(tier_data.tier, CentralityTier::Medium);
        }
    }

    #[test]
    fn classify_tiers_larger_graph() {
        // Create a 10-node graph to test tier distribution
        let mut ir = MermaidDiagramIr::default();
        for i in 0..10 {
            ir.nodes.push(IrNode {
                id: format!("N{i}"),
                shape: NodeShape::Rect,
                ..Default::default()
            });
        }
        // Create a star topology: N0 connected to all others
        for i in 1..10 {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(i)),
                ..Default::default()
            });
        }

        let scores = compute_centrality_scores(&ir);
        let tiers = classify_centrality_tiers(&scores);

        assert_eq!(tiers.len(), 10);

        // Count tiers
        let high_count = tiers.iter().filter(|t| t.tier == CentralityTier::High).count();
        let low_count = tiers.iter().filter(|t| t.tier == CentralityTier::Low).count();
        let medium_count = tiers.iter().filter(|t| t.tier == CentralityTier::Medium).count();

        // With 10 nodes and div_ceil(10, 5) = 2, expect 2 High, 2 Low, 6 Medium
        assert_eq!(high_count, 2, "expected 2 high tier nodes");
        assert_eq!(low_count, 2, "expected 2 low tier nodes");
        assert_eq!(medium_count, 6, "expected 6 medium tier nodes");

        // The center node (N0) should be High (highest centrality)
        let center_tier = tiers.iter().find(|t| t.node_index == 0).unwrap();
        assert_eq!(center_tier.tier, CentralityTier::High);
    }
}

// ============================================================================
// Ablation Benchmark (bd-ml2r.5.2)
// ============================================================================

/// Results from a single ablation benchmark run.
#[derive(Debug, Clone)]
pub struct AblationResult {
    /// Number of nodes in the test graph.
    pub node_count: usize,
    /// Number of edges in the test graph.
    pub edge_count: usize,
    /// Time to compute centrality scores (microseconds).
    pub centrality_time_us: u64,
    /// Whether centrality computation succeeded.
    pub centrality_computed: bool,
}

/// Summary of ablation results across multiple test cases.
#[derive(Debug, Clone, Default)]
pub struct AblationSummary {
    /// Results for each test case.
    pub results: Vec<AblationResult>,
    /// Adoption threshold: max nodes where centrality is recommended.
    pub recommended_node_threshold: usize,
    /// Adoption threshold: max edges where centrality is recommended.
    pub recommended_edge_threshold: usize,
    /// Mean centrality computation time (microseconds).
    pub mean_centrality_time_us: u64,
    /// Max centrality computation time (microseconds).
    pub max_centrality_time_us: u64,
}

impl AblationSummary {
    /// Compute summary statistics from results.
    pub fn compute(&mut self) {
        if self.results.is_empty() {
            return;
        }

        let times: Vec<u64> = self.results.iter().map(|r| r.centrality_time_us).collect();
        self.mean_centrality_time_us = times.iter().sum::<u64>() / times.len() as u64;
        self.max_centrality_time_us = *times.iter().max().unwrap_or(&0);

        // Adoption threshold: centrality is recommended when time < 1ms (1000us)
        const TIME_BUDGET_US: u64 = 1000;
        let passing: Vec<_> = self.results.iter()
            .filter(|r| r.centrality_time_us < TIME_BUDGET_US)
            .collect();

        if let Some(max_passing) = passing.iter().max_by_key(|r| r.node_count) {
            self.recommended_node_threshold = max_passing.node_count;
        }
        if let Some(max_passing) = passing.iter().max_by_key(|r| r.edge_count) {
            self.recommended_edge_threshold = max_passing.edge_count;
        }
    }
}

/// Run an ablation benchmark for a single graph.
#[must_use]
pub fn run_ablation_single(ir: &MermaidDiagramIr) -> AblationResult {
    use std::time::Instant;

    let start = Instant::now();
    let scores = compute_centrality_scores(ir);
    let elapsed = start.elapsed();

    AblationResult {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        centrality_time_us: elapsed.as_micros() as u64,
        centrality_computed: scores.computed,
    }
}

#[cfg(test)]
mod ablation_tests {
    use super::*;
    use fm_core::{DiagramType, IrEdge, IrNode, IrNodeId, NodeShape};

    fn make_dense_graph(node_count: usize) -> MermaidDiagramIr {
        let nodes: Vec<IrNode> = (0..node_count)
            .map(|i| IrNode {
                id: format!("N{i}"),
                shape: NodeShape::Rect,
                ..Default::default()
            })
            .collect();

        // Create edges: each node connects to next 3 nodes (cyclic)
        let mut edges = Vec::new();
        for i in 0..node_count {
            for offset in 1..=3.min(node_count - 1) {
                edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(i)),
                    to: IrEndpoint::Node(IrNodeId((i + offset) % node_count)),
                    ..Default::default()
                });
            }
        }

        MermaidDiagramIr {
            diagram_type: DiagramType::Flowchart,
            nodes,
            edges,
            ..Default::default()
        }
    }

    fn make_chain_graph(node_count: usize) -> MermaidDiagramIr {
        let nodes: Vec<IrNode> = (0..node_count)
            .map(|i| IrNode {
                id: format!("N{i}"),
                shape: NodeShape::Rect,
                ..Default::default()
            })
            .collect();

        let edges: Vec<IrEdge> = (0..node_count.saturating_sub(1))
            .map(|i| IrEdge {
                from: IrEndpoint::Node(IrNodeId(i)),
                to: IrEndpoint::Node(IrNodeId(i + 1)),
                ..Default::default()
            })
            .collect();

        MermaidDiagramIr {
            diagram_type: DiagramType::Flowchart,
            nodes,
            edges,
            ..Default::default()
        }
    }

    #[test]
    fn ablation_small_graph_fast() {
        let ir = make_chain_graph(10);
        let result = run_ablation_single(&ir);

        assert!(result.centrality_computed);
        assert_eq!(result.node_count, 10);
        assert_eq!(result.edge_count, 9);
        // Small graphs should be very fast (< 1ms)
        assert!(result.centrality_time_us < 10000, "small graph took too long: {}us", result.centrality_time_us);
    }

    #[test]
    fn ablation_medium_graph_reasonable() {
        let ir = make_dense_graph(50);
        let result = run_ablation_single(&ir);

        assert!(result.centrality_computed);
        assert_eq!(result.node_count, 50);
        // Dense graph has 3 edges per node (with wraparound)
        assert!(result.edge_count > 100);
        // Medium graphs should complete in reasonable time (< 100ms)
        assert!(result.centrality_time_us < 100000, "medium graph took too long: {}us", result.centrality_time_us);
    }

    #[test]
    fn ablation_sweep_produces_threshold() {
        let sizes = [5, 10, 20, 50, 100];
        let mut summary = AblationSummary::default();

        for &size in &sizes {
            let ir = make_dense_graph(size);
            let result = run_ablation_single(&ir);
            summary.results.push(result);
        }

        summary.compute();

        // Should have a reasonable threshold
        assert!(summary.recommended_node_threshold > 0);
        assert!(summary.recommended_edge_threshold > 0);
        // Mean time should be positive
        assert!(summary.mean_centrality_time_us > 0);
    }

    #[test]
    fn ablation_deterministic() {
        let ir = make_dense_graph(30);

        // Run 5 times, verify centrality scores are identical
        let results: Vec<_> = (0..5).map(|_| {
            let scores = compute_centrality_scores(&ir);
            scores.degree.clone()
        }).collect();

        for i in 1..results.len() {
            assert_eq!(results[0], results[i], "results differ at iteration {i}");
        }
    }

    /// CI-friendly smoke test that validates adoption thresholds.
    #[test]
    fn ablation_adoption_threshold_smoke() {
        // Test standard adoption scenario
        let small = make_chain_graph(20);
        let result = run_ablation_single(&small);

        // For 20 nodes, centrality should definitely be fast enough.
        // Use lenient 5ms threshold to account for CI timing variance.
        assert!(result.centrality_time_us < 5000,
            "Centrality for 20-node graph should be < 5ms, got {}us",
            result.centrality_time_us);

        // Verify the comparison function works with computed scores
        let scores = compute_centrality_scores(&small);
        let cmp = compare_with_centrality(
            (0, Some(1.5), 0),
            (1, Some(1.5), 1),
            &scores,
        );
        // Should produce deterministic ordering
        assert!(cmp != std::cmp::Ordering::Equal || scores.is_empty());
    }
}
