//! A declared CLUSTER LABEL colour reaches the canvas, and no label paint is forwarded unchecked
//! (bd-lvj3).
//!
//! Two related things, both measured against the SVG arm:
//!
//!   * `style one color:#ff00ff` on a subgraph — SVG emits it, the canvas drew every subgraph
//!     title in `config.label_color` regardless. The third surface to learn `color`, after nodes
//!     and edges.
//!   * The node and edge LABEL paths forwarded a declared `color` to `set_fill_style` WITHOUT
//!     sanitising it, while the cluster SHAPE path has sanitised since it landed. That asymmetry
//!     matters more than it looks: a canvas silently IGNORES an unparsable `fillStyle` and keeps
//!     the PREVIOUS colour, so junk paints the text in whatever was drawn last — a
//!     position-dependent wrong colour instead of a visible failure.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// The fill in force when `needle` was drawn, by replaying the op stream in order.
fn fill_when_text_drawn(ops_debug: &str, needle: &str) -> Option<String> {
    let mut current: Option<String> = None;
    let mut rest = ops_debug;
    loop {
        let next_fill = rest.find("SetFillStyle(\"");
        let next_text = rest.find(&format!("FillText({needle:?}"));
        match (next_fill, next_text) {
            (_, None) => return None,
            (Some(f), Some(t)) if f < t => {
                let after = &rest[f + "SetFillStyle(\"".len()..];
                let end = after.find('"')?;
                current = Some(after[..end].to_string());
                rest = &after[end..];
            }
            (_, Some(_)) => return current,
        }
    }
}

const STYLED_CLUSTER: &str =
    "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one color:#ff00ff\n";

/// A subgraph's declared label colour reaches its title.
#[test]
fn a_declared_cluster_label_colour_reaches_the_canvas() {
    let ops = canvas_ops(STYLED_CLUSTER);

    let fill = fill_when_text_drawn(&ops, "One")
        .expect("the cluster title was never drawn, so this proves nothing");
    assert!(
        fill.to_lowercase().contains("ff00ff"),
        "the declared cluster label colour never reached the canvas; title drawn in {fill:?}"
    );
}

/// CONTROL: the cluster's label colour must not repaint the nodes inside it.
///
/// `color` on a subgraph targets the subgraph's own title. A resolver that set the fill and never
/// restored it would colour every node label drawn afterwards, and the test above would still pass.
#[test]
fn a_cluster_label_colour_does_not_repaint_its_children() {
    let ops = canvas_ops(STYLED_CLUSTER);

    let child = fill_when_text_drawn(&ops, "Alpha").expect("the child node label was never drawn");
    assert!(
        !child.to_lowercase().contains("ff00ff"),
        "the cluster's label colour leaked onto a node inside it ({child:?})"
    );
}

/// CONTROL: an unstyled subgraph keeps the theme label colour.
#[test]
fn an_unstyled_cluster_title_keeps_the_theme_colour() {
    let ops = canvas_ops("flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n");

    let fill = fill_when_text_drawn(&ops, "One").expect("the cluster title was never drawn");
    assert_eq!(
        fill.to_lowercase(),
        CanvasRenderConfig::default().label_color.to_lowercase(),
        "an unstyled cluster title was not drawn in the theme colour"
    );
}

/// A malformed label colour is REFUSED on every surface that accepts `color`.
///
/// The one that matters, and the reason this file covers all three surfaces rather than only the
/// cluster it adds. A canvas ignores an unparsable `fillStyle` and keeps the previous colour, so a
/// forwarded `not a colour;` would paint the text in whatever was drawn last — silently, and
/// differently depending on draw order. Falling back to the theme is the visible failure.
#[test]
fn a_malformed_label_colour_is_refused_on_every_surface() {
    let theme = CanvasRenderConfig::default().label_color.to_lowercase();

    let cases = [
        (
            "node",
            "flowchart TD\n  a[Alpha]\n  style a color:not a colour;\n",
            "Alpha",
        ),
        (
            "edge",
            "flowchart TD\n  a[A] -->|hi| b[B]\n  linkStyle 0 color:not a colour;\n",
            "hi",
        ),
        (
            "cluster",
            "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one color:not a colour;\n",
            "One",
        ),
    ];

    for (surface, source, text) in cases {
        let ops = canvas_ops(source);
        let fill = fill_when_text_drawn(&ops, text)
            .unwrap_or_else(|| panic!("{surface}: the text {text:?} was never drawn"));

        assert!(
            !fill.contains(';'),
            "{surface}: a malformed colour was forwarded to the canvas verbatim ({fill:?}); a \
             canvas ignores it and keeps the PREVIOUS colour, so the text is painted whatever was \
             drawn last"
        );
        assert_eq!(
            fill.to_lowercase(),
            theme,
            "{surface}: a malformed colour did not fall back to the theme"
        );
    }
}
