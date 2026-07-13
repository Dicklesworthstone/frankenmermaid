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
        s.push_str(&format!(
            "  Class{i} --> Class{} : uses\n",
            (i + 2) % classes
        ));
    }
    s
}

/// A state diagram: `states` states with transitions (and a couple of guarded/labelled ones) —
/// exercises the state transition parser (shares `find_operator` + the general parser).
fn gen_state(states: usize) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    s.push_str("  [*] --> S0\n");
    for i in 0..states.saturating_sub(1) {
        s.push_str(&format!("  S{i} --> S{} : event{i}\n", i + 1));
    }
    s.push_str(&format!("  S{} --> [*]\n", states.saturating_sub(1)));
    s
}

/// An ER diagram: `entities` entities each with an attribute block, plus cardinality relationships
/// between them — exercises the ER relationship (`||--o{`) + attribute-block parser.
fn gen_er(entities: usize) -> String {
    let mut s = String::from("erDiagram\n");
    for i in 0..entities.saturating_sub(1) {
        s.push_str(&format!("  ENTITY{i} ||--o{{ ENTITY{} : rel{i}\n", i + 1));
    }
    for i in 0..entities {
        s.push_str(&format!("  ENTITY{i} {{\n"));
        s.push_str(&format!("    string name{i}\n"));
        s.push_str(&format!("    int value{i}\n"));
        s.push_str("  }\n");
    }
    s
}

/// A gantt chart: `tasks` tasks across a few sections — exercises the gantt task/section/date parser.
fn gen_gantt(tasks: usize) -> String {
    let mut s = String::from("gantt\n  title Project Plan\n  dateFormat YYYY-MM-DD\n");
    for i in 0..tasks {
        if i % 10 == 0 {
            s.push_str(&format!("  section Section {}\n", i / 10));
        }
        if i == 0 {
            s.push_str(&format!("  Task {i} :a{i}, 2024-01-01, 5d\n"));
        } else {
            s.push_str(&format!(
                "  Task {i} :a{i}, after a{}, {}d\n",
                i - 1,
                3 + i % 7
            ));
        }
    }
    s
}

/// A mindmap: `nodes` nodes across an indentation hierarchy — exercises the mindmap indentation/shape parser.
fn gen_mindmap(nodes: usize) -> String {
    let mut s = String::from("mindmap\n  root((Root))\n");
    for i in 0..nodes {
        let depth = 2 + (i % 3);
        let indent = "  ".repeat(depth);
        s.push_str(&format!("{indent}Node {i} idea\n"));
    }
    s
}

/// A user-journey diagram: `steps` task lines (`Task: score: actors`) across sections.
fn gen_journey(steps: usize) -> String {
    let mut s = String::from("journey\n  title My Working Day\n");
    for i in 0..steps {
        if i % 8 == 0 {
            s.push_str(&format!("  section Section {}\n", i / 8));
        }
        s.push_str(&format!(
            "    Task {i}: {}: Actor{}, Actor{}\n",
            1 + i % 5,
            i % 3,
            (i + 1) % 3
        ));
    }
    s
}

/// A timeline with one period and one event node per data line.
fn gen_timeline(event_count: usize) -> String {
    let mut lines = vec![String::from("timeline"), String::from("  title Timeline")];
    let sections = ((event_count as f64).sqrt() as usize).max(2);
    for section in 0..sections {
        lines.push(format!("  section Period {section}"));
        let per_section = event_count / sections;
        for item in 0..per_section {
            let index = section * per_section + item;
            lines.push(format!("    {} : Event {index}", 2000 + index));
        }
    }
    lines.join("\n")
}

/// A Kanban board with roughly square column/card dimensions — exercises card interning,
/// class assignment, and cluster/subgraph membership on the common no-metadata path.
fn gen_kanban(cards: usize) -> String {
    let mut s = String::from("kanban\n");
    let columns = (cards as f64).sqrt() as usize;
    let columns = columns.max(2);
    let cards_per_column = cards / columns;
    for column in 0..columns {
        s.push_str(&format!("  col{column}[Column {column}]\n"));
        for card in 0..cards_per_column {
            let index = column * cards_per_column + card;
            s.push_str(&format!("    task{index}[Task {index}]\n"));
        }
    }
    s
}

/// A gitgraph: `commands` commit/branch/checkout/merge commands — exercises the gitgraph command parser.
fn gen_gitgraph(commands: usize) -> String {
    let mut s = String::from("gitGraph\n");
    for i in 0..commands {
        match i % 6 {
            0 => s.push_str(&format!("  commit id: \"c{i}\"\n")),
            1 => s.push_str(&format!("  branch feature{i}\n")),
            2 => s.push_str(&format!("  checkout feature{i}\n")),
            3 => s.push_str(&format!("  commit id: \"c{i}\" tag: \"v{i}\"\n")),
            4 => s.push_str("  checkout main\n"),
            _ => s.push_str(&format!("  merge feature{}\n", i - 4)),
        }
    }
    s
}

/// A C4 context diagram: `elements` Person/System elements + Rel relationships (function-call syntax).
fn gen_c4(elements: usize) -> String {
    let mut s = String::from("C4Context\n  title System Context\n");
    for i in 0..elements {
        if i % 2 == 0 {
            s.push_str(&format!(
                "  Person(user{i}, \"User {i}\", \"A user number {i}\")\n"
            ));
        } else {
            s.push_str(&format!(
                "  System(sys{i}, \"System {i}\", \"The system {i}\")\n"
            ));
        }
    }
    for i in 0..elements.saturating_sub(1) {
        s.push_str(&format!(
            "  Rel(user{}, sys{}, \"Uses\", \"HTTPS\")\n",
            i & !1,
            (i + 1) | 1
        ));
    }
    s
}

/// A requirement diagram: `n` requirement/element blocks + relationships.
fn gen_requirement(n: usize) -> String {
    let mut s = String::from("requirementDiagram\n");
    for i in 0..n {
        s.push_str(&format!("  requirement req_{i} {{\n    id: {i}\n    text: requirement text number {i}\n    risk: high\n    verifymethod: test\n  }}\n"));
        s.push_str(&format!(
            "  element elem_{i} {{\n    type: simulation\n    docref: doc{i}\n  }}\n"
        ));
    }
    for i in 0..n.saturating_sub(1) {
        s.push_str(&format!("  elem_{i} - satisfies -> req_{}\n", i + 1));
    }
    s
}

/// A DOT graph bridge input: node declarations plus chained directed edges with labels.
fn gen_dot(nodes: usize) -> String {
    let mut s = String::from("digraph G {\n");
    for i in 0..nodes {
        s.push_str(&format!("  N{i} [label=\"Node {i}\"];\n"));
    }
    for i in 0..nodes.saturating_sub(1) {
        s.push_str(&format!("  N{i} -> N{} [label=\"edge {i}\"];\n", i + 1));
    }
    s.push_str("}\n");
    s
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");

    for (label, participants, messages) in
        [("seq_12x50", 12_usize, 50_usize), ("seq_12x200", 12, 200)]
    {
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

    for (label, states) in [("state_30", 30_usize), ("state_100", 100_usize)] {
        let input = gen_state(states);
        group.bench_with_input(BenchmarkId::new("state", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, entities) in [("er_30", 30_usize), ("er_100", 100_usize)] {
        let input = gen_er(entities);
        group.bench_with_input(BenchmarkId::new("er", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, tasks) in [("gantt_50", 50_usize), ("gantt_200", 200_usize)] {
        let input = gen_gantt(tasks);
        group.bench_with_input(BenchmarkId::new("gantt", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, nodes) in [("mindmap_50", 50_usize), ("mindmap_200", 200_usize)] {
        let input = gen_mindmap(nodes);
        group.bench_with_input(BenchmarkId::new("mindmap", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, steps) in [("journey_50", 50_usize), ("journey_200", 200_usize)] {
        let input = gen_journey(steps);
        group.bench_with_input(BenchmarkId::new("journey", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    let input = gen_timeline(1600);
    group.bench_with_input(
        BenchmarkId::new("timeline", "timeline_1600"),
        &input,
        |b, input| {
            b.iter(|| fm_parser::parse(input));
        },
    );

    for (label, cards) in [("kanban_400", 400_usize), ("kanban_1600", 1600_usize)] {
        let input = gen_kanban(cards);
        group.bench_with_input(BenchmarkId::new("kanban", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, cmds) in [("gitgraph_50", 50_usize), ("gitgraph_200", 200_usize)] {
        let input = gen_gitgraph(cmds);
        group.bench_with_input(BenchmarkId::new("gitgraph", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, elements) in [("c4_50", 50_usize), ("c4_100", 100_usize)] {
        let input = gen_c4(elements);
        group.bench_with_input(BenchmarkId::new("c4", label), &input, |b, input| {
            b.iter(|| fm_parser::parse(input));
        });
    }

    for (label, n) in [("req_30", 30_usize), ("req_100", 100_usize)] {
        let input = gen_requirement(n);
        group.bench_with_input(
            BenchmarkId::new("requirement", label),
            &input,
            |b, input| {
                b.iter(|| fm_parser::parse(input));
            },
        );
    }

    for (label, nodes) in [("dot_50", 50_usize), ("dot_200", 200_usize)] {
        let input = gen_dot(nodes);
        group.bench_with_input(BenchmarkId::new("dot", label), &input, |b, input| {
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
