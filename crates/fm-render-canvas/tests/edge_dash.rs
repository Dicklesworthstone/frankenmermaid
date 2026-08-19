//! A declared EDGE DASH reaches the canvas (bd-lvj3).
//!
//! The edge twin of the node dash, and the last dash channel this bead measured as missing:
//!
//!     linkStyle 0 stroke-dasharray:7 3    svg emits `stroke-dasharray:7 3`    canvas=FALSE
//!
//! The canvas used the ARROW-DERIVED pattern from `legacy_edge_stroke` regardless, so a solid
//! `-->` the author asked to be dashed stayed solid, and a dotted `-.->` they asked to be solid
//! stayed dotted. The declared pattern overrides the arrow-derived one — they are two answers to
//! the same question, and the explicit declaration is the more specific one.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// An indexed `linkStyle` dash reaches the edge.
#[test]
fn a_declared_edge_dash_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-dasharray:7 3\n");

    assert!(
        ops.contains("SetLineDash([7.0, 3.0])"),
        "the declared edge dash never reached the canvas: {ops}"
    );
}

/// `linkStyle default` dashes every edge.
#[test]
fn a_default_linkstyle_dash_reaches_every_edge() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n  linkStyle default stroke-dasharray:7 3\n",
    );

    assert!(
        ops.matches("SetLineDash([7.0, 3.0])").count() >= 2,
        "the default dash did not reach both edges: {ops}"
    );
}

/// The declared dash OVERRIDES the arrow-derived pattern.
///
/// `-.->` is dotted by arrow type, so this is the case that distinguishes "the author's
/// declaration is used" from "the arrow glyph still wins". Without it, an implementation that
/// ignored the declaration whenever the arrow already implied a dash would pass the cases above.
#[test]
fn a_declared_dash_overrides_the_arrow_derived_one() {
    let ops = canvas_ops("flowchart TD\n  a[A] -.-> b[B]\n  linkStyle 0 stroke-dasharray:7 3\n");

    assert!(
        ops.contains("SetLineDash([7.0, 3.0])"),
        "the declared dash lost to the arrow-derived pattern: {ops}"
    );
}

/// CONTROL: an undeclared edge keeps its arrow-derived pattern.
///
/// Guards the `None` arm from the other side. A resolver that returned an empty pattern when
/// nothing was declared would make every dotted arrow solid — a regression invisible to any test
/// that only checks declared edges.
#[test]
fn an_undeclared_dotted_arrow_keeps_its_dash() {
    let solid = canvas_ops("flowchart TD\n  a[A] --> b[B]\n");
    let dotted = canvas_ops("flowchart TD\n  a[A] -.-> b[B]\n");

    assert_ne!(
        solid, dotted,
        "a dotted arrow rendered identically to a solid one, so the arrow-derived dash was lost"
    );
    assert!(
        !dotted.contains("SetLineDash([7.0, 3.0])"),
        "an undeclared edge acquired the test's declared pattern: {dotted}"
    );
}

/// CONTROL: a malformed edge dash falls back rather than forwarding junk.
#[test]
fn a_malformed_edge_dash_falls_back_to_the_arrow_pattern() {
    for source in [
        "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-dasharray:wide\n",
        "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-dasharray:-7 3\n",
        "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-dasharray:0 0\n",
    ] {
        let ops = canvas_ops(source);
        assert!(
            !ops.contains("SetLineDash([-"),
            "{source:?} forwarded a negative dash: {ops}"
        );
        assert!(
            !ops.contains("SetLineDash([0.0, 0.0])"),
            "{source:?} forwarded a degenerate zero-length dash: {ops}"
        );
        assert!(!ops.contains("NaN"), "{source:?} forwarded NaN: {ops}");
    }
}
