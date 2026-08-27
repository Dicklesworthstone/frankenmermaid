//! ER crow's-foot cardinality on the canvas and WebGPU surfaces (bd-hh0o7).
//!
//! THE DEFECT. bd-dun16 landed mermaid's four crow's-foot shapes in fm-render-svg (6/6 agree
//! against the pinned 11.15.0 bundle, `scripts/headtohead/er_marker_diff.mjs`). Canvas2D and
//! WebGPU drew NOTHING for the same eight `MarkerKind` variants.
//!
//! ⚠️ DRAWING NOTHING WAS THE DELIBERATE CHOICE, NOT AN OVERSIGHT. `GpuMarkerKind`'s fallback is
//! `Arrow`, and an arrowhead where a crow's foot belongs does not read as "cardinality
//! unavailable" — it reads as a DIFFERENT cardinality, so the diagram states something FALSE
//! rather than something incomplete. That reasoning is now satisfied by drawing the right shape,
//! not by drawing none, and `collect_scene_markers` still refuses to map these onto the arrowhead
//! shader: the outline goes through the segment pipeline instead.
//!
//! THE CONTROL THE BEAD REQUIRES: asserting "a marker was drawn" passes on one shared glyph used
//! for all four forms. So every assertion here is a DIFFERENCE — between forms, and between the
//! two ends of one form.

use fm_render_canvas::{Canvas2dRenderer, CanvasRenderConfig, GpuRenderPlan, MockCanvas2dContext};

/// The four cardinality notations, each as a complete ER document.
const FORMS: [(&str, &str); 4] = [
    ("exactly one", "erDiagram\n  A ||--|| B : r\n"),
    ("zero or one", "erDiagram\n  A |o--o| B : r\n"),
    ("one or more", "erDiagram\n  A }|--|{ B : r\n"),
    ("zero or more", "erDiagram\n  A }o--o{ B : r\n"),
];

fn plan(source: &str) -> GpuRenderPlan {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    GpuRenderPlan::from_layout(&ir, &layout, 1.5)
}

/// One segment as rounded integer endpoints, so float noise cannot make two identical shapes
/// compare different.
type SegmentKey = (i64, i64, i64, i64);

/// A form's marker segments as a sorted key set. Positions are relative to nothing in particular —
/// what matters is the SET.
fn segment_shape(source: &str) -> Vec<SegmentKey> {
    let mut shape: Vec<SegmentKey> = plan(source)
        .er_marker_segments
        .iter()
        .map(|s| {
            (
                (f64::from(s.from[0]) * 100.0).round() as i64,
                (f64::from(s.from[1]) * 100.0).round() as i64,
                (f64::from(s.to[0]) * 100.0).round() as i64,
                (f64::from(s.to[1]) * 100.0).round() as i64,
            )
        })
        .collect();
    shape.sort_unstable();
    shape
}

/// THE NEGATIVE CASE on the GPU path: an ER relationship emits geometry where it emitted none.
#[test]
fn er_cardinality_is_not_an_empty_gpu_scene() {
    for (name, source) in FORMS {
        let segments = plan(source).er_marker_segments;
        assert!(
            !segments.is_empty(),
            "{name} still plans zero crow's-foot segments"
        );
        for segment in &segments {
            assert!(
                segment.from != segment.to,
                "{name} planned a degenerate zero-length segment"
            );
        }
    }
}

/// THE CONTROL THE BEAD REQUIRES: the four forms must draw DIFFERENT shapes.
///
/// Asserting only "a marker exists" passes on a single shared glyph reused for all four, which is
/// the bd-vfxu failure shape. Comparing the four segment sets pairwise is what makes a shared glyph
/// impossible to ship.
#[test]
fn the_four_cardinality_forms_draw_different_shapes() {
    let shapes: Vec<(&str, Vec<SegmentKey>)> = FORMS
        .iter()
        .map(|(name, source)| (*name, segment_shape(source)))
        .collect();
    for i in 0..shapes.len() {
        for j in (i + 1)..shapes.len() {
            assert_ne!(
                shapes[i].1, shapes[j].1,
                "{} and {} draw the same shape, so the glyph is shared rather than per-form",
                shapes[i].0, shapes[j].0
            );
        }
    }
}

/// ⚠️ THE TWO ENDS OF ONE FORM ARE DIFFERENT GLYPHS, and a set-of-markers comparison passes on a
/// swap. So this checks an ASYMMETRIC notation: `A ||--o{ B` is "exactly one" at the left and
/// "zero or more" at the right, which must produce two visibly different clusters of segments.
///
/// Counted rather than positioned, because the count is what distinguishes the shapes: an
/// exactly-one end is two bars (2 segments) and a zero-or-more end is a sampled lens plus a sampled
/// bubble (24 + 16). A renderer that drew one glyph at both ends produces a segment total that is
/// exactly twice one of them, which no correct pairing can be.
#[test]
fn the_two_ends_of_an_asymmetric_relationship_differ() {
    let segments = plan("erDiagram\n  A ||--o{ B : r\n").er_marker_segments;
    // 2 bars at the "exactly one" end, 24 lens + 16 bubble at the "zero or more" end.
    assert_eq!(
        segments.len(),
        2 + 24 + 16,
        "the two ends are not the two distinct glyphs the notation asks for"
    );

    let symmetric_one = plan("erDiagram\n  A ||--|| B : r\n")
        .er_marker_segments
        .len();
    let symmetric_many = plan("erDiagram\n  A }o--o{ B : r\n")
        .er_marker_segments
        .len();
    assert_eq!(symmetric_one, 4, "exactly-one at both ends is four bars");
    assert_eq!(
        symmetric_many,
        2 * (24 + 16),
        "zero-or-more at both ends is two lens-plus-bubble clusters"
    );
    assert_ne!(
        segments.len(),
        symmetric_one,
        "the asymmetric relationship drew the symmetric one's shape"
    );
    assert_ne!(
        segments.len(),
        symmetric_many,
        "the asymmetric relationship drew the other symmetric shape"
    );
}

/// The arrowhead shader is still NOT asked to draw these.
///
/// The whole reason the old code drew nothing: `GpuMarkerKind`'s fallback is `Arrow`. If a crow's
/// foot ever reaches the marker instance buffer it will render as an arrowhead and state a
/// cardinality the source never declared — so the refusal in `collect_scene_markers` has to survive
/// this change, and a passing segment test does not prove that on its own.
#[test]
fn crows_feet_never_reach_the_arrowhead_instance_buffer() {
    for (name, source) in FORMS {
        let plan = plan(source);
        assert!(
            !plan.er_marker_segments.is_empty(),
            "{name} planned no segments, so this assertion proves nothing"
        );
        assert!(
            plan.arrowheads.is_empty(),
            "{name} put a crow's foot into the arrowhead buffer, where it renders as an arrow"
        );
    }
}

/// THE NEGATIVE CASE on the Canvas2D path, and the CONTROL, in one test.
///
/// ⚠️ NEITHER A DRAW-CALL COUNT NOR A COMPARISON AGAINST AN UNMARKED DIAGRAM DISCRIMINATES, and
/// both were tried. An ER diagram has entities, labels and an edge to draw, so its call count
/// exceeds a bare relationship's whether or not one crow's foot is drawn — that version passed with
/// the marker code DISARMED. And the counts do not separate the forms either: exactly-one is four
/// bar strokes and zero-or-more is two lenses plus two bubbles, both four calls.
///
/// What DOES discriminate is the shape of the recorded operations, because the four glyphs are made
/// of different primitives:
///
///   exactly one   bars only          -> no arcs, no quadratics
///   zero or one   bar + bubble       -> arcs, no quadratics
///   one or more   lens + bar         -> quadratics, no arcs
///   zero or more  lens + bubble      -> both
///
/// A surface drawing nothing gives all four the baseline counts; a surface drawing one shared glyph
/// gives all four the same counts as each other. Neither passes.
#[test]
fn each_form_draws_its_own_primitives_on_the_canvas() {
    use fm_render_canvas::DrawOperation;

    // The same diagram with no cardinality at all, so whatever the entities and edge draw is
    // subtracted rather than mistaken for a marker.
    let (base_arcs, base_quads) = primitive_counts("erDiagram\n  A .. B : r\n");

    let mut seen = Vec::new();
    for (name, source) in FORMS {
        let (arcs, quads) = primitive_counts(source);
        let extra = (
            arcs.saturating_sub(base_arcs),
            quads.saturating_sub(base_quads),
        );
        seen.push((name, extra));
    }

    // Two per relationship: one glyph at each end.
    assert_eq!(seen[0].1, (0, 0), "exactly one should draw bars only");
    assert_eq!(seen[1].1, (2, 0), "zero or one should draw two bubbles");
    assert_eq!(seen[2].1, (0, 4), "one or more should draw two lenses");
    assert_eq!(
        seen[3].1,
        (2, 4),
        "zero or more should draw two lenses and two bubbles"
    );

    // And the bars are there for the form that is nothing but bars, which the counts above cannot
    // see — otherwise "exactly one" would pass by drawing nothing at all.
    let ops = operations("erDiagram\n  A ||--|| B : r\n");
    let plain_ops = operations("erDiagram\n  A .. B : r\n");
    let lines = |ops: &[DrawOperation]| {
        ops.iter()
            .filter(|op| matches!(op, DrawOperation::LineTo(_, _)))
            .count()
    };
    assert!(
        lines(&ops) > lines(&plain_ops),
        "exactly-one drew no extra line segments, so its four bars are missing"
    );
}

fn operations(source: &str) -> Vec<fm_render_canvas::DrawOperation> {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
    let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
    renderer.render(&layout, &ir, &mut ctx);
    ctx.operations().to_vec()
}

fn primitive_counts(source: &str) -> (usize, usize) {
    use fm_render_canvas::DrawOperation;
    let ops = operations(source);
    let arcs = ops
        .iter()
        .filter(|op| matches!(op, DrawOperation::Arc(..)))
        .count();
    let quads = ops
        .iter()
        .filter(|op| matches!(op, DrawOperation::QuadraticCurveTo(..)))
        .count();
    (arcs, quads)
}

/// A non-ER diagram plans none of this.
#[test]
fn other_diagram_types_plan_no_crows_feet() {
    let flowchart = plan("flowchart LR\n  A --> B\n");
    assert!(
        flowchart.er_marker_segments.is_empty(),
        "a flowchart planned crow's-foot segments"
    );
    assert!(
        !flowchart.edge_segments.is_empty(),
        "the flowchart planned no edges at all, so this control proves nothing"
    );
}
