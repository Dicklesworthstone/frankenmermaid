//! architecture-beta edge directions are a PLACEMENT grammar (bd-zce4).
//!
//! `a:R --> L:b` means b sits to the RIGHT of a. mermaid models this per edge as `sourceDir` and
//! `targetDir` (`ArchitectureDirection` in the pinned 11.15.0 bundle). We drop both.
//!
//! This file is the executable acceptance spec for that bead. It replaces a prose fix plan that was
//! WRONG: the bead originally proposed creating ports and anchoring edge endpoints on the declared
//! side. Measured, architecture-beta is dispatched to the general graph path and lands on SUGIYAMA,
//! which stacks the nodes VERTICALLY — so anchoring endpoints would have satisfied the plumbing and
//! left the picture just as wrong. Worse, `clip_to_shape_border` (the function that plan targeted)
//! has both of its call sites in the FORCE builder, which this diagram type never reaches.
//!
//! So the acceptance test asserts the thing a reader actually sees — WHERE THE NODES ARE — and not
//! the mechanism, which is the implementer's choice.

use fm_layout::layout_diagram;

fn node_x(source: &str, id: &str) -> f32 {
    let ir = fm_parser::parse(source).ir;
    let layout = layout_diagram(&ir);
    layout
        .nodes
        .iter()
        .find(|node| node.node_id == id)
        .unwrap_or_else(|| panic!("node {id} missing from layout"))
        .bounds
        .x
}

const RIGHT_OF: &str =
    "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:R --> L:b\n";

/// CONTROL, and it must pass TODAY. The reproducer below has to fail because the DIRECTION is
/// ignored, not because the diagram failed to parse or lost a node — a reproducer that fails for
/// the wrong reason certifies nothing when someone later makes it pass.
#[test]
fn architecture_places_both_declared_services() {
    let ir = fm_parser::parse(RIGHT_OF).ir;
    let layout = layout_diagram(&ir);

    assert_eq!(
        layout.nodes.len(),
        2,
        "both services must reach the layout before their POSITIONS can be asserted"
    );
    assert_eq!(
        layout.edges.len(),
        1,
        "the edge must exist; only its direction semantics are at issue"
    );
}

/// ACCEPTANCE GATE for bd-zce4. `a:R --> L:b` must put b to the RIGHT of a.
///
/// ⚠️ `#[ignore]` BECAUSE IT REPRODUCES A LIVE DEFECT, not because it is unfinished — the standing
/// this repo gives an acceptance test for an open bead. Run with `--ignored`; un-ignoring it is how
/// bd-zce4 closes.
///
/// Measured today: both services land at the SAME x and are stacked vertically, with the edge
/// routed straight down from (53.83, 98.75) to (53.83, 218.75).
#[test]
#[ignore = "bd-zce4: architecture-beta ignores edge direction; nodes are stacked vertically"]
fn architecture_edge_direction_places_the_target_to_the_right() {
    let a_x = node_x(RIGHT_OF, "a");
    let b_x = node_x(RIGHT_OF, "b");

    assert!(
        b_x > a_x,
        "`a:R --> L:b` must place b to the RIGHT of a, but a.x={a_x} and b.x={b_x}"
    );
}

/// The direction must also be READ, not merely honoured for one hard-coded case: reversing it has
/// to reverse the placement. Without this, a layout that always placed the second node to the right
/// would pass the gate above while ignoring the grammar just as completely.
#[test]
#[ignore = "bd-zce4: architecture-beta ignores edge direction"]
fn architecture_reversed_edge_direction_reverses_the_placement() {
    let left_of =
        "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:L --> R:b\n";

    let a_x = node_x(left_of, "a");
    let b_x = node_x(left_of, "b");

    assert!(
        b_x < a_x,
        "`a:L --> R:b` must place b to the LEFT of a, but a.x={a_x} and b.x={b_x}"
    );
}
