//! A declared node OPACITY reaches the canvas (bd-lvj3).
//!
//!     style a opacity:0.5    svg emits `opacity:0.5`    canvas=FALSE  ->  fixed
//!
//! ⚠️ WRITTEN AND COMMITTED UNBUILT. The host was saturated (runq 97, CPU idle 0, three local and
//! four remote builds live) and building was forbidden for the tick, so these assertions are
//! reasoned from the operation stream's shape rather than observed. The needles follow `f64`'s
//! `Debug` (`0.5`, `1.0`), which is what the sibling `SetLineDash([6.0, 4.0])` assertions in this
//! crate already rely on. First green build should confirm or correct them.
//!
//! The instrument had to be repaired first, for the THIRD time in this crate:
//! `MockCanvas2dContext::set_global_alpha` updated `current_state` and pushed no operation, so
//! nothing could observe that a node was drawn faded — the same silent no-op that `set_font` and
//! `set_line_dash` each turned out to be. A context method that only writes `current_state` makes
//! every assertion about its property vacuous, and vacuous assertions pass.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// A `style` directive's opacity reaches the canvas.
#[test]
fn a_declared_node_opacity_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a opacity:0.5\n");

    assert!(
        ops.contains("SetGlobalAlpha(0.5)"),
        "the declared opacity never reached the canvas: {ops}"
    );
}

/// The same declaration through `classDef`.
#[test]
fn a_classdef_opacity_reaches_the_canvas() {
    let ops =
        canvas_ops("flowchart TD\n  a[Alpha]\n  classDef faded opacity:0.25\n  class a faded\n");

    assert!(
        ops.contains("SetGlobalAlpha(0.25)"),
        "a classDef opacity was dropped: {ops}"
    );
}

/// CONTROL, and the one that matters: the fade MUST NOT leak to the rest of the diagram.
///
/// `globalAlpha` is canvas STATE, not a draw argument. Left set, it fades every subsequent node,
/// edge and label — a worse defect than the one being fixed, and invisible to any test that
/// renders a single node.
///
/// Asserted structurally: the LAST alpha operation must be the restore, not the fade. Counting
/// occurrences cannot express it, since a correct implementation legitimately emits the fade once
/// per faded node.
#[test]
fn a_faded_node_does_not_fade_the_rest_of_the_diagram() {
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha] --> b[Beta]\n  b --> c[Gamma]\n  style a opacity:0.5\n",
    );

    assert!(
        ops.contains("SetGlobalAlpha(0.5)"),
        "the declared opacity never reached the canvas, so this control proves nothing: {ops}"
    );

    let last_fade = ops.rfind("SetGlobalAlpha(0.5)").expect("fade present");
    let last_restore = ops.rfind("SetGlobalAlpha(1.0)");
    assert!(
        last_restore.is_some_and(|restore| restore > last_fade),
        "the fade was never restored, so it leaks onto everything drawn afterwards: {ops}"
    );
}

/// CONTROL: an undeclared diagram emits NO alpha operations at all.
///
/// Guards the common path. An implementation that set alpha unconditionally — even to 1.0 — would
/// add an operation per node to every diagram ever rendered, and would satisfy every other test
/// here.
#[test]
fn an_undeclared_diagram_touches_alpha_not_at_all() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha] --> b[Beta]\n");

    assert!(
        ops.contains("FillText(\"Alpha\""),
        "the diagram did not render, so this control proves nothing: {ops}"
    );
    assert!(
        !ops.contains("SetGlobalAlpha"),
        "an unstyled diagram touched globalAlpha: {ops}"
    );
}

/// A fully transparent node is ACCEPTED, because SVG honours `opacity:0`.
///
/// Deliberately not treated as junk: an author asking for an invisible node is asking for
/// something the reference implementation grants.
#[test]
fn a_fully_transparent_node_is_honoured() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a opacity:0\n");

    assert!(
        ops.contains("SetGlobalAlpha(0.0)"),
        "opacity:0 was refused, but SVG honours it: {ops}"
    );
}

/// CONTROL: a malformed or out-of-range opacity is REFUSED.
///
/// `globalAlpha` outside `0..=1` is IGNORED by a canvas, which leaves the PREVIOUS alpha in force
/// — so forwarding `1.5` would not merely fail to fade this node, it would fade whatever came
/// next, or not, depending on draw order. Refusing is the visible failure.
#[test]
fn a_malformed_opacity_is_refused() {
    for declared in ["ghostly", "-0.5", "1.5", "NaN"] {
        let source = format!("flowchart TD\n  a[Alpha]\n  style a opacity:{declared}\n");
        let ops = canvas_ops(&source);

        assert!(
            ops.contains("FillText(\"Alpha\""),
            "{declared}: the node did not render, so this proves nothing: {ops}"
        );
        assert!(
            !ops.contains("SetGlobalAlpha"),
            "{declared}: a malformed opacity was forwarded to the canvas: {ops}"
        );
    }
}
