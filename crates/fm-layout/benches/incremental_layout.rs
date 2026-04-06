//! Criterion benchmarks comparing incremental vs full layout recompute.
//!
//! bd-20fq.5: Quantify incremental subgraph re-layout speedup.

#![allow(unused_must_use)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fm_core::{
    ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrGraphEdge, IrGraphNode, IrLabel,
    IrLabelId, IrNode, IrNodeId, IrSubgraph, IrSubgraphId, MermaidDiagramIr, Span,
};
use fm_layout::{
    IncrementalLayoutEngine, LayoutAlgorithm, LayoutGuardrails,
    layout_diagram_traced_with_config_and_guardrails,
};

/// Build a labeled graph with two subgraphs, suitable for incremental benchmarking.
fn bench_ir(nodes_per_subgraph: usize) -> MermaidDiagramIr {
    let total = nodes_per_subgraph * 2;
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    ir.direction = GraphDirection::TB;

    for i in 0..total {
        ir.labels.push(IrLabel {
            text: format!("Node {i}"),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: Some(IrLabelId(i)),
            ..IrNode::default()
        });
    }

    // Chain within each subgraph.
    for i in 0..nodes_per_subgraph.saturating_sub(1) {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(i)),
            to: IrEndpoint::Node(IrNodeId(i + 1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
    }
    for i in nodes_per_subgraph..total.saturating_sub(1) {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(i)),
            to: IrEndpoint::Node(IrNodeId(i + 1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
    }

    // Build graph representation.
    ir.graph.nodes = (0..total)
        .map(|node_index| IrGraphNode {
            node_id: IrNodeId(node_index),
            clusters: Vec::new(),
            subgraphs: Vec::new(),
            ..IrGraphNode::default()
        })
        .collect();
    ir.graph.edges = ir
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| IrGraphEdge {
            edge_id: edge_index,
            from: edge.from,
            to: edge.to,
            span: edge.span,
            ..IrGraphEdge::default()
        })
        .collect();

    // Two explicit subgraphs.
    ir.graph.subgraphs.push(IrSubgraph {
        id: IrSubgraphId(0),
        key: "left".to_string(),
        title: None,
        parent: None,
        children: Vec::new(),
        members: (0..nodes_per_subgraph).map(IrNodeId).collect(),
        cluster: None,
        grid_span: 1,
        span: Span::at_line(1, 1),
        direction: None,
    });
    ir.graph.subgraphs.push(IrSubgraph {
        id: IrSubgraphId(1),
        key: "right".to_string(),
        title: None,
        parent: None,
        children: Vec::new(),
        members: (nodes_per_subgraph..total).map(IrNodeId).collect(),
        cluster: None,
        grid_span: 1,
        span: Span::at_line(2, 1),
        direction: None,
    });
    for node_index in 0..nodes_per_subgraph {
        ir.graph.nodes[node_index].subgraphs.push(IrSubgraphId(0));
    }
    for node_index in nodes_per_subgraph..total {
        ir.graph.nodes[node_index].subgraphs.push(IrSubgraphId(1));
    }

    ir
}

fn bench_single_node_label_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_node_label_edit");

    for nodes_per_subgraph in [50, 100, 250, 500] {
        let total = nodes_per_subgraph * 2;

        // Benchmark incremental path.
        group.bench_with_input(
            BenchmarkId::new("incremental", total),
            &nodes_per_subgraph,
            |b, &n| {
                let mut engine = IncrementalLayoutEngine::default();
                let mut ir = bench_ir(n);
                let config = fm_layout::LayoutConfig::default();
                let guardrails = LayoutGuardrails::default();

                // Warm cache.
                engine.layout_diagram_traced_with_config_and_guardrails(
                    &ir,
                    LayoutAlgorithm::Auto,
                    config.clone(),
                    guardrails,
                );

                let mut variant = 0_u32;
                b.iter(|| {
                    let label_index = ir.nodes[5].label.unwrap().0;
                    ir.labels[label_index].text = format!("Edited v{variant}");
                    variant = variant.wrapping_add(1);
                    engine.layout_diagram_traced_with_config_and_guardrails(
                        &ir,
                        LayoutAlgorithm::Auto,
                        config.clone(),
                        guardrails,
                    )
                });
            },
        );

        // Benchmark full recompute path.
        group.bench_with_input(
            BenchmarkId::new("full_recompute", total),
            &nodes_per_subgraph,
            |b, &n| {
                let mut ir = bench_ir(n);
                let config = fm_layout::LayoutConfig::default();
                let guardrails = LayoutGuardrails::default();

                let mut variant = 0_u32;
                b.iter(|| {
                    let label_index = ir.nodes[5].label.unwrap().0;
                    ir.labels[label_index].text = format!("Edited v{variant}");
                    variant = variant.wrapping_add(1);
                    layout_diagram_traced_with_config_and_guardrails(
                        &ir,
                        LayoutAlgorithm::Auto,
                        config.clone(),
                        guardrails,
                    )
                });
            },
        );
    }

    group.finish();
}

fn bench_five_node_cluster_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("five_node_cluster_edit");

    for nodes_per_subgraph in [50, 100, 250, 500] {
        let total = nodes_per_subgraph * 2;

        group.bench_with_input(
            BenchmarkId::new("incremental", total),
            &nodes_per_subgraph,
            |b, &n| {
                let mut engine = IncrementalLayoutEngine::default();
                let mut ir = bench_ir(n);
                let config = fm_layout::LayoutConfig::default();
                let guardrails = LayoutGuardrails::default();

                engine.layout_diagram_traced_with_config_and_guardrails(
                    &ir,
                    LayoutAlgorithm::Auto,
                    config.clone(),
                    guardrails,
                );

                let mut variant = 0_u32;
                b.iter(|| {
                    for node_index in 0..5 {
                        let label_index = ir.nodes[node_index].label.unwrap().0;
                        ir.labels[label_index].text =
                            format!("Cluster edit {node_index} v{variant}");
                    }
                    variant = variant.wrapping_add(1);
                    engine.layout_diagram_traced_with_config_and_guardrails(
                        &ir,
                        LayoutAlgorithm::Auto,
                        config.clone(),
                        guardrails,
                    )
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("full_recompute", total),
            &nodes_per_subgraph,
            |b, &n| {
                let mut ir = bench_ir(n);
                let config = fm_layout::LayoutConfig::default();
                let guardrails = LayoutGuardrails::default();

                let mut variant = 0_u32;
                b.iter(|| {
                    for node_index in 0..5 {
                        let label_index = ir.nodes[node_index].label.unwrap().0;
                        ir.labels[label_index].text =
                            format!("Cluster edit {node_index} v{variant}");
                    }
                    variant = variant.wrapping_add(1);
                    layout_diagram_traced_with_config_and_guardrails(
                        &ir,
                        LayoutAlgorithm::Auto,
                        config.clone(),
                        guardrails,
                    )
                });
            },
        );
    }

    group.finish();
}

fn bench_bypass_small_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_graph_bypass");

    // Small graphs (< 50 nodes) should bypass incremental and use full recompute.
    // Verify no regression from the bypass check overhead.
    for node_count in [10, 20, 40] {
        group.bench_with_input(
            BenchmarkId::new("engine", node_count),
            &node_count,
            |b, &n| {
                let mut engine = IncrementalLayoutEngine::default();
                let half = n / 2;
                let mut ir = bench_ir(half);
                let config = fm_layout::LayoutConfig::default();
                let guardrails = LayoutGuardrails::default();

                engine.layout_diagram_traced_with_config_and_guardrails(
                    &ir,
                    LayoutAlgorithm::Auto,
                    config.clone(),
                    guardrails,
                );

                let mut variant = 0_u32;
                b.iter(|| {
                    if !ir.nodes.is_empty() {
                        let label_index = ir.nodes[0].label.unwrap().0;
                        ir.labels[label_index].text = format!("Small v{variant}");
                    }
                    variant = variant.wrapping_add(1);
                    engine.layout_diagram_traced_with_config_and_guardrails(
                        &ir,
                        LayoutAlgorithm::Auto,
                        config.clone(),
                        guardrails,
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_single_node_label_edit,
    bench_five_node_cluster_edit,
    bench_bypass_small_graph,
);
criterion_main!(benches);
