//! E-graph equality saturation for crossing minimization.
//!
//! Implements the egg crate integration for finding optimal node orderings
//! in Sugiyama layers. Uses equality saturation to explore the space of
//! equivalent orderings and extract the one with minimum edge crossings.
//!
//! # Implementation (bd-1xma.2)
//!
//! This module provides:
//! - `OrderingLang`: The egg Language trait for layer orderings
//! - Rewrite rules for generating equivalent orderings
//! - `CrossingCost`: Cost function for extraction based on crossing count
//! - `saturate_layer`: Main entry point for equality saturation

use crate::egraph_ordering::{LayerEdges, LayerOrdering};
use egg::{EGraph, Id, RecExpr, Runner, define_language, rewrite};

// ============================================================================
// Language Definition
// ============================================================================

// E-graph language for layer orderings.
//
// We encode orderings as sequences of "place" operations:
// `(seq n0 (seq n1 (seq n2 nil)))` represents ordering [n0, n1, n2].
//
// Using symbols for node IDs (node_0, node_1, ...) allows pattern matching.
define_language! {
    pub enum OrderingLang {
        // Sequence cons: (seq <node_id> <rest>)
        "seq" = Seq([Id; 2]),
        // Empty sequence terminator
        "nil" = Nil,
        // Node identifier (encoded as symbol)
        Symbol(egg::Symbol),
    }
}

/// Convert a LayerOrdering to an egg RecExpr.
///
/// Encodes [n0, n1, n2] as `(seq node_0 (seq node_1 (seq node_2 nil)))`.
#[must_use]
pub fn ordering_to_expr(ordering: &LayerOrdering) -> RecExpr<OrderingLang> {
    let mut expr = RecExpr::default();

    // Build from right to left: nil, then seq wrappers
    let nil_id = expr.add(OrderingLang::Nil);

    let mut current_id = nil_id;
    for &node_id in ordering.order.iter().rev() {
        let node_sym = expr.add(OrderingLang::Symbol(format!("node_{node_id}").into()));
        current_id = expr.add(OrderingLang::Seq([node_sym, current_id]));
    }

    expr
}

/// Extract a LayerOrdering from an egg RecExpr.
///
/// Reverses the encoding from `ordering_to_expr`.
#[must_use]
pub fn expr_to_ordering(expr: &RecExpr<OrderingLang>) -> Option<LayerOrdering> {
    let mut order = Vec::new();
    let root = Id::from(expr.as_ref().len() - 1);
    extract_ordering_recursive(expr, root, &mut order)?;
    Some(LayerOrdering::new(order))
}

fn extract_ordering_recursive(
    expr: &RecExpr<OrderingLang>,
    id: Id,
    order: &mut Vec<usize>,
) -> Option<()> {
    match &expr[id] {
        OrderingLang::Nil => Some(()),
        OrderingLang::Seq([node_id, rest_id]) => {
            // Extract node ID from symbol
            if let OrderingLang::Symbol(sym) = &expr[*node_id] {
                let s = sym.as_str();
                if let Some(num_str) = s.strip_prefix("node_") {
                    let node: usize = num_str.parse().ok()?;
                    order.push(node);
                }
            }
            extract_ordering_recursive(expr, *rest_id, order)
        }
        OrderingLang::Symbol(_) => None, // Unexpected at top level
    }
}

// ============================================================================
// Rewrite Rules
// ============================================================================

/// Generate rewrite rules for adjacent swaps.
///
/// Pattern: `(seq ?a (seq ?b ?rest))` => `(seq ?b (seq ?a ?rest))`
/// This swaps any two adjacent elements.
#[must_use]
pub fn adjacent_swap_rules() -> Vec<egg::Rewrite<OrderingLang, ()>> {
    vec![rewrite!("swap-adjacent"; "(seq ?a (seq ?b ?rest))" => "(seq ?b (seq ?a ?rest))")]
}

// ============================================================================
// Analysis and Cost Function
// ============================================================================

/// Context for crossing count computation during extraction.
#[derive(Clone, Default)]
pub struct CrossingContext {
    /// Fixed upper layer ordering (if any).
    pub upper_ordering: Option<LayerOrdering>,
    /// Edges from upper layer to current layer.
    pub upper_edges: Option<LayerEdges>,
    /// Fixed lower layer ordering (if any).
    pub lower_ordering: Option<LayerOrdering>,
    /// Edges from current layer to lower layer.
    pub lower_edges: Option<LayerEdges>,
}

/// Extract the best ordering from a saturated e-graph.
///
/// Collects all orderings represented in the e-graph (up to a limit) and returns
/// the one with minimum crossing count.
#[must_use]
pub fn extract_best_ordering(
    egraph: &EGraph<OrderingLang, ()>,
    root: Id,
    ctx: &CrossingContext,
) -> Option<(LayerOrdering, usize)> {
    // Collect all orderings in the e-graph
    let orderings = collect_orderings(egraph, root, 1000);

    // Find the one with minimum crossings
    let mut best: Option<(LayerOrdering, usize)> = None;

    for ordering in orderings {
        let crossings = crate::egraph_ordering::local_crossing_count(
            &ordering,
            ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
            ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
        );

        match &best {
            Some((_, best_crossings)) if crossings < *best_crossings => {
                best = Some((ordering, crossings));
            }
            Some(_) => {}
            None => {
                best = Some((ordering, crossings));
            }
        }

        if crossings == 0 {
            break; // Early exit if we found optimal
        }
    }

    best
}

/// Collect all orderings represented in an e-class, up to a limit.
fn collect_orderings(
    egraph: &EGraph<OrderingLang, ()>,
    root: Id,
    limit: usize,
) -> Vec<LayerOrdering> {
    let mut orderings = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![(root, Vec::<usize>::new())];

    while let Some((id, partial)) = stack.pop() {
        if orderings.len() >= limit {
            break;
        }

        let canonical = egraph.find(id);
        let eclass = &egraph[canonical];

        for node in &eclass.nodes {
            match node {
                OrderingLang::Nil => {
                    // Complete ordering
                    if seen.insert(partial.clone()) {
                        orderings.push(LayerOrdering::new(partial.clone()));
                    }
                }
                OrderingLang::Symbol(_) => {
                    // Symbols shouldn't appear at sequence level
                }
                OrderingLang::Seq([node_id, rest_id]) => {
                    // Get node symbols and recurse
                    let node_canonical = egraph.find(*node_id);
                    for node_node in &egraph[node_canonical].nodes {
                        if let OrderingLang::Symbol(sym) = node_node
                            && let Some(node_idx) = parse_node_symbol(sym)
                        {
                            let mut new_partial = partial.clone();
                            new_partial.push(node_idx);
                            stack.push((*rest_id, new_partial));
                        }
                    }
                }
            }
        }
    }

    orderings
}

/// Parse a node symbol like "node_42" to extract the index.
fn parse_node_symbol(sym: &egg::Symbol) -> Option<usize> {
    sym.as_str().strip_prefix("node_")?.parse().ok()
}

// ============================================================================
// Budget and Configuration (bd-1xma.3)
// ============================================================================

/// Type of budget that was exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetType {
    /// E-graph node count exceeded max_enodes.
    NodeLimit,
    /// Wall-clock time exceeded max_time.
    TimeLimit,
    /// Iteration count exceeded max_iterations.
    IterationLimit,
}

/// Diagnostic information when a budget is exhausted.
#[derive(Debug, Clone)]
pub struct BudgetExhausted {
    /// Which budget type was exhausted.
    pub budget_type: BudgetType,
    /// Current value when budget fired.
    pub value: u64,
    /// Configured limit.
    pub limit: u64,
    /// Iterations completed before exhaustion.
    pub iterations_completed: usize,
    /// Best crossing count found so far.
    pub best_cost: usize,
}

/// Configuration for equality saturation with budget guards.
///
/// Implements the budget model from §6.6.3:
/// - Node budget: `min(100_000, 50 * |V|²)`
/// - Time budget: `min(500ms, 10ms * |layers|)`
/// - Iteration budget: 1000 rewrites
#[derive(Debug, Clone)]
pub struct SaturationConfig {
    /// Maximum number of e-graph nodes before stopping.
    pub node_limit: usize,
    /// Maximum number of iterations before stopping.
    pub iter_limit: usize,
    /// Maximum wall-clock time in milliseconds.
    pub time_limit_ms: u64,
}

impl Default for SaturationConfig {
    fn default() -> Self {
        Self {
            node_limit: 10_000,
            iter_limit: 30,
            time_limit_ms: 100,
        }
    }
}

impl SaturationConfig {
    /// Compute budget limits based on graph properties.
    ///
    /// Uses the formulas from §6.6:
    /// - Node budget: `min(100_000, 50 * node_count²)`
    /// - Time budget: `min(500ms, 10ms * layer_count)`
    /// - Iteration budget: 1000
    #[must_use]
    pub fn for_graph(node_count: usize, layer_count: usize) -> Self {
        // Use saturating arithmetic to prevent overflow for large graphs
        let node_limit = 50_usize
            .saturating_mul(node_count)
            .saturating_mul(node_count)
            .min(100_000);
        let time_limit_ms = (10_u64.saturating_mul(layer_count as u64)).min(500);
        let iter_limit = 1000;

        tracing::debug!(
            node_count,
            layer_count,
            node_limit,
            time_limit_ms,
            iter_limit,
            "egraph.budget.computed"
        );

        Self {
            node_limit,
            iter_limit,
            time_limit_ms,
        }
    }

    /// Create conservative config for interactive use (small graphs).
    #[must_use]
    pub fn interactive() -> Self {
        Self {
            node_limit: 5_000,
            iter_limit: 20,
            time_limit_ms: 50,
        }
    }

    /// Create permissive config for batch processing.
    #[must_use]
    pub fn batch() -> Self {
        Self {
            node_limit: 100_000,
            iter_limit: 100,
            time_limit_ms: 1000,
        }
    }
}

/// Result of equality saturation for a layer.
#[derive(Debug, Clone)]
pub struct SaturationResult {
    /// Best ordering found.
    pub ordering: LayerOrdering,
    /// Crossing count of best ordering.
    pub crossing_count: usize,
    /// Number of e-graph nodes at termination.
    pub egraph_nodes: usize,
    /// Number of iterations run.
    pub iterations: usize,
    /// Whether saturation hit a limit (vs. natural saturation).
    pub hit_limit: bool,
    /// Details about budget exhaustion (if any).
    pub budget_exhausted: Option<BudgetExhausted>,
}

/// Run equality saturation on a layer ordering.
///
/// This is the main entry point for e-graph-based crossing minimization.
/// Given an initial ordering and context (adjacent layers, edges), it:
/// 1. Encodes the ordering as an e-graph expression
/// 2. Applies rewrite rules until saturation or limit
/// 3. Extracts the ordering with minimum crossing count
///
/// # Arguments
/// * `initial` - Starting layer ordering
/// * `ctx` - Context with adjacent layer orderings and edges
/// * `config` - Saturation limits
#[must_use]
pub fn saturate_layer(
    initial: &LayerOrdering,
    ctx: &CrossingContext,
    config: &SaturationConfig,
) -> SaturationResult {
    let expr = ordering_to_expr(initial);
    let rules = adjacent_swap_rules();

    let runner = Runner::default()
        .with_expr(&expr)
        .with_node_limit(config.node_limit)
        .with_iter_limit(config.iter_limit)
        .with_time_limit(std::time::Duration::from_millis(config.time_limit_ms))
        .run(&rules);

    let egraph_nodes = runner.egraph.total_size();
    let iterations = runner.iterations.len();

    // Determine budget exhaustion details
    let (hit_limit, budget_exhausted) = match &runner.stop_reason {
        Some(egg::StopReason::NodeLimit(n)) => {
            tracing::warn!(
                node_count = *n,
                limit = config.node_limit,
                iterations,
                "egraph.budget.node_limit_exhausted"
            );
            (
                true,
                Some(BudgetExhausted {
                    budget_type: BudgetType::NodeLimit,
                    value: *n as u64,
                    limit: config.node_limit as u64,
                    iterations_completed: iterations,
                    best_cost: 0, // Will be updated after extraction
                }),
            )
        }
        Some(egg::StopReason::IterationLimit(n)) => {
            tracing::warn!(
                iteration_count = *n,
                limit = config.iter_limit,
                "egraph.budget.iteration_limit_exhausted"
            );
            (
                true,
                Some(BudgetExhausted {
                    budget_type: BudgetType::IterationLimit,
                    value: *n as u64,
                    limit: config.iter_limit as u64,
                    iterations_completed: iterations,
                    best_cost: 0,
                }),
            )
        }
        Some(egg::StopReason::TimeLimit(duration_secs)) => {
            let elapsed_ms = (*duration_secs * 1000.0) as u64;
            tracing::warn!(
                elapsed_ms,
                limit_ms = config.time_limit_ms,
                iterations,
                "egraph.budget.time_limit_exhausted"
            );
            (
                true,
                Some(BudgetExhausted {
                    budget_type: BudgetType::TimeLimit,
                    value: elapsed_ms,
                    limit: config.time_limit_ms,
                    iterations_completed: iterations,
                    best_cost: 0,
                }),
            )
        }
        Some(egg::StopReason::Saturated) | None => {
            tracing::debug!(
                egraph_nodes,
                iterations,
                "egraph.saturation.completed_naturally"
            );
            (false, None)
        }
        Some(egg::StopReason::Other(_)) => (false, None),
    };

    // Extract best ordering
    let root = runner.roots[0];
    let (ordering, crossing_count) = extract_best_ordering(&runner.egraph, root, ctx)
        .unwrap_or_else(|| {
            // Fallback to initial ordering if extraction fails
            let crossings = crate::egraph_ordering::local_crossing_count(
                initial,
                ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
                ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
            );
            (initial.clone(), crossings)
        });

    // Update best_cost in budget_exhausted
    let budget_exhausted = budget_exhausted.map(|mut b| {
        b.best_cost = crossing_count;
        b
    });

    SaturationResult {
        ordering,
        crossing_count,
        egraph_nodes,
        iterations,
        hit_limit,
        budget_exhausted,
    }
}

/// Run saturation and return best ordering if it improves over initial.
///
/// Convenience wrapper that only returns a result if saturation found
/// a better ordering than the input.
#[must_use]
pub fn saturate_layer_if_improves(
    initial: &LayerOrdering,
    ctx: &CrossingContext,
    config: &SaturationConfig,
) -> Option<SaturationResult> {
    let initial_crossings = crate::egraph_ordering::local_crossing_count(
        initial,
        ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
        ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
    );

    let result = saturate_layer(initial, ctx, config);

    if result.crossing_count < initial_crossings {
        Some(result)
    } else {
        None
    }
}

// ============================================================================
// Fallback Mechanism (bd-1xma.4)
// ============================================================================

/// Strategy that produced the final result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackStrategy {
    /// E-graph saturation completed within budget.
    EGraphCompleted,
    /// E-graph exceeded budget but still produced the best result.
    EGraphExceededButWon,
    /// Greedy heuristic produced a better result than e-graph.
    GreedyWon,
    /// Greedy heuristic used because e-graph was not attempted.
    GreedyOnly,
}

/// Diagnostic information for layout degradation events.
#[derive(Debug, Clone)]
pub struct FallbackDiagnostic {
    /// Which budget was exceeded (if any).
    pub budget_type: Option<BudgetType>,
    /// Budget value when exceeded.
    pub budget_value: Option<u64>,
    /// Budget limit that was exceeded.
    pub budget_limit: Option<u64>,
    /// Strategy that produced the final result.
    pub fallback_strategy: FallbackStrategy,
    /// Crossing count of e-graph result.
    pub egraph_crossings: Option<usize>,
    /// Crossing count of greedy result.
    pub greedy_crossings: usize,
    /// Delta: greedy - final (positive means we improved over greedy).
    pub crossing_delta: isize,
}

/// Result of saturation with fallback to greedy heuristic.
#[derive(Debug, Clone)]
pub struct FallbackResult {
    /// Best ordering found (from either strategy).
    pub ordering: LayerOrdering,
    /// Crossing count of best ordering.
    pub crossing_count: usize,
    /// Which strategy produced this result.
    pub strategy: FallbackStrategy,
    /// E-graph saturation result (if attempted).
    pub egraph_result: Option<SaturationResult>,
    /// Greedy optimization result.
    pub greedy_result: crate::egraph_ordering::LayerOptimizationResult,
    /// Diagnostic information for telemetry.
    pub diagnostic: FallbackDiagnostic,
}

/// Run equality saturation with automatic fallback to greedy heuristic.
///
/// This implements the "anytime algorithm" pattern from bd-1xma.4:
/// 1. If layer is too large (>100 nodes), skip e-graph and use greedy only.
/// 2. Otherwise, run e-graph saturation with budget limits.
/// 3. In parallel (conceptually), run greedy barycenter heuristic.
/// 4. If e-graph exceeds budget, compare best-so-far with greedy result.
/// 5. Return the better of the two, with diagnostic telemetry.
///
/// Guarantees:
/// - Result is never worse than greedy-only baseline.
/// - Completes within config's time budget + small overhead.
/// - Emits structured diagnostics for all fallback events.
#[must_use]
pub fn saturate_with_fallback(
    initial: &LayerOrdering,
    ctx: &CrossingContext,
    config: &SaturationConfig,
) -> FallbackResult {
    // Always compute greedy result as baseline
    let greedy_result = crate::egraph_ordering::optimize_layer_ordering(
        initial,
        ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
        ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
    );

    // Check if e-graph is practical for this layer size
    if !crate::egraph_ordering::should_use_egraph(initial.len()) {
        tracing::debug!(
            layer_size = initial.len(),
            greedy_crossings = greedy_result.crossing_count,
            "egraph.fallback.skipped_large_layer"
        );

        return FallbackResult {
            ordering: greedy_result.ordering.clone(),
            crossing_count: greedy_result.crossing_count,
            strategy: FallbackStrategy::GreedyOnly,
            egraph_result: None,
            greedy_result: greedy_result.clone(),
            diagnostic: FallbackDiagnostic {
                budget_type: None,
                budget_value: None,
                budget_limit: None,
                fallback_strategy: FallbackStrategy::GreedyOnly,
                egraph_crossings: None,
                greedy_crossings: greedy_result.crossing_count,
                crossing_delta: 0,
            },
        };
    }

    // Run e-graph saturation
    let egraph_result = saturate_layer(initial, ctx, config);

    // Determine strategy and pick winner
    let (ordering, crossing_count, strategy) = if egraph_result.hit_limit {
        // E-graph exceeded budget - compare with greedy
        if egraph_result.crossing_count <= greedy_result.crossing_count {
            // E-graph still won despite budget exhaustion
            tracing::info!(
                egraph_crossings = egraph_result.crossing_count,
                greedy_crossings = greedy_result.crossing_count,
                budget_type = ?egraph_result.budget_exhausted.as_ref().map(|b| b.budget_type),
                "egraph.fallback.egraph_exceeded_but_won"
            );
            (
                egraph_result.ordering.clone(),
                egraph_result.crossing_count,
                FallbackStrategy::EGraphExceededButWon,
            )
        } else {
            // Greedy won
            tracing::info!(
                egraph_crossings = egraph_result.crossing_count,
                greedy_crossings = greedy_result.crossing_count,
                budget_type = ?egraph_result.budget_exhausted.as_ref().map(|b| b.budget_type),
                "egraph.fallback.greedy_won"
            );
            (
                greedy_result.ordering.clone(),
                greedy_result.crossing_count,
                FallbackStrategy::GreedyWon,
            )
        }
    } else {
        // E-graph completed within budget - use its result if better
        if egraph_result.crossing_count <= greedy_result.crossing_count {
            (
                egraph_result.ordering.clone(),
                egraph_result.crossing_count,
                FallbackStrategy::EGraphCompleted,
            )
        } else {
            // Rare: greedy beat e-graph even though e-graph completed
            // This can happen due to e-graph extraction heuristics
            tracing::debug!(
                egraph_crossings = egraph_result.crossing_count,
                greedy_crossings = greedy_result.crossing_count,
                "egraph.fallback.greedy_beat_completed_egraph"
            );
            (
                greedy_result.ordering.clone(),
                greedy_result.crossing_count,
                FallbackStrategy::GreedyWon,
            )
        }
    };

    let crossing_delta = greedy_result.crossing_count as isize - crossing_count as isize;

    let diagnostic = FallbackDiagnostic {
        budget_type: egraph_result
            .budget_exhausted
            .as_ref()
            .map(|b| b.budget_type),
        budget_value: egraph_result.budget_exhausted.as_ref().map(|b| b.value),
        budget_limit: egraph_result.budget_exhausted.as_ref().map(|b| b.limit),
        fallback_strategy: strategy,
        egraph_crossings: Some(egraph_result.crossing_count),
        greedy_crossings: greedy_result.crossing_count,
        crossing_delta,
    };

    // Emit structured diagnostic event for all fallback cases
    if egraph_result.hit_limit {
        tracing::warn!(
            budget_type = ?diagnostic.budget_type,
            budget_value = ?diagnostic.budget_value,
            budget_limit = ?diagnostic.budget_limit,
            fallback_strategy = ?diagnostic.fallback_strategy,
            egraph_crossings = ?diagnostic.egraph_crossings,
            greedy_crossings = diagnostic.greedy_crossings,
            crossing_delta = diagnostic.crossing_delta,
            "layout.degradation.egraph_budget_exceeded"
        );
    }

    FallbackResult {
        ordering,
        crossing_count,
        strategy,
        egraph_result: Some(egraph_result),
        greedy_result,
        diagnostic,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ordering_to_expr() {
        let ordering = LayerOrdering::new(vec![0, 1, 2, 3]);
        let expr = ordering_to_expr(&ordering);
        let recovered = expr_to_ordering(&expr).unwrap();
        assert_eq!(ordering, recovered);
    }

    #[test]
    fn roundtrip_ordering_empty() {
        let ordering = LayerOrdering::new(vec![]);
        let expr = ordering_to_expr(&ordering);
        let recovered = expr_to_ordering(&expr).unwrap();
        assert_eq!(ordering, recovered);
    }

    #[test]
    fn roundtrip_ordering_single() {
        let ordering = LayerOrdering::new(vec![42]);
        let expr = ordering_to_expr(&ordering);
        let recovered = expr_to_ordering(&expr).unwrap();
        assert_eq!(ordering, recovered);
    }

    #[test]
    fn swap_rules_generate_swaps() {
        let ordering = LayerOrdering::new(vec![0, 1]);
        let expr = ordering_to_expr(&ordering);
        let rules = adjacent_swap_rules();

        let runner = Runner::default()
            .with_expr(&expr)
            .with_iter_limit(5)
            .run(&rules);

        // Should have at least 2 orderings: [0,1] and [1,0]
        assert!(runner.egraph.total_size() >= 2);
    }

    #[test]
    fn saturation_finds_optimal_2_node() {
        // Upper: [0, 1], Lower: [2, 3]
        // Edges: 0->3, 1->2 (1 crossing in initial [0,1])
        // Optimal: [1, 0] gives 0 crossings
        let initial = LayerOrdering::new(vec![0, 1]);
        let lower = LayerOrdering::new(vec![2, 3]);
        let edges = LayerEdges {
            edges: vec![(0, 3), (1, 2)],
        };

        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower.clone()),
            lower_edges: Some(edges.clone()),
        };

        let result = saturate_layer(&initial, &ctx, &SaturationConfig::default());

        // Must find 0-crossing solution
        assert_eq!(
            result.crossing_count, 0,
            "Expected 0 crossings but got {}, ordering: {:?}",
            result.crossing_count, result.ordering.order
        );
        assert_eq!(result.ordering.order, vec![1, 0]);
    }

    #[test]
    fn saturation_3_node_layer() {
        // Upper: [0, 1, 2], Lower: [3, 4, 5]
        // Edges: 0->5, 1->4, 2->3 (3 crossings in initial order)
        // Optimal: [2, 1, 0] has 0 crossings
        let initial = LayerOrdering::new(vec![0, 1, 2]);
        let lower = LayerOrdering::new(vec![3, 4, 5]);
        let edges = LayerEdges {
            edges: vec![(0, 5), (1, 4), (2, 3)],
        };

        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower.clone()),
            lower_edges: Some(edges.clone()),
        };

        let result = saturate_layer(&initial, &ctx, &SaturationConfig::default());

        // Must find 0-crossing solution
        assert_eq!(
            result.crossing_count, 0,
            "Expected 0 crossings but got {}, ordering: {:?}",
            result.crossing_count, result.ordering.order
        );
        assert_eq!(result.ordering.order, vec![2, 1, 0]);
    }

    #[test]
    fn saturation_respects_node_limit() {
        let initial = LayerOrdering::new(vec![0, 1, 2, 3, 4]);
        let config = SaturationConfig {
            node_limit: 50,
            iter_limit: 100,
            time_limit_ms: 1000,
        };

        let result = saturate_layer(&initial, &CrossingContext::default(), &config);

        // Should stop before exploding
        assert!(result.egraph_nodes <= 100); // Some slack for final iteration
    }

    #[test]
    fn saturation_if_improves_returns_none_when_no_improvement() {
        // No edges = 0 crossings already
        let initial = LayerOrdering::new(vec![0, 1, 2]);
        let ctx = CrossingContext::default();

        let result = saturate_layer_if_improves(&initial, &ctx, &SaturationConfig::default());

        assert!(result.is_none());
    }

    #[test]
    fn collect_orderings_finds_all_permutations_2_node() {
        // For a 2-node layer, saturation should find both [0,1] and [1,0]
        let initial = LayerOrdering::new(vec![0, 1]);
        let expr = ordering_to_expr(&initial);
        let rules = adjacent_swap_rules();

        let runner = Runner::default()
            .with_expr(&expr)
            .with_iter_limit(10)
            .run(&rules);

        let orderings = collect_orderings(&runner.egraph, runner.roots[0], 100);

        // Should find exactly 2 orderings: [0,1] and [1,0]
        assert_eq!(orderings.len(), 2);
        let orders: std::collections::HashSet<_> =
            orderings.iter().map(|o| o.order.clone()).collect();
        assert!(orders.contains(&vec![0, 1]));
        assert!(orders.contains(&vec![1, 0]));
    }

    #[test]
    fn collect_orderings_finds_all_permutations_3_node() {
        // For a 3-node layer, saturation should find all 6 permutations
        let initial = LayerOrdering::new(vec![0, 1, 2]);
        let expr = ordering_to_expr(&initial);
        let rules = adjacent_swap_rules();

        let runner = Runner::default()
            .with_expr(&expr)
            .with_iter_limit(20)
            .run(&rules);

        let orderings = collect_orderings(&runner.egraph, runner.roots[0], 100);

        // Should find all 6 permutations: 3! = 6
        assert_eq!(
            orderings.len(),
            6,
            "Expected 6 permutations, got {}: {:?}",
            orderings.len(),
            orderings.iter().map(|o| &o.order).collect::<Vec<_>>()
        );
    }

    #[test]
    fn config_defaults_are_reasonable() {
        let config = SaturationConfig::default();
        assert_eq!(config.node_limit, 10_000);
        assert_eq!(config.iter_limit, 30);
        assert_eq!(config.time_limit_ms, 100);
    }

    #[test]
    fn config_for_graph_uses_formula() {
        // Small graph: 10 nodes, 5 layers
        let config = SaturationConfig::for_graph(10, 5);
        assert_eq!(config.node_limit, 50 * 10 * 10); // 5000
        assert_eq!(config.time_limit_ms, 10 * 5); // 50
        assert_eq!(config.iter_limit, 1000);
    }

    #[test]
    fn config_for_graph_clamps_to_max() {
        // Large graph: should hit caps
        let config = SaturationConfig::for_graph(100, 100);
        assert_eq!(config.node_limit, 100_000); // capped
        assert_eq!(config.time_limit_ms, 500); // capped
    }

    #[test]
    fn config_for_graph_handles_overflow() {
        // Very large node count - should not overflow
        let config = SaturationConfig::for_graph(100_000, 10);
        assert_eq!(config.node_limit, 100_000); // saturates and clamps
    }

    #[test]
    fn saturation_reports_node_limit_exhaustion() {
        // Force node limit with very small budget
        let initial = LayerOrdering::new(vec![0, 1, 2, 3, 4]);
        let config = SaturationConfig {
            node_limit: 10, // Very small - will be hit
            iter_limit: 1000,
            time_limit_ms: 10000,
        };

        let result = saturate_layer(&initial, &CrossingContext::default(), &config);

        assert!(result.hit_limit);
        assert!(result.budget_exhausted.is_some());
        let exhausted = result.budget_exhausted.unwrap();
        assert_eq!(exhausted.budget_type, BudgetType::NodeLimit);
        assert_eq!(exhausted.limit, 10);
    }

    #[test]
    fn saturation_reports_iteration_limit_exhaustion() {
        // Force iteration limit
        let initial = LayerOrdering::new(vec![0, 1, 2, 3]);
        let config = SaturationConfig {
            node_limit: 100_000,
            iter_limit: 1, // Very small - will be hit
            time_limit_ms: 10000,
        };

        let result = saturate_layer(&initial, &CrossingContext::default(), &config);

        assert!(result.hit_limit);
        assert!(result.budget_exhausted.is_some());
        let exhausted = result.budget_exhausted.unwrap();
        assert_eq!(exhausted.budget_type, BudgetType::IterationLimit);
        assert_eq!(exhausted.limit, 1);
    }

    #[test]
    fn config_interactive_is_conservative() {
        let config = SaturationConfig::interactive();
        assert!(config.node_limit <= 5_000);
        assert!(config.iter_limit <= 20);
        assert!(config.time_limit_ms <= 50);
    }

    #[test]
    fn config_batch_is_permissive() {
        let config = SaturationConfig::batch();
        assert!(config.node_limit >= 100_000);
        assert!(config.time_limit_ms >= 1000);
    }

    // -------------------------------------------------------------------------
    // Fallback mechanism tests (bd-1xma.4)
    // -------------------------------------------------------------------------

    #[test]
    fn fallback_uses_egraph_when_completed() {
        // Simple case: e-graph should complete and win
        let initial = LayerOrdering::new(vec![0, 1, 2]);
        let ctx = CrossingContext::default();
        let config = SaturationConfig::default();

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // E-graph should complete (no budget exceeded for small layer)
        assert!(
            matches!(
                result.strategy,
                FallbackStrategy::EGraphCompleted | FallbackStrategy::GreedyWon
            ),
            "Expected EGraphCompleted or GreedyWon, got {:?}",
            result.strategy
        );
        assert!(result.egraph_result.is_some());
    }

    #[test]
    fn fallback_greedy_wins_when_egraph_exceeds_budget() {
        // Force node limit, then verify fallback logic
        let initial = LayerOrdering::new(vec![0, 1, 2, 3, 4]);
        let lower = LayerOrdering::new(vec![5, 6, 7, 8, 9]);
        let edges = LayerEdges {
            edges: vec![(0, 9), (1, 8), (2, 7), (3, 6), (4, 5)],
        };
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig {
            node_limit: 5, // Very small - will be exceeded
            iter_limit: 1000,
            time_limit_ms: 10000,
        };

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // Should have fallback diagnostic
        assert!(result.diagnostic.budget_type.is_some());
        // Result should never be worse than greedy
        assert!(result.crossing_count <= result.greedy_result.crossing_count);
    }

    #[test]
    fn fallback_result_never_worse_than_greedy() {
        // Property: final result is always <= greedy result
        for size in 2..=5 {
            let initial = LayerOrdering::identity(size);
            let ctx = CrossingContext::default();
            let config = SaturationConfig::default();

            let result = saturate_with_fallback(&initial, &ctx, &config);

            assert!(
                result.crossing_count <= result.greedy_result.crossing_count,
                "Fallback result ({}) worse than greedy ({}) for size {}",
                result.crossing_count,
                result.greedy_result.crossing_count,
                size
            );
        }
    }

    #[test]
    fn fallback_diagnostic_has_crossing_delta() {
        let initial = LayerOrdering::new(vec![0, 1, 2]);
        let ctx = CrossingContext::default();
        let config = SaturationConfig::default();

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // Delta should be greedy - final (positive means improvement over greedy)
        let expected_delta =
            result.greedy_result.crossing_count as isize - result.crossing_count as isize;
        assert_eq!(result.diagnostic.crossing_delta, expected_delta);
    }

    #[test]
    fn fallback_skips_egraph_for_large_layers() {
        // Create a layer larger than should_use_egraph threshold (100)
        let initial = LayerOrdering::identity(150);
        let ctx = CrossingContext::default();
        let config = SaturationConfig::default();

        let result = saturate_with_fallback(&initial, &ctx, &config);

        assert_eq!(result.strategy, FallbackStrategy::GreedyOnly);
        assert!(result.egraph_result.is_none());
    }

    #[test]
    fn fallback_emits_strategy_info() {
        let initial = LayerOrdering::new(vec![0, 1, 2]);
        let ctx = CrossingContext::default();
        let config = SaturationConfig::default();

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // Should have a valid strategy
        assert!(matches!(
            result.strategy,
            FallbackStrategy::EGraphCompleted
                | FallbackStrategy::EGraphExceededButWon
                | FallbackStrategy::GreedyWon
                | FallbackStrategy::GreedyOnly
        ));

        // Diagnostic strategy should match result strategy
        assert_eq!(result.diagnostic.fallback_strategy, result.strategy);
    }

    // -------------------------------------------------------------------------
    // Fault injection tests (bd-1s1g.1)
    // -------------------------------------------------------------------------

    /// Generate a complete bipartite graph K_{n,m} for stress testing.
    fn generate_bipartite(n: usize, m: usize) -> (LayerOrdering, LayerOrdering, LayerEdges) {
        let upper = LayerOrdering::new((0..n).collect());
        let lower = LayerOrdering::new((n..n + m).collect());
        let mut edges = Vec::with_capacity(n * m);
        for &src in &upper.order {
            for &tgt in &lower.order {
                edges.push((src, tgt));
            }
        }
        (upper, lower, LayerEdges { edges })
    }

    #[test]
    fn fault_node_budget_hit_on_dense_graph() {
        // Fault scenario 1: K_{15,15} with tiny node budget should exhaust budget
        let (upper, lower, edges) = generate_bipartite(15, 15);
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig {
            node_limit: 20, // Very small - will definitely be hit
            iter_limit: 1000,
            time_limit_ms: 10000,
        };

        let result = saturate_with_fallback(&upper, &ctx, &config);

        // Should not panic
        assert!(!result.ordering.order.is_empty());

        // Should produce valid layout (result never worse than greedy)
        assert!(result.crossing_count <= result.greedy_result.crossing_count);

        // If e-graph was attempted, budget should be recorded
        if let Some(ref egraph) = result.egraph_result
            && egraph.hit_limit
        {
            assert!(result.diagnostic.budget_type.is_some());
        }
    }

    #[test]
    fn fault_iteration_limit_hit() {
        // Fault scenario 4: Force iteration limit
        let initial = LayerOrdering::new(vec![0, 1, 2, 3, 4, 5]);
        let lower = LayerOrdering::new(vec![6, 7, 8, 9, 10, 11]);
        let edges = LayerEdges {
            edges: vec![(0, 11), (1, 10), (2, 9), (3, 8), (4, 7), (5, 6)],
        };
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig {
            node_limit: 100_000,
            iter_limit: 1, // Very small - will be hit
            time_limit_ms: 10000,
        };

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // Should not panic and produce valid result
        assert!(!result.ordering.order.is_empty());
        assert!(result.crossing_count <= result.greedy_result.crossing_count);
    }

    #[test]
    fn fault_zero_node_layer() {
        // Fault scenario 5: Empty layer
        let initial = LayerOrdering::new(vec![]);
        let ctx = CrossingContext::default();
        let config = SaturationConfig::default();

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // Should not panic on empty input
        assert!(result.ordering.order.is_empty());
        assert_eq!(result.crossing_count, 0);
    }

    #[test]
    fn fault_single_node_layer() {
        // Fault scenario 6: Single node layer
        let initial = LayerOrdering::new(vec![42]);
        let lower = LayerOrdering::new(vec![99]);
        let edges = LayerEdges {
            edges: vec![(42, 99)],
        };
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig::default();

        let result = saturate_with_fallback(&initial, &ctx, &config);

        // Should not panic and produce trivial result
        assert_eq!(result.ordering.order, vec![42]);
        assert_eq!(result.crossing_count, 0);
    }

    #[test]
    fn fault_deterministic_output_across_runs() {
        // Verify determinism: same input should produce identical output
        let (upper, lower, edges) = generate_bipartite(8, 8);
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig::default();

        let results: Vec<_> = (0..10)
            .map(|_| saturate_with_fallback(&upper, &ctx, &config))
            .collect();

        // All results should be identical
        let first = &results[0];
        for (i, result) in results.iter().enumerate().skip(1) {
            assert_eq!(
                first.ordering.order, result.ordering.order,
                "Run {i} produced different ordering"
            );
            assert_eq!(
                first.crossing_count, result.crossing_count,
                "Run {i} produced different crossing count"
            );
            assert_eq!(
                first.strategy, result.strategy,
                "Run {i} produced different strategy"
            );
        }
    }

    #[test]
    fn fault_structured_diagnostic_complete() {
        // Verify diagnostic has all required fields when budget exceeded
        let (upper, lower, edges) = generate_bipartite(10, 10);
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig {
            node_limit: 10, // Force budget hit
            iter_limit: 1000,
            time_limit_ms: 10000,
        };

        let result = saturate_with_fallback(&upper, &ctx, &config);

        // Diagnostic should be populated
        let diag = &result.diagnostic;
        assert_eq!(diag.greedy_crossings, result.greedy_result.crossing_count);

        // If e-graph hit budget, diagnostic should record it
        if let Some(ref egraph) = result.egraph_result
            && egraph.hit_limit
        {
            assert!(
                diag.budget_type.is_some(),
                "Budget type should be recorded when limit hit"
            );
            assert!(
                diag.budget_value.is_some(),
                "Budget value should be recorded"
            );
            assert!(
                diag.budget_limit.is_some(),
                "Budget limit should be recorded"
            );
        }
    }

    #[test]
    fn fault_fallback_within_time_budget() {
        // Verify that even worst-case graphs complete within reasonable time
        use std::time::Instant;

        let (upper, lower, edges) = generate_bipartite(20, 20);
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(lower),
            lower_edges: Some(edges),
        };
        let config = SaturationConfig {
            node_limit: 50,
            iter_limit: 10,
            time_limit_ms: 100,
        };

        let start = Instant::now();
        let result = saturate_with_fallback(&upper, &ctx, &config);
        let elapsed = start.elapsed();

        // Should complete within 500ms even on slow hardware
        assert!(
            elapsed.as_millis() < 500,
            "Fallback took too long: {:?}",
            elapsed
        );

        // Result should still be valid
        assert!(result.crossing_count <= result.greedy_result.crossing_count);
    }
}
