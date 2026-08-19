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

/// A flowchart `subgraph` + `style one fill:#f00` NOW COLOURS THE CLUSTER (bd-xfmm).
///
/// This test used to assert the opposite. It was written as a deliberate stale-detector while the
/// gap was three layers upstream of this crate — the parser resolved a `style` target only through
/// `node_id_by_key`, a subgraph key is not in the node index, and `IrStyleTarget` had no `Cluster`
/// variant to record it in — with the instruction to INVERT it rather than delete it once the
/// colour began arriving. That is what happened on the first build after the freeze, so the
/// assertion is inverted here.
///
/// What closed it: `IrStyleTarget::Cluster`, plus `cluster_index_by_key` learning that a flowchart
/// subgraph is keyed COMPOSITELY (`one@title:One`) and must be resolved through `IrSubgraph::key`.
#[test]
fn a_styled_subgraph_colours_the_cluster() {
    let ops = canvas_ops(
        "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one fill:#ff0000\n",
    );
    let fills = fill_styles(&ops);

    assert!(
        fills.iter().any(|f| f.to_lowercase().contains("ff0000")),
        "the declared subgraph colour stopped reaching the canvas: {fills:?}"
    );
    // NON-VACUITY: other fills must still be present, or "the colour is there" could hold for a
    // render that drew nothing but the cluster.
    assert!(
        fills.len() > 1,
        "only one fill was recorded, so this assertion proves little: {fills:?}"
    );
}

/// The `--` concurrency separator is DRAWN, and drawn dashed (bd-dgnm4).
///
/// `state Big { A --> B  --  C --> D }` declares two parallel regions. fm-render-svg drew the
/// boundary and this surface drew nothing, so the regions ran together into one box.
///
/// Asserted on the DASH PATTERN, which is what distinguishes a region boundary from an ordinary
/// cluster edge — and which was unassertable until `MockCanvas2dContext::set_line_dash` stopped
/// being a silent no-op. Before that, the strongest available claim was "some line was stroked",
/// which passes whether or not the feature works.
#[test]
fn a_state_concurrency_divider_is_drawn_dashed() {
    let ops = canvas_ops("stateDiagram-v2\n  state Big {\n    A --> B\n    --\n    C --> D\n  }\n");

    assert!(
        ops.contains("SetLineDash([6.0, 4.0])"),
        "no dashed stroke was recorded, so the region divider was not drawn: {ops}"
    );
}

/// CONTROL: the same composite state WITHOUT the separator draws no dashed divider.
///
/// This is the assertion that makes the one above mean something. A state diagram still draws a
/// cluster box, edges and labels either way, so "a line was stroked" cannot separate the two —
/// only the dash can, and only if it is absent when the syntax is absent.
#[test]
fn a_composite_state_without_a_separator_draws_no_divider() {
    let ops = canvas_ops("stateDiagram-v2\n  state Big {\n    A --> B\n    C --> D\n  }\n");

    assert!(
        !ops.contains("SetLineDash([6.0, 4.0])"),
        "a dashed divider was drawn for a state with no `--` separator: {ops}"
    );
}

/// ER cardinality reaches the canvas (bd-2h3pp).
///
/// `}o--o|` declares "0..*" and "0..1". fm-render-svg drew both and this surface drew neither, so an
/// ER relationship arrived as a bare line with its cardinality missing — the bd-039t family, where
/// one renderer omits content another draws.
///
/// Asserted through `FillText(` rather than a coordinate: the op NAME is the stable part of this
/// Debug stream, and the two canvas tests I wrote against a guessed op earlier today both failed
/// because they named one the renderer never emits. Position is deliberately not asserted — the
/// labels are inset along the edge, and pinning where would fail on any future routing change
/// without saying anything about whether the cardinality is drawn.
#[test]
fn er_cardinality_reaches_the_canvas() {
    let ops = canvas_ops("erDiagram\n  CUSTOMER }o--o| ORDER : places\n");

    assert!(
        ops.contains("FillText(\"CUSTOMER\""),
        "the entities were not drawn, so the assertions below prove nothing: {ops}"
    );
    assert!(
        ops.contains("FillText(\"0..*\""),
        "the source cardinality never reached the canvas: {ops}"
    );
    assert!(
        ops.contains("FillText(\"0..1\""),
        "the target cardinality never reached the canvas: {ops}"
    );
}

/// CONTROL: a bare `--` relation declares no cardinality and must draw none.
///
/// The shared mapping returns `""` for a connector with no markers and the placement closure skips
/// empty text. Without this, an implementation that emitted a default marker — or that drew the
/// notation string itself — would pass the case above.
#[test]
fn a_bare_er_relation_draws_no_cardinality_on_the_canvas() {
    let ops = canvas_ops("erDiagram\n  CUSTOMER -- ORDER : places\n");

    assert!(
        ops.contains("FillText(\"CUSTOMER\""),
        "the entities were not drawn, so this control proves nothing: {ops}"
    );
    assert!(
        !ops.contains("FillText(\"0.."),
        "a relation with no declared cardinality drew one anyway: {ops}"
    );
}

/// A declared node BORDER WIDTH reaches the canvas (bd-lvj3).
///
/// The edge half of this bead has read `stroke-width` off its merge chain since it landed; the node
/// half never did, so every node border was drawn at `config.node_stroke_width` however the author
/// declared it. Same three channels, same merge order, one property later.
#[test]
fn a_declared_node_stroke_width_reaches_the_canvas() {
    let ops = canvas_ops("flowchart TD\n  a[A]\n  style a stroke-width:4px\n");

    assert!(
        ops.contains("SetLineWidth(4.0)"),
        "the declared border width never reached the canvas: {ops}"
    );
}

/// The same declaration through `classDef`, which is the channel a canvas cannot get for free.
///
/// fm-render-svg emits a CSS class and lets the BROWSER cascade it; a canvas has no cascade, so a
/// `classDef` width that works in SVG is silently dropped here unless it is resolved explicitly.
#[test]
fn a_classdef_stroke_width_reaches_the_canvas() {
    let ops = canvas_ops(
        "flowchart TD\n  a[A]\n  classDef thick stroke-width:6\n  class a thick\n",
    );

    assert!(
        ops.contains("SetLineWidth(6.0)"),
        "a classDef border width was dropped: {ops}"
    );
}

/// CONTROL: a malformed width is REFUSED, and the theme default stands.
///
/// This is the one that matters. A canvas silently ignores a draw call carrying NaN or a negative
/// line width, so forwarding a junk value would make the border simply not be there, with nothing
/// in the output to say why -- strictly worse than ignoring the declaration. Both spellings are
/// checked because they fail differently: `wide` does not parse at all, `-2px` parses fine and is
/// then rejected on sign.
#[test]
fn a_malformed_node_stroke_width_falls_back_to_the_theme() {
    for source in [
        "flowchart TD\n  a[A]\n  style a stroke-width:wide\n",
        "flowchart TD\n  a[A]\n  style a stroke-width:-2px\n",
    ] {
        let ops = canvas_ops(source);
        assert!(
            ops.contains("SetLineWidth(1.5)"),
            "{source:?} did not fall back to the theme width: {ops}"
        );
        assert!(
            !ops.contains("SetLineWidth(-"),
            "{source:?} forwarded a negative width to the canvas: {ops}"
        );
        assert!(
            !ops.contains("NaN"),
            "{source:?} forwarded NaN to the canvas: {ops}"
        );
    }
}
