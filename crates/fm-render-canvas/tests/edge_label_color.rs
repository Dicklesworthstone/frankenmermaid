//! A declared EDGE LABEL colour reaches the canvas (bd-lvj3).
//!
//! The last of the styling channels this bead measured as missing. `linkStyle 0 color:#ff00ff`
//! sets the colour of the edge's LABEL — `stroke` is the line — and the canvas drew every edge
//! label in `config.label_color` regardless. Measured before the fix, SVG as control:
//!
//!     linkStyle 0 color:#ff00ff    svg emits ff00ff    canvas=FALSE
//!
//! These pair each drawn text with the fill IN FORCE when it was drawn, rather than scanning a
//! bare list of fills. A list cannot say which text got which colour, and "which text got which
//! colour" is the entire defect: a renderer that coloured the NODE labels with the edge's
//! declaration would satisfy any assertion that merely looked for the colour somewhere.

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

const LABELLED: &str = "flowchart TD\n  a[Alpha] -->|hi| b[Beta]\n";

/// An indexed `linkStyle` colours its edge's label.
#[test]
fn a_linkstyle_colour_reaches_the_edge_label() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha] -->|hi| b[Beta]\n  linkStyle 0 color:#ff00ff\n");

    let fill = fill_when_text_drawn(&ops, "hi")
        .expect("the edge label was never drawn, so this proves nothing");
    assert!(
        fill.to_lowercase().contains("ff00ff"),
        "the declared edge label colour never reached the canvas; label drawn in {fill:?}"
    );
}

/// `linkStyle default` colours every edge's label.
#[test]
fn a_linkstyle_default_colour_reaches_every_edge_label() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] -->|one| b[B]\n  b -->|two| c[C]\n  linkStyle default color:#ff00ff\n",
    );

    for label in ["one", "two"] {
        let fill = fill_when_text_drawn(&ops, label)
            .unwrap_or_else(|| panic!("edge label {label:?} was never drawn"));
        assert!(
            fill.to_lowercase().contains("ff00ff"),
            "edge label {label:?} was drawn in {fill:?}, not the declared default"
        );
    }
}

/// CONTROL: an indexed declaration does not leak to another edge's label.
///
/// The discriminating case. A renderer that applied the first declaration it found to every label
/// would pass both tests above.
#[test]
fn an_indexed_linkstyle_colour_does_not_leak_to_another_label() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A] -->|one| b[B]\n  b -->|two| c[C]\n  linkStyle 0 color:#ff00ff\n",
    );

    let coloured = fill_when_text_drawn(&ops, "one").expect("first edge label not drawn");
    let untouched = fill_when_text_drawn(&ops, "two").expect("second edge label not drawn");

    assert!(
        coloured.to_lowercase().contains("ff00ff"),
        "the declared colour did not reach its own edge label ({coloured:?}), so the leak check \
         below proves nothing"
    );
    assert!(
        !untouched.to_lowercase().contains("ff00ff"),
        "an indexed linkStyle colour leaked onto an edge it does not target ({untouched:?})"
    );
}

/// CONTROL: an edge label colour must not repaint the NODE labels.
///
/// `color` on an edge targets that edge's own text. A resolver that set the fill and never
/// restored it — or that applied the edge merge to nodes — would colour the node labels too, and
/// every assertion above would still pass.
#[test]
fn an_edge_label_colour_does_not_repaint_node_labels() {
    let ops = canvas_ops("flowchart TD\n  a[Alpha] -->|hi| b[Beta]\n  linkStyle 0 color:#ff00ff\n");

    for node_label in ["Alpha", "Beta"] {
        let fill = fill_when_text_drawn(&ops, node_label)
            .unwrap_or_else(|| panic!("node label {node_label:?} was never drawn"));
        assert!(
            !fill.to_lowercase().contains("ff00ff"),
            "node label {node_label:?} was repainted in the edge's declared colour ({fill:?})"
        );
    }
}

/// CONTROL: an undeclared edge label keeps the theme colour.
///
/// Guards the `None` arm against a resolver that invents a colour when nothing was declared.
#[test]
fn an_undeclared_edge_label_keeps_the_theme_colour() {
    let ops = canvas_ops(LABELLED);

    let fill = fill_when_text_drawn(&ops, "hi").expect("the edge label was never drawn");
    assert_eq!(
        fill.to_lowercase(),
        CanvasRenderConfig::default().label_color.to_lowercase(),
        "an unstyled edge label was not drawn in the theme colour"
    );
}

/// CONTROL: the instrument itself works.
///
/// `fill_when_text_drawn` replays the op stream, and every test above would report a false PASS if
/// it silently returned the wrong fill. This pins it against a known-different pair: with no
/// styling at all, the node and edge labels are both drawn in the theme colour, and asking for a
/// text that was never drawn must return `None` rather than the last fill seen.
#[test]
fn the_fill_probe_reports_what_was_actually_in_force() {
    let ops = canvas_ops(LABELLED);

    assert_eq!(
        fill_when_text_drawn(&ops, "hi"),
        fill_when_text_drawn(&ops, "Alpha"),
        "unstyled edge and node labels should share the theme colour"
    );
    assert_eq!(
        fill_when_text_drawn(&ops, "NoSuchLabelAnywhere"),
        None,
        "the probe invented a fill for text that was never drawn"
    );
}
