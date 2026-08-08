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
