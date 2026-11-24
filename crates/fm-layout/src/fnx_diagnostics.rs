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
//! - FNX-BUDGET-001: Structural analysis stopped before its budgeted work completed
//!
//! # Recommendations
//!
//! Each diagnostic includes a structured recommendation with:
//! - Category (Simplify, Connect, Redundancy, Clarify)
//! - Confidence level (High, Medium, Low)
//! - Rationale explaining why this is suggested
//! - Concrete action steps

use std::cell::RefCell;
use std::time::Instant;

use fm_core::{IrNodeId, MermaidDiagramIr, Span};
use fnx_algorithms::{
    ArticulationPointsResult, BridgesResult, ComponentsResult, CycleBasisResult,
    articulation_points, bridges, connected_components, cycle_basis,
};

use crate::FnxExecutionTrace;
use crate::fnx_adapter::{ProjectionConfig, ProjectionTable, ir_to_graph_with_policy};
use crate::fnx_budget::{AnalysisBudget, BudgetExceededReason, BudgetExecutor, BudgetResult};

thread_local! {
    /// Evidence belongs to the current layout call, never to a later request on this thread.
    static FNX_EXECUTION: RefCell<Option<FnxExecutionTrace>> = const { RefCell::new(None) };
}

struct ExecutionCaptureGuard {
    previous: Option<FnxExecutionTrace>,
}

impl Drop for ExecutionCaptureGuard {
    fn drop(&mut self) {
        FNX_EXECUTION.with(|execution| {
            execution.replace(self.previous.take());
        });
    }
}

/// Capture only algorithms invoked within this call, restoring any outer capture on unwind.
pub(crate) fn capture_fnx_execution<T>(f: impl FnOnce() -> T) -> (T, FnxExecutionTrace) {
    let previous =
        FNX_EXECUTION.with(|execution| execution.replace(Some(FnxExecutionTrace::default())));
    let guard = ExecutionCaptureGuard { previous };
    let result = f();
    let trace = FNX_EXECUTION.with(|execution| execution.borrow_mut().take().unwrap_or_default());
    drop(guard);
    (result, trace)
}

struct AlgorithmTimer {
    started: Instant,
}

impl Drop for AlgorithmTimer {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed();
        FNX_EXECUTION.with(|execution| {
            if let Some(trace) = execution.borrow_mut().as_mut() {
                // Preserve sub-microsecond contributions until the final evidence conversion.
                trace.analysis_time = trace.analysis_time.saturating_add(elapsed);
            }
        });
    }
}

/// Record an actual FNX invocation and its duration without holding a borrow across the call.
pub(crate) fn measure_fnx_algorithm<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let capturing = FNX_EXECUTION.with(|execution| {
        let mut execution = execution.borrow_mut();
        let Some(trace) = execution.as_mut() else {
            return false;
        };
        if !trace.algorithms_invoked.contains(&name) {
            trace.algorithms_invoked.push(name);
        }
        true
    });
    if !capturing {
        return f();
    }

    let timer = AlgorithmTimer {
        started: Instant::now(),
    };
    let result = f();
    drop(timer);
    result
}

/// Preserve the first observed cancellation, including cancellation before any algorithm ran.
pub(crate) fn record_fnx_budget_exceeded(reason: &BudgetExceededReason) {
    FNX_EXECUTION.with(|execution| {
        if let Some(trace) = execution.borrow_mut().as_mut() {
            trace
                .budget_exceeded_reason
                .get_or_insert(reason.reason_code());
        }
    });
}

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
    pub const BUDGET_EXCEEDED: Self = Self {
        category: "BUDGET",
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

/// A structural analysis pass whose results have been collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnxAnalysisPass {
    ConnectedComponents,
    ArticulationPoints,
    Bridges,
    CycleBasis,
}

impl FnxAnalysisPass {
    /// Algorithm name used by analysis witnesses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConnectedComponents => "connected_components",
            Self::ArticulationPoints => "articulation_points",
            Self::Bridges => "bridges",
            Self::CycleBasis => "cycle_basis",
        }
    }
}

/// Results from FNX structural analysis, including its actual progress.
///
/// Counts are meaningful only for passes present in `completed_passes`. In particular, a zero
/// count for a pass that never ran does not prove the absence of that structural feature.
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
    /// Passes that ran to completion, in execution order. Projection is not an analysis pass.
    pub completed_passes: Vec<FnxAnalysisPass>,
    /// Cancellation reason, retained even when a caller extracts the partial result.
    pub budget_exceeded: Option<BudgetExceededReason>,
}

impl FnxAnalysisResults {
    /// Whether the result contains findings from this pass.
    #[must_use]
    pub fn completed(&self, pass: FnxAnalysisPass) -> bool {
        self.completed_passes.contains(&pass)
    }

    fn record_budget_exceeded(&mut self, reason: BudgetExceededReason) {
        let completed = if self.completed_passes.is_empty() {
            "none".to_string()
        } else {
            self.completed_passes
                .iter()
                .map(|pass| pass.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        self.diagnostics.push(FnxDiagnostic {
            code: FnxDiagnosticCode::BUDGET_EXCEEDED,
            severity: FnxDiagnosticSeverity::Warning,
            message: format!(
                "FNX structural analysis exceeded its budget; unperformed checks have no results. \
                 {}. Completed analyses: {completed}.",
                reason.as_diagnostic()
            ),
            span: None,
            related_nodes: Vec::new(),
            related_edges: Vec::new(),
            suggestion: Some(
                "Reduce the diagram size and retry. Library callers can set a larger budget with \
                 analyze_structure_with_budget before relying on complete FNX structural validation."
                    .to_string(),
            ),
            recommendation: None,
        });
        self.budget_exceeded = Some(reason);
    }
}

// ============================================================================
// Analysis Functions
// ============================================================================

/// Run all FNX structural analyses on the given IR.
///
/// Returns analysis results with diagnostics that can be surfaced to users. A budget-limited
/// result retains the cancellation reason and a warning alongside the completed passes' findings.
#[must_use]
pub fn analyze_structure(ir: &MermaidDiagramIr) -> FnxAnalysisResults {
    analyze_structure_with_budget(ir, AnalysisBudget::default())
        .into_value()
        .unwrap_or_default()
}

/// Run FNX structural analyses with deterministic LRU caching via [`crate::fnx_cache::AnalysisCache`] (`bd-lw6f5`).
pub fn analyze_structure_cached(
    ir: &MermaidDiagramIr,
    config: &crate::fnx_adapter::ProjectionConfig,
    cache: &mut crate::fnx_cache::AnalysisCache,
) -> FnxAnalysisResults {
    if let Some(cached) = cache.get_diagnostics(ir, config) {
        return cached;
    }
    let results =
        analyze_structure_with_projection_and_budget(ir, config, AnalysisBudget::default())
            .into_value()
            .unwrap_or_default();
    // A transient timeout must not turn an incomplete analysis into the permanent cached answer.
    if results.budget_exceeded.is_none() {
        cache.put_diagnostics(ir, config, results.clone());
    }
    results
}

/// Run FNX structural analyses under an explicit [`AnalysisBudget`].
///
/// Tracks graph-projection and per-pass work units via [`BudgetExecutor`] and stops
/// running further expensive passes (bridges, cycle basis) early if the work or time
/// budget has been exceeded.
#[must_use]
pub fn analyze_structure_with_budget(
    ir: &MermaidDiagramIr,
    budget: AnalysisBudget,
) -> BudgetResult<FnxAnalysisResults> {
    analyze_structure_with_projection_and_budget(ir, &ProjectionConfig::default(), budget)
}

fn analyze_structure_with_projection_and_budget(
    ir: &MermaidDiagramIr,
    projection: &ProjectionConfig,
    budget: AnalysisBudget,
) -> BudgetResult<FnxAnalysisResults> {
    let mut executor = BudgetExecutor::new(budget);
    let mut outcome = executor.execute_with_progress(|exec| {
        if ir.nodes.is_empty() {
            return FnxAnalysisResults::default();
        }

        let base_work = (ir.nodes.len() + ir.edges.len()).max(1) as u64;
        let (graph, table, _) = ir_to_graph_with_policy(ir, projection);
        exec.record_work(base_work);
        let mut results = FnxAnalysisResults::default();
        if exec.is_exceeded() {
            return results;
        }

        // Run connected components analysis
        let comp_result =
            measure_fnx_algorithm("connected_components", || connected_components(&graph));
        analyze_components(ir, &table, &comp_result, &mut results);
        results
            .completed_passes
            .push(FnxAnalysisPass::ConnectedComponents);
        exec.record_work(base_work);
        if exec.is_exceeded() {
            return results;
        }

        // Articulation points and bridges remain meaningful within disconnected components.
        let art_result =
            measure_fnx_algorithm("articulation_points", || articulation_points(&graph));
        analyze_articulation_points(ir, &table, &art_result, &mut results);
        results
            .completed_passes
            .push(FnxAnalysisPass::ArticulationPoints);
        exec.record_work(base_work);
        if exec.is_exceeded() {
            return results;
        }

        let bridge_result = measure_fnx_algorithm("bridges", || bridges(&graph));
        analyze_bridges(ir, &table, &bridge_result, &mut results);
        results.completed_passes.push(FnxAnalysisPass::Bridges);
        exec.record_work(base_work);
        if exec.is_exceeded() {
            return results;
        }

        // Run cycle analysis
        let cycle_result = measure_fnx_algorithm("cycle_basis", || cycle_basis(&graph, None));
        analyze_cycles(ir, &table, &cycle_result, &mut results);
        results.completed_passes.push(FnxAnalysisPass::CycleBasis);
        exec.record_work(base_work);

        results
    });
    if let BudgetResult::Cancelled { reason, partial } = &mut outcome {
        record_fnx_budget_exceeded(reason);
        // Preserve cancellation in the value itself: the default entry point and cache return
        // this value directly, and `.into_value()` must not erase an incomplete-analysis warning.
        partial
            .get_or_insert_with(FnxAnalysisResults::default)
            .record_budget_exceeded(reason.clone());
    }
    outcome
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
    out.bridge_count = 0;

    for (source_fnx, target_fnx) in &result.edges {
        let Some(source_idx) = table.get_ir_node_index(source_fnx) else {
            continue;
        };
        let Some(target_idx) = table.get_ir_node_index(target_fnx) else {
            continue;
        };
        let (first, second) = if source_idx <= target_idx {
            (source_fnx, target_fnx)
        } else {
            (target_fnx, source_fnx)
        };
        let contributors = table.get_edge_meta_all(first, second);
        let [meta] = contributors.as_slice() else {
            // Removing one of several parallel or opposing IR edges leaves this connection.
            continue;
        };
        let source_idx = meta.source_index;
        let target_idx = meta.target_index;
        let source_node = ir.nodes.get(source_idx);
        let target_node = ir.nodes.get(target_idx);

        let (source_name, target_name) = match (source_node, target_node) {
            (Some(s), Some(t)) => (s.id.as_str(), t.id.as_str()),
            _ => continue,
        };

        let edge_span = ir.edges.get(meta.ir_edge_index).map(|edge| edge.span);

        out.bridge_count += 1;
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

    #[test]
    fn execution_capture_deduplicates_invocations_and_accumulates_time() {
        use std::time::Duration;

        measure_fnx_algorithm("outside_capture", || 7);
        let (first_duration, trace) = capture_fnx_execution(|| {
            measure_fnx_algorithm("first", || std::thread::sleep(Duration::from_millis(1)));
            let first_duration =
                FNX_EXECUTION.with(|execution| execution.borrow().as_ref().unwrap().analysis_time);
            measure_fnx_algorithm("second", || std::thread::sleep(Duration::from_millis(1)));
            assert_eq!(measure_fnx_algorithm("first", || 42), 42);
            first_duration
        });
        assert_eq!(trace.algorithms_invoked, ["first", "second"]);
        assert!(first_duration >= Duration::from_millis(1));
        assert!(trace.analysis_time >= first_duration + Duration::from_millis(1));

        let mut other_timing = trace.clone();
        other_timing.analysis_time += Duration::from_secs(1);
        assert_eq!(trace, other_timing, "elapsed time is not trace identity");
        other_timing.algorithms_invoked.push("another_pass");
        assert_ne!(trace, other_timing);

        let (_, next) = capture_fnx_execution(|| ());
        assert!(next.algorithms_invoked.is_empty());
        assert!(next.analysis_time.is_zero());
        assert!(next.budget_exceeded_reason.is_none());
    }

    #[test]
    fn execution_captures_restore_outer_scope_after_nesting_and_unwind() {
        let (inner, outer) = capture_fnx_execution(|| {
            measure_fnx_algorithm("outer_before", || ());
            let (_, inner) = capture_fnx_execution(|| {
                measure_fnx_algorithm("inner", || ());
            });
            let aborted = std::panic::catch_unwind(|| {
                capture_fnx_execution(|| {
                    measure_fnx_algorithm::<()>("aborted_inner", || panic!("test unwind"));
                });
            });
            assert!(aborted.is_err());
            let attempted = std::panic::catch_unwind(|| {
                measure_fnx_algorithm::<()>("attempted", || panic!("test algorithm unwind"));
            });
            assert!(attempted.is_err());
            measure_fnx_algorithm("outer_after", || ());
            inner
        });
        assert_eq!(inner.algorithms_invoked, ["inner"]);
        assert_eq!(
            outer.algorithms_invoked,
            ["outer_before", "attempted", "outer_after"]
        );

        assert!(
            std::panic::catch_unwind(|| {
                capture_fnx_execution(|| panic!("test outer unwind"));
            })
            .is_err()
        );
        let (_, next) = capture_fnx_execution(|| ());
        assert!(next.algorithms_invoked.is_empty());
        assert!(next.analysis_time.is_zero());
    }

    #[test]
    fn execution_captures_keep_concurrent_threads_independent() {
        let (worker_trace, main_trace) = capture_fnx_execution(|| {
            measure_fnx_algorithm("main_before", || ());
            let worker = std::thread::spawn(|| {
                measure_fnx_algorithm("uncaptured_worker", || ());
                capture_fnx_execution(|| measure_fnx_algorithm("worker", || ())).1
            });
            measure_fnx_algorithm("main_after", || ());
            worker.join().unwrap()
        });
        assert_eq!(main_trace.algorithms_invoked, ["main_before", "main_after"]);
        assert_eq!(worker_trace.algorithms_invoked, ["worker"]);
    }

    #[test]
    fn execution_diagnostics_report_actual_passes_and_skip_cache_hits() {
        let ir = make_simple_chain();
        let (result, trace) = capture_fnx_execution(|| {
            analyze_structure_with_budget(&ir, AnalysisBudget::unlimited())
        });
        assert!(result.is_completed());
        assert_eq!(
            trace.algorithms_invoked,
            [
                "connected_components",
                "articulation_points",
                "bridges",
                "cycle_basis"
            ]
        );
        assert!(trace.budget_exceeded_reason.is_none());

        let mut cache = crate::fnx_cache::AnalysisCache::default();
        let projection = ProjectionConfig::default();
        let results = result.into_value().unwrap();
        cache.put_diagnostics(&ir, &projection, results);
        let (cached, trace) =
            capture_fnx_execution(|| analyze_structure_cached(&ir, &projection, &mut cache));
        assert_eq!(cached.completed_passes.len(), 4);
        assert!(
            trace.algorithms_invoked.is_empty(),
            "cached results are not new invocations"
        );
        assert!(trace.analysis_time.is_zero());
    }

    #[test]
    fn execution_diagnostics_preserve_budget_evidence_before_and_after_a_pass() {
        let ir = make_simple_chain();
        let base_work = (ir.nodes.len() + ir.edges.len()) as u64;
        for (work_budget, expected) in [(0, vec![]), (base_work + 1, vec!["connected_components"])]
        {
            let (result, trace) = capture_fnx_execution(|| {
                analyze_structure_with_budget(
                    &ir,
                    AnalysisBudget::unlimited().with_work_budget(work_budget),
                )
            });
            assert!(result.is_cancelled());
            assert_eq!(trace.algorithms_invoked, expected);
            assert_eq!(trace.budget_exceeded_reason, Some("work_exceeded"));
            if expected.is_empty() {
                assert!(trace.analysis_time.is_zero());
            }
        }
    }

    #[test]
    fn execution_ordering_reports_only_invoked_centrality() {
        let ir = make_simple_chain();
        let (scores, trace) =
            capture_fnx_execution(|| crate::fnx_ordering::compute_centrality_scores(&ir));
        assert!(scores.computed);
        assert_eq!(trace.algorithms_invoked, ["degree_centrality"]);

        let (_, empty) = capture_fnx_execution(|| {
            crate::fnx_ordering::compute_centrality_scores(&MermaidDiagramIr::default())
        });
        assert!(empty.algorithms_invoked.is_empty());
        assert!(empty.analysis_time.is_zero());
    }

    #[test]
    fn execution_criticality_reports_actual_calls_and_partial_budget_work() {
        use crate::fnx_cycle_scorer::{
            CriticalityScoringConfig, compute_criticality_scores_with_budget,
        };

        let ir = make_simple_chain();
        let config = CriticalityScoringConfig::default();
        let base_work = (ir.nodes.len() + ir.edges.len()) as u64;
        let (result, full) = capture_fnx_execution(|| {
            compute_criticality_scores_with_budget(&ir, &config, AnalysisBudget::unlimited())
        });
        assert!(result.is_completed());
        assert_eq!(
            full.algorithms_invoked,
            ["bridges", "articulation_points", "degree_centrality"]
        );
        assert!(full.budget_exceeded_reason.is_none());

        for (work_budget, expected) in [(0, vec![]), (base_work + 1, vec!["bridges"])] {
            let (result, trace) = capture_fnx_execution(|| {
                compute_criticality_scores_with_budget(
                    &ir,
                    &config,
                    AnalysisBudget::unlimited().with_work_budget(work_budget),
                )
            });
            assert!(result.is_cancelled());
            assert_eq!(trace.algorithms_invoked, expected);
            assert_eq!(trace.budget_exceeded_reason, Some("work_exceeded"));
            assert!(result.into_value().unwrap().scores.is_empty());
        }
    }

    #[test]
    fn execution_budget_reason_is_first_observed_and_scoped() {
        let work = BudgetExceededReason::WorkExceeded {
            budget: 0,
            consumed: 1,
        };
        let time = BudgetExceededReason::TimeExceeded {
            budget_ms: 0,
            elapsed_ms: 1,
        };
        record_fnx_budget_exceeded(&work);
        let (inner, outer) = capture_fnx_execution(|| {
            record_fnx_budget_exceeded(&work);
            let (_, inner) = capture_fnx_execution(|| record_fnx_budget_exceeded(&time));
            record_fnx_budget_exceeded(&time);
            inner
        });
        assert_eq!(inner.budget_exceeded_reason, Some("time_exceeded"));
        assert_eq!(outer.budget_exceeded_reason, Some("work_exceeded"));
        assert!(outer.algorithms_invoked.is_empty());
        assert!(outer.analysis_time.is_zero());
        let (_, next) = capture_fnx_execution(|| ());
        assert!(next.budget_exceeded_reason.is_none());
        assert_ne!(outer, next, "budget cancellation is part of trace identity");
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
    fn disconnected_graph_keeps_component_articulation_and_bridge_findings() {
        let mut ir = make_simple_chain();
        ir.nodes.push(IrNode {
            id: "Island".to_string(),
            ..Default::default()
        });
        let results = analyze_structure(&ir);
        assert!(!results.is_connected);
        assert_eq!(results.component_count, 2);
        assert_eq!(results.articulation_point_count, 1);
        assert_eq!(results.bridge_count, 2);
        assert!(results.completed(FnxAnalysisPass::ArticulationPoints));
        assert!(results.completed(FnxAnalysisPass::Bridges));
        assert!(results.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == FnxDiagnosticCode::ARTICULATION_POINT
                && diagnostic.related_nodes == vec![IrNodeId(1)]
        }));
        assert_eq!(
            results
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == FnxDiagnosticCode::BRIDGE_EDGE)
                .count(),
            2
        );
    }

    #[test]
    fn parallel_and_opposing_edges_are_not_individual_bridges() {
        let mut ir = make_simple_chain();
        ir.edges.push(ir.edges[0].clone());
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(0)),
            ..Default::default()
        });
        for collapse_parallel_edges in [false, true] {
            let projection = ProjectionConfig {
                collapse_parallel_edges,
                ..Default::default()
            };
            let mut cache = crate::fnx_cache::AnalysisCache::default();
            let results = analyze_structure_cached(&ir, &projection, &mut cache);
            assert_eq!(results.bridge_count, 1);
            let bridges: Vec<_> = results
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == FnxDiagnosticCode::BRIDGE_EDGE)
                .collect();
            assert_eq!(bridges.len(), 1);
            assert_eq!(bridges[0].related_edges, vec![(1, 2)]);
            assert_eq!(bridges[0].span, Some(ir.edges[1].span));
        }
    }

    #[test]
    fn bridge_diagnostic_preserves_original_direction_and_source_span() {
        let mut ir = make_simple_chain();
        ir.edges[0].from = IrEndpoint::Node(IrNodeId(1));
        ir.edges[0].to = IrEndpoint::Node(IrNodeId(0));
        ir.edges[0].span.start.line = 17;
        ir.edges[0].span.start.col = 4;
        ir.edges[0].span.start.byte = 120;
        ir.edges[0].span.end = ir.edges[0].span.start;
        let results = analyze_structure(&ir);
        let bridge = results
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.related_edges == vec![(1, 0)])
            .expect("B to A bridge");
        assert_eq!(bridge.code, FnxDiagnosticCode::BRIDGE_EDGE);
        assert_eq!(bridge.related_nodes, vec![IrNodeId(1), IrNodeId(0)]);
        assert_eq!(bridge.span, Some(ir.edges[0].span));
        assert!(bridge.message.contains("'B' → 'A'"));
        assert_eq!(results.bridge_count, 2);
    }

    #[test]
    fn cached_diagnostics_apply_self_loop_projection_policy() {
        let mut ir = make_simple_chain();
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(1)),
            ..Default::default()
        });
        let mut cache = crate::fnx_cache::AnalysisCache::default();
        let keep = ProjectionConfig::default();
        let remove = ProjectionConfig {
            preserve_self_loops: false,
            ..Default::default()
        };
        let preserved = analyze_structure_cached(&ir, &keep, &mut cache);
        let removed = analyze_structure_cached(&ir, &remove, &mut cache);
        assert_eq!(preserved.cycle_count, 1);
        assert_eq!(removed.cycle_count, 0);
        assert_eq!(removed.bridge_count, 2);
        let hits = cache.stats().hits;
        let cached = analyze_structure_cached(&ir, &remove, &mut cache);
        assert_eq!(cached.cycle_count, 0);
        assert_eq!(cache.stats().hits, hits + 1);
        assert!(cached.budget_exceeded.is_none());
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

    #[test]
    fn analyze_structure_with_budget_enforces_work_limit() {
        let ir = make_simple_chain();
        let tight = AnalysisBudget::unlimited().with_work_budget(1);
        let result = analyze_structure_with_budget(&ir, tight);
        assert!(result.is_cancelled());
        let partial = result.into_value().expect("partial analysis");
        assert!(partial.completed_passes.is_empty());
        assert_eq!(
            partial.budget_exceeded,
            Some(BudgetExceededReason::WorkExceeded {
                budget: 1,
                consumed: 5,
            })
        );
        assert_eq!(partial.diagnostics.len(), 1);
        assert_eq!(
            partial.diagnostics[0].code,
            FnxDiagnosticCode::BUDGET_EXCEEDED
        );
        assert_eq!(
            partial.diagnostics[0].severity,
            FnxDiagnosticSeverity::Warning
        );
        assert!(
            partial.diagnostics[0]
                .message
                .contains("Completed analyses: none")
        );
    }

    #[test]
    fn budget_progress_records_only_passes_that_actually_ran() {
        let ir = make_simple_chain();
        let passes = [
            FnxAnalysisPass::ConnectedComponents,
            FnxAnalysisPass::ArticulationPoints,
            FnxAnalysisPass::Bridges,
            FnxAnalysisPass::CycleBasis,
        ];
        // This graph has three vertices and two edges. Projection and each analysis consume
        // five work units, so these exact boundaries exercise every possible stopping point.
        for (limit, completed) in [(0, 0), (5, 1), (10, 2), (15, 3), (20, 4)] {
            let result = analyze_structure_with_budget(
                &ir,
                AnalysisBudget::unlimited().with_work_budget(limit),
            );
            assert!(result.is_cancelled(), "limit {limit}");
            let partial = result.into_value().expect("partial analysis");
            assert_eq!(
                partial.completed_passes,
                passes[..completed],
                "limit {limit}"
            );
            assert!(partial.budget_exceeded.is_some());
            assert_eq!(
                partial
                    .diagnostics
                    .iter()
                    .filter(|diag| diag.code == FnxDiagnosticCode::BUDGET_EXCEEDED)
                    .count(),
                1
            );
            assert_eq!(partial.completed(FnxAnalysisPass::Bridges), completed >= 3);
        }

        let result =
            analyze_structure_with_budget(&ir, AnalysisBudget::unlimited().with_work_budget(25));
        assert!(result.is_completed());
        let complete = result.into_value().expect("complete analysis");
        assert_eq!(complete.completed_passes, passes);
        assert!(complete.budget_exceeded.is_none());
        assert_eq!(complete.component_count, 1);
        assert_eq!(complete.articulation_point_count, 1);
        assert_eq!(complete.bridge_count, 2);
        assert_eq!(complete.cycle_count, 0);
        assert!(
            complete
                .diagnostics
                .iter()
                .all(|diag| diag.code != FnxDiagnosticCode::BUDGET_EXCEEDED)
        );
    }

    #[test]
    fn cancelled_analysis_preserves_findings_from_completed_passes() {
        let ir = make_disconnected_graph();
        let result =
            analyze_structure_with_budget(&ir, AnalysisBudget::unlimited().with_work_budget(6));
        assert!(result.is_cancelled());
        let partial = result.into_value().expect("partial analysis");
        assert_eq!(
            partial.completed_passes,
            [FnxAnalysisPass::ConnectedComponents]
        );
        assert_eq!(partial.component_count, 2);
        assert!(!partial.is_connected);
        assert_eq!(partial.diagnostics.len(), 2);
        assert_eq!(
            partial.diagnostics[0].code,
            FnxDiagnosticCode::DISCONNECTED_COMPONENT
        );
        assert_eq!(
            partial.diagnostics[1].code,
            FnxDiagnosticCode::BUDGET_EXCEEDED
        );
    }

    #[test]
    fn default_entry_point_preserves_cancellation_and_does_not_cache_it() {
        let mut ir = make_simple_chain();
        // Projection alone exceeds the default work budget, independently of clock speed.
        ir.edges = vec![ir.edges[0].clone(); 10_001];
        let results = analyze_structure(&ir);
        assert!(results.budget_exceeded.is_some());
        assert!(results.completed_passes.is_empty());
        assert_eq!(
            results.diagnostics[0].code,
            FnxDiagnosticCode::BUDGET_EXCEEDED
        );

        let mut cache = crate::fnx_cache::AnalysisCache::default();
        let config = crate::fnx_adapter::ProjectionConfig::default();
        for _ in 0..2 {
            let results = analyze_structure_cached(&ir, &config, &mut cache);
            assert!(results.budget_exceeded.is_some());
        }
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 2);
    }

    #[test]
    fn empty_analysis_does_not_claim_to_have_invoked_algorithms() {
        let result = analyze_structure_with_budget(
            &MermaidDiagramIr::default(),
            AnalysisBudget::unlimited().with_work_budget(0),
        );
        assert!(result.is_completed());
        let results = result.into_value().expect("empty analysis");
        assert!(results.completed_passes.is_empty());
        assert!(results.budget_exceeded.is_none());
        assert!(results.diagnostics.is_empty());
    }
}
