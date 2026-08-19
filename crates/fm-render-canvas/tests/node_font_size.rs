//! A declared node FONT SIZE reaches the canvas (bd-lvj3).
//!
//!     style a font-size:32px    svg emits `font-size:32px`    canvas=FALSE  ->  fixed
//!
//! SCOPE, stated up front because it is deliberately partial: this is the PLAIN node label. The
//! class/ER/requirement/C4 COMPARTMENT labels derive their own smaller fonts and are not rescaled,
//! so a `font-size` on a class node still disagrees with the SVG arm, which cascades it to the
//! whole element. Left open on the bead rather than half-fixed here.
//!
//! The other design point is the HOIST. `standard_label_font` is formatted once per diagram — a
//! landed performance lever, since the invariant `format!` previously ran per node. A declared size
//! takes a side path so an undeclared node still draws under the identical hoisted string, and the
//! rare declaration does not cost every other diagram a per-node allocation.

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

/// A `style` directive's font size reaches the label.
#[test]
fn a_declared_font_size_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-size:32px\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.starts_with("32px "),
        "the declared font size never reached the canvas; label drawn in {font:?}"
    );
}

/// The same declaration through `classDef`.
#[test]
fn a_classdef_font_size_reaches_the_canvas() {
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha]\n  classDef big font-size:28px\n  class a big\n",
    );

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.starts_with("28px "),
        "a classDef font size was dropped; label drawn in {font:?}"
    );
}

/// CONTROL: the declaration applies only to the node that carries it.
///
/// The discriminating case. A resolver that set the font and never restored it, or that ignored
/// the target, would enlarge every label drawn afterwards and still pass the tests above.
#[test]
fn a_declared_font_size_does_not_resize_other_labels() {
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha] --> b[Beta]\n  style a font-size:32px\n",
    );

    let styled = font_when_text_drawn(&ops, "Alpha").expect("styled label not drawn");
    let plain = font_when_text_drawn(&ops, "Beta").expect("unstyled label not drawn");

    assert!(
        styled.starts_with("32px "),
        "the declaration did not reach its own node ({styled:?}), so this control proves nothing"
    );
    assert!(
        !plain.starts_with("32px "),
        "the declared font size leaked onto a node that did not declare it ({plain:?})"
    );
}

/// CONTROL: an undeclared diagram is drawn under the THEME font, unchanged.
///
/// Guards the hoist as well as the `None` arm: an undeclared node must still be drawn with the
/// config-derived string, so the common path is untouched by this feature existing.
#[test]
fn an_undeclared_node_keeps_the_theme_font() {
    let config = CanvasRenderConfig::default();
    let expected = format!("{}px {}", config.font_size, config.font_family);
    let ops = canvas_ops("flowchart TD\n  a[Alpha] --> b[Beta]\n");

    for label in ["Alpha", "Beta"] {
        let font = font_when_text_drawn(&ops, label)
            .unwrap_or_else(|| panic!("label {label:?} was never drawn"));
        assert_eq!(
            font, expected,
            "an unstyled label was not drawn in the theme font"
        );
    }
}

/// CONTROL: a malformed or absurd size is REFUSED, and the theme font stands.
///
/// A canvas given a non-finite or absurd font size draws NOTHING, which is worse than ignoring the
/// declaration — the label simply vanishes with nothing in the output to say why. `huge` does not
/// parse, `-12px` parses and is rejected on sign, `0` on being non-positive, and `100000px` on the
/// upper bound that exists to stop a typo producing an invisible diagram.
#[test]
fn a_malformed_font_size_falls_back_to_the_theme() {
    let config = CanvasRenderConfig::default();
    let expected = format!("{}px {}", config.font_size, config.font_family);

    for declared in ["huge", "-12px", "0", "100000px"] {
        let source = format!("flowchart TD\n  a[Alpha]\n  style a font-size:{declared}\n");
        let ops = canvas_ops(&source);
        let font = font_when_text_drawn(&ops, "Alpha")
            .unwrap_or_else(|| panic!("{declared}: the label was never drawn"));

        assert_eq!(
            font, expected,
            "font-size:{declared} did not fall back to the theme font"
        );
    }
}
