//! Full-pipeline coverage for the seven syntax families at the scale where mermaid-js stops.
//!
//! WHY THIS FILE EXISTS. The head-to-head corpus reached thousands of nodes through exactly five
//! generators — `flowchart`, `architecture`, `er_schema`, `edit_trace`, `doc_build`. The other
//! seven stopped at the pinned baseline sizes: 20 participants, 40 states, 40 ER entities, 50
//! classes, 100 cyclic nodes, 200 dense-DAG nodes, 512 wide-layout nodes. So for seven of the
//! twelve families this project can express, nothing in the test suite ever asked whether we
//! render them AT ALL past a few hundred nodes — and that is precisely the regime where the
//! incumbent is reported to fail outright with a `RangeError`.
//!
//! Rendering correct output where the comparator cannot run is a capability claim, and a
//! capability claim needs a test, not a benchmark row. These are the tests.
//!
//! THE GENERATORS MIRROR `scripts/headtohead/corpus.mjs`. Same shapes, same parameters, same ids
//! as the `*_xl_*` corpus items, so a failure here and a failure in the harness are the same
//! failure. They are duplicated in Rust rather than read from the corpus JSON because a unit test
//! must not depend on a node script having been run first.
//!
//! WHAT EACH CASE ASSERTS, and why each one can fail:
//!   * every declared node reaches the layout — catches a renderer that silently truncates or
//!     de-duplicates at scale, which is the failure mode a `RangeError`-class engine exhibits;
//!   * the LAST declared node is present — a tail-dropping implementation passes a count check
//!     that uses `>=`, and fails this;
//!   * `fm_layout::invariants::layout_geometry_violations` is empty — the shared checker the
//!     fuzzer and the reducer use, so a geometry break here is expressible as a fuzz artifact;
//!   * the SVG is non-trivial and closes — catches a render that bails halfway;
//!   * the pipeline is deterministic across two independent runs — catches scale-dependent
//!     nondeterminism (iteration order over a hash map is the classic one).

use fm_layout::invariants::layout_geometry_violations;
use fm_layout::layout_diagram;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};

/// `wide`: layered flowchart, 50 layers x 50 wide = 2,500 nodes, each node linked to two nodes in
/// the next layer. Mirrors `wide_xl_50x50`.
fn wide(layers: usize, width: usize) -> String {
    let mut lines = vec!["flowchart TD".to_string()];
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

/// `cyclic`: rings of `ring` nodes, each ring fully cyclic, plus a forward stride edge. Mirrors
/// `cyclic_scc_xl_2500` — 500 strongly connected components, which is what exercises the cycle
/// removal path rather than the acyclic fast path.
fn cyclic(n: usize, ring: usize) -> String {
    let mut lines = vec!["flowchart TD".to_string()];
    for i in 0..n {
        lines.push(format!("  C{i}[C{i}]"));
    }
    for i in 0..n {
        let ring_start = (i / ring) * ring;
        let next = ring_start + ((i - ring_start + 1) % ring);
        if next < n {
            lines.push(format!("  C{i}-->C{next}"));
        }
        if i + ring < n {
            lines.push(format!("  C{i}-->C{}", i + ring));
        }
    }
    lines.join("\n")
}

/// `dense_dag`: every node points at the next `fanout` nodes. Mirrors `dense_dag_xl_2000` —
/// ~8,000 edges over 2,000 nodes, the highest edge density in the corpus.
fn dense_dag(n: usize, fanout: usize) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    for i in 0..n {
        lines.push(format!("  D{i}[D{i}]"));
    }
    for i in 0..n {
        for k in 1..=fanout {
            if i + k < n {
                lines.push(format!("  D{i}-->D{}", i + k));
            }
        }
    }
    lines.join("\n")
}

/// `sequence`: `n` participants with a request/response pair between each neighbour. Mirrors
/// `sequence_xl_2000`.
fn sequence(n: usize) -> String {
    let mut lines = vec!["sequenceDiagram".to_string()];
    for i in 0..n {
        lines.push(format!("  participant P{i}"));
    }
    for i in 0..n.saturating_sub(1) {
        lines.push(format!("  P{i}->>P{}: request {i}", i + 1));
        lines.push(format!("  P{}-->>P{i}: response {i}", i + 1));
    }
    lines.join("\n")
}

/// `class`: `n` classes each carrying a field and a method, chained by inheritance. Mirrors
/// `class_xl_2000` — members are what make this different from a bare node chain, because they
/// drive per-node box sizing.
fn class_diagram(n: usize) -> String {
    let mut lines = vec!["classDiagram".to_string()];
    for i in 0..n {
        lines.push(format!("  class C{i} {{"));
        lines.push(format!("    +int field{i}"));
        lines.push(format!("    +method{i}() bool"));
        lines.push("  }".to_string());
    }
    for i in 0..n.saturating_sub(1) {
        lines.push(format!("  C{i} <|-- C{}", i + 1));
    }
    lines.join("\n")
}

/// `state`: a linear state machine of `n` states with labelled transitions, entered and left
/// through the start/end pseudo-states. Mirrors `state_xl_2000`.
fn state_diagram(n: usize) -> String {
    let mut lines = vec!["stateDiagram-v2".to_string(), "  [*] --> S0".to_string()];
    for i in 0..n.saturating_sub(1) {
        lines.push(format!("  S{i} --> S{}: event{i}", i + 1));
    }
    lines.push(format!("  S{} --> [*]", n - 1));
    lines.join("\n")
}

/// `er`: a chain of `n` entities joined by one-to-many relationships. Mirrors `er_xl_2000`.
fn er_diagram(n: usize) -> String {
    let mut lines = vec!["erDiagram".to_string()];
    for i in 0..n.saturating_sub(1) {
        lines.push(format!("  E{i} ||--o{{ E{} : has", i + 1));
    }
    lines.join("\n")
}

/// Drive the whole pipeline and assert the capability claim.
///
/// `expected_nodes` is the count the SOURCE declares. Asserting equality rather than a lower bound
/// is deliberate: a renderer that drops the tail, or that collapses same-shaped nodes, satisfies
/// `>=` and fails this.
fn assert_renders_at_scale(case: &str, source: &str, expected_nodes: usize, last_node_id: &str) {
    let parsed = parse(source);
    assert_eq!(
        parsed.ir.nodes.len(),
        expected_nodes,
        "{case}: parser produced {} nodes for a source declaring {expected_nodes}",
        parsed.ir.nodes.len(),
    );
    assert!(
        parsed.ir.nodes.iter().any(|node| node.id == last_node_id),
        "{case}: the last declared node '{last_node_id}' is missing — the tail was dropped",
    );

    let layout = layout_diagram(&parsed.ir);
    assert_eq!(
        layout.nodes.len(),
        expected_nodes,
        "{case}: layout placed {} of {expected_nodes} nodes",
        layout.nodes.len(),
    );
    let violations = layout_geometry_violations(&layout);
    assert!(
        violations.is_empty(),
        "{case}: {} geometry invariant violation(s), first: {}",
        violations.len(),
        violations
            .first()
            .map_or_else(String::new, ToString::to_string),
    );

    let config = SvgRenderConfig::default();
    let svg = render_svg_with_layout(&parsed.ir, &layout, &config);
    assert!(
        svg.starts_with("<svg") || svg.contains("<svg"),
        "{case}: output is not an SVG document",
    );
    assert!(
        svg.trim_end().ends_with("</svg>"),
        "{case}: SVG document does not close — the render bailed part way",
    );

    // Determinism at scale, run independently rather than by cloning the first layout: a hash-order
    // dependence only shows up when the containers are rebuilt.
    let reparsed = parse(source);
    let relayout = layout_diagram(&reparsed.ir);
    let resvg = render_svg_with_layout(&reparsed.ir, &relayout, &config);
    assert_eq!(
        svg.len(),
        resvg.len(),
        "{case}: two independent runs produced different SVG lengths",
    );
    assert!(
        svg == resvg,
        "{case}: two independent runs disagreed byte for byte"
    );
}

#[test]
fn wide_layered_flowchart_renders_at_2500_nodes() {
    assert_renders_at_scale("wide_xl_50x50", &wide(50, 50), 2_500, "N49_49");
}

#[test]
fn cyclic_scc_flowchart_renders_at_2500_nodes() {
    assert_renders_at_scale("cyclic_scc_xl_2500", &cyclic(2_500, 5), 2_500, "C2499");
}

#[test]
fn dense_dag_renders_at_2000_nodes() {
    assert_renders_at_scale("dense_dag_xl_2000", &dense_dag(2_000, 4), 2_000, "D1999");
}

#[test]
fn sequence_renders_at_2000_participants() {
    assert_renders_at_scale("sequence_xl_2000", &sequence(2_000), 2_000, "P1999");
}

#[test]
fn class_diagram_renders_at_2000_classes() {
    assert_renders_at_scale("class_xl_2000", &class_diagram(2_000), 2_000, "C1999");
}

#[test]
fn state_diagram_renders_at_2000_states() {
    // The start and end pseudo-states are declared nodes too, so the source declares n + 2.
    assert_renders_at_scale("state_xl_2000", &state_diagram(2_000), 2_002, "S1999");
}

#[test]
fn er_diagram_renders_at_2000_entities() {
    assert_renders_at_scale("er_xl_2000", &er_diagram(2_000), 2_000, "E1999");
}
