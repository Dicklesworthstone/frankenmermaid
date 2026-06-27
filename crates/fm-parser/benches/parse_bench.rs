//! Standalone parse-stage benchmarks for `fm-parser`.
//!
//! Run with: `cargo bench -p fm-parser`
//!
//! Unlike the full-pipeline bench in `fm-cli` (which pulls in `fm-layout` →
//! `highs-sys` → `cmake`), this crate has no `cmake`-dependent build step, so it
//! builds and benches reliably on every remote worker — making the parse stage
//! (≈21% of the wide pipeline) independently measurable without the `highs-sys`
//! cmake toolchain hazard that intermittently blocks `pipeline_bench`.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn gen_flowchart(node_count: usize) -> String {
    let mut lines = vec![String::from("flowchart LR")];
    for i in 0..node_count {
        lines.push(format!("  N{i}[Node {i}]"));
    }
    for i in 0..node_count.saturating_sub(1) {
        lines.push(format!("  N{i}-->N{}", i + 1));
    }
    if node_count > 4 {
        lines.push(format!("  N0-->N{}", node_count / 2));
        lines.push(format!("  N{}-->N{}", node_count / 3, node_count - 1));
    }
    lines.join("\n")
}

/// A *wide* layered DAG: `layers` ranks of `width` nodes, each fanning out to two
/// nodes in the next rank — the edge-heavy shape that dominates the wide pipeline.
fn gen_wide(layers: usize, width: usize) -> String {
    let mut lines = vec![String::from("flowchart TD")];
    for layer in 0..layers {
        for w in 0..width {
            lines.push(format!("  N{layer}_{w}[L{layer} W{w}]"));
        }
    }
    for layer in 0..layers.saturating_sub(1) {
        for w in 0..width {
            lines.push(format!("  N{layer}_{w}-->N{}_{w}", layer + 1));
            lines.push(format!(
                "  N{layer}_{w}-->N{}_{}",
                layer + 1,
                (w + 1) % width
            ));
        }
    }
    lines.join("\n")
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");

    for (label, input) in [
        ("small_10", gen_flowchart(10)),
        ("medium_100", gen_flowchart(100)),
        ("large_1000", gen_flowchart(1000)),
    ] {
        group.bench_with_input(BenchmarkId::new("flowchart", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, layers, width) in [
        ("8x16", 8_usize, 16_usize),
        ("12x24", 12, 24),
        ("16x32", 16, 32),
    ] {
        let input = gen_wide(layers, width);
        group.bench_with_input(BenchmarkId::new("wide", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
