//! The canvas has NO interaction channel, and a tooltip must not leak into the drawing (bd-bk7h).
//!
//! `click nodeId "url" "tooltip"` populates three fields on `IrNodeInteraction`. fm-render-svg
//! emits all three — `href`, the callback wiring, and a `title` attribute — because an SVG has
//! ELEMENTS to hang them on. This renderer has none: measured, `fm-render-canvas/src` contains zero
//! references to `href`, `callback`, `tooltip` or any hit-region export.
//!
//! ⚠️ THAT IS A DESIGN BOUNDARY, NOT A TOOLTIP BUG, and the distinction matters for whoever picks
//! this up. An immediate-mode raster surface has nowhere to put an attribute and no hover to fire
//! it, so tooltips are one of THREE interaction features it cannot express, alongside links and
//! callbacks. Giving the canvas tooltips means designing a hit-region export the embedding
//! application can consult — a WebGPU/host-integration item, not a renderer patch.
//!
//! What this file pins is the failure mode available TODAY: a future change drawing the tooltip
//! text into the diagram body. That would put author-private hover text permanently on the canvas,
//! visible in every export, which is worse than not supporting it.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

const CLICKED: &str =
    "flowchart TD\n  a[Alpha] --> b[Beta]\n  click a \"https://example.com\" \"HOVERTEXT\"\n";

/// The tooltip text is never drawn.
#[test]
fn a_click_tooltip_is_not_drawn_onto_the_canvas() {
    let ops = canvas_ops(CLICKED);

    assert!(
        ops.contains("FillText(\"Alpha\""),
        "the diagram did not render, so this proves nothing: {ops}"
    );
    assert!(
        !ops.contains("HOVERTEXT"),
        "the click tooltip was drawn into the canvas body; it is hover text, and a canvas has no \
         hover, so drawing it puts it permanently in every export: {ops}"
    );
}

/// The click URL is not drawn either.
///
/// Same reasoning one field over, and worth its own assertion because a URL is longer and more
/// obviously wrong on the page — an implementation that drew "interaction data" generically would
/// fail here first.
#[test]
fn a_click_url_is_not_drawn_onto_the_canvas() {
    let ops = canvas_ops(CLICKED);

    assert!(
        !ops.contains("example.com"),
        "the click URL was drawn into the canvas body: {ops}"
    );
}

/// CONTROL: the click declaration reaches the IR, so the assertions above are about a RENDERING
/// choice and not about a parser that dropped the data.
///
/// Without this, a parser regression that discarded `click` entirely would make both tests above
/// pass while the feature silently vanished upstream — the exact shape that let this field sit
/// dead in three renderers until bd-bk7h measured it.
#[test]
fn the_click_declaration_really_reaches_the_ir() {
    let parsed = fm_parser::parse(CLICKED).ir;

    let tooltips: Vec<_> = parsed
        .nodes
        .iter()
        .filter_map(|node| node.tooltip())
        .collect();
    assert!(
        tooltips.iter().any(|t| t.contains("HOVERTEXT")),
        "the click tooltip never reached the IR, so the canvas assertions prove nothing: \
         {tooltips:?}"
    );
}
