//! The author's own styling must reach the canvas (bd-lvj3).
//!
//! Measured before the fix: the canvas emitted only 3-4 distinct fills for an ENTIRE diagram,
//! whatever the source declared. Counted at the same time — fm-render-svg reads `inline_style` 30
//! times, `classes` 19 and `style_refs` 11; this crate read all three ZERO times. The canvas path is
//! what fm-wasm ships to a browser, so a user who coloured their diagram saw it uncoloured in the
//! preview and correctly coloured in exported SVG — the two outputs disagreeing about the document.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

/// Fill styles a canvas run set, in order.
fn fill_styles(ops_debug: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ops_debug;
    while let Some(i) = rest.find("SetFillStyle(\"") {
        rest = &rest[i + "SetFillStyle(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// Stroke styles a canvas run set, in order.
fn stroke_styles(ops_debug: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ops_debug;
    while let Some(i) = rest.find("SetStrokeStyle(\"") {
        rest = &rest[i + "SetStrokeStyle(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// A `style` directive reaches the canvas.
#[test]
fn a_style_directive_colours_its_node() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n  style a fill:#ff0000\n");
    let fills = fill_styles(&ops);

    assert!(
        fills.iter().any(|f| f.to_lowercase().contains("ff0000")),
        "the declared fill never reached the canvas: {fills:?}"
    );
}

/// A `classDef` reaches the canvas.
///
/// This is the channel fm-render-svg gets for free by emitting a CSS class and letting the BROWSER
/// cascade it. A canvas has no cascade, so the class has to be resolved in the renderer — which is
/// why the SVG helper could not simply be reused: it returns CSS strings, not colours.
#[test]
fn a_classdef_colours_the_nodes_that_name_it() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] --> b[B]\n  classDef hot fill:#ff0000,stroke:#00ff00\n  class a hot\n",
    );

    assert!(
        fill_styles(&ops).iter().any(|f| f.to_lowercase().contains("ff0000")),
        "the classDef fill never reached the canvas: {:?}",
        fill_styles(&ops)
    );
    assert!(
        stroke_styles(&ops).iter().any(|s| s.to_lowercase().contains("00ff00")),
        "the classDef stroke never reached the canvas: {:?}",
        stroke_styles(&ops)
    );
}

/// CONTROL: an UNSTYLED diagram keeps the theme defaults.
///
/// The resolver returns `None` per channel when nothing was declared, so the caller keeps the config
/// colour rather than being handed one the resolver invented. Without this, a bug that always
/// returned some colour would satisfy the tests above while repainting every diagram.
#[test]
fn an_unstyled_diagram_keeps_the_theme_colours() {
    let ops = canvas_ops("flowchart TD\n  a[A] --> b[B]\n");
    let fills = fill_styles(&ops);

    assert!(!fills.is_empty(), "nothing was filled, so this control proves nothing");
    assert!(
        !fills.iter().any(|f| f.to_lowercase().contains("ff0000")),
        "an unstyled diagram acquired a colour from nowhere: {fills:?}"
    );
    assert!(
        fills.iter().any(|f| f == "#ffffff"),
        "the default node fill is missing: {fills:?}"
    );
}

/// CONTROL: only the NAMED node is styled.
///
/// `class a hot` styles `a` and not `b`. A resolver that ignored the target and applied every
/// declared style to every node would pass the positive tests above and be badly wrong.
#[test]
fn styling_applies_only_to_the_named_node() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] --> b[B]\n  classDef hot fill:#ff0000\n  class a hot\n",
    );
    let fills = fill_styles(&ops);

    let styled = fills.iter().filter(|f| f.to_lowercase().contains("ff0000")).count();
    let default = fills.iter().filter(|f| f.as_str() == "#ffffff").count();

    assert!(styled >= 1, "the styled node lost its colour: {fills:?}");
    assert!(
        default >= 1,
        "the UNstyled node did not keep the default fill, so the style leaked across nodes: \
         {fills:?}"
    );
}
