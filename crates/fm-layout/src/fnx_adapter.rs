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
use fnx_classes::digraph::DiGraph;
use fnx_classes::Graph;
use fnx_runtime::CgseValue;

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
        attrs.insert("shape".to_string(), CgseValue::String(format!("{:?}", node.shape)));

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
            IrLabel { text: "Node A".to_string(), span: Span::default() },
            IrLabel { text: "Node B".to_string(), span: Span::default() },
            IrLabel { text: "Node C".to_string(), span: Span::default() },
            IrLabel { text: "edge 1".to_string(), span: Span::default() },
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
        assert_eq!(
            table.get_projected_id("n0"),
            Some(&ProjectedId::Node(0))
        );
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
        let meta_first = table
            .get_edge_meta_by_index(0)
            .expect("first edge meta");
        let meta_second = table
            .get_edge_meta_by_index(2)
            .expect("second edge meta");

        assert_eq!(meta_first.source_index, 0);
        assert_eq!(meta_first.target_index, 1);
        assert_eq!(meta_second.source_index, 0);
        assert_eq!(meta_second.target_index, 1);

        let pair_meta = table
            .get_edge_meta("n0", "n1")
            .expect("pair meta");
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
            let ids: Vec<_> = table.all_ids_ordered().iter().map(|(k, v)| ((*k).clone(), v.to_string())).collect();
            results.push(ids);
        }

        // All results should be identical
        for i in 1..results.len() {
            assert_eq!(results[0], results[i], "non-deterministic ordering at iteration {i}");
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
}
