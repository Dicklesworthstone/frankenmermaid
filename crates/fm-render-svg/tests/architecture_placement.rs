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

fn node_bounds(source: &str, id: &str) -> fm_layout::LayoutRect {
    let ir = fm_parser::parse(source).ir;
    let layout = layout_diagram(&ir);
    layout
        .nodes
        .iter()
        .find(|node| node.node_id == id)
        .unwrap_or_else(|| panic!("node {id} missing from layout"))
        .bounds
}

fn node_x(source: &str, id: &str) -> f32 {
    node_bounds(source, id).x
}

fn node_y(source: &str, id: &str) -> f32 {
    node_bounds(source, id).y
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
/// Was `#[ignore]`d while it reproduced the live defect: both services landed at the SAME x,
/// stacked vertically, with the edge routed straight down from (53.83, 98.75) to (53.83, 218.75).
/// Un-ignored when the direction-aware placement landed.
#[test]
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

/// The AXIS has to be read too, not just the sign. A placement that honoured L/R but treated T/B
/// as "some horizontal direction" would pass both tests above while drawing every vertical
/// relationship sideways.
#[test]
fn architecture_vertical_direction_places_the_target_below() {
    let below = "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:B --> T:b\n";

    let a = node_bounds(below, "a");
    let b = node_bounds(below, "b");

    assert!(
        b.y > a.y,
        "`a:B --> T:b` must place b BELOW a, but a.y={} and b.y={}",
        a.y,
        b.y
    );
    assert!(
        (b.x - a.x).abs() < 1.0,
        "a vertical relationship must not also shift the column: a.x={} and b.x={}",
        a.x,
        b.x
    );
}

/// NEGATIVE CASE for the `<--` arm. `a:R <-- L:b` swaps the ENDPOINTS, so it must swap the SIDES
/// with them: the edge runs b → a leaving b's LEFT face, which puts a to the LEFT of b.
///
/// An implementation that reverses the endpoints and leaves the sides where they were reads a's
/// `R` as the source side and places a to the RIGHT of b — exactly backwards, and invisible to
/// every other test here because they all use the forward operator.
#[test]
fn architecture_reverse_operator_swaps_the_sides_with_the_endpoints() {
    let reversed =
        "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a:R <-- L:b\n";

    let a_x = node_x(reversed, "a");
    let b_x = node_x(reversed, "b");

    assert!(
        a_x < b_x,
        "`a:R <-- L:b` leaves b's LEFT face, so a is to the LEFT of b, but a.x={a_x} and b.x={b_x}"
    );
}

/// Two edges can send two services to the SAME cell. The chosen resolution is to FAN OUT along the
/// axis perpendicular to the step: both targets stay on a's right, at different rows. The one
/// outcome that is not acceptable is two boxes on top of each other, so that is what is asserted —
/// not the specific offset, which is the layout's business.
#[test]
fn architecture_colliding_targets_do_not_overlap() {
    let fan = "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  service c(cloud)[C]\n  a:R --> L:b\n  a:R --> L:c\n";

    let a = node_bounds(fan, "a");
    let b = node_bounds(fan, "b");
    let c = node_bounds(fan, "c");

    assert!(
        b.x > a.x && c.x > a.x,
        "both targets were sent to a's right and must both land there: a.x={}, b.x={}, c.x={}",
        a.x,
        b.x,
        c.x
    );

    let overlaps =
        b.x < c.x + c.width && c.x < b.x + b.width && b.y < c.y + c.height && c.y < b.y + b.height;
    assert!(
        !overlaps,
        "colliding targets must not be drawn on top of one another: b={b:?} c={c:?}"
    );
}

/// FALLBACK CONTROL: an architecture diagram that declares NO side keeps the layout it had before
/// bd-zce4 — the general path, which stacks the services in one column. Without this, the fix
/// could have been "always grid every architecture diagram", which would silently re-position
/// every side-less diagram in the corpus.
#[test]
fn architecture_without_declared_sides_keeps_the_general_layout() {
    let plain = "architecture-beta\n  service a(cloud)[A]\n  service b(cloud)[B]\n  a --> b\n";

    let a_x = node_x(plain, "a");
    let b_x = node_x(plain, "b");
    let a_y = node_y(plain, "a");
    let b_y = node_y(plain, "b");

    assert!(
        (a_x - b_x).abs() < 1.0 && b_y > a_y,
        "a side-less architecture diagram must still stack vertically: a=({a_x}, {a_y}) b=({b_x}, {b_y})"
    );
}
