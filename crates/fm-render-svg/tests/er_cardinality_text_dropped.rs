//! ER cardinality is drawn ONCE — as a marker, not also as text (bd-m2t99).
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
/// ⚠️ IGNORED BECAUSE THE CHANGE IT PINS WAS REVERTED, NOT BECAUSE IT IS FLAKY. Suppressing the text
/// measured green against the incumbent (chromium_text_diff.mjs er_basic: AGREE, 13 runs) and was
/// reverted anyway: it breaks `the_three_renderers_agree_on_declared_text`, because the terminal is
/// a character grid that can never carry a marker, so the three surfaces stop agreeing on this datum
/// permanently. That gate is correct and editing it to pass would be weakening a gate to land a
/// change. Un-ignore this when bd-m2t99's carrier question is decided.
#[ignore = "bd-m2t99: blocked on whether one datum may use different carriers per surface"]
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

/// ⚠️ THE FALLBACK, AND WHY THIS IS NOT AN UNCONDITIONAL DELETION.
///
/// The label mapping and the shape mapping do not cover the same inputs, deliberately:
/// `parse_er_cardinality` degrades an unrecognised marker containing `{` to `*`, while
/// `parse_er_cardinality_forms` has no fallback arm at all, because there is no "approximately a
/// crow's foot". A side with a label but no shape therefore draws no marker, and dropping its text
/// too would remove the cardinality from the document entirely.
///
/// ⚠️ THE IR IS HAND-BUILT HERE FOR A MEASURED REASON, not for convenience. The parser NORMALISES an
/// unrecognised marker away — `A o--|| B` arrives as notation `"--"` with empty labels, verified by
/// probe — so no parser input can reach this branch. `er_notation` is a public field, so any IR
/// consumer (the WASM API, a downstream tool) can. This is that path.
#[test]
fn a_side_with_no_marker_shape_keeps_its_text() {
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
    assert_eq!(
        cardinality_texts(&svg),
        vec!["0".to_string(), "0".to_string()],
        "a side whose marker shape is unknown lost its cardinality entirely"
    );
    assert!(
        !marker_refs(&svg).iter().any(|m| m.contains("er-")),
        "a shape was invented for a marker the table does not know"
    );
}

/// NON-VACUITY: the reader finds text when text is drawn. A helper that always returned an empty
/// vector would satisfy the first test and prove nothing.
#[test]
fn the_reader_actually_finds_cardinality_text() {
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
            er_notation: Some("o--o".into()),
            ..IrEdgeExtras::default()
        })),
        ..IrEdge::default()
    });
    assert!(
        !cardinality_texts(&fm_render_svg::render_svg(&ir)).is_empty(),
        "the reader found no cardinality text even where it is drawn"
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
