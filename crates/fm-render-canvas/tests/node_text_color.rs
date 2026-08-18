//! A declared TEXT colour reaches the canvas (bd-lvj3).
//!
//! The fill/stroke half of bd-lvj3 landed earlier: this renderer passed `config.node_fill` and
//! `config.node_stroke` unconditionally, so every declared colour was discarded. The TEXT half was
//! left, and it is the same shape one channel later — every label was drawn in
//! `config.label_color` whatever the author wrote.
//!
//! `style a color:#f00`, a `classDef` carrying `color:`, and an inline style all set the label
//! colour. fm-render-svg honours it by mapping `color` onto the text element's `fill`
//! (`split_style_properties`: `color` is the single TEXT_STYLE_PROPERTIES entry that is RENAMED
//! rather than passed through). A canvas has no cascade, so it must be resolved and handed to
//! `set_fill_style` before the text is drawn.
//!
//! Ops are read from the recorded Debug form, as `node_styling.rs` does — `DrawOperation` is not
//! exported from this crate, so a test cannot name it.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

/// `(drawn text, fill style in force when it was drawn)`, in order.
///
/// The PAIRING is the point: a bare list of fills cannot tell you which colour a given label came
/// out in, and this defect is precisely about one label getting the wrong one.
fn text_fills(source: &str) -> Vec<(String, String)> {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let ops = format!("{:?}", context.operations());

    let mut out = Vec::new();
    let mut current = String::new();
    let mut rest = ops.as_str();
    loop {
        let fill_at = rest.find("SetFillStyle(\"");
        let text_at = rest.find("FillText(\"");
        match (fill_at, text_at) {
            (Some(f), Some(t)) if f < t => {
                rest = &rest[f + "SetFillStyle(\"".len()..];
                if let Some(end) = rest.find('"') {
                    current = rest[..end].to_string();
                    rest = &rest[end..];
                }
            }
            (_, Some(t)) => {
                rest = &rest[t + "FillText(\"".len()..];
                if let Some(end) = rest.find('"') {
                    out.push((rest[..end].to_string(), current.clone()));
                    rest = &rest[end..];
                }
            }
            (Some(f), None) => {
                rest = &rest[f + "SetFillStyle(\"".len()..];
                if let Some(end) = rest.find('"') {
                    current = rest[..end].to_string();
                    rest = &rest[end..];
                }
            }
            (None, None) => break,
        }
    }
    out
}

fn fill_of(fills: &[(String, String)], label: &str) -> String {
    fills
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("{label:?} was never drawn: {fills:?}"))
        .1
        .to_ascii_lowercase()
}

/// A `style` directive naming a colour must colour that node's label.
#[test]
fn a_style_directive_colours_the_node_label() {
    let fills = text_fills("flowchart TD\n  a[Alpha] --> b[Beta]\n  style a color:#ff0000\n");

    assert_eq!(
        fill_of(&fills, "Alpha"),
        "#ff0000",
        "the declared text colour did not reach the label: {fills:?}"
    );
}

/// A `classDef` carrying `color:` is a different channel into the same merge, and must work too.
#[test]
fn a_classdef_colours_the_node_label() {
    let fills = text_fills(
        "flowchart TD\n  a[Alpha] --> b[Beta]\n  classDef loud color:#00ff00\n  class a loud\n",
    );

    assert_eq!(fill_of(&fills, "Alpha"), "#00ff00", "{fills:?}");
}

/// THE DISCRIMINATING CONTROL: an UNSTYLED node in the SAME diagram keeps the theme colour.
///
/// Without it, "colour every label with the first declared colour" — or resolving the style once
/// for the whole diagram rather than per node — passes both tests above.
#[test]
fn an_unstyled_node_in_the_same_diagram_keeps_the_theme_colour() {
    let fills = text_fills("flowchart TD\n  a[Alpha] --> b[Beta]\n  style a color:#ff0000\n");

    assert_ne!(
        fill_of(&fills, "Alpha"),
        fill_of(&fills, "Beta"),
        "styled and unstyled labels came out identical, so the colour is not per-node: {fills:?}"
    );
    assert_eq!(
        fill_of(&fills, "Beta"),
        CanvasRenderConfig::default().label_color.to_ascii_lowercase(),
        "the unstyled label lost the theme default: {fills:?}"
    );
}

/// CONTROL: with NO styling declared, every label keeps the theme colour. This is what stops the
/// resolver leaking an empty or invented colour onto every node.
#[test]
fn an_unstyled_diagram_draws_every_label_in_the_theme_colour() {
    let fills = text_fills("flowchart TD\n  a[Alpha] --> b[Beta]\n");
    let theme = CanvasRenderConfig::default().label_color.to_ascii_lowercase();

    assert!(!fills.is_empty(), "nothing was drawn, so this proves nothing");
    for (text, fill) in &fills {
        assert_eq!(
            fill.to_ascii_lowercase(),
            theme,
            "label {text:?} came out {fill:?} with no styling declared"
        );
    }
}

/// `color:` is a TEXT property: it must not repaint the SHAPE. A resolver that read the wrong key
/// would colour the box instead of the words, and every assertion above would still pass.
#[test]
fn a_text_colour_does_not_repaint_the_shape() {
    let ir = fm_parser::parse("flowchart TD\n  a[Alpha]\n  style a color:#ff0000\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let ops = format!("{:?}", context.operations());

    // The fill in force at each SHAPE fill, in order — same dual-scan shape as `text_fills`, so it
    // advances on every iteration and cannot spin.
    let mut shape_fills = Vec::new();
    let mut current = String::new();
    let mut rest = ops.as_str();
    loop {
        let set_at = rest.find("SetFillStyle(\"");
        let rect_at = rest.find("FillRect(");
        match (set_at, rect_at) {
            (Some(f), Some(r)) if f < r => {
                rest = &rest[f + "SetFillStyle(\"".len()..];
                if let Some(end) = rest.find('"') {
                    current = rest[..end].to_string();
                    rest = &rest[end..];
                } else {
                    break;
                }
            }
            (_, Some(r)) => {
                shape_fills.push(current.clone());
                rest = &rest[r + "FillRect(".len()..];
            }
            (Some(f), None) => {
                rest = &rest[f + "SetFillStyle(\"".len()..];
                if let Some(end) = rest.find('"') {
                    current = rest[..end].to_string();
                    rest = &rest[end..];
                } else {
                    break;
                }
            }
            (None, None) => break,
        }
    }

    // NON-VACUITY: a shape must actually have been filled, or "not repainted" is an empty claim.
    assert!(
        !shape_fills.is_empty(),
        "no shape fill was recorded, so this control proves nothing"
    );
    assert!(
        !shape_fills
            .iter()
            .any(|f| f.to_ascii_lowercase() == "#ff0000"),
        "a TEXT colour was used to fill a shape: {shape_fills:?}"
    );
}
