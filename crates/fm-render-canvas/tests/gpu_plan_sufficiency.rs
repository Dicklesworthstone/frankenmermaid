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

use fm_render_canvas::{CanvasRenderConfig, GpuRenderPlan, MockCanvas2dContext, render_to_canvas};

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

/// The plan now carries text, and this asserts the RUN COUNT matches the raster pass.
///
/// This test previously asserted the ABSENCE of a text buffer and instructed its own replacement
/// once the glyph atlas landed (bd-2u0.2 component 3). That has happened, so the shortfall
/// assertion is gone and the comparison it named is here instead.
///
/// RUNS, not glyphs, is the comparable unit: the raster pass issues one `fill_text` per label, so a
/// glyph count would compare 24 against 3 and never agree. Each run carries its own quad range, so
/// the glyph detail is still checked — by `a_node_label_becomes_one_run_of_glyph_quads` in the
/// module's unit tests, where the fixture is small enough for an exact count to mean something.
#[test]
fn the_plan_carries_one_text_run_per_drawn_label() {
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

    assert_eq!(
        plan.text_runs.len(),
        drawn_text,
        "the plan carries {} text runs against {drawn_text} the raster pass draws",
        plan.text_runs.len()
    );

    // Every run must actually carry glyphs, or the count above is satisfied by empty runs.
    for run in &plan.text_runs {
        assert!(
            run.quad_count > 0,
            "a text run carries no glyph quads: {run:?}"
        );
        let end = usize::try_from(run.first_quad + run.quad_count).unwrap_or(usize::MAX);
        assert!(
            end <= plan.text_quads.len(),
            "run range {run:?} overruns the {} quads in the buffer",
            plan.text_quads.len()
        );
    }

    // And the atlas must cover them: a quad pointing at a cell the atlas does not have would sample
    // whatever else happens to live at those UVs.
    assert!(
        !plan.glyph_atlas.cells.is_empty(),
        "text quads exist but the atlas is empty"
    );
}
