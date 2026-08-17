//! Cluster and sequence-rect backgrounds must honour a declared colour (bd-lvj3).
//!
//! Final row of that bead's measured table:
//!
//!     seq_rect_color   rect rgb(255,0,0)   svg=true   canvas=FALSE
//!
//! `draw_sequence_fragments` hardcoded `rgba(226,232,240,0.2)` while `fragment.color` sat unread on
//! the very struct the loop iterates, and `draw_clusters` did the same with `config.cluster_fill`.
//!
//! These live in their own file rather than in `node_styling.rs` for a boring reason worth recording:
//! that file was under another agent's exclusive lease when the fix landed, and the pre-commit guard
//! blocks a commit touching it. Landing the implementation without its gate was the worse option, so
//! the gate went somewhere the guard permits. They can be folded back together later; nothing here
//! depends on staying separate.

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

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// A sequence `rect rgb(...)` background reaches the canvas.
#[test]
fn a_sequence_rect_colour_reaches_the_canvas() {
    let ops = canvas_ops(
        "sequenceDiagram\n  participant A\n  participant B\n  rect rgb(255,0,0)\n  A->>B: hi\n  end\n",
    );
    let fills = fill_styles(&ops);

    assert!(
        fills.iter().any(|f| f.replace(' ', "").contains("rgb(255,0,0)")),
        "the declared rect colour never reached the canvas: {fills:?}"
    );
}

/// CONTROL: an uncoloured cluster keeps the theme background.
///
/// Guards the `None` arm. A renderer that always produced some colour would satisfy the test above
/// while repainting every subgraph in every diagram.
#[test]
fn an_uncoloured_cluster_keeps_the_theme_background() {
    let ops = canvas_ops("flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n");
    let fills = fill_styles(&ops);

    assert!(!fills.is_empty(), "nothing was filled, so this control proves nothing");
    assert!(
        fills.iter().any(|f| f.starts_with("rgba(226,232,240")),
        "the default cluster fill is missing: {fills:?}"
    );
}

/// CONTROL: a malformed colour is REFUSED, not forwarded.
///
/// This is the one that matters for a canvas. A browser ignores an unparsable `fillStyle` and keeps
/// the PREVIOUS colour, so forwarding junk would paint the shape with whatever was drawn last — a
/// silent, position-dependent wrong colour. Falling back to the theme is the visible failure.
#[test]
fn a_malformed_cluster_colour_falls_back_to_the_theme() {
    let ops = canvas_ops(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one fill:not a colour;\n",
    );
    let fills = fill_styles(&ops);

    assert!(
        !fills.iter().any(|f| f.contains(';')),
        "a malformed colour was forwarded to the canvas verbatim: {fills:?}"
    );
    assert!(
        fills.iter().any(|f| f.starts_with("rgba(226,232,240")),
        "the theme fallback did not apply: {fills:?}"
    );
}

/// NOT TESTED HERE, deliberately: a flowchart `subgraph` + `style one fill:#f00` does not colour,
/// and the cause is three layers upstream of this crate. The parser resolves a `style` target only
/// via `node_id_by_key` (`mermaid_parser.rs:11521`), a subgraph key is not in the node index, and
/// `IrStyleTarget` has no `Cluster` variant to record it in — so the colour is destroyed before any
/// renderer sees it, and fm-render-svg is equally blind.
///
/// Filed as bd-xfmm with the full chain in `docs/parked-levers/bd_xfmm_cluster_style_channel.md`.
/// A renderer test for it would fail for a reason in another crate, which is a broken gate rather
/// than a reproducer.
#[test]
fn a_styled_subgraph_is_a_known_upstream_gap_not_a_renderer_bug() {
    let ops = canvas_ops(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one fill:#ff0000\n",
    );
    let fills = fill_styles(&ops);

    // Asserting the CURRENT truth, so this test starts failing the moment bd-xfmm is fixed and the
    // colour begins arriving — at which point the assertion below should be inverted, not deleted.
    assert!(
        !fills.iter().any(|f| f.to_lowercase().contains("ff0000")),
        "bd-xfmm appears to be FIXED — a subgraph colour now reaches the canvas. Invert this \
         assertion and move the case into the positive set: {fills:?}"
    );
}
