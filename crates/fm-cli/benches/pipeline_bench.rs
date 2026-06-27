//! Performance benchmarks for the `FrankenMermaid` pipeline (bd-2xl.4).
//!
//! Run with: `cargo bench -p fm-cli`
//!
//! Budget targets:
//! - Parse: <1ms small, <10ms medium, <100ms large
//! - Layout: <5ms small, <50ms medium, <500ms large
//! - Render SVG: <2ms small, <20ms medium, <200ms large
//! - Full pipeline: <10ms typical, <1s largest

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn gen_flowchart(node_count: usize) -> String {
    let mut lines = vec![String::from("flowchart LR")];
    for i in 0..node_count {
        lines.push(format!("  N{i}[Node {i}]"));
    }
    for i in 0..node_count.saturating_sub(1) {
        lines.push(format!("  N{i}-->N{}", i + 1));
    }
    // Add some cross-edges for non-trivial layout
    if node_count > 4 {
        lines.push(format!("  N0-->N{}", node_count / 2));
        lines.push(format!("  N{}-->N{}", node_count / 3, node_count - 1));
    }
    lines.join("\n")
}

fn gen_sequence(participant_count: usize) -> String {
    let mut lines = vec![String::from("sequenceDiagram")];
    for i in 0..participant_count {
        lines.push(format!("  participant P{i}"));
    }
    for i in 0..participant_count.saturating_sub(1) {
        lines.push(format!("  P{i}->>P{}: message {i}", i + 1));
    }
    lines.join("\n")
}

fn gen_cyclic(node_count: usize) -> String {
    let mut lines = vec![String::from("flowchart TD")];
    for i in 0..node_count {
        lines.push(format!("  N{i}-->N{}", (i + 1) % node_count));
    }
    // Extra cross-edges
    for i in (0..node_count).step_by(3) {
        lines.push(format!("  N{i}-->N{}", (i + 2) % node_count));
    }
    lines.join("\n")
}

/// Generate a *wide* layered DAG: `layers` ranks of `width` nodes each, with each
/// node fanning out to two nodes in the next layer. This produces ranks with many
/// nodes — the realistic shape for fan-out pipelines, ER/state diagrams, and org
/// charts — which exercises the crossing-minimization barycenter sweep far more than
/// a linear chain (where every rank holds a single node).
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

// ─── Parse benchmarks ───────────────────────────────────────────────────────

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

    let seq = gen_sequence(20);
    group.bench_with_input(
        BenchmarkId::new("sequence", "20_participants"),
        &seq,
        |b, input| {
            b.iter(|| fm_parser::parse(input));
        },
    );

    let pie = "pie\n  \"A\" : 30\n  \"B\" : 50\n  \"C\" : 20";
    group.bench_with_input(BenchmarkId::new("pie", "3_slices"), &pie, |b, input| {
        b.iter(|| fm_parser::parse(input));
    });

    group.finish();
}

// ─── Layout benchmarks ──────────────────────────────────────────────────────

fn bench_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout");

    for (label, input) in [
        ("small_10", gen_flowchart(10)),
        ("medium_100", gen_flowchart(100)),
        ("large_500", gen_flowchart(500)),
    ] {
        let parsed = fm_parser::parse(&input);
        group.bench_with_input(BenchmarkId::new("flowchart", label), &parsed.ir, |b, ir| {
            b.iter(|| fm_layout::layout_diagram(ir));
        });
    }

    for (label, input) in [
        ("cyclic_10", gen_cyclic(10)),
        ("cyclic_50", gen_cyclic(50)),
        ("cyclic_200", gen_cyclic(200)),
    ] {
        let parsed = fm_parser::parse(&input);
        group.bench_with_input(BenchmarkId::new("cyclic", label), &parsed.ir, |b, ir| {
            b.iter(|| fm_layout::layout_diagram(ir));
        });
    }

    group.finish();
}

// ─── Wide layered layout benchmarks ─────────────────────────────────────────
// These isolate the crossing-minimization barycenter sweep on graphs whose ranks
// contain many nodes (the cost driver that linear-chain benches never trigger).

fn bench_layout_wide(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_wide");

    for (label, layers, width) in [
        ("8x16", 8_usize, 16_usize),
        ("12x24", 12, 24),
        ("16x32", 16, 32),
    ] {
        let input = gen_wide(layers, width);
        let parsed = fm_parser::parse(&input);
        group.bench_with_input(BenchmarkId::new("layered", label), &parsed.ir, |b, ir| {
            b.iter(|| fm_layout::layout_diagram(ir));
        });
    }

    group.finish();
}

fn bench_full_pipeline_wide(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline_wide");
    let config = fm_render_svg::SvgRenderConfig::default();

    for (label, layers, width) in [
        ("8x16", 8_usize, 16_usize),
        ("12x24", 12, 24),
        ("16x32", 16, 32),
    ] {
        let input = gen_wide(layers, width);
        group.bench_with_input(
            BenchmarkId::new("parse_layout_svg", label),
            &input,
            |b, input| {
                b.iter(|| {
                    let parsed = fm_parser::parse(input);
                    let layout = fm_layout::layout_diagram(&parsed.ir);
                    fm_render_svg::render_svg_with_layout(&parsed.ir, &layout, &config)
                });
            },
        );
    }

    group.finish();
}

// ─── Render SVG benchmarks ──────────────────────────────────────────────────

fn bench_render_svg(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_svg");
    let config = fm_render_svg::SvgRenderConfig::default();

    for (label, input) in [
        ("small_10", gen_flowchart(10)),
        ("medium_100", gen_flowchart(100)),
        ("large_500", gen_flowchart(500)),
    ] {
        let parsed = fm_parser::parse(&input);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        group.bench_with_input(
            BenchmarkId::new("flowchart", label),
            &(&parsed.ir, &layout),
            |b, (ir, layout)| {
                b.iter(|| fm_render_svg::render_svg_with_layout(ir, layout, &config));
            },
        );
    }

    group.finish();
}

// ─── Full pipeline benchmarks ───────────────────────────────────────────────

fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");
    let config = fm_render_svg::SvgRenderConfig::default();

    for (label, input) in [
        ("small_10", gen_flowchart(10)),
        ("medium_100", gen_flowchart(100)),
        ("large_500", gen_flowchart(500)),
        ("cyclic_50", gen_cyclic(50)),
    ] {
        group.bench_with_input(
            BenchmarkId::new("parse_layout_svg", label),
            &input,
            |b, input| {
                b.iter(|| {
                    let parsed = fm_parser::parse(input);
                    let layout = fm_layout::layout_diagram(&parsed.ir);
                    fm_render_svg::render_svg_with_layout(&parsed.ir, &layout, &config)
                });
            },
        );
    }

    // Typical real-world diagram
    let typical = r"flowchart TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Process A]
    B -->|No| D[Process B]
    C --> E[Merge]
    D --> E
    E --> F[End]
    subgraph sub1[Phase 1]
        B
        C
        D
    end";
    group.bench_with_input(
        BenchmarkId::new("parse_layout_svg", "typical_7_nodes"),
        &typical,
        |b, input| {
            b.iter(|| {
                let parsed = fm_parser::parse(input);
                let layout = fm_layout::layout_diagram(&parsed.ir);
                fm_render_svg::render_svg_with_layout(&parsed.ir, &layout, &config)
            });
        },
    );

    group.finish();
}

/// Per-stage split (parse vs layout vs render) on the *wide* (edge-heavy) corpus.
///
/// The other groups bench layout in isolation (`layout_wide`) or the whole pipeline
/// fused (`full_pipeline_wide`), and the `render_svg` group only renders *linear*
/// flowcharts. None of them reveal where wide-pipeline time actually goes — which
/// turns out to be SVG render of the many-edge graph, not layout. This group isolates
/// each stage on the same wide inputs so render work can be targeted directly.
fn bench_wide_stages(c: &mut Criterion) {
    let mut group = c.benchmark_group("wide_stages");
    let config = fm_render_svg::SvgRenderConfig::default();

    for (label, layers, width) in [
        ("8x16", 8_usize, 16_usize),
        ("12x24", 12, 24),
        ("16x32", 16, 32),
    ] {
        let input = gen_wide(layers, width);
        let parsed = fm_parser::parse(&input);
        let layout = fm_layout::layout_diagram(&parsed.ir);

        group.bench_with_input(BenchmarkId::new("parse", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
        group.bench_with_input(BenchmarkId::new("layout", label), &parsed.ir, |b, ir| {
            b.iter(|| fm_layout::layout_diagram(ir));
        });
        group.bench_with_input(
            BenchmarkId::new("render", label),
            &(&parsed.ir, &layout),
            |b, (ir, layout)| {
                b.iter(|| fm_render_svg::render_svg_with_layout(ir, layout, &config));
            },
        );
    }

    group.finish();
}

/// Render the wide corpus with `include_source_spans = true`. Source-span metadata is
/// off by default (matching Mermaid.js, which emits no source maps), so the default-config
/// groups never exercise the span-emission path. This group isolates the spans-on render
/// cost — where redundant per-element source attributes dominate output bytes — so that
/// emit-side reductions there can be measured.
fn bench_render_spans_on(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_spans_on");
    let mut config = fm_render_svg::SvgRenderConfig::default();
    config.include_source_spans = true;

    for (label, layers, width) in [
        ("8x16", 8_usize, 16_usize),
        ("12x24", 12, 24),
        ("16x32", 16, 32),
    ] {
        let input = gen_wide(layers, width);
        let parsed = fm_parser::parse(&input);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        group.bench_with_input(
            BenchmarkId::new("render", label),
            &(&parsed.ir, &layout),
            |b, (ir, layout)| {
                b.iter(|| fm_render_svg::render_svg_with_layout(ir, layout, &config));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parse,
    bench_layout,
    bench_layout_wide,
    bench_full_pipeline_wide,
    bench_render_svg,
    bench_full_pipeline,
    bench_wide_stages,
    bench_render_spans_on
);
criterion_main!(benches);
