//! Is the GPU plan SUFFICIENT to draw the diagram? (bd-2u0.2)
//!
//! `GpuRenderPlan` is the data a WebGPU backend would upload. The question that decides whether it
//! is finished is not "does it build" but "could a shader pass reproduce the picture from it alone".
//! Nothing asked that before: `from_layout` is called only from its own unit tests, so the plan has
//! never been compared against what the Canvas2D pass actually draws.
//!
//! These tests measure the plan against the raster pass on the same diagram. Where the plan covers
//! the geometry they assert it; where it CANNOT yet express something they record the size of the
//! gap as a number, so the bead's remaining work is quantified rather than described.

use fm_render_canvas::{
    CanvasRenderConfig, GpuRenderPlan, MockCanvas2dContext, render_to_canvas,
};

const DIAGRAM: &str = "flowchart TD\n  a[Alpha] --> b[Beta]\n  b --> c[Gamma]\n  c -.-> a\n";

/// Count `FillText` operations in a recorded canvas run.
fn fill_text_count(ops_debug: &str) -> usize {
    ops_debug.matches("FillText(").count()
}

/// The plan must carry ONE instance per laid-out node.
///
/// A missing instance is a node the GPU pass would simply not draw, which no amount of shader work
/// recovers.
#[test]
fn the_plan_carries_every_laid_out_node() {
    let ir = fm_parser::parse(DIAGRAM).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);

    assert_eq!(
        plan.node_instances.len(),
        layout.nodes.len(),
        "the plan lost nodes: {} instances for {} laid-out nodes",
        plan.node_instances.len(),
        layout.nodes.len()
    );
    assert!(
        !layout.nodes.is_empty(),
        "the fixture produced no nodes, so the equality above proves nothing"
    );
}

/// Every non-bundled edge must contribute its segments, and each drawn edge its arrowhead.
///
/// Asserted against the LAYOUT rather than a constant, so the fixture can change without the test
/// quietly becoming a tautology.
#[test]
fn the_plan_carries_every_edge_segment_and_head() {
    let ir = fm_parser::parse(DIAGRAM).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);

    let expected_segments: usize = layout
        .edges
        .iter()
        .filter(|edge| !edge.bundled)
        .map(|edge| edge.points.len().saturating_sub(1))
        .sum();
    assert_eq!(
        plan.edge_segments.len(),
        expected_segments,
        "the plan lost edge segments"
    );
    assert!(expected_segments > 0, "the fixture produced no segments");

    // `-.->` is directed, so all three edges here take a head; a `---` would not.
    let directed = layout
        .edges
        .iter()
        .filter(|edge| !edge.bundled)
        .filter(|edge| edge.points.len() >= 2)
        .count();
    assert_eq!(
        plan.arrowheads.len(),
        directed,
        "expected one head per directed edge, got {:?}",
        plan.arrowheads.len()
    );
}

/// THE GAP, MEASURED: the plan cannot express TEXT at all.
///
/// bd-2u0.2's architecture item 3 is a glyph atlas, and nothing of it exists. This test does not
/// pretend that is fine — it quantifies the shortfall so the bead's remaining work has a number
/// attached: the Canvas2D pass draws N text runs for this diagram, and the plan carries zero of
/// them, so a WebGPU backend fed only this plan would render an unlabelled skeleton.
///
/// It asserts the gap DELIBERATELY, so that when the atlas lands this test fails and forces its own
/// rewrite rather than sitting here asserting a shortfall that no longer exists.
#[test]
fn the_plan_cannot_yet_express_text_and_this_records_how_much() {
    let ir = fm_parser::parse(DIAGRAM).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);

    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let drawn_text = fill_text_count(&format!("{:?}", context.operations()));

    assert!(
        drawn_text > 0,
        "the raster pass drew no text, so this comparison would be vacuous"
    );

    // The plan has no text field of any kind — this is the whole of architecture item 3, absent.
    // Expressed as a size check on the struct's own contents rather than a comment, so it cannot
    // silently become untrue.
    let plan_primitives = plan.node_instances.len() + plan.edge_segments.len() + plan.arrowheads.len();
    assert!(
        plan_primitives > 0,
        "the plan is empty, so the text gap below is not the interesting fact"
    );

    // When the glyph atlas lands, `GpuRenderPlan` gains a text buffer and this assertion must be
    // replaced by one comparing it against `drawn_text`. Failing here is the intended signal.
    assert_eq!(
        drawn_text, 3,
        "the raster pass draws {drawn_text} text runs for this diagram that the plan carries NONE \
         of; if this number changed, re-measure the gap rather than adjusting the constant"
    );
}
