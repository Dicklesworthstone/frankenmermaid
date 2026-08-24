//! `accTitle:` / `accDescr:` must never become a drawn shape (bd-yfcfv).
//!
//! Both are valid in EVERY mermaid diagram, and the pinned incumbent's grammar accepts them in all
//! three types exercised here. This repo already had the predicate for it —
//! `is_accessibility_directive_statement`, added by bd-7oyz — and four sibling parsers already paired
//! it with the style predicate at the same call site. Three did not: gantt, pie and quadrant kept
//! only the style half, so the accessibility directive fell through to the item parser and was
//! interned as a visible chart item. A pie chart rendered a SLICE captioned `accTitle`, in the wedge
//! and again in the legend.
//!
//! Worst instance of the phantom family (bd-871ka, bd-xfmm, bd-yrxu, bd-6r13, bd-0audg) precisely
//! because of what it is: the author's accessibility text becomes a phantom whose own accessible name
//! is the raw directive, so assistive technology announces the syntax as chart data.
//!
//! Every test here pairs the two halves — no phantom AND the text still reaches `<title>`/`<desc>`.
//! Dropping the line entirely would remove the phantom and silently discard the accessibility data,
//! which is a worse bug wearing this one's passing test.

fn text_runs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</text>") else {
            break;
        };
        out.push(rest[..close].to_string());
        rest = &rest[close + "</text>".len()..];
    }
    out
}

/// Contents of the first `<tag>…</tag>` pair, for the document-level `<title>`/`<desc>`.
fn first_element_text<'a>(svg: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = svg.find(&open)? + open.len();
    let end = svg[start..].find(&close)? + start;
    Some(&svg[start..end])
}

/// Assert the shared contract for one diagram: no phantom, and the a11y text survives.
fn assert_accessibility_is_described_not_drawn(label: &str, source: &str, expected_item: &str) {
    let ir = fm_parser::parse(source).ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();

    assert!(
        !ids.iter()
            .any(|id| id.contains("accTitle") || id.contains("accDescr")),
        "{label}: an accessibility directive was interned as a chart item: {ids:?}"
    );

    let svg = fm_render_svg::render_svg(&ir);
    let runs = text_runs(&svg);
    assert!(
        !runs
            .iter()
            .any(|run| run.contains("accTitle") || run.contains("accDescr")),
        "{label}: the directive was DRAWN; text runs were {runs:?}"
    );

    // CONTROL, and the half that a "just drop the line" fix breaks: the text must still be
    // published as the document's accessible name and description.
    assert_eq!(
        first_element_text(&svg, "title"),
        Some("My Title"),
        "{label}: accTitle stopped reaching the document <title>"
    );
    assert_eq!(
        first_element_text(&svg, "desc"),
        Some("My description"),
        "{label}: accDescr stopped reaching the document <desc>"
    );

    // NON-VACUITY: the real content must still be drawn, or "no phantom" is a statement about an
    // empty picture.
    assert!(
        runs.iter().any(|run| run.contains(expected_item)),
        "{label}: CONTROL FAILED — the real item {expected_item:?} was not drawn either; \
         text runs were {runs:?}"
    );
}

/// A pie chart rendered a slice captioned `accTitle`, in the wedge and again in the legend.
#[test]
fn a_pie_chart_describes_its_accessibility_text_instead_of_slicing_it() {
    assert_accessibility_is_described_not_drawn(
        "pie",
        "pie\n  accTitle: My Title\n  accDescr: My description\n  \"Alpha\" : 10\n",
        "Alpha",
    );
}

/// A gantt chart interned the directive as a TASK.
///
/// Its rects carry no `data-id`, which is why an SVG-attribute probe once reported this path as
/// clean while it had a phantom — so this checks the IR items and the drawn text, not attributes.
#[test]
fn a_gantt_chart_describes_its_accessibility_text_instead_of_scheduling_it() {
    assert_accessibility_is_described_not_drawn(
        "gantt",
        "gantt\n  accTitle: My Title\n  accDescr: My description\n  section S\n  \
         Alpha :a1, 2024-01-01, 30d\n",
        "Alpha",
    );
}

/// A quadrant chart interned the directive as a POINT.
#[test]
fn a_quadrant_chart_describes_its_accessibility_text_instead_of_plotting_it() {
    assert_accessibility_is_described_not_drawn(
        "quadrant",
        "quadrantChart\n  accTitle: My Title\n  accDescr: My description\n  Alpha: [0.3, 0.6]\n",
        "Alpha",
    );
}

/// REFERENCE ARM: mindmap already paired both predicates, and must behave identically.
///
/// Pinning a parser that was ALREADY correct is what turns three separate fixes into one contract:
/// if a later change re-introduces the asymmetry, this fails alongside the three above rather than
/// leaving them looking like special cases.
#[test]
fn the_already_guarded_mindmap_parser_behaves_the_same() {
    assert_accessibility_is_described_not_drawn(
        "mindmap",
        "mindmap\n  accTitle: My Title\n  accDescr: My description\n  root((Alpha))\n",
        "Alpha",
    );
}
