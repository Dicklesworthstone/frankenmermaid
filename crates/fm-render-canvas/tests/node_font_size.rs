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

/// A declared `font-weight` reaches the canvas (bd-lvj3).
///
/// Confirmed on both sides before implementing: SVG emits `font-weight:bold` for
/// `style a font-weight:bold`, and the canvas drew every label at the theme weight.
#[test]
fn a_declared_font_weight_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-weight:bold\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.starts_with("bold "),
        "the declared font weight never reached the canvas; label drawn in {font:?}"
    );
}

/// Weight alone keeps the THEME size, since a canvas font string cannot say "inherit the size".
#[test]
fn a_weight_without_a_size_keeps_the_theme_size() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-weight:bold\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font,
        format!("bold {}px {}", config.font_size, config.font_family),
        "declaring only a weight changed the size as well"
    );
}

/// Both properties compose into one font string.
#[test]
fn a_weight_and_a_size_compose() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha]\n  style a font-weight:bold,font-size:32px\n",
    );

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font,
        format!("bold 32px {}", config.font_family),
        "the two declarations did not compose into one canvas font string"
    );
}

/// CONTROL, and the one that carries the real risk: a junk WEIGHT must not discard a valid SIZE.
///
/// A canvas given an unparsable font string ignores the WHOLE assignment and keeps the previous
/// font. So passing `font-weight:heavyish` straight through would not merely lose the weight — it
/// would lose the 32px beside it and draw the label in whatever the last draw left behind. The
/// weight is validated against a closed set for exactly this reason, and this asserts the size
/// survives its rejection.
#[test]
fn a_malformed_weight_does_not_discard_a_valid_size() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha]\n  style a font-weight:heavyish,font-size:32px\n",
    );

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font,
        format!("32px {}", config.font_family),
        "a malformed weight took the valid size down with it"
    );
    assert!(
        !font.contains("heavyish"),
        "a malformed weight was forwarded into the canvas font string ({font:?})"
    );
}

/// CONTROL: a malformed weight alone leaves the theme font untouched.
#[test]
fn a_malformed_weight_alone_falls_back_to_the_theme_font() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-weight:heavyish\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font,
        format!("{}px {}", config.font_size, config.font_family),
        "a malformed weight alone did not fall back to the theme font"
    );
}

/// Numeric weights are accepted, since `font-weight: 700` is as ordinary as `bold`.
#[test]
fn a_numeric_font_weight_is_accepted() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-weight:700\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.starts_with("700 "),
        "a numeric font weight was rejected; label drawn in {font:?}"
    );
}

/// A declared `font-style` reaches the canvas (bd-lvj3).
#[test]
fn a_declared_font_style_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-style:italic\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.starts_with("italic "),
        "the declared font style never reached the canvas; label drawn in {font:?}"
    );
}

/// A declared `font-family` reaches the canvas (bd-lvj3).
#[test]
fn a_declared_font_family_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-family:Courier\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.ends_with(" Courier"),
        "the declared font family never reached the canvas; label drawn in {font:?}"
    );
}

/// All four components compose in CSS SHORTHAND ORDER.
///
/// Order is not free: a canvas PARSES this string, and `style weight size family` is the only
/// arrangement it accepts. Getting it wrong makes the assignment unparsable, which is silently
/// equivalent to setting no font at all — the label then draws in whatever the last draw left.
#[test]
fn every_font_component_composes_in_shorthand_order() {
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha]\n  style a font-style:italic,font-weight:bold,font-size:32px,font-family:Courier\n",
    );

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font, "italic bold 32px Courier",
        "the four declarations did not compose in CSS shorthand order"
    );
}

/// A quoted multi-word family stack survives, because that is what real declarations look like.
#[test]
fn a_quoted_font_family_stack_is_accepted() {
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha]\n  style a font-family:'Fira Code', monospace\n",
    );

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert!(
        font.contains("Fira Code") && font.contains("monospace"),
        "a quoted multi-word family stack was rejected; label drawn in {font:?}"
    );
}

/// CONTROL: a family carrying font-string punctuation is REFUSED.
///
/// The family lands in a string the canvas PARSES, so a stray brace or backslash does not merely
/// look wrong — it makes the WHOLE assignment unparsable and the label is drawn in whatever font
/// the last draw left behind, silently and position-dependently. Same reasoning as the paint
/// sanitiser, one grammar over.
#[test]
fn a_font_family_with_punctuation_is_refused() {
    let config = CanvasRenderConfig::default();

    for declared in ["Courier{}", "Courier<b>", "Courier()", "Courier!"] {
        let source = format!("flowchart TD\n  a[Alpha]\n  style a font-family:{declared}\n");
        let ops = canvas_ops(&source);
        let font = font_when_text_drawn(&ops, "Alpha")
            .unwrap_or_else(|| panic!("{declared}: the label was never drawn"));

        assert_eq!(
            font,
            format!("{}px {}", config.font_size, config.font_family),
            "font-family:{declared} was not refused"
        );
    }
}

/// A SEMICOLON CANNOT REACH THIS SANITISER, and my first draft of the test above wrongly assumed
/// it could.
///
/// `;` is the PROPERTY SEPARATOR in a mermaid style string, so `parse_style_string` consumes it
/// before a value exists: `font-family:Courier;` yields the family `Courier`, an ordinary
/// declaration that is correctly accepted. Measured, not assumed — the earlier version of this
/// file asserted a theme fallback here and failed with `left: "14px Courier"`.
///
/// Worth pinning rather than deleting, because it records that the most obvious injection
/// character for a canvas font string is structurally unable to get there. If the style grammar
/// ever changes so a raw `;` survives into a value, this flips to failing and says so.
#[test]
fn a_semicolon_is_consumed_by_the_style_grammar_not_by_the_sanitiser() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops("flowchart TD\n  a[Alpha]\n  style a font-family:Courier;\n");

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font,
        format!("{}px Courier", config.font_size),
        "a trailing semicolon changed how the family was read"
    );
    assert!(
        !font.contains(';'),
        "a semicolon reached the canvas font string ({font:?})"
    );
}

/// CONTROL: a malformed STYLE does not discard a valid size beside it.
#[test]
fn a_malformed_font_style_does_not_discard_a_valid_size() {
    let config = CanvasRenderConfig::default();
    let ops = canvas_ops(
        "flowchart TD\n  a[Alpha]\n  style a font-style:slanty,font-size:32px\n",
    );

    let font = font_when_text_drawn(&ops, "Alpha").expect("the node label was never drawn");
    assert_eq!(
        font,
        format!("32px {}", config.font_family),
        "a malformed font style took the valid size down with it"
    );
}
