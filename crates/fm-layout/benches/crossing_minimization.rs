//! Criterion benchmarks comparing E-graph vs greedy crossing minimization.
//!
//! bd-1xma.5: Benchmark E-graph equality saturation against greedy heuristics.
//!
//! This benchmark suite evaluates:
//! - Crossing count quality (C_e vs C_g)
//! - Runtime performance (T_e vs T_g)
//! - Improvement ratio: (C_g - C_e) / C_g * 100%
//! - Quality-adjusted throughput: crossings_reduced / ms

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fm_layout::egraph_crossing::{
    CrossingContext, FallbackStrategy, SaturationConfig, saturate_with_fallback,
};
use fm_layout::egraph_ordering::{
    LayerEdges, LayerOrdering, local_crossing_count, optimize_layer_ordering,
};

// ============================================================================
// Test Graph Generation
// ============================================================================

/// Generate a random layered graph with specified parameters.
///
/// Returns (layer orderings, inter-layer edges).
fn generate_layered_graph(
    layer_count: usize,
    nodes_per_layer: usize,
    edge_density: f64,
    seed: u64,
) -> (Vec<LayerOrdering>, Vec<LayerEdges>) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Simple deterministic PRNG from seed
    let mut state = seed;
    let mut next_rand = || {
        let mut hasher = DefaultHasher::new();
        state.hash(&mut hasher);
        state = hasher.finish();
        (state as f64) / (u64::MAX as f64)
    };

    // Create layer orderings
    let mut orderings: Vec<LayerOrdering> = (0..layer_count)
        .map(|layer| {
            let base = layer * nodes_per_layer;
            LayerOrdering::new((base..base + nodes_per_layer).collect())
        })
        .collect();

    // Create edges between adjacent layers based on density
    let mut all_edges = Vec::with_capacity(layer_count - 1);
    for layer in 0..layer_count.saturating_sub(1) {
        let upper = &orderings[layer];
        let lower = &orderings[layer + 1];
        let mut edges = Vec::new();

        // Generate edges based on density
        for &src in &upper.order {
            for &tgt in &lower.order {
                if next_rand() < edge_density {
                    edges.push((src, tgt));
                }
            }
        }

        // Ensure at least one edge per layer for connectivity
        if edges.is_empty() && !upper.order.is_empty() && !lower.order.is_empty() {
            edges.push((upper.order[0], lower.order[0]));
        }

        all_edges.push(LayerEdges { edges });
    }

    // Shuffle initial orderings to create non-trivial input
    for ordering in &mut orderings {
        for i in (1..ordering.order.len()).rev() {
            let j = (next_rand() * (i + 1) as f64) as usize;
            ordering.order.swap(i, j);
        }
    }

    (orderings, all_edges)
}

/// Generate a complete bipartite graph K_{n,m}.
fn generate_bipartite(n: usize, m: usize) -> (Vec<LayerOrdering>, Vec<LayerEdges>) {
    let upper = LayerOrdering::new((0..n).collect());
    let lower = LayerOrdering::new((n..n + m).collect());

    // All pairs from upper to lower
    let mut edges = Vec::with_capacity(n * m);
    for &src in &upper.order {
        for &tgt in &lower.order {
            edges.push((src, tgt));
        }
    }

    (vec![upper, lower], vec![LayerEdges { edges }])
}

// ============================================================================
// Benchmark Metrics
// ============================================================================

/// Metrics from a crossing minimization run.
#[derive(Debug, Clone)]
struct CrossingMetrics {
    /// Final crossing count.
    crossing_count: usize,
    /// Time in microseconds.
    time_us: u64,
    /// Strategy used.
    strategy: &'static str,
}

/// Comparison between E-graph and greedy.
#[derive(Debug)]
struct Comparison {
    egraph: CrossingMetrics,
    greedy: CrossingMetrics,
    /// Improvement ratio: (greedy - egraph) / greedy * 100
    improvement_pct: f64,
    /// Speedup: greedy_time / egraph_time (> 1 means egraph faster)
    time_ratio: f64,
}

/// Run both strategies on a single layer and compare.
fn compare_strategies(
    ordering: &LayerOrdering,
    ctx: &CrossingContext,
    config: &SaturationConfig,
) -> Comparison {
    use std::time::Instant;

    // Run greedy
    let greedy_start = Instant::now();
    let greedy_result = optimize_layer_ordering(
        ordering,
        ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
        ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
    );
    let greedy_time = greedy_start.elapsed();

    // Run E-graph with fallback
    let egraph_start = Instant::now();
    let egraph_result = saturate_with_fallback(ordering, ctx, config);
    let egraph_time = egraph_start.elapsed();

    let egraph_metrics = CrossingMetrics {
        crossing_count: egraph_result.crossing_count,
        time_us: egraph_time.as_micros() as u64,
        strategy: match egraph_result.strategy {
            FallbackStrategy::EGraphCompleted => "egraph_completed",
            FallbackStrategy::EGraphExceededButWon => "egraph_exceeded_won",
            FallbackStrategy::GreedyWon => "greedy_won",
            FallbackStrategy::GreedyOnly => "greedy_only",
        },
    };

    let greedy_metrics = CrossingMetrics {
        crossing_count: greedy_result.crossing_count,
        time_us: greedy_time.as_micros() as u64,
        strategy: "greedy",
    };

    let improvement_pct = if greedy_result.crossing_count > 0 {
        (greedy_result.crossing_count as f64 - egraph_result.crossing_count as f64)
            / greedy_result.crossing_count as f64
            * 100.0
    } else {
        0.0
    };

    let time_ratio = if egraph_time.as_nanos() > 0 {
        greedy_time.as_nanos() as f64 / egraph_time.as_nanos() as f64
    } else {
        0.0
    };

    Comparison {
        egraph: egraph_metrics,
        greedy: greedy_metrics,
        improvement_pct,
        time_ratio,
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn benchmark_sparse_dag(c: &mut Criterion) {
    let mut group = c.benchmark_group("crossing_min/sparse_dag");

    for nodes_per_layer in [10, 20, 50] {
        let (orderings, edges) = generate_layered_graph(
            5, // 5 layers
            nodes_per_layer,
            0.1,   // 10% edge density
            12345, // seed
        );

        // Benchmark middle layer optimization
        let layer_idx = 2;
        let initial = &orderings[layer_idx];
        let ctx = CrossingContext {
            upper_ordering: Some(orderings[layer_idx - 1].clone()),
            upper_edges: Some(edges[layer_idx - 1].clone()),
            lower_ordering: Some(orderings[layer_idx + 1].clone()),
            lower_edges: Some(edges[layer_idx].clone()),
        };
        let config = SaturationConfig::default();

        group.bench_with_input(
            BenchmarkId::new("egraph", nodes_per_layer),
            &nodes_per_layer,
            |b, _| {
                b.iter(|| saturate_with_fallback(initial, &ctx, &config));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("greedy", nodes_per_layer),
            &nodes_per_layer,
            |b, _| {
                b.iter(|| {
                    optimize_layer_ordering(
                        initial,
                        ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
                        ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
                    )
                });
            },
        );
    }

    group.finish();
}

fn benchmark_dense_dag(c: &mut Criterion) {
    let mut group = c.benchmark_group("crossing_min/dense_dag");

    for nodes_per_layer in [5, 10, 15, 20] {
        let (orderings, edges) = generate_layered_graph(
            3, // 3 layers
            nodes_per_layer,
            0.4,   // 40% edge density (dense)
            54321, // seed
        );

        let layer_idx = 1;
        let initial = &orderings[layer_idx];
        let ctx = CrossingContext {
            upper_ordering: Some(orderings[0].clone()),
            upper_edges: Some(edges[0].clone()),
            lower_ordering: Some(orderings[2].clone()),
            lower_edges: Some(edges[1].clone()),
        };
        let config = SaturationConfig::default();

        group.bench_with_input(
            BenchmarkId::new("egraph", nodes_per_layer),
            &nodes_per_layer,
            |b, _| {
                b.iter(|| saturate_with_fallback(initial, &ctx, &config));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("greedy", nodes_per_layer),
            &nodes_per_layer,
            |b, _| {
                b.iter(|| {
                    optimize_layer_ordering(
                        initial,
                        ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
                        ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
                    )
                });
            },
        );
    }

    group.finish();
}

fn benchmark_bipartite(c: &mut Criterion) {
    let mut group = c.benchmark_group("crossing_min/bipartite");

    for (n, m) in [(5, 5), (10, 10), (5, 15)] {
        let (orderings, edges) = generate_bipartite(n, m);
        let initial = &orderings[0];
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(orderings[1].clone()),
            lower_edges: Some(edges[0].clone()),
        };
        let config = SaturationConfig::default();

        let label = format!("K{n}_{m}");

        group.bench_with_input(BenchmarkId::new("egraph", &label), &label, |b, _| {
            b.iter(|| saturate_with_fallback(initial, &ctx, &config));
        });

        group.bench_with_input(BenchmarkId::new("greedy", &label), &label, |b, _| {
            b.iter(|| {
                optimize_layer_ordering(
                    initial,
                    ctx.upper_ordering.as_ref().zip(ctx.upper_edges.as_ref()),
                    ctx.lower_ordering.as_ref().zip(ctx.lower_edges.as_ref()),
                )
            });
        });
    }

    group.finish();
}

/// Summary benchmark that prints comparison statistics.
fn benchmark_quality_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("crossing_min/quality_summary");

    // Run comparison across different graph types
    let test_cases = vec![
        ("sparse_10", generate_layered_graph(5, 10, 0.1, 111)),
        ("sparse_20", generate_layered_graph(5, 20, 0.1, 222)),
        ("dense_10", generate_layered_graph(3, 10, 0.4, 333)),
        ("dense_15", generate_layered_graph(3, 15, 0.4, 444)),
        ("bipartite_5_5", generate_bipartite(5, 5)),
        ("bipartite_10_10", generate_bipartite(10, 10)),
    ];

    for (name, (orderings, edges)) in test_cases {
        // Optimize first layer against second
        let layer_idx = 0;
        let initial = &orderings[layer_idx];
        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: orderings.get(1).cloned(),
            lower_edges: edges.first().cloned(),
        };
        let config = SaturationConfig::default();

        group.bench_function(BenchmarkId::new("compare", name), |b| {
            b.iter(|| {
                let comparison = compare_strategies(initial, &ctx, &config);
                // Return comparison to prevent optimization
                comparison.improvement_pct
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_sparse_dag,
    benchmark_dense_dag,
    benchmark_bipartite,
    benchmark_quality_comparison,
);
criterion_main!(benches);

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_layered_graph_produces_valid_structure() {
        let (orderings, edges) = generate_layered_graph(3, 5, 0.5, 42);

        assert_eq!(orderings.len(), 3);
        assert_eq!(edges.len(), 2);

        for o in &orderings {
            assert_eq!(o.len(), 5);
        }

        for e in &edges {
            assert!(!e.edges.is_empty(), "Each layer should have edges");
        }
    }

    #[test]
    fn generate_bipartite_produces_complete_graph() {
        let (orderings, edges) = generate_bipartite(3, 4);

        assert_eq!(orderings.len(), 2);
        assert_eq!(orderings[0].len(), 3);
        assert_eq!(orderings[1].len(), 4);
        assert_eq!(edges[0].edges.len(), 3 * 4); // Complete bipartite
    }

    #[test]
    fn compare_strategies_returns_valid_comparison() {
        let (orderings, edges) = generate_layered_graph(3, 5, 0.3, 99);

        let ctx = CrossingContext {
            upper_ordering: None,
            upper_edges: None,
            lower_ordering: Some(orderings[1].clone()),
            lower_edges: Some(edges[0].clone()),
        };
        let config = SaturationConfig::default();

        let comparison = compare_strategies(&orderings[0], &ctx, &config);

        // E-graph result should never be worse than greedy
        assert!(
            comparison.egraph.crossing_count <= comparison.greedy.crossing_count,
            "E-graph ({}) worse than greedy ({})",
            comparison.egraph.crossing_count,
            comparison.greedy.crossing_count
        );

        // Improvement should be non-negative
        assert!(
            comparison.improvement_pct >= 0.0,
            "Negative improvement: {}%",
            comparison.improvement_pct
        );
    }

    #[test]
    fn deterministic_results_with_same_seed() {
        let (orderings1, edges1) = generate_layered_graph(3, 10, 0.2, 12345);
        let (orderings2, edges2) = generate_layered_graph(3, 10, 0.2, 12345);

        assert_eq!(orderings1[0].order, orderings2[0].order);
        assert_eq!(edges1[0].edges, edges2[0].edges);
    }
}
