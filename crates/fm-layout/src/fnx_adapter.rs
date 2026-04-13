//! FNX adapter: stable ID projection between MermaidDiagramIr and fnx graphs.
//!
//! This module provides deterministic, bidirectional mapping between fm-core IR
//! entities (nodes, edges, clusters) and fnx graph structures. Key invariants:
//!
//! - **Stable IDs**: IR index -> fnx node ID mapping is deterministic and collision-free
//! - **Reverse lookup**: fnx node ID -> IR index in O(log N) via BTreeMap
//! - **Metadata preservation**: edge weights, labels, and routing hints are preserved
//! - **Deterministic ordering**: all collections use BTreeMap/BTreeSet for stable iteration

use std::collections::BTreeMap;

use fm_core::{IrEndpoint, IrLabelId, MermaidDiagramIr};
use fnx_classes::Graph;
use fnx_classes::digraph::DiGraph;
use fnx_runtime::CgseValue;

// ============================================================================
// Projection Policy (bd-ml2r.2.2)
// ============================================================================

/// Policy for projecting directed edges to undirected graph representation.
///
/// Many fnx algorithms (e.g., community detection, centrality measures) operate on
/// undirected graphs. This policy determines how directed edges from the IR are
/// mapped to undirected edges while preserving semantic correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirectedProjectionPolicy {
    /// Ignore edge direction: A->B becomes {A,B}. Self-loops are preserved.
    /// - Use when: Algorithm treats connectivity as symmetric (e.g., community detection)
    /// - Valid diagnostics: connectivity, components, degree centrality
    /// - Invalid: reachability, dominator analysis, cycle detection semantics
    #[default]
    IgnoreDirection,

    /// Bidirectional expansion: A->B becomes both {A,B} with weight 0.5 each.
    /// - Use when: Need to preserve "there is a connection" while distributing weight
    /// - Valid diagnostics: flow approximation, weighted centrality
    /// - Invalid: exact reachability analysis
    BidirectionalHalfWeight,

    /// Bidirectional full weight: A->B becomes {A,B} with full original weight.
    /// - Use when: Treating directed edge as strong symmetric connection
    /// - Valid diagnostics: clustering coefficient, modularity
    /// - Invalid: directional flow analysis
    BidirectionalFullWeight,

    /// Penalty-weighted: A->B becomes {A,B} with configurable penalty factor.
    /// Applied as: undirected_weight = directed_weight * penalty_factor
    /// - Use when: Need to de-prioritize undirected analysis relative to directed
    /// - penalty_factor stored separately in ProjectionConfig
    PenaltyWeighted,
}

impl DirectedProjectionPolicy {
    /// Human-readable description for diagnostics/traces.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::IgnoreDirection => "ignore_direction",
            Self::BidirectionalHalfWeight => "bidirectional_half_weight",
            Self::BidirectionalFullWeight => "bidirectional_full_weight",
            Self::PenaltyWeighted => "penalty_weighted",
        }
    }

    /// List of diagnostic types that are valid under this projection policy.
    #[must_use]
    pub const fn valid_diagnostics(&self) -> &'static [&'static str] {
        match self {
            Self::IgnoreDirection => &[
                "connectivity",
                "components",
                "degree_centrality",
                "clustering_coefficient",
            ],
            Self::BidirectionalHalfWeight => {
                &["weighted_centrality", "flow_approximation", "betweenness"]
            }
            Self::BidirectionalFullWeight => &[
                "clustering_coefficient",
                "modularity",
                "community_detection",
            ],
            Self::PenaltyWeighted => &["weighted_centrality", "importance_ranking"],
        }
    }

    /// List of diagnostic types that are INVALID under this projection policy.
    #[must_use]
    pub const fn invalid_diagnostics(&self) -> &'static [&'static str] {
        match self {
            Self::IgnoreDirection => &[
                "reachability",
                "dominator_analysis",
                "directed_cycle_detection",
            ],
            Self::BidirectionalHalfWeight => &["exact_reachability", "topological_sort"],
            Self::BidirectionalFullWeight => &["directional_flow", "source_sink_analysis"],
            Self::PenaltyWeighted => &["exact_reachability", "directional_flow"],
        }
    }
}

/// Configuration for graph projection operations.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionConfig {
    /// Policy for directed->undirected projection.
    pub directed_policy: DirectedProjectionPolicy,
    /// Penalty factor for PenaltyWeighted policy (default 0.5).
    pub penalty_factor: f64,
    /// Whether to preserve self-loops in undirected projection.
    pub preserve_self_loops: bool,
    /// Whether to collapse parallel edges (sum weights) or keep first.
    pub collapse_parallel_edges: bool,
}

impl Default for ProjectionConfig {
    fn default() -> Self {
        Self {
            directed_policy: DirectedProjectionPolicy::default(),
            penalty_factor: 0.5,
            preserve_self_loops: true,
            collapse_parallel_edges: true,
        }
    }
}

/// Trace record for projection operations, surfaced in layout diagnostics.
#[derive(Debug, Clone, Default)]
pub struct ProjectionTrace {
    /// Which policy was applied.
    pub policy: &'static str,
    /// Number of edges in original directed graph.
    pub directed_edge_count: usize,
    /// Number of edges in resulting undirected graph.
    pub undirected_edge_count: usize,
    /// Number of self-loops preserved or removed.
    pub self_loop_count: usize,
    /// Number of parallel edge pairs collapsed.
    pub collapsed_parallel_count: usize,
    /// Diagnostics that are valid for this projection.
    pub valid_diagnostics: &'static [&'static str],
    /// Diagnostics that are INVALID for this projection.
    pub invalid_diagnostics: &'static [&'static str],
}

/// Build undirected fnx Graph from MermaidDiagramIr with configurable projection policy.
///
/// This is the policy-aware version of `ir_to_graph` that surfaces projection
/// decisions in the trace for diagnostic validation.
#[must_use]
pub fn ir_to_graph_with_policy(
    ir: &MermaidDiagramIr,
    config: &ProjectionConfig,
) -> (Graph, ProjectionTable, ProjectionTrace) {
    let mut graph = Graph::hardened();
    let mut table = ProjectionTable::from_ir(ir);
    let mut trace = ProjectionTrace {
        policy: config.directed_policy.as_str(),
        valid_diagnostics: config.directed_policy.valid_diagnostics(),
        invalid_diagnostics: config.directed_policy.invalid_diagnostics(),
        ..Default::default()
    };

    // Add nodes
    for (idx, node) in ir.nodes.iter().enumerate() {
        let fnx_id = ProjectedId::Node(idx).to_fnx_id();
        let mut attrs = BTreeMap::new();
        if let Some(label_text) = resolve_label(ir, node.label) {
            attrs.insert("label".to_string(), CgseValue::String(label_text));
        }
        attrs.insert("ir_index".to_string(), CgseValue::Int(idx as i64));
        let _ = graph.add_node_with_attrs(&fnx_id, attrs);
    }

    // Track edges for parallel edge detection
    let mut edge_pairs: BTreeMap<(usize, usize), Vec<(usize, f64)>> = BTreeMap::new();

    // Collect edges with policy-adjusted weights
    for (idx, edge) in ir.edges.iter().enumerate() {
        let Some(source_idx) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target_idx) = endpoint_node_index(ir, edge.to) else {
            continue;
        };

        trace.directed_edge_count += 1;

        // Check for self-loop
        if source_idx == target_idx {
            trace.self_loop_count += 1;
            if !config.preserve_self_loops {
                continue;
            }
        }

        // Compute weight based on policy
        let base_weight = 1.0;
        let weight = match config.directed_policy {
            DirectedProjectionPolicy::IgnoreDirection => base_weight,
            DirectedProjectionPolicy::BidirectionalHalfWeight => base_weight * 0.5,
            DirectedProjectionPolicy::BidirectionalFullWeight => base_weight,
            DirectedProjectionPolicy::PenaltyWeighted => base_weight * config.penalty_factor,
        };

        // Normalize edge pair (smaller index first for undirected)
        let pair = if source_idx <= target_idx {
            (source_idx, target_idx)
        } else {
            (target_idx, source_idx)
        };

        edge_pairs.entry(pair).or_default().push((idx, weight));
    }

    // Add edges to graph, handling parallel edges
    for ((source_idx, target_idx), edges) in &edge_pairs {
        let source_fnx = ProjectedId::Node(*source_idx).to_fnx_id();
        let target_fnx = ProjectedId::Node(*target_idx).to_fnx_id();

        if edges.len() > 1 {
            trace.collapsed_parallel_count += edges.len() - 1;
        }

        let (edge_idx, final_weight) = if config.collapse_parallel_edges && edges.len() > 1 {
            // Sum weights and use first edge's index
            let total_weight: f64 = edges.iter().map(|(_, w)| w).sum();
            (edges[0].0, total_weight)
        } else {
            edges[0]
        };

        let mut attrs = BTreeMap::new();
        attrs.insert("ir_edge_index".to_string(), CgseValue::Int(edge_idx as i64));
        attrs.insert("weight".to_string(), CgseValue::Float(final_weight));
        attrs.insert(
            "projection_policy".to_string(),
            CgseValue::String(config.directed_policy.as_str().to_string()),
        );
        let _ = graph.add_edge_with_attrs(&source_fnx, &target_fnx, attrs);

        let label_text = ir
            .edges
            .get(edge_idx)
            .and_then(|e| resolve_label(ir, e.label));
        let arrow_str = ir
            .edges
            .get(edge_idx)
            .map(|e| e.arrow.as_str().to_string())
            .unwrap_or_default();

        let meta = ProjectedEdgeMeta {
            ir_edge_index: edge_idx,
            source_index: *source_idx,
            target_index: *target_idx,
            label: label_text,
            arrow: arrow_str,
            weight: final_weight,
            reversed: false,
            is_long_edge: false,
        };
        table.add_edge_meta(&source_fnx, &target_fnx, meta);

        trace.undirected_edge_count += 1;
    }

    (graph, table, trace)
}

// ============================================================================
// Core Adapter Types
// ============================================================================

/// Resolve IR endpoint to node index.
fn endpoint_node_index(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> Option<usize> {
    match endpoint {
        IrEndpoint::Node(node) => {
            if node.0 < ir.nodes.len() {
                Some(node.0)
            } else {
                None
            }
        }
        IrEndpoint::Port(port) => {
            let node_idx = ir.ports.get(port.0).map(|port_ref| port_ref.node.0)?;
            if node_idx < ir.nodes.len() {
                Some(node_idx)
            } else {
                None
            }
        }
        IrEndpoint::Unresolved => None,
    }
}

/// Stable identifier for projected graph entities.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProjectedId {
    /// Regular node from IR.nodes[index]
    Node(usize),
    /// Virtual node for long edge routing (edge_index, rank_level)
    Virtual(usize, usize),
    /// Cluster/subgraph boundary node
    Cluster(usize),
}

impl ProjectedId {
    /// Convert to stable string ID for fnx graph.
    #[must_use]
    pub fn to_fnx_id(&self) -> String {
        match self {
            Self::Node(idx) => format!("n{idx}"),
            Self::Virtual(edge_idx, rank) => format!("v{edge_idx}_{rank}"),
            Self::Cluster(idx) => format!("c{idx}"),
        }
    }

    /// Parse fnx ID back to ProjectedId.
    #[must_use]
    pub fn from_fnx_id(id: &str) -> Option<Self> {
        if id.is_empty() {
            return None;
        }
        let bytes = id.as_bytes();
        match bytes[0] {
            b'n' => id[1..].parse().ok().map(Self::Node),
            b'v' => {
                let rest = &id[1..];
                let parts: Vec<&str> = rest.splitn(2, '_').collect();
                if parts.len() == 2 {
                    let edge_idx = parts[0].parse().ok()?;
                    let rank = parts[1].parse().ok()?;
                    Some(Self::Virtual(edge_idx, rank))
                } else {
                    None
                }
            }
            b'c' => id[1..].parse().ok().map(Self::Cluster),
            _ => None,
        }
    }
}

/// Edge metadata preserved during projection.
#[derive(Debug, Clone, Default)]
pub struct ProjectedEdgeMeta {
    /// Original edge index in IR.edges
    pub ir_edge_index: usize,
    /// Source node index in IR.nodes
    pub source_index: usize,
    /// Target node index in IR.nodes
    pub target_index: usize,
    /// Edge label if present (resolved from IR label table)
    pub label: Option<String>,
    /// Arrow type string for diagnostic context (e.g., "-->", "-.-")
    pub arrow: String,
    /// Numeric weight for layout algorithms (default 1.0)
    pub weight: f64,
    /// Whether this edge was reversed for cycle removal
    pub reversed: bool,
    /// Edge spans multiple ranks (requires virtual nodes)
    pub is_long_edge: bool,
}

/// Bidirectional projection table for IR <-> fnx mapping.
#[derive(Debug, Clone, Default)]
pub struct ProjectionTable {
    /// IR index -> fnx node ID
    forward: BTreeMap<ProjectedId, String>,
    /// fnx node ID -> IR index
    reverse: BTreeMap<String, ProjectedId>,
    /// Edge metadata by IR edge index
    edge_meta_by_index: BTreeMap<usize, ProjectedEdgeMeta>,
    /// Edge indices by (source_fnx_id, target_fnx_id)
    edge_indices_by_pair: BTreeMap<(String, String), Vec<usize>>,
    /// Total node count for determinism checks
    node_count: usize,
    /// Total edge count for determinism checks
    edge_count: usize,
}

impl ProjectionTable {
    /// Create a new empty projection table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build projection table from IR.
    #[must_use]
    pub fn from_ir(ir: &MermaidDiagramIr) -> Self {
        let mut table = Self::new();

        // Project nodes in deterministic index order
        for (idx, _node) in ir.nodes.iter().enumerate() {
            let proj_id = ProjectedId::Node(idx);
            let fnx_id = proj_id.to_fnx_id();
            table.forward.insert(proj_id.clone(), fnx_id.clone());
            table.reverse.insert(fnx_id, proj_id);
        }

        // Project clusters/subgraphs
        for (idx, _cluster) in ir.clusters.iter().enumerate() {
            let proj_id = ProjectedId::Cluster(idx);
            let fnx_id = proj_id.to_fnx_id();
            table.forward.insert(proj_id.clone(), fnx_id.clone());
            table.reverse.insert(fnx_id, proj_id);
        }

        table.node_count = ir.nodes.len();
        table.edge_count = ir.edges.len();
        table
    }

    /// Look up fnx ID for a projected entity.
    #[must_use]
    pub fn get_fnx_id(&self, id: &ProjectedId) -> Option<&String> {
        self.forward.get(id)
    }

    /// Look up projected ID from fnx ID.
    #[must_use]
    pub fn get_projected_id(&self, fnx_id: &str) -> Option<&ProjectedId> {
        self.reverse.get(fnx_id)
    }

    /// Get IR node index from fnx ID.
    #[must_use]
    pub fn get_ir_node_index(&self, fnx_id: &str) -> Option<usize> {
        match self.reverse.get(fnx_id)? {
            ProjectedId::Node(idx) => Some(*idx),
            _ => None,
        }
    }

    /// Register a virtual node for long edge routing.
    pub fn add_virtual_node(&mut self, edge_index: usize, rank: usize) -> String {
        let proj_id = ProjectedId::Virtual(edge_index, rank);
        let fnx_id = proj_id.to_fnx_id();
        self.forward.insert(proj_id.clone(), fnx_id.clone());
        self.reverse.insert(fnx_id.clone(), proj_id);
        fnx_id
    }

    /// Register edge metadata.
    pub fn add_edge_meta(&mut self, source: &str, target: &str, meta: ProjectedEdgeMeta) {
        self.edge_meta_by_index
            .insert(meta.ir_edge_index, meta.clone());
        let indices = self
            .edge_indices_by_pair
            .entry((source.to_string(), target.to_string()))
            .or_default();
        indices.push(meta.ir_edge_index);
        indices.sort_unstable();
    }

    /// Get edge metadata.
    #[must_use]
    pub fn get_edge_meta(&self, source: &str, target: &str) -> Option<&ProjectedEdgeMeta> {
        let key = (source.to_string(), target.to_string());
        let index = self.edge_indices_by_pair.get(&key)?.first()?;
        self.edge_meta_by_index.get(index)
    }

    /// Get all edge metadata entries for a source/target pair in stable index order.
    #[must_use]
    pub fn get_edge_meta_all(&self, source: &str, target: &str) -> Vec<&ProjectedEdgeMeta> {
        let key = (source.to_string(), target.to_string());
        let Some(indices) = self.edge_indices_by_pair.get(&key) else {
            return Vec::new();
        };
        indices
            .iter()
            .filter_map(|index| self.edge_meta_by_index.get(index))
            .collect()
    }

    /// Get edge metadata by IR edge index.
    #[must_use]
    pub fn get_edge_meta_by_index(&self, ir_edge_index: usize) -> Option<&ProjectedEdgeMeta> {
        self.edge_meta_by_index.get(&ir_edge_index)
    }

    /// Check determinism: verify counts match expected.
    #[must_use]
    pub fn verify_counts(&self, expected_nodes: usize, expected_edges: usize) -> bool {
        self.node_count == expected_nodes && self.edge_count == expected_edges
    }

    /// Get all node fnx IDs in deterministic order.
    #[must_use]
    pub fn node_ids_ordered(&self) -> Vec<&String> {
        self.forward
            .iter()
            .filter(|(k, _)| matches!(k, ProjectedId::Node(_)))
            .map(|(_, v)| v)
            .collect()
    }

    /// Get all projected IDs in deterministic order.
    #[must_use]
    pub fn all_ids_ordered(&self) -> Vec<(&ProjectedId, &String)> {
        self.forward.iter().collect()
    }

    /// Total entries in projection table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Check if projection table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }
}

/// Resolve label ID to label text.
fn resolve_label(ir: &MermaidDiagramIr, label_id: Option<IrLabelId>) -> Option<String> {
    label_id.and_then(|id| ir.labels.get(id.0).map(|l| l.text.clone()))
}

/// Build fnx DiGraph from MermaidDiagramIr with full metadata preservation.
///
/// Returns the graph and projection table for bidirectional lookup.
#[must_use]
pub fn ir_to_digraph(ir: &MermaidDiagramIr) -> (DiGraph, ProjectionTable) {
    let mut graph = DiGraph::hardened();
    let mut table = ProjectionTable::from_ir(ir);

    // Add nodes with attributes
    for (idx, node) in ir.nodes.iter().enumerate() {
        let fnx_id = ProjectedId::Node(idx).to_fnx_id();
        let mut attrs = BTreeMap::new();

        // Resolve label from IR label table
        if let Some(label_text) = resolve_label(ir, node.label) {
            attrs.insert("label".to_string(), CgseValue::String(label_text));
        }
        attrs.insert("ir_index".to_string(), CgseValue::Int(idx as i64));
        attrs.insert(
            "shape".to_string(),
            CgseValue::String(format!("{:?}", node.shape)),
        );

        // Ignore result - node might already exist in hardened mode
        let _ = graph.add_node_with_attrs(&fnx_id, attrs);
    }

    // Add edges with metadata
    for (idx, edge) in ir.edges.iter().enumerate() {
        let Some(source_idx) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target_idx) = endpoint_node_index(ir, edge.to) else {
            continue;
        };

        let source_fnx = ProjectedId::Node(source_idx).to_fnx_id();
        let target_fnx = ProjectedId::Node(target_idx).to_fnx_id();

        let label_text = resolve_label(ir, edge.label);

        let mut attrs = BTreeMap::new();
        attrs.insert("ir_edge_index".to_string(), CgseValue::Int(idx as i64));
        if let Some(ref label) = label_text {
            attrs.insert("label".to_string(), CgseValue::String(label.clone()));
        }
        attrs.insert("weight".to_string(), CgseValue::Float(1.0));

        // Ignore result - edge might fail in hardened mode
        let _ = graph.add_edge_with_attrs(&source_fnx, &target_fnx, attrs);

        // Store edge metadata in projection table
        let meta = ProjectedEdgeMeta {
            ir_edge_index: idx,
            source_index: source_idx,
            target_index: target_idx,
            label: label_text,
            arrow: edge.arrow.as_str().to_string(),
            weight: 1.0,
            reversed: false,
            is_long_edge: false,
        };
        table.add_edge_meta(&source_fnx, &target_fnx, meta);
    }

    (graph, table)
}

/// Build undirected fnx Graph from MermaidDiagramIr.
///
/// Useful for algorithms that require undirected graph semantics.
#[must_use]
pub fn ir_to_graph(ir: &MermaidDiagramIr) -> (Graph, ProjectionTable) {
    let mut graph = Graph::hardened();
    let mut table = ProjectionTable::from_ir(ir);

    // Add nodes
    for (idx, node) in ir.nodes.iter().enumerate() {
        let fnx_id = ProjectedId::Node(idx).to_fnx_id();
        let mut attrs = BTreeMap::new();
        if let Some(label_text) = resolve_label(ir, node.label) {
            attrs.insert("label".to_string(), CgseValue::String(label_text));
        }
        attrs.insert("ir_index".to_string(), CgseValue::Int(idx as i64));
        let _ = graph.add_node_with_attrs(&fnx_id, attrs);
    }

    // Add edges (undirected)
    for (idx, edge) in ir.edges.iter().enumerate() {
        let Some(source_idx) = endpoint_node_index(ir, edge.from) else {
            continue;
        };
        let Some(target_idx) = endpoint_node_index(ir, edge.to) else {
            continue;
        };

        let source_fnx = ProjectedId::Node(source_idx).to_fnx_id();
        let target_fnx = ProjectedId::Node(target_idx).to_fnx_id();

        let label_text = resolve_label(ir, edge.label);

        let mut attrs = BTreeMap::new();
        attrs.insert("ir_edge_index".to_string(), CgseValue::Int(idx as i64));
        let _ = graph.add_edge_with_attrs(&source_fnx, &target_fnx, attrs);

        let meta = ProjectedEdgeMeta {
            ir_edge_index: idx,
            source_index: source_idx,
            target_index: target_idx,
            label: label_text,
            arrow: edge.arrow.as_str().to_string(),
            weight: 1.0,
            reversed: false,
            is_long_edge: false,
        };
        table.add_edge_meta(&source_fnx, &target_fnx, meta);
    }

    (graph, table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{IrEdge, IrLabel, IrNode, IrNodeId, NodeShape, Span};

    fn make_test_ir() -> MermaidDiagramIr {
        // Create labels first
        let labels = vec![
            IrLabel {
                text: "Node A".to_string(),
                span: Span::default(),
            },
            IrLabel {
                text: "Node B".to_string(),
                span: Span::default(),
            },
            IrLabel {
                text: "Node C".to_string(),
                span: Span::default(),
            },
            IrLabel {
                text: "edge 1".to_string(),
                span: Span::default(),
            },
        ];

        MermaidDiagramIr {
            labels,
            nodes: vec![
                IrNode {
                    id: "A".to_string(),
                    label: Some(IrLabelId(0)),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "B".to_string(),
                    label: Some(IrLabelId(1)),
                    shape: NodeShape::Circle,
                    ..Default::default()
                },
                IrNode {
                    id: "C".to_string(),
                    label: Some(IrLabelId(2)),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
            ],
            edges: vec![
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(1)),
                    label: Some(IrLabelId(3)),
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

    #[test]
    fn projected_id_roundtrip() {
        let cases = [
            ProjectedId::Node(0),
            ProjectedId::Node(42),
            ProjectedId::Virtual(5, 3),
            ProjectedId::Cluster(7),
        ];
        for id in cases {
            let fnx_id = id.to_fnx_id();
            let parsed = ProjectedId::from_fnx_id(&fnx_id);
            assert_eq!(parsed, Some(id.clone()), "roundtrip failed for {id:?}");
        }
    }

    #[test]
    fn projection_table_from_ir() {
        let ir = make_test_ir();
        let table = ProjectionTable::from_ir(&ir);

        assert_eq!(table.len(), 3); // 3 nodes, no clusters
        assert!(table.verify_counts(3, 2));

        // Check forward lookup
        assert_eq!(
            table.get_fnx_id(&ProjectedId::Node(0)),
            Some(&"n0".to_string())
        );
        assert_eq!(
            table.get_fnx_id(&ProjectedId::Node(1)),
            Some(&"n1".to_string())
        );

        // Check reverse lookup
        assert_eq!(table.get_projected_id("n0"), Some(&ProjectedId::Node(0)));
        assert_eq!(table.get_ir_node_index("n1"), Some(1));
    }

    #[test]
    fn ir_to_digraph_preserves_structure() {
        let ir = make_test_ir();
        let (graph, table) = ir_to_digraph(&ir);

        // Check node count
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        // Check nodes exist
        assert!(graph.has_node("n0"));
        assert!(graph.has_node("n1"));
        assert!(graph.has_node("n2"));

        // Check edges
        assert!(graph.has_edge("n0", "n1"));
        assert!(graph.has_edge("n1", "n2"));

        // Check edge metadata in table
        let meta = table.get_edge_meta("n0", "n1").expect("edge meta");
        assert_eq!(meta.ir_edge_index, 0);
        assert_eq!(meta.source_index, 0);
        assert_eq!(meta.target_index, 1);
        assert_eq!(meta.arrow, "---"); // Default ArrowType::Line
        assert_eq!(meta.label, Some("edge 1".to_string()));
    }

    #[test]
    fn ir_to_graph_undirected() {
        let ir = make_test_ir();
        let (graph, _table) = ir_to_graph(&ir);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
        assert!(!graph.is_directed());
    }

    #[test]
    fn edge_meta_tracks_parallel_edges_deterministically() {
        let mut ir = make_test_ir();
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            ..Default::default()
        });

        let (_graph, table) = ir_to_digraph(&ir);
        let meta_first = table.get_edge_meta_by_index(0).expect("first edge meta");
        let meta_second = table.get_edge_meta_by_index(2).expect("second edge meta");

        assert_eq!(meta_first.source_index, 0);
        assert_eq!(meta_first.target_index, 1);
        assert_eq!(meta_second.source_index, 0);
        assert_eq!(meta_second.target_index, 1);

        let pair_meta = table.get_edge_meta("n0", "n1").expect("pair meta");
        assert_eq!(pair_meta.ir_edge_index, 0);

        let all_meta = table.get_edge_meta_all("n0", "n1");
        assert_eq!(all_meta.len(), 2);
        assert_eq!(all_meta[0].ir_edge_index, 0);
        assert_eq!(all_meta[1].ir_edge_index, 2);
    }

    #[test]
    fn deterministic_ordering_under_repeated_builds() {
        let ir = make_test_ir();

        // Build multiple times and verify deterministic output
        let mut results = Vec::new();
        for _ in 0..5 {
            let table = ProjectionTable::from_ir(&ir);
            let ids: Vec<_> = table
                .all_ids_ordered()
                .iter()
                .map(|(k, v)| ((*k).clone(), v.to_string()))
                .collect();
            results.push(ids);
        }

        // All results should be identical
        for i in 1..results.len() {
            assert_eq!(
                results[0], results[i],
                "non-deterministic ordering at iteration {i}"
            );
        }
    }

    #[test]
    fn virtual_node_registration() {
        let ir = make_test_ir();
        let mut table = ProjectionTable::from_ir(&ir);

        let v1 = table.add_virtual_node(0, 1);
        let v2 = table.add_virtual_node(0, 2);

        assert_eq!(v1, "v0_1");
        assert_eq!(v2, "v0_2");

        // Verify reverse lookup
        assert_eq!(
            table.get_projected_id("v0_1"),
            Some(&ProjectedId::Virtual(0, 1))
        );
        assert_eq!(
            table.get_projected_id("v0_2"),
            Some(&ProjectedId::Virtual(0, 2))
        );
    }

    #[test]
    fn projection_policy_applies_weights_and_collapses_parallel() {
        let mut ir = make_test_ir();
        // Add a parallel edge A->B and a self-loop B->B
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(1)), // self-loop
            ..Default::default()
        });

        // Test IgnoreDirection (default)
        let config = ProjectionConfig::default();
        let (graph, _table, trace) = ir_to_graph_with_policy(&ir, &config);

        assert_eq!(trace.policy, "ignore_direction");
        assert_eq!(trace.directed_edge_count, 4); // 2 original + 1 parallel + 1 self-loop
        assert_eq!(trace.self_loop_count, 1);
        assert_eq!(trace.collapsed_parallel_count, 1); // 2 A->B edges collapsed to 1
        assert_eq!(graph.node_count(), 3);
        // 2 unique edges + 1 self-loop = 3 edges in undirected view
        assert_eq!(graph.edge_count(), 3);

        // Test PenaltyWeighted with self-loop removal
        let config = ProjectionConfig {
            directed_policy: DirectedProjectionPolicy::PenaltyWeighted,
            penalty_factor: 0.25,
            preserve_self_loops: false,
            collapse_parallel_edges: true,
        };
        let (graph, _table, trace) = ir_to_graph_with_policy(&ir, &config);

        assert_eq!(trace.policy, "penalty_weighted");
        assert_eq!(trace.self_loop_count, 1); // still counted
        assert_eq!(graph.edge_count(), 2); // self-loop removed

        // Verify valid/invalid diagnostics are populated
        assert!(trace.valid_diagnostics.contains(&"weighted_centrality"));
        assert!(trace.invalid_diagnostics.contains(&"exact_reachability"));
    }
}
