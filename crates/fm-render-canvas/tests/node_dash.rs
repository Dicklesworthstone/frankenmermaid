//! A declared node BORDER DASH reaches the canvas (bd-lvj3).
//!
//! The fourth channel of the same declaration this bead has been closing one property at a time:
//! fill and stroke, then text `color`, then `stroke-width`, and now `stroke-dasharray`. Measured
//! before the fix, with the SVG arm as the control:
//!
//!     style a stroke-dasharray:5 5    svg emits `stroke-dasharray:5 5`    canvas=FALSE
//!
//! Same asymmetric-sibling shape as `stroke-width`: the EDGE path has drawn dashes since the edge
//! half landed, and the node path never learned the property, so a node the author asked to be
//! dashed drew a solid border.
//!
//! These assert on `SetLineDash`, which the mock records for real — it was a silent no-op until
//! the state-divider work made it observable, and before that the strongest available claim was
//! "something was stroked", which is true whether or not the feature works.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// A `style` directive's dash reaches the canvas.
#[test]
fn a_declared_node_dash_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[A]\n  style a stroke-dasharray:5 5\n");

    assert!(
        ops.contains("SetLineDash([5.0, 5.0])"),
        "the declared border dash never reached the canvas: {ops}"
    );
}

/// The same declaration through `classDef`, the channel a canvas cannot get for free.
///
/// fm-render-svg emits a CSS class and lets the browser cascade it; a canvas has no cascade, so a
/// `classDef` dash that works in SVG is dropped here unless it is resolved explicitly.
#[test]
fn a_classdef_dash_reaches_the_canvas() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A]\n  classDef dashed stroke-dasharray:3 2\n  class a dashed\n",
    );

    assert!(
        ops.contains("SetLineDash([3.0, 2.0])"),
        "a classDef border dash was dropped: {ops}"
    );
}

/// Comma-separated is the same declaration; SVG accepts either separator.
#[test]
fn a_comma_separated_dash_is_the_same_declaration() {
    let ops = canvas_ops("flowchart TD\n  a[A]\n  style a stroke-dasharray:5,5\n");

    assert!(
        ops.contains("SetLineDash([5.0, 5.0])"),
        "a comma-separated dash was not understood: {ops}"
    );
}

/// CONTROL, and the one that matters most here: the dash MUST NOT LEAK to the next node.
///
/// `lineDash` is canvas STATE, not a draw argument — it persists until something sets it again. An
/// implementation that set the dash and never cleared it would satisfy every assertion above while
/// dashing every subsequent shape in the diagram, which is a worse defect than the one being
/// fixed and is invisible to any test that renders a single node.
///
/// Asserted structurally: the LAST dash-affecting operation recorded must be the reset, not the
/// pattern. Counting occurrences would not do it — the correct implementation legitimately emits
/// the pattern once per dashed node.
#[test]
fn a_dashed_node_does_not_dash_the_rest_of_the_diagram() {
    let ops =
        canvas_ops("flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n  style a stroke-dasharray:5 5\n");

    assert!(
        ops.contains("SetLineDash([5.0, 5.0])"),
        "the declared dash never reached the canvas, so this control proves nothing: {ops}"
    );

    let last_pattern = ops.rfind("SetLineDash([5.0, 5.0])").expect("dash present");
    let last_reset = ops.rfind("SetLineDash([])");
    assert!(
        last_reset.is_some_and(|reset| reset > last_pattern),
        "the dash was never cleared after the node it belongs to, so it leaks onto every shape \
         drawn afterwards: {ops}"
    );
}

/// CONTROL: an undeclared node acquires no dash.
///
/// Guards the `None` arm. A resolver that returned a default pattern would satisfy the cases above
/// while dashing every border in every diagram.
#[test]
fn an_undeclared_node_keeps_a_solid_border() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n");

    assert!(
        ops.contains("Stroke") || ops.contains("StrokeRect"),
        "nothing was stroked, so this control proves nothing: {ops}"
    );
    assert!(
        !ops.contains("SetLineDash([5.0, 5.0])"),
        "an undeclared node was drawn dashed: {ops}"
    );
}

/// CONTROL: a malformed or degenerate dash is REFUSED, not forwarded.
///
/// `wide` does not parse; `-5 5` parses and is rejected on sign; `0 0` parses, is non-negative,
/// and is still refused — a zero-length pattern is not a no-op on a canvas, so forwarding it would
/// be strictly worse than ignoring the declaration.
#[test]
fn a_malformed_dash_falls_back_to_solid() {
    for source in [
        "flowchart TD\n  a[A]\n  style a stroke-dasharray:wide\n",
        "flowchart TD\n  a[A]\n  style a stroke-dasharray:-5 5\n",
        "flowchart TD\n  a[A]\n  style a stroke-dasharray:0 0\n",
    ] {
        let ops = canvas_ops(source);
        assert!(
            !ops.contains("SetLineDash([-"),
            "{source:?} forwarded a negative dash to the canvas: {ops}"
        );
        assert!(
            !ops.contains("SetLineDash([0.0, 0.0])"),
            "{source:?} forwarded a degenerate zero-length dash: {ops}"
        );
        assert!(
            !ops.contains("NaN"),
            "{source:?} forwarded NaN to the canvas: {ops}"
        );
    }
}
