//! ER cardinality is drawn ONCE — as a marker, not also as text (bd-m2t99; the carrier
//! question it was blocked on is DECIDED in bd-b1sy2: markers on SVG/Canvas, text on terminal).
//!
//! REFERENCE BEHAVIOR, measured with `scripts/headtohead/chromium_text_diff.mjs` against the pinned
//! mermaid 11.15.0 bundle in Chromium 151, on `crates/fm-cli/tests/golden/er_basic.mmd`:
//!
//! ```text
//!   before:  incumbent 13 runs, ours 17 — surplus ["1","1","0..*","1..*"]
//!   after :  AGREE, 13 runs
//! ```
//!
//! mermaid draws no cardinality text at all. It encodes cardinality as crow's-foot markers, which
//! this renderer started drawing in bd-dun16 — at which point the labels became a second copy of
//! information already on the line, and er_basic reached full text parity by removing them.
//!
//! ⚠️ THE ORDER MATTERED AND WAS NOT ARBITRARY. bd-dun16 explicitly refused to delete this text
//! before the markers existed, because until then it was the ONLY carrier the cardinality had.
//! Deleting it first would have made the text oracle green by destroying information.

use fm_core::{DiagramType, IrEdge, IrEdgeExtras, IrNode, MermaidDiagramIr};

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

fn cardinality_texts(svg: &str) -> Vec<String> {
    let needle = "class=\"fm-er-cardinality\">";
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find(needle) {
        rest = &rest[at + needle.len()..];
        if let Some(end) = rest.find('<') {
            out.push(rest[..end].to_string());
        }
    }
    out
}

fn marker_refs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    for needle in ["marker-start=\"", "marker-end=\""] {
        let mut rest = svg;
        while let Some(at) = rest.find(needle) {
            rest = &rest[at + needle.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }
    out
}

/// The four labels er_basic used to draw are gone, and the diagram still says what it said.
///
/// Re-armed by bd-b1sy2: the carrier question is decided (markers on SVG/Canvas, text on the
/// terminal — pinned by `cardinality_is_carried_surface_appropriately` in renderer_agreement.rs,
/// which carries the surface-split for the text-agreement gate rather than weakening it). The
/// terminal kept the text, so the three surfaces still agree on the datum; only the SVG's
/// duplicate channel is gone.
#[test]
fn a_recognised_notation_draws_a_marker_and_no_text() {
    let svg = render("erDiagram\n  A ||--o{ B : places\n");
    assert!(
        cardinality_texts(&svg).is_empty(),
        "the cardinality is drawn twice — as a marker AND as text: {:?}",
        cardinality_texts(&svg)
    );
    // ⚠️ THE CONTROL bd-m2t99 REQUIRES: "text removed" must be distinguishable from "cardinality
    // lost". Asserting only the absence above is satisfied by a renderer that drops the cardinality
    // entirely, which is the outcome bd-dun16 warned against.
    let markers = marker_refs(&svg);
    assert!(
        markers.contains(&"url(#er-onlyOneStart)".to_string())
            && markers.contains(&"url(#er-zeroOrMoreEnd)".to_string()),
        "the text went but no marker carries the cardinality: {markers:?}"
    );
}

/// ⚠️ THE HAND-BUILT IR IS FOR A MEASURED REASON, not for convenience. The parser NORMALISES an
/// unrecognised marker away — `A o--|| B` arrives as notation `"--"` with empty labels, verified by
/// probe — so no parser input can reach this branch. `er_notation` is a public field, so any IR
/// consumer (the WASM API, a downstream tool) can. This is that path.
///
/// bd-b1sy2 removed the text fallback with the rest of the channel: the SVG now draws neither
/// text nor marker for a shape the table does not know — "draw nothing" instead of inventing a
/// cardinality, which is the same no-fallback principle `parse_er_cardinality_forms` documents.
/// (For the PARSER-reachable bare-`o` input, the incumbent draws the raw notation text — that
/// split is bd-5ir5r's.)
#[test]
fn a_side_with_no_marker_shape_draws_neither_text_nor_marker() {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
    ir.nodes.push(IrNode {
        id: "A".to_string(),
        ..IrNode::default()
    });
    ir.nodes.push(IrNode {
        id: "B".to_string(),
        ..IrNode::default()
    });
    ir.edges.push(IrEdge {
        from: fm_core::IrEndpoint::Node(fm_core::IrNodeId(0)),
        to: fm_core::IrEndpoint::Node(fm_core::IrNodeId(1)),
        extras: Some(Box::new(IrEdgeExtras {
            // `o` on both sides: the label table reads it as "0", the shape table knows no such
            // marker and yields None.
            er_notation: Some("o--o".into()),
            ..IrEdgeExtras::default()
        })),
        ..IrEdge::default()
    });

    let svg = fm_render_svg::render_svg(&ir);
    assert!(
        cardinality_texts(&svg).is_empty(),
        "a shape the table does not know must not fall back to text on this surface: {:?}",
        cardinality_texts(&svg)
    );
    assert!(
        !marker_refs(&svg).iter().any(|m| m.contains("er-")),
        "a shape was invented for a marker the table does not know"
    );
}

/// CONTROL: a class diagram's cardinality text is a DIFFERENT channel (`source_cardinality` /
/// `target_cardinality`, drawn by `write_class_cardinality_labels_into`) and must be untouched.
/// mermaid does draw those as text.
#[test]
fn class_diagram_cardinality_text_is_untouched() {
    let svg = render("classDiagram\n  A \"1\" --> \"0..*\" B\n");
    assert!(
        svg.contains("0..*"),
        "a class diagram lost its cardinality labels: {svg}"
    );
}
