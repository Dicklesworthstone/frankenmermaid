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
use egg::{define_language, rewrite, EGraph, Id, RecExpr, Runner};

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
#[derive(Clone)]
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

impl Default for CrossingContext {
    fn default() -> Self {
        Self {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: None,
            lower_edges: None,
        }
    }
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
                        if let OrderingLang::Symbol(sym) = node_node {
                            if let Some(node_idx) = parse_node_symbol(sym) {
                                let mut new_partial = partial.clone();
                                new_partial.push(node_idx);
                                stack.push((*rest_id, new_partial));
                            }
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
// Saturation Entry Point
// ============================================================================

/// Configuration for equality saturation.
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
    let hit_limit = runner.stop_reason.as_ref().map_or(false, |r| {
        matches!(
            r,
            egg::StopReason::NodeLimit(_)
                | egg::StopReason::IterationLimit(_)
                | egg::StopReason::TimeLimit(_)
        )
    });

    // Extract best ordering
    let root = runner.roots[0];
    let (ordering, crossing_count) =
        extract_best_ordering(&runner.egraph, root, ctx).unwrap_or_else(|| {
            // Fallback to initial ordering if extraction fails
            let crossings = crate::egraph_ordering::local_crossing_count(
                initial,
                ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
                ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
            );
            (initial.clone(), crossings)
        });

    SaturationResult {
        ordering,
        crossing_count,
        egraph_nodes,
        iterations,
        hit_limit,
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
        let orders: std::collections::HashSet<_> = orderings.iter().map(|o| o.order.clone()).collect();
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
        assert_eq!(orderings.len(), 6, "Expected 6 permutations, got {}: {:?}",
            orderings.len(), orderings.iter().map(|o| &o.order).collect::<Vec<_>>());
    }

    #[test]
    fn config_defaults_are_reasonable() {
        let config = SaturationConfig::default();
        assert_eq!(config.node_limit, 10_000);
        assert_eq!(config.iter_limit, 30);
        assert_eq!(config.time_limit_ms, 100);
    }
}
