//! EDGE and CLUSTER opacity, and the edge label's font size (bd-lvj3).
//!
//! The last three reachable rows of the styling matrix. All three confirmed against the SVG arm
//! before implementing — it emits `opacity:0.5` for both surfaces and `font-size:22px` for the
//! edge label — so each was a real disagreement between two renderers.
//!
//! `globalAlpha` is canvas STATE, which is why every test here has a restore control beside it.
//! The realistic failure is not "the fade is missing" but "the fade never stopped", and that one
//! is invisible to any test that renders a single element.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// The font in force when `needle` was drawn, by replaying the op stream in order.
fn font_when_text_drawn(ops_debug: &str, needle: &str) -> Option<String> {
    let mut current: Option<String> = None;
    let mut rest = ops_debug;
    loop {
        let next_font = rest.find("SetFont(\"");
        let next_text = rest.find(&format!("FillText({needle:?}"));
        match (next_font, next_text) {
            (_, None) => return None,
            (Some(f), Some(t)) if f < t => {
                let after = &rest[f + "SetFont(\"".len()..];
                let end = after.find('"')?;
                current = Some(after[..end].to_string());
                rest = &after[end..];
            }
            (_, Some(_)) => return current,
        }
    }
}

/// The fade must be restored, or it leaks onto everything drawn afterwards.
fn assert_fade_is_restored(ops: &str, context: &str) {
    let last_fade = ops.rfind("SetGlobalAlpha(0.5)").unwrap_or_else(|| {
        panic!("{context}: the declared opacity never reached the canvas: {ops}")
    });
    let last_restore = ops.rfind("SetGlobalAlpha(1.0)");
    assert!(
        last_restore.is_some_and(|restore| restore > last_fade),
        "{context}: the fade was never restored, so it leaks onto the rest of the diagram: {ops}"
    );
}

/// A declared edge opacity reaches the canvas, and is restored.
#[test]
fn a_declared_edge_opacity_reaches_the_canvas_and_is_restored() {
    let ops =
        canvas_ops("flowchart TD\n  a[A] --> b[B]\n  b --> c[C]\n  linkStyle 0 opacity:0.5\n");

    assert!(
        ops.contains("SetGlobalAlpha(0.5)"),
        "the declared edge opacity never reached the canvas: {ops}"
    );
    assert_fade_is_restored(&ops, "edge");
}

/// A declared cluster opacity reaches the canvas, and is restored.
///
/// The restore matters more here than anywhere else: the nodes of a subgraph are drawn AFTER their
/// container, so an unrestored cluster fade silently fades its own children.
#[test]
fn a_declared_cluster_opacity_reaches_the_canvas_and_is_restored() {
    let ops = canvas_ops(
        "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one opacity:0.5\n",
    );

    assert!(
        ops.contains("SetGlobalAlpha(0.5)"),
        "the declared cluster opacity never reached the canvas: {ops}"
    );
    assert_fade_is_restored(&ops, "cluster");
}

/// A declared edge-label font size reaches the label.
#[test]
fn a_declared_edge_label_font_size_reaches_the_label() {
    let ops = canvas_ops("flowchart TD\n  a[A] -->|hi| b[B]\n  linkStyle 0 font-size:22px\n");

    let font = font_when_text_drawn(&ops, "hi").expect("the edge label was never drawn");
    assert!(
        font.starts_with("22px "),
        "the declared edge label font size never reached the canvas; drawn in {font:?}"
    );
}

/// CONTROL: the declared edge-label size does not resize another edge's label.
#[test]
fn an_edge_label_font_size_does_not_resize_another_label() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] -->|one| b[B]\n  b -->|two| c[C]\n  linkStyle 0 font-size:22px\n",
    );

    let sized = font_when_text_drawn(&ops, "one").expect("first edge label not drawn");
    let plain = font_when_text_drawn(&ops, "two").expect("second edge label not drawn");

    assert!(
        sized.starts_with("22px "),
        "the declaration did not reach its own edge ({sized:?}), so this control proves nothing"
    );
    assert!(
        !plain.starts_with("22px "),
        "the declared size leaked onto an edge that did not declare it ({plain:?})"
    );
}

/// CONTROL: an undeclared diagram touches neither alpha nor the hoisted label font.
///
/// Guards the common path on both features at once: an undeclared edge must still draw under the
/// hoisted secondary-label font, which is what keeps the invariant `format!` a single call.
#[test]
fn an_undeclared_diagram_touches_neither_channel() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops("flowchart TD\n  a[A] -->|hi| b[B]\n");

    assert!(
        !ops.contains("SetGlobalAlpha"),
        "an unstyled diagram touched globalAlpha: {ops}"
    );

    let font = font_when_text_drawn(&ops, "hi").expect("the edge label was never drawn");
    assert_eq!(
        font,
        format!("{}px {}", config.font_size * 0.85, config.font_family),
        "an unstyled edge label was not drawn in the hoisted secondary font"
    );
}

/// CONTROL: malformed opacity is refused on both surfaces.
#[test]
fn a_malformed_opacity_is_refused_on_both_surfaces() {
    for declared in ["ghostly", "-0.5", "1.5"] {
        for source in [
            format!("flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 opacity:{declared}\n"),
            format!(
                "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one opacity:{declared}\n"
            ),
        ] {
            let ops = canvas_ops(&source);
            assert!(
                !ops.contains("SetGlobalAlpha"),
                "opacity:{declared} was forwarded to the canvas: {ops}"
            );
        }
    }
}
