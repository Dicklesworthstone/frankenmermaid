//! End-to-end coverage that DOT `rank=same` reaches the layout constraint solver.
//!
//! The solver in `fm-layout` was fully implemented and unit-tested but UNREACHABLE from any input:
//! nothing in the parser ever produced an `IrConstraint`, so `ir.constraints` was always empty and
//! the solver returned at its first guard. These tests exist to keep that path live — they fail if
//! the parser stops emitting the constraint, or if the solver stops honoring it.

use fm_core::IrConstraint;
use fm_layout::{ConstraintSolverMode, LayoutConfig, layout_diagram, layout_diagram_with_config};
use fm_parser::parse;

/// `b` and `c` would land on different ranks by structure — `a -> b -> c` is a chain — so any
/// same-rank agreement in the output has to come from the constraint.
const CHAIN_WITH_RANK_GROUP: &str = "digraph G {\n  { rank=same; b; c; }\n  a -> b;\n  b -> c;\n}";
const CHAIN_WITHOUT_RANK_GROUP: &str = "digraph G {\n  a -> b;\n  b -> c;\n}";

fn node_position(layout: &fm_layout::DiagramLayout, id: &str) -> (f32, f32) {
    let node = layout
        .nodes
        .iter()
        .find(|node| node.node_id == id)
        .unwrap_or_else(|| panic!("node {id} must be laid out"));
    (node.bounds.x, node.bounds.y)
}

#[test]
fn dot_rank_same_produces_a_constraint_the_layout_can_see() {
    let parsed = parse(CHAIN_WITH_RANK_GROUP);
    assert_eq!(
        parsed.ir.constraints.len(),
        1,
        "the parser must hand the solver a constraint: {:?}",
        parsed.ir.constraints
    );
    assert!(matches!(
        &parsed.ir.constraints[0],
        IrConstraint::SameRank { node_ids, .. } if node_ids.len() == 2
    ));

    // Without the group there is nothing for the solver to apply, which is the state every input
    // was in before: a fully implemented solver that never ran.
    assert!(parse(CHAIN_WITHOUT_RANK_GROUP).ir.constraints.is_empty());
}

#[test]
fn dot_rank_same_moves_the_constrained_nodes_onto_one_rank() {
    let constrained = layout_diagram(&parse(CHAIN_WITH_RANK_GROUP).ir);
    let (bx, by) = node_position(&constrained, "b");
    let (cx, cy) = node_position(&constrained, "c");

    // DOT defaults to top-to-bottom, so a shared rank means a shared y.
    assert!(
        (by - cy).abs() < 0.5,
        "rank=same must put b and c on one rank: b=({bx},{by}) c=({cx},{cy})"
    );

    // And the constraint has to be what did it: the same chain without the group must NOT have them
    // level, or this test would pass on a solver that does nothing.
    let unconstrained = layout_diagram(&parse(CHAIN_WITHOUT_RANK_GROUP).ir);
    let (_, plain_by) = node_position(&unconstrained, "b");
    let (_, plain_cy) = node_position(&unconstrained, "c");
    assert!(
        (plain_by - plain_cy).abs() > 0.5,
        "without the group b and c must stay on different ranks, else the assertion above proves \
         nothing: b_y={plain_by} c_y={plain_cy}"
    );
}

#[test]
fn rank_assignment_honors_the_constraint_even_with_the_lp_solver_disabled() {
    // TWO subsystems consume `IrConstraint::SameRank`, and only one of them is gated:
    // `apply_ir_constraints` adjusts RANK ASSIGNMENT unconditionally, while the LP solver that
    // refines coordinates is what `ConstraintSolverMode` switches off. Both were unreachable before
    // the parser emitted a constraint. Pinned here because the distinction is not obvious from the
    // config name, and assuming the mode gates everything is the mistake this test was written
    // after making.
    let ir = parse(CHAIN_WITH_RANK_GROUP).ir;
    let disabled = layout_diagram_with_config(
        &ir,
        LayoutConfig {
            constraint_solver: ConstraintSolverMode::Disabled,
            ..LayoutConfig::default()
        },
    );

    let b = disabled
        .nodes
        .iter()
        .find(|node| node.node_id == "b")
        .expect("b laid out");
    let c = disabled
        .nodes
        .iter()
        .find(|node| node.node_id == "c")
        .expect("c laid out");
    assert_eq!(
        b.rank, c.rank,
        "rank assignment must honor SameRank regardless of the solver mode"
    );

    // And the structural chain really does separate them when no constraint exists, so the equality
    // above is the constraint's doing rather than an artifact of this tiny graph.
    let plain = layout_diagram(&parse(CHAIN_WITHOUT_RANK_GROUP).ir);
    let plain_b = plain
        .nodes
        .iter()
        .find(|node| node.node_id == "b")
        .expect("b laid out");
    let plain_c = plain
        .nodes
        .iter()
        .find(|node| node.node_id == "c")
        .expect("c laid out");
    assert_ne!(plain_b.rank, plain_c.rank);
}

#[test]
fn declaration_order_alone_does_not_change_the_ranks() {
    // The control that makes the two tests above airtight. `{ rank=same; b; c; }` declares b and c
    // BEFORE a, so a skeptic could argue the shared rank comes from declaration order rather than
    // from the constraint. It does not: the same order without a group ranks the chain 0/1/2.
    for source in [
        "digraph G {\n  b;\n  c;\n  a;\n  a -> b;\n  b -> c;\n}",
        "flowchart TD\n  b\n  c\n  a\n  a-->b\n  b-->c",
    ] {
        let layout = layout_diagram(&parse(source).ir);
        let rank = |id: &str| {
            layout
                .nodes
                .iter()
                .find(|node| node.node_id == id)
                .map(|node| node.rank)
                .unwrap_or_else(|| panic!("{id} must be laid out for {source:?}"))
        };
        assert_eq!(rank("a"), 0, "{source:?}");
        assert_eq!(rank("b"), 1, "{source:?}");
        assert_eq!(
            rank("c"),
            2,
            "b -> c must still separate the ranks when nothing constrains them: {source:?}"
        );
    }
}

#[test]
fn dot_minlen_pushes_the_target_further_down_the_ranks() {
    // `MinLength` is the second constraint kind that was implemented and unreachable. A plain
    // `a -> b` puts b one rank below a; `minlen=3` must push it at least three.
    let plain = layout_diagram(&parse("digraph G {\n  a -> b;\n}").ir);
    let stretched = layout_diagram(&parse("digraph G {\n  a -> b [minlen=3];\n}").ir);

    let rank_of = |layout: &fm_layout::DiagramLayout, id: &str| {
        layout
            .nodes
            .iter()
            .find(|node| node.node_id == id)
            .map(|node| node.rank)
            .unwrap_or_else(|| panic!("{id} must be laid out"))
    };

    assert_eq!(rank_of(&plain, "b") - rank_of(&plain, "a"), 1);
    let stretched_span = rank_of(&stretched, "b") - rank_of(&stretched, "a");
    assert!(
        stretched_span >= 3,
        "minlen=3 must span at least three ranks, got {stretched_span}"
    );

    // The vertical distance must grow with it, or the constraint changed a number nobody draws.
    let plain_gap = node_position(&plain, "b").1 - node_position(&plain, "a").1;
    let stretched_gap = node_position(&stretched, "b").1 - node_position(&stretched, "a").1;
    assert!(
        stretched_gap > plain_gap + 1.0,
        "the drawn gap must grow: plain={plain_gap} stretched={stretched_gap}"
    );
}

#[test]
fn dot_rankdir_lr_lays_the_graph_out_left_to_right() {
    // `rankdir=LR` is one of the most common lines in real .dot files and was dropped with every
    // other graph attribute, so a graph asking to flow left-to-right rendered top-to-bottom.
    let top_down = layout_diagram(&parse("digraph G {\n  a -> b;\n}").ir);
    let left_right = layout_diagram(&parse("digraph G {\n  rankdir=LR;\n  a -> b;\n}").ir);

    let (ax, ay) = node_position(&top_down, "a");
    let (bx, by) = node_position(&top_down, "b");
    assert!(
        by > ay + 1.0 && (bx - ax).abs() < 1.0,
        "the default must flow downward: a=({ax},{ay}) b=({bx},{by})"
    );

    let (ax, ay) = node_position(&left_right, "a");
    let (bx, by) = node_position(&left_right, "b");
    assert!(
        bx > ax + 1.0 && (by - ay).abs() < 1.0,
        "rankdir=LR must flow rightward: a=({ax},{ay}) b=({bx},{by})"
    );
}

#[test]
fn dot_default_attribute_statements_do_not_render_phantom_nodes() {
    // `node [shape=box]` sets a default; it is not a node. The node parser used to claim these
    // statements first and add stray boxes labelled graph/node/edge to the drawing.
    let layout = layout_diagram(
        &parse("digraph G {\n  graph [bgcolor=white];\n  node [shape=box];\n  edge [color=red];\n  a -> b;\n}")
            .ir,
    );
    let ids: Vec<&str> = layout
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        2,
        "only a and b may be drawn, got {ids:?} — a phantom box is a visible defect"
    );
    assert!(!ids.contains(&"graph") && !ids.contains(&"node") && !ids.contains(&"edge"));
}

#[test]
fn dot_colors_reach_the_rendered_svg() {
    // The style refs are only worth emitting if the renderer honors them, so this asserts on the
    // drawn output rather than on the IR. Without it, the parser could be producing style entries
    // nothing consumes — the same unreachable-work trap as an unparsed attribute.
    let parsed = parse("digraph G {\n  a [style=filled, color=red];\n  a -> b [color=blue];\n}");
    let svg = fm_render_svg::render_svg(&parsed.ir);

    // Bare colour NAMES are useless as needles: `red` occurs inside `prefers-reduced-motion` and
    // `blue` inside the `blueprint` theme name, both of which the stylesheet emits unconditionally.
    // Assert the property:value pairs instead.
    assert!(
        svg.contains("fill:red"),
        "the node fill must reach the SVG: {svg:.600}"
    );
    assert!(
        svg.contains("stroke:blue"),
        "the edge stroke must reach the SVG: {svg:.600}"
    );

    // The control, with the same precise needles.
    let plain = fm_render_svg::render_svg(&parse("digraph G {\n  a -> b;\n}").ir);
    assert!(
        !plain.contains("fill:red") && !plain.contains("stroke:blue"),
        "an uncoloured graph must carry neither"
    );
}

#[test]
fn dot_penwidth_and_font_attributes_reach_the_rendered_svg() {
    // Emitting CSS the renderer drops would be work nothing consumes, so this asserts on output.
    let parsed = parse(
        "digraph G {\n  a [penwidth=3, fontsize=18, fontname=Georgia];\n  a -> b [penwidth=2];\n}",
    );
    let svg = fm_render_svg::render_svg(&parsed.ir);

    for needle in [
        "stroke-width:3",
        "font-size:18pt",
        "font-family:Georgia",
        "stroke-width:2",
    ] {
        assert!(
            svg.contains(needle),
            "{needle} must reach the SVG: {svg:.600}"
        );
    }

    let plain = fm_render_svg::render_svg(&parse("digraph G {\n  a -> b;\n}").ir);
    for needle in ["stroke-width:3", "font-size:18pt", "font-family:Georgia"] {
        assert!(
            !plain.contains(needle),
            "an unstyled graph must not carry {needle}"
        );
    }
}

#[test]
fn dot_cluster_label_is_drawn_and_adds_no_phantom_box() {
    // `label="…"` inside a subgraph is the standard DOT way to name a cluster. It used to be read as
    // a node id, so a stray box appeared and the title was lost.
    let parsed = parse(
        "digraph G {\n  subgraph cluster_0 {\n    label=\"Backend\";\n    a;\n    b;\n  }\n}",
    );
    let layout = layout_diagram(&parsed.ir);

    let ids: Vec<&str> = layout
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        2,
        "only a and b may be drawn, got {ids:?} — `label` must not be a node"
    );

    let svg = fm_render_svg::render_svg(&parsed.ir);
    assert!(
        svg.contains("Backend"),
        "the cluster title must reach the drawing: {svg:.400}"
    );
}

#[test]
fn a_rank_group_inside_a_cluster_keeps_every_node_in_the_cluster() {
    // The brace-scope regression this shipped with: an anonymous `{ … }` group's closing brace used
    // to pop the enclosing cluster, so `d` fell outside it. Checked here at the IR level because a
    // dropped cluster member is a rendering defect, not just a parser detail.
    let parsed = parse(
        "digraph G {\n  subgraph cluster_0 {\n    a;\n    { rank=same; b; c; }\n    d;\n  }\n  a -> d;\n}",
    );

    assert_eq!(parsed.ir.clusters.len(), 1, "{:?}", parsed.ir.clusters);
    let members: Vec<&str> = parsed.ir.clusters[0]
        .members
        .iter()
        .map(|member| parsed.ir.nodes[member.0].id.as_str())
        .collect();
    assert_eq!(members, ["a", "b", "c", "d"]);
    assert_eq!(parsed.ir.constraints.len(), 1);
}
