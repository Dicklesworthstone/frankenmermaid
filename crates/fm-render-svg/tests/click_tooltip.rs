//! A `click` tooltip must reach the rendered document (bd-bk7h).
//!
//! `click nodeId "url" "some tooltip"` populated `IrNodeInteraction.tooltip` and NOTHING rendered
//! it: fm-render-svg and fm-render-canvas referenced the field zero times, and fm-render-term's
//! only four uses are in `diff.rs` — which reports that a tooltip CHANGED without any backend ever
//! having drawn one. That is the dead-IR-field class this project has already found twice, in
//! bd-jgco (gitGraph branch names) and bd-jerh (ER attribute comments).
//!
//! INCUMBENT: mermaid 11.15.0 does `t.tooltip && n.attr("title", t.tooltip)` — a `title` ATTRIBUTE
//! on the node element, which is what a browser surfaces on hover.

const WITH_TOOLTIP: &str =
    "flowchart TD\n  a[A] --> b[B]\n  click a \"https://example.com\" \"Go to example\"\n";
const WITHOUT_TOOLTIP: &str = "flowchart TD\n  a[A] --> b[B]\n  click a \"https://example.com\"\n";

/// NON-VACUITY: the parser must actually capture the tooltip, or every assertion below would be
/// testing the parser's silence rather than the renderer's output.
#[test]
fn the_parser_captures_the_click_tooltip() {
    let ir = fm_parser::parse(WITH_TOOLTIP).ir;

    let tooltip = ir
        .nodes
        .iter()
        .find_map(fm_core::IrNode::tooltip)
        .map(str::to_string);

    assert_eq!(
        tooltip.as_deref(),
        Some("Go to example"),
        "the parser did not capture the click tooltip, so this file cannot test the renderer"
    );
}

/// The tooltip reaches the SVG as a `title` ATTRIBUTE, mirroring the incumbent.
///
/// ⚠️ ASSERTED AS `title="…"`, NOT as the bare word "title". This file already renders
/// `<title>Node: …</title>` a11y CHILDREN on every node, so a substring test for `title` passes
/// whether or not the tooltip is emitted at all — the same substring trap that let a C4 boundary
/// defect hide behind `renderer_agreement`'s matcher.
#[test]
fn the_tooltip_is_emitted_as_a_title_attribute() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(WITH_TOOLTIP).ir);

    assert!(
        svg.contains("title=\"Go to example\""),
        "the click tooltip never reached the SVG as a title attribute:\n{svg}"
    );
}

/// CONTROL: a node whose `click` declares NO tooltip must gain no tooltip attribute. Without this,
/// emitting a constant or echoing the label would satisfy the test above.
#[test]
fn a_click_without_a_tooltip_emits_no_tooltip_attribute() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(WITHOUT_TOOLTIP).ir);

    assert!(
        !svg.contains("title=\""),
        "a click with no tooltip still emitted a title attribute:\n{svg}"
    );
}

/// CONTROL FOR THE FAST-PATH GATES, and it is the assertion that catches the likely failure mode.
///
/// The emission sits on the decoration path that handles href and callback, which the streaming
/// fast paths refuse. Ten of those gates gained `node.tooltip().is_none()`; if any ONE was missed,
/// a node matching that path keeps its tooltip dropped while the others work. A plain styled node
/// exercises a different fast path from the bare one above, so the two together cover more than one
/// gate — and the href is present in both, since a tooltip only ever arrives with a `click`.
#[test]
fn a_tooltip_survives_on_nodes_that_would_otherwise_take_a_fast_path() {
    for source in [
        // bare node with a link + tooltip
        "flowchart TD\n  a[A]\n  click a \"https://example.com\" \"Tip one\"\n",
        // classed node: a different fast path
        "flowchart TD\n  a[A]\n  classDef big fill:#f00\n  class a big\n  click a \"https://example.com\" \"Tip two\"\n",
        // node carrying an inline style
        "flowchart TD\n  a[A]\n  style a fill:#0f0\n  click a \"https://example.com\" \"Tip three\"\n",
    ] {
        let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
        assert!(
            svg.contains("title=\"Tip"),
            "a tooltip was dropped by a fast path for:\n{source}\n{svg}"
        );
    }
}

/// CONTROL: an ordinary diagram with no `click` at all must be unchanged in this respect — no
/// tooltip attribute anywhere. This is what stops the gate additions from leaking an empty
/// attribute onto every node.
#[test]
fn a_diagram_without_any_click_has_no_tooltip_attribute() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse("flowchart TD\n  a[A] --> b[B]\n").ir);

    assert!(
        !svg.contains("title=\""),
        "a diagram with no click gained a tooltip attribute:\n{svg}"
    );
}
