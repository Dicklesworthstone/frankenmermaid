//! FNX-powered structural diagnostics for diagram analysis.
//!
//! This module runs graph-theoretic analyses on parsed IR and produces
//! diagnostic records with spans for user-actionable feedback.
//!
//! Diagnostic codes follow the pattern `FNX-{category}-{number}`:
//! - FNX-CONN-001: Disconnected components (islands)
//! - FNX-CHOKE-001: Articulation point (single point of failure)
//! - FNX-BRIDGE-001: Bridge edge (fragile connection)
//! - FNX-CYCLE-001: Dense cycle detected

use fm_core::{IrNodeId, MermaidDiagramIr, Span};
use fnx_algorithms::{
    articulation_points, bridges, connected_components, cycle_basis, ArticulationPointsResult,
    BridgesResult, ComponentsResult, CycleBasisResult,
};

use crate::fnx_adapter::{ProjectionTable, ir_to_graph};

// ============================================================================
// Diagnostic Codes
// ============================================================================

/// Machine-readable diagnostic code for FNX-detected issues.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnxDiagnosticCode {
    pub category: &'static str,
    pub number: u16,
}

impl FnxDiagnosticCode {
    pub const DISCONNECTED_COMPONENT: Self = Self {
        category: "CONN",
        number: 1,
    };
    pub const ARTICULATION_POINT: Self = Self {
        category: "CHOKE",
        number: 1,
    };
    pub const BRIDGE_EDGE: Self = Self {
        category: "BRIDGE",
        number: 1,
    };
    pub const DENSE_CYCLE: Self = Self {
        category: "CYCLE",
        number: 1,
    };

    /// Render the code as a string like "FNX-CONN-001".
    #[must_use]
    pub fn as_str(&self) -> String {
        format!("FNX-{}-{:03}", self.category, self.number)
    }
}

// ============================================================================
// Diagnostic Severity
// ============================================================================

/// Severity level for structural diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FnxDiagnosticSeverity {
    /// Informational - not necessarily a problem.
    Info,
    /// Warning - potential issue that may affect layout or readability.
    Warning,
    /// Error - significant structural problem.
    Error,
}

impl FnxDiagnosticSeverity {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

// ============================================================================
// Diagnostic Record
// ============================================================================

/// A structural diagnostic produced by FNX analysis.
#[derive(Debug, Clone)]
pub struct FnxDiagnostic {
    /// Machine-readable code.
    pub code: FnxDiagnosticCode,
    /// Severity level.
    pub severity: FnxDiagnosticSeverity,
    /// Human-readable message.
    pub message: String,
    /// Optional span pointing to relevant source location.
    pub span: Option<Span>,
    /// Related node IDs (for highlighting in editors).
    pub related_nodes: Vec<IrNodeId>,
    /// Related edge indices (source, target pairs).
    pub related_edges: Vec<(usize, usize)>,
    /// Remediation suggestion.
    pub suggestion: Option<String>,
}

// ============================================================================
// Analysis Results
// ============================================================================

/// Results from running all FNX structural analyses.
#[derive(Debug, Clone, Default)]
pub struct FnxAnalysisResults {
    /// All diagnostics produced.
    pub diagnostics: Vec<FnxDiagnostic>,
    /// Number of connected components.
    pub component_count: usize,
    /// Number of articulation points.
    pub articulation_point_count: usize,
    /// Number of bridges.
    pub bridge_count: usize,
    /// Number of cycles detected.
    pub cycle_count: usize,
    /// Whether the graph is connected.
    pub is_connected: bool,
}

// ============================================================================
// Analysis Functions
// ============================================================================

/// Run all FNX structural analyses on the given IR.
///
/// Returns analysis results with diagnostics that can be surfaced to users.
#[must_use]
pub fn analyze_structure(ir: &MermaidDiagramIr) -> FnxAnalysisResults {
    if ir.nodes.is_empty() {
        return FnxAnalysisResults::default();
    }

    let (graph, table) = ir_to_graph(ir);
    let mut results = FnxAnalysisResults::default();

    // Run connected components analysis
    let comp_result = connected_components(&graph);
    analyze_components(ir, &table, &comp_result, &mut results);

    // Run articulation points analysis (only meaningful for connected graphs)
    if results.is_connected {
        let art_result = articulation_points(&graph);
        analyze_articulation_points(ir, &table, &art_result, &mut results);

        let bridge_result = bridges(&graph);
        analyze_bridges(ir, &table, &bridge_result, &mut results);
    }

    // Run cycle analysis
    let cycle_result = cycle_basis(&graph, None);
    analyze_cycles(ir, &table, &cycle_result, &mut results);

    results
}

fn analyze_components(
    ir: &MermaidDiagramIr,
    table: &ProjectionTable,
    result: &ComponentsResult,
    out: &mut FnxAnalysisResults,
) {
    out.component_count = result.components.len();
    out.is_connected = result.components.len() <= 1;

    if result.components.len() > 1 {
        // Sort components by size descending to identify the main component
        let mut sorted_components: Vec<_> = result.components.iter().collect();
        sorted_components.sort_by_key(|c| std::cmp::Reverse(c.len()));

        // Skip the largest component, report others as disconnected islands
        for (i, component) in sorted_components.iter().skip(1).enumerate() {
            let node_ids: Vec<IrNodeId> = component
                .iter()
                .filter_map(|fnx_id| table.get_ir_node_index(fnx_id))
                .map(IrNodeId)
                .collect();

            // Get span from first node in the component
            let span = node_ids
                .first()
                .and_then(|id| ir.nodes.get(id.0))
                .map(|n| n.span_primary);

            let node_names: Vec<&str> = node_ids
                .iter()
                .filter_map(|id| ir.nodes.get(id.0).map(|n| n.id.as_str()))
                .collect();

            out.diagnostics.push(FnxDiagnostic {
                code: FnxDiagnosticCode::DISCONNECTED_COMPONENT,
                severity: FnxDiagnosticSeverity::Warning,
                message: format!(
                    "Disconnected component {} with {} node(s): [{}]",
                    i + 1,
                    component.len(),
                    node_names.join(", ")
                ),
                span,
                related_nodes: node_ids,
                related_edges: Vec::new(),
                suggestion: Some(
                    "Consider adding edges to connect this component to the main graph".to_string(),
                ),
            });
        }
    }
}

fn analyze_articulation_points(
    ir: &MermaidDiagramIr,
    table: &ProjectionTable,
    result: &ArticulationPointsResult,
    out: &mut FnxAnalysisResults,
) {
    out.articulation_point_count = result.nodes.len();

    for fnx_id in &result.nodes {
        let Some(ir_idx) = table.get_ir_node_index(fnx_id) else {
            continue;
        };
        let Some(node) = ir.nodes.get(ir_idx) else {
            continue;
        };

        out.diagnostics.push(FnxDiagnostic {
            code: FnxDiagnosticCode::ARTICULATION_POINT,
            severity: FnxDiagnosticSeverity::Info,
            message: format!(
                "Node '{}' is an articulation point (removing it would disconnect the graph)",
                node.id
            ),
            span: Some(node.span_primary),
            related_nodes: vec![IrNodeId(ir_idx)],
            related_edges: Vec::new(),
            suggestion: Some(
                "Consider adding redundant paths around this node for resilience".to_string(),
            ),
        });
    }
}

fn analyze_bridges(
    ir: &MermaidDiagramIr,
    table: &ProjectionTable,
    result: &BridgesResult,
    out: &mut FnxAnalysisResults,
) {
    out.bridge_count = result.edges.len();

    for (source_fnx, target_fnx) in &result.edges {
        let Some(source_idx) = table.get_ir_node_index(source_fnx) else {
            continue;
        };
        let Some(target_idx) = table.get_ir_node_index(target_fnx) else {
            continue;
        };
        let source_node = ir.nodes.get(source_idx);
        let target_node = ir.nodes.get(target_idx);

        let (source_name, target_name) = match (source_node, target_node) {
            (Some(s), Some(t)) => (s.id.as_str(), t.id.as_str()),
            _ => continue,
        };

        // Find the edge span if available
        let edge_span = ir
            .edges
            .iter()
            .find(|e| {
                let from_idx = match e.from {
                    fm_core::IrEndpoint::Node(id) => id.0,
                    _ => return false,
                };
                let to_idx = match e.to {
                    fm_core::IrEndpoint::Node(id) => id.0,
                    _ => return false,
                };
                (from_idx == source_idx && to_idx == target_idx)
                    || (from_idx == target_idx && to_idx == source_idx)
            })
            .map(|e| e.span);

        out.diagnostics.push(FnxDiagnostic {
            code: FnxDiagnosticCode::BRIDGE_EDGE,
            severity: FnxDiagnosticSeverity::Info,
            message: format!(
                "Edge '{source_name}' → '{target_name}' is a bridge (removing it would disconnect the graph)"
            ),
            span: edge_span,
            related_nodes: vec![IrNodeId(source_idx), IrNodeId(target_idx)],
            related_edges: vec![(source_idx, target_idx)],
            suggestion: Some(
                "Consider adding parallel paths for redundancy".to_string(),
            ),
        });
    }
}

fn analyze_cycles(
    ir: &MermaidDiagramIr,
    table: &ProjectionTable,
    result: &CycleBasisResult,
    out: &mut FnxAnalysisResults,
) {
    out.cycle_count = result.cycles.len();

    // Only report cycles with more than 4 nodes as "dense"
    for cycle in &result.cycles {
        if cycle.len() <= 4 {
            continue;
        }

        let node_ids: Vec<IrNodeId> = cycle
            .iter()
            .filter_map(|fnx_id| table.get_ir_node_index(fnx_id))
            .map(IrNodeId)
            .collect();

        let node_names: Vec<&str> = node_ids
            .iter()
            .filter_map(|id| ir.nodes.get(id.0).map(|n| n.id.as_str()))
            .collect();

        // Get span from first node
        let span = node_ids
            .first()
            .and_then(|id| ir.nodes.get(id.0))
            .map(|n| n.span_primary);

        out.diagnostics.push(FnxDiagnostic {
            code: FnxDiagnosticCode::DENSE_CYCLE,
            severity: FnxDiagnosticSeverity::Info,
            message: format!(
                "Dense cycle with {} nodes: [{}]",
                cycle.len(),
                node_names.join(" → ")
            ),
            span,
            related_nodes: node_ids,
            related_edges: Vec::new(),
            suggestion: Some(
                "Dense cycles may affect layout clarity; consider simplifying".to_string(),
            ),
        });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{IrEdge, IrEndpoint, IrNode, NodeShape};

    fn make_simple_chain() -> MermaidDiagramIr {
        // A -> B -> C (simple chain, no cycles)
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

    fn make_disconnected_graph() -> MermaidDiagramIr {
        // A -> B and C -> D (two disconnected components)
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
                IrNode {
                    id: "D".to_string(),
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
                    from: IrEndpoint::Node(IrNodeId(2)),
                    to: IrEndpoint::Node(IrNodeId(3)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn make_articulation_graph() -> MermaidDiagramIr {
        // A - B - C where B is an articulation point
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

    fn make_cycle_graph() -> MermaidDiagramIr {
        // A -> B -> C -> D -> E -> A (5-node cycle, counted as dense)
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
                IrNode {
                    id: "D".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                },
                IrNode {
                    id: "E".to_string(),
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
                    to: IrEndpoint::Node(IrNodeId(3)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(3)),
                    to: IrEndpoint::Node(IrNodeId(4)),
                    ..Default::default()
                },
                IrEdge {
                    from: IrEndpoint::Node(IrNodeId(4)),
                    to: IrEndpoint::Node(IrNodeId(0)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn empty_graph_produces_no_diagnostics() {
        let ir = MermaidDiagramIr::default();
        let results = analyze_structure(&ir);

        assert!(results.diagnostics.is_empty());
        assert_eq!(results.component_count, 0);
    }

    #[test]
    fn connected_graph_no_disconnected_warnings() {
        let ir = make_simple_chain();
        let results = analyze_structure(&ir);

        assert!(results.is_connected);
        assert_eq!(results.component_count, 1);

        // No disconnected component warnings
        let disconnected_count = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::DISCONNECTED_COMPONENT)
            .count();
        assert_eq!(disconnected_count, 0);
    }

    #[test]
    fn disconnected_graph_warns_about_islands() {
        let ir = make_disconnected_graph();
        let results = analyze_structure(&ir);

        assert!(!results.is_connected);
        assert_eq!(results.component_count, 2);

        // Should have exactly one disconnected component warning
        let disconnected = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::DISCONNECTED_COMPONENT)
            .collect::<Vec<_>>();
        assert_eq!(disconnected.len(), 1);
        assert_eq!(disconnected[0].severity, FnxDiagnosticSeverity::Warning);
    }

    #[test]
    fn articulation_points_detected() {
        let ir = make_articulation_graph();
        let results = analyze_structure(&ir);

        assert!(results.is_connected);
        assert_eq!(results.articulation_point_count, 1);

        let art_diags = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::ARTICULATION_POINT)
            .collect::<Vec<_>>();
        assert_eq!(art_diags.len(), 1);
        assert!(art_diags[0].message.contains("B"));
    }

    #[test]
    fn bridges_detected() {
        let ir = make_simple_chain();
        let results = analyze_structure(&ir);

        assert_eq!(results.bridge_count, 2);

        let bridge_diags = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::BRIDGE_EDGE)
            .count();
        assert_eq!(bridge_diags, 2);
    }

    #[test]
    fn dense_cycles_detected() {
        let ir = make_cycle_graph();
        let results = analyze_structure(&ir);

        assert_eq!(results.cycle_count, 1);

        let cycle_diags = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::DENSE_CYCLE)
            .collect::<Vec<_>>();
        assert_eq!(cycle_diags.len(), 1);
        assert!(cycle_diags[0].message.contains("5 nodes"));
    }

    #[test]
    fn diagnostic_codes_format_correctly() {
        assert_eq!(
            FnxDiagnosticCode::DISCONNECTED_COMPONENT.as_str(),
            "FNX-CONN-001"
        );
        assert_eq!(
            FnxDiagnosticCode::ARTICULATION_POINT.as_str(),
            "FNX-CHOKE-001"
        );
        assert_eq!(FnxDiagnosticCode::BRIDGE_EDGE.as_str(), "FNX-BRIDGE-001");
        assert_eq!(FnxDiagnosticCode::DENSE_CYCLE.as_str(), "FNX-CYCLE-001");
    }

    #[test]
    fn analysis_is_deterministic() {
        let ir = make_disconnected_graph();

        // Run 5 times and verify results are identical
        let results: Vec<_> = (0..5).map(|_| analyze_structure(&ir)).collect();

        for i in 1..results.len() {
            assert_eq!(
                results[0].component_count, results[i].component_count,
                "component_count differs at iteration {i}"
            );
            assert_eq!(
                results[0].diagnostics.len(),
                results[i].diagnostics.len(),
                "diagnostic count differs at iteration {i}"
            );
            for (d0, di) in results[0].diagnostics.iter().zip(&results[i].diagnostics) {
                assert_eq!(d0.code, di.code, "diagnostic code differs at iteration {i}");
                assert_eq!(
                    d0.message, di.message,
                    "diagnostic message differs at iteration {i}"
                );
            }
        }
    }
}
