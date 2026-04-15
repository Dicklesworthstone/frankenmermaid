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
//!
//! # Recommendations
//!
//! Each diagnostic includes a structured recommendation with:
//! - Category (Simplify, Connect, Redundancy, Clarify)
//! - Confidence level (High, Medium, Low)
//! - Rationale explaining why this is suggested
//! - Concrete action steps

use fm_core::{IrNodeId, MermaidDiagramIr, Span};
use fnx_algorithms::{
    ArticulationPointsResult, BridgesResult, ComponentsResult, CycleBasisResult,
    articulation_points, bridges, connected_components, cycle_basis,
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
// Recommendation Types
// ============================================================================

/// Category of structural recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendationCategory {
    /// Simplify the diagram structure (reduce complexity).
    Simplify,
    /// Connect disconnected parts.
    Connect,
    /// Add redundancy for resilience.
    Redundancy,
    /// Improve layout clarity.
    Clarify,
}

impl RecommendationCategory {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Simplify => "simplify",
            Self::Connect => "connect",
            Self::Redundancy => "redundancy",
            Self::Clarify => "clarify",
        }
    }

    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Simplify => "Simplify Structure",
            Self::Connect => "Improve Connectivity",
            Self::Redundancy => "Add Redundancy",
            Self::Clarify => "Clarify Layout",
        }
    }
}

/// Confidence level for a recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecommendationConfidence {
    /// Low confidence - suggestion may not apply.
    Low,
    /// Medium confidence - likely helpful but context-dependent.
    Medium,
    /// High confidence - strongly recommended action.
    High,
}

impl RecommendationConfidence {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Numeric score for sorting (higher = more confident).
    #[must_use]
    pub const fn score(&self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }
}

/// A structured recommendation for diagram improvement.
///
/// Provides machine-readable metadata for front-end display in CLI and WASM.
#[derive(Debug, Clone)]
pub struct StructuredRecommendation {
    /// Category of this recommendation.
    pub category: RecommendationCategory,
    /// Confidence level.
    pub confidence: RecommendationConfidence,
    /// Why this recommendation is being made.
    pub rationale: String,
    /// Concrete action to take.
    pub action: String,
    /// Optional example showing the fix.
    pub example: Option<String>,
}

impl StructuredRecommendation {
    /// Create a new recommendation.
    #[must_use]
    pub fn new(
        category: RecommendationCategory,
        confidence: RecommendationConfidence,
        rationale: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            category,
            confidence,
            rationale: rationale.into(),
            action: action.into(),
            example: None,
        }
    }

    /// Add an example to the recommendation.
    #[must_use]
    pub fn with_example(mut self, example: impl Into<String>) -> Self {
        self.example = Some(example.into());
        self
    }

    /// Create a recommendation for disconnected components.
    #[must_use]
    pub fn for_disconnected_component(component_size: usize, node_names: &[&str]) -> Self {
        let names_preview = if node_names.len() > 3 {
            format!(
                "{}, {}, {} and {} more",
                node_names[0],
                node_names[1],
                node_names[2],
                node_names.len() - 3
            )
        } else {
            node_names.join(", ")
        };

        Self::new(
            RecommendationCategory::Connect,
            if component_size == 1 {
                RecommendationConfidence::High
            } else {
                RecommendationConfidence::Medium
            },
            format!(
                "This group of {} node(s) ({}) is not connected to the main diagram, \
                 which may indicate missing relationships or an incomplete design.",
                component_size, names_preview
            ),
            "Add edges to connect these nodes to the main flow, or remove them if they're not needed.",
        )
        .with_example("A --> DisconnectedNode")
    }

    /// Create a recommendation for articulation points.
    #[must_use]
    pub fn for_articulation_point(node_name: &str) -> Self {
        Self::new(
            RecommendationCategory::Redundancy,
            RecommendationConfidence::Medium,
            format!(
                "Node '{}' is a single point of failure. Removing it would split the diagram \
                 into disconnected parts, which may indicate a fragile design.",
                node_name
            ),
            "Consider adding alternative paths that bypass this node.",
        )
        .with_example("Add: OtherNode --> AlternativePath --> DownstreamNode")
    }

    /// Create a recommendation for bridge edges.
    #[must_use]
    pub fn for_bridge_edge(source: &str, target: &str) -> Self {
        Self::new(
            RecommendationCategory::Redundancy,
            RecommendationConfidence::Low,
            format!(
                "The edge from '{}' to '{}' is the only connection between two parts of the diagram. \
                 This may be intentional, but could indicate a fragile design.",
                source, target
            ),
            "If redundancy is needed, add parallel paths between these regions.",
        )
    }

    /// Create a recommendation for dense cycles.
    #[must_use]
    pub fn for_dense_cycle(cycle_size: usize, node_names: &[&str]) -> Self {
        let names_preview = if node_names.len() > 4 {
            format!(
                "{} → {} → ... → {}",
                node_names[0],
                node_names[1],
                node_names.last().unwrap_or(&"")
            )
        } else {
            node_names.join(" → ")
        };

        Self::new(
            RecommendationCategory::Simplify,
            if cycle_size > 6 {
                RecommendationConfidence::Medium
            } else {
                RecommendationConfidence::Low
            },
            format!(
                "A cycle with {} nodes ({}) may make the diagram harder to read. \
                 Large cycles often indicate tightly coupled components.",
                cycle_size, names_preview
            ),
            "Consider breaking the cycle by extracting shared functionality into a separate node.",
        )
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
    /// Remediation suggestion (simple text, for backwards compatibility).
    pub suggestion: Option<String>,
    /// Structured recommendation with category, confidence, and rationale.
    pub recommendation: Option<StructuredRecommendation>,
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

            let recommendation =
                StructuredRecommendation::for_disconnected_component(component.len(), &node_names);
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
                suggestion: Some(recommendation.action.clone()),
                recommendation: Some(recommendation),
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

        let recommendation = StructuredRecommendation::for_articulation_point(&node.id);
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
            suggestion: Some(recommendation.action.clone()),
            recommendation: Some(recommendation),
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
                let from_idx = e.from.resolved_node_id(&ir.ports).map(|id| id.0);
                let to_idx = e.to.resolved_node_id(&ir.ports).map(|id| id.0);
                match (from_idx, to_idx) {
                    (Some(f), Some(t)) => {
                        (f == source_idx && t == target_idx) || (f == target_idx && t == source_idx)
                    }
                    _ => false,
                }
            })
            .map(|e| e.span);

        let recommendation = StructuredRecommendation::for_bridge_edge(source_name, target_name);
        out.diagnostics.push(FnxDiagnostic {
            code: FnxDiagnosticCode::BRIDGE_EDGE,
            severity: FnxDiagnosticSeverity::Info,
            message: format!(
                "Edge '{source_name}' → '{target_name}' is a bridge (removing it would disconnect the graph)"
            ),
            span: edge_span,
            related_nodes: vec![IrNodeId(source_idx), IrNodeId(target_idx)],
            related_edges: vec![(source_idx, target_idx)],
            suggestion: Some(recommendation.action.clone()),
            recommendation: Some(recommendation),
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

        let recommendation = StructuredRecommendation::for_dense_cycle(cycle.len(), &node_names);
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
            suggestion: Some(recommendation.action.clone()),
            recommendation: Some(recommendation),
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

    // ========================================================================
    // Edge Case Tests (bd-ml2r.11.1)
    // ========================================================================

    #[test]
    fn single_node_graph_no_diagnostics() {
        let ir = MermaidDiagramIr {
            nodes: vec![IrNode {
                id: "Lonely".to_string(),
                shape: NodeShape::Rect,
                ..Default::default()
            }],
            edges: vec![],
            ..Default::default()
        };
        let results = analyze_structure(&ir);

        assert_eq!(results.component_count, 1);
        assert!(results.is_connected);
        // Single node has no articulation points, bridges, or dense cycles
        assert_eq!(results.articulation_point_count, 0);
        assert_eq!(results.bridge_count, 0);
        assert_eq!(results.cycle_count, 0);
        // No warnings expected
        assert!(results.diagnostics.is_empty());
    }

    #[test]
    fn small_cycle_not_reported_as_dense() {
        // A -> B -> C -> A (3-node cycle, should NOT produce dense cycle warning)
        let ir = MermaidDiagramIr {
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
        };
        let results = analyze_structure(&ir);

        assert_eq!(results.cycle_count, 1);
        // Should NOT have dense cycle diagnostic (only 3 nodes, threshold is >4)
        let dense_cycle_diags = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::DENSE_CYCLE)
            .count();
        assert_eq!(dense_cycle_diags, 0);
    }

    #[test]
    fn four_node_cycle_not_reported_as_dense() {
        // A -> B -> C -> D -> A (4-node cycle, exactly at threshold)
        let ir = MermaidDiagramIr {
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
                    to: IrEndpoint::Node(IrNodeId(0)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let results = analyze_structure(&ir);

        assert_eq!(results.cycle_count, 1);
        // 4-node cycle should NOT be reported as dense (threshold is >4)
        let dense_cycle_diags = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::DENSE_CYCLE)
            .count();
        assert_eq!(dense_cycle_diags, 0);
    }

    #[test]
    fn multiple_disconnected_components_all_reported() {
        // Three isolated nodes: A, B, C (three separate components)
        let ir = MermaidDiagramIr {
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
            edges: vec![],
            ..Default::default()
        };
        let results = analyze_structure(&ir);

        assert_eq!(results.component_count, 3);
        assert!(!results.is_connected);
        // Two components should be reported (the largest is skipped)
        let disconnected_count = results
            .diagnostics
            .iter()
            .filter(|d| d.code == FnxDiagnosticCode::DISCONNECTED_COMPONENT)
            .count();
        assert_eq!(disconnected_count, 2);
    }

    #[test]
    fn self_loop_handled_gracefully() {
        // A -> A (self-loop)
        let ir = MermaidDiagramIr {
            nodes: vec![IrNode {
                id: "A".to_string(),
                shape: NodeShape::Rect,
                ..Default::default()
            }],
            edges: vec![IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(0)),
                ..Default::default()
            }],
            ..Default::default()
        };
        let results = analyze_structure(&ir);

        // Should not panic, and should report as connected
        assert_eq!(results.component_count, 1);
        assert!(results.is_connected);
    }

    // ========================================================================
    // Recommendation Factory Tests
    // ========================================================================

    #[test]
    fn recommendation_for_disconnected_component_single_node_high_confidence() {
        let rec = StructuredRecommendation::for_disconnected_component(1, &["Orphan"]);
        assert_eq!(rec.category, RecommendationCategory::Connect);
        assert_eq!(rec.confidence, RecommendationConfidence::High);
        assert!(rec.rationale.contains("1 node"));
        assert!(rec.example.is_some());
    }

    #[test]
    fn recommendation_for_disconnected_component_multi_node_medium_confidence() {
        let rec = StructuredRecommendation::for_disconnected_component(3, &["A", "B", "C"]);
        assert_eq!(rec.category, RecommendationCategory::Connect);
        assert_eq!(rec.confidence, RecommendationConfidence::Medium);
        assert!(rec.rationale.contains("3 node"));
    }

    #[test]
    fn recommendation_for_disconnected_component_truncates_long_node_list() {
        let rec = StructuredRecommendation::for_disconnected_component(
            5,
            &["N1", "N2", "N3", "N4", "N5"],
        );
        // Should truncate to "N1, N2, N3 and 2 more"
        assert!(rec.rationale.contains("and 2 more"));
    }

    #[test]
    fn recommendation_for_articulation_point_has_example() {
        let rec = StructuredRecommendation::for_articulation_point("Gateway");
        assert_eq!(rec.category, RecommendationCategory::Redundancy);
        assert_eq!(rec.confidence, RecommendationConfidence::Medium);
        assert!(rec.rationale.contains("Gateway"));
        assert!(rec.example.is_some());
    }

    #[test]
    fn recommendation_for_bridge_edge_low_confidence() {
        let rec = StructuredRecommendation::for_bridge_edge("Src", "Dst");
        assert_eq!(rec.category, RecommendationCategory::Redundancy);
        assert_eq!(rec.confidence, RecommendationConfidence::Low);
        assert!(rec.rationale.contains("Src"));
        assert!(rec.rationale.contains("Dst"));
    }

    #[test]
    fn recommendation_for_dense_cycle_small_low_confidence() {
        let rec = StructuredRecommendation::for_dense_cycle(5, &["A", "B", "C", "D", "E"]);
        assert_eq!(rec.category, RecommendationCategory::Simplify);
        assert_eq!(rec.confidence, RecommendationConfidence::Low);
        assert!(rec.rationale.contains("5 nodes"));
    }

    #[test]
    fn recommendation_for_dense_cycle_large_medium_confidence() {
        let nodes: Vec<&str> = (0..8).map(|_| "N").collect();
        let rec = StructuredRecommendation::for_dense_cycle(8, &nodes);
        assert_eq!(rec.confidence, RecommendationConfidence::Medium);
    }

    #[test]
    fn recommendation_category_display_names() {
        assert_eq!(
            RecommendationCategory::Simplify.display_name(),
            "Simplify Structure"
        );
        assert_eq!(
            RecommendationCategory::Connect.display_name(),
            "Improve Connectivity"
        );
        assert_eq!(
            RecommendationCategory::Redundancy.display_name(),
            "Add Redundancy"
        );
        assert_eq!(
            RecommendationCategory::Clarify.display_name(),
            "Clarify Layout"
        );
    }

    #[test]
    fn recommendation_confidence_score_ordering() {
        assert!(RecommendationConfidence::Low.score() < RecommendationConfidence::Medium.score());
        assert!(RecommendationConfidence::Medium.score() < RecommendationConfidence::High.score());
    }

    #[test]
    fn severity_ordering() {
        assert!(FnxDiagnosticSeverity::Info < FnxDiagnosticSeverity::Warning);
        assert!(FnxDiagnosticSeverity::Warning < FnxDiagnosticSeverity::Error);
    }

    #[test]
    fn severity_as_str() {
        assert_eq!(FnxDiagnosticSeverity::Info.as_str(), "info");
        assert_eq!(FnxDiagnosticSeverity::Warning.as_str(), "warning");
        assert_eq!(FnxDiagnosticSeverity::Error.as_str(), "error");
    }
}
