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

/// A sequence diagram: `participants` actors and `messages` arrow lines — the message lines
/// exercise the general arrow-operator scanner (`find_operator_from_index`), unlike the flowchart
/// fast-edge byte path.
fn gen_sequence(participants: usize, messages: usize) -> String {
    let mut s = String::from("sequenceDiagram\n");
    for i in 0..participants {
        s.push_str(&format!("  participant P{i} as Participant {i}\n"));
    }
    for m in 0..messages {
        let a = m % participants;
        let b = (m + 1) % participants;
        s.push_str(&format!("  P{a}->>P{b}: Message number {m}\n"));
    }
    s
}

/// A class diagram: `classes` classes each with a few members, plus inheritance/association
/// relationships between them — exercises the class member/relationship parser.
fn gen_class(classes: usize) -> String {
    let mut s = String::from("classDiagram\n");
    for i in 0..classes {
        s.push_str(&format!("  class Class{i} {{\n"));
        s.push_str(&format!("    +String field{i}\n"));
        s.push_str(&format!("    +int count{i}\n"));
        s.push_str(&format!("    +compute{i}() int\n"));
        s.push_str("  }\n");
    }
    for i in 0..classes.saturating_sub(1) {
        s.push_str(&format!("  Class{i} <|-- Class{}\n", i + 1));
        s.push_str(&format!("  Class{i} --> Class{} : uses\n", (i + 2) % classes));
    }
    s
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");

    for (label, participants, messages) in [
        ("seq_12x50", 12_usize, 50_usize),
        ("seq_12x200", 12, 200),
    ] {
        let input = gen_sequence(participants, messages);
        group.bench_with_input(BenchmarkId::new("sequence", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, classes) in [("class_30", 30_usize), ("class_100", 100_usize)] {
        let input = gen_class(classes);
        group.bench_with_input(BenchmarkId::new("class", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

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
