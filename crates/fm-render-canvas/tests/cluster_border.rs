//! A declared cluster BORDER WIDTH and DASH reach the canvas (bd-lvj3).
//!
//! Found by a third probe wave on this bead, after it had twice looked finished. Both confirmed
//! against the SVG arm before implementing:
//!
//!     style one stroke-width:5px        svg emits stroke-width:5px       canvas=FALSE
//!     style one stroke-dasharray:9 4    svg emits stroke-dasharray:9 4   canvas=FALSE
//!
//! The width was not merely unread — the cluster draw HARDCODED `set_line_width(1.0)`.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

const SUBGRAPH: &str = "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n";

/// A declared cluster border width reaches the canvas.
#[test]
fn a_declared_cluster_border_width_reaches_the_canvas() {
    let ops = canvas_ops(&format!("{SUBGRAPH}  style one stroke-width:5px\n"));

    assert!(
        ops.contains("SetLineWidth(5.0)"),
        "the declared cluster border width never reached the canvas: {ops}"
    );
}

/// A declared cluster dash reaches the canvas.
#[test]
fn a_declared_cluster_dash_reaches_the_canvas() {
    let ops = canvas_ops(&format!("{SUBGRAPH}  style one stroke-dasharray:9 4\n"));

    assert!(
        ops.contains("SetLineDash([9.0, 4.0])"),
        "the declared cluster dash never reached the canvas: {ops}"
    );
}

/// CONTROL: the cluster dash must not leak onto the nodes inside it.
///
/// `lineDash` is canvas STATE. A dashed subgraph that never cleared it would dash every node
/// border drawn afterwards — and the nodes of a subgraph are drawn after their container.
#[test]
fn a_dashed_cluster_does_not_dash_its_children() {
    let ops = canvas_ops(&format!("{SUBGRAPH}  style one stroke-dasharray:9 4\n"));

    let last_pattern = ops
        .rfind("SetLineDash([9.0, 4.0])")
        .expect("the declared dash never reached the canvas, so this control proves nothing");
    let last_reset = ops.rfind("SetLineDash([])");
    assert!(
        last_reset.is_some_and(|reset| reset > last_pattern),
        "the cluster dash was never cleared, so it leaks onto everything drawn after it: {ops}"
    );
}

/// CONTROL: an unstyled subgraph is drawn exactly as before — width 1.0, no dash.
///
/// Guards the `None` arms on both properties at once. The width in particular was a hardcoded
/// constant, so this pins that the constant is still what an undeclared cluster gets.
#[test]
fn an_unstyled_cluster_keeps_its_previous_border() {
    let ops = canvas_ops(SUBGRAPH);

    assert!(
        ops.contains("SetLineWidth(1.0)"),
        "an unstyled cluster no longer draws at the previous width: {ops}"
    );
    assert!(
        !ops.contains("SetLineDash([9.0, 4.0])"),
        "an unstyled cluster acquired a dash: {ops}"
    );
}

/// CONTROL: malformed values are refused on both properties.
#[test]
fn a_malformed_cluster_border_falls_back() {
    for declared in ["stroke-width:wide", "stroke-width:-3px", "stroke-dasharray:0 0"] {
        let ops = canvas_ops(&format!("{SUBGRAPH}  style one {declared}\n"));

        assert!(
            !ops.contains("SetLineWidth(-"),
            "{declared:?} forwarded a negative width: {ops}"
        );
        assert!(
            !ops.contains("SetLineDash([0.0, 0.0])"),
            "{declared:?} forwarded a degenerate dash: {ops}"
        );
        assert!(!ops.contains("NaN"), "{declared:?} forwarded NaN: {ops}");
    }
}
