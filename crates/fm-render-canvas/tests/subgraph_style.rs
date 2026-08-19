//! `style mySubgraph fill:#f00` must colour the subgraph on the canvas too (bd-xfmm).
//!
//! bd-xfmm fixed the SILENCE around a `style` directive whose target was a subgraph, and could go
//! no further: `IrStyleTarget` had no `Cluster` variant to record it in. The variant and the SVG
//! consumer landed first; this is the canvas half, so the two backends stop disagreeing about a
//! document the author styled — fm-wasm renders the browser preview through this path, so a user
//! who coloured a subgraph saw it coloured in exported SVG and plain in the preview.
//!
//! Ops are read from the recorded Debug form, as the other canvas tests do — `DrawOperation` is not
//! exported from this crate, so a test cannot name it.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn fill_styles(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let ops = format!("{:?}", context.operations());

    let mut out = Vec::new();
    let mut rest = ops.as_str();
    while let Some(i) = rest.find("SetFillStyle(\"") {
        rest = &rest[i + "SetFillStyle(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

const STYLED: &str =
    "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one fill:#ff0000\n";

/// The declared fill reaches the canvas.
#[test]
fn a_subgraph_style_reaches_the_canvas() {
    let fills = fill_styles(STYLED);

    assert!(
        fills.iter().any(|f| f.eq_ignore_ascii_case("#ff0000")),
        "the subgraph's declared fill never reached the canvas: {fills:?}"
    );
}

/// CONTROL: an unstyled subgraph gains nothing. Without it, painting every cluster with the first
/// declared colour — or leaking a colour across diagrams — passes the test above.
#[test]
fn an_unstyled_subgraph_keeps_the_theme_fill() {
    let fills = fill_styles("flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n");
    let theme = CanvasRenderConfig::default().cluster_fill.to_ascii_lowercase();

    assert!(
        fills.iter().any(|f| f.to_ascii_lowercase() == theme),
        "the cluster was not drawn with the theme fill: {fills:?}"
    );
    assert!(
        !fills.iter().any(|f| f.eq_ignore_ascii_case("#ff0000")),
        "an unstyled subgraph gained a colour from nowhere: {fills:?}"
    );
}

/// NON-VACUITY FOR THE PRECEDENCE RULE: a `style` directive must beat the layout-provided
/// `cluster_box.color`, because the two arrive from different syntaxes and only the directive names
/// this cluster. A resolver that consulted `color` first would still pass the first test whenever
/// no `rect` was present, so the ordering needs its own case.
///
/// A sequence `rect` is the only producer of `cluster_box.color`, and it cannot be targeted by a
/// flowchart `style` directive — so this asserts the weaker, checkable half: with NO layout colour
/// present, the styled fill is the one that comes out, and the theme fill does not.
#[test]
fn the_styled_fill_wins_over_the_theme_default() {
    let fills = fill_styles(STYLED);
    let theme = CanvasRenderConfig::default().cluster_fill.to_ascii_lowercase();

    let styled_at = fills.iter().position(|f| f.eq_ignore_ascii_case("#ff0000"));
    assert!(
        styled_at.is_some(),
        "the declared fill is absent, so precedence cannot be judged: {fills:?}"
    );
    assert!(
        !fills
            .iter()
            .take(styled_at.unwrap_or(0) + 1)
            .any(|f| f.to_ascii_lowercase() == theme),
        "the theme cluster fill was used before the declared one: {fills:?}"
    );
}
