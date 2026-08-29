//! A sankey LINK carries no label; the flow value reaches the reader through the node totals.
//!
//! THE DEFECT. We drew a text label on every sankey link. mermaid 11.15.0 draws text only on the
//! NODES — measured in Chromium 151 against the pinned bundle, reading every `<text>`, `<tspan>` and
//! `foreignObject` in the rendered document:
//!
//! ```text
//!   source            reference drawn text          ours (before)
//!   A,B,10            ["A\n10", "B\n10"]            + a standalone "10" on the link
//!   A,B,3 / A,C,7     ["A\n10", "B\n3", "C\n7"]     + "3" and "7" on the links
//! ```
//!
//! ⚠️ NOTHING IS HIDDEN BY REMOVING IT, which is why this is parity rather than a loss. A sankey
//! link's label IS its flow value, and that value already reaches the picture through the node
//! totals at both ends: in the fan case above, `B 3` and `C 7` state each flow at the node it
//! terminates at. That is exactly how the reference conveys per-flow values, and it is why removing
//! the link text does not remove information.
//!
//! ⚠️ THIS SUPERSEDES A FORMATTING FIX, DELIBERATELY AND IN THAT ORDER. The previous bead routed the
//! link label through `format_sankey_total`, because the link was printing `124.729` beside a node
//! printing `124.73` — the same quantity spelled two ways. That was the right answer to the question
//! then being asked (WHICH DIGITS?) and is moot once the prior question (IS THERE A LABEL AT ALL?)
//! is answered "no". The element-existence question was recorded as its own bead instead of being
//! folded into a formatting change on the way past; this is that bead, and the superseded branch was
//! removed rather than left unreachable.

/// The drawn text of an SVG: `<text>` bodies with nested tags stripped, entities resolved.
///
/// A `>` outside a tag is TEXT — the writer escapes `<` but leaves `>` literal (valid XML), so a
/// depth tracker consuming every `>` would eat real characters.
fn drawn_text(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        let Some(open_end) = rest[start..].find('>') else {
            break;
        };
        let body_start = start + open_end + 1;
        let Some(close) = rest[body_start..].find("</text>") else {
            break;
        };
        let body = &rest[body_start..body_start + close];
        let mut text = String::new();
        let mut depth = 0usize;
        for ch in body.chars() {
            match ch {
                '<' => depth += 1,
                '>' if depth > 0 => depth -= 1,
                _ if depth == 0 => text.push(ch),
                _ => {}
            }
        }
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
        rest = &rest[body_start + close..];
    }
    out
}

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// No link label is drawn, and no `edge-label` element is emitted for one.
#[test]
fn a_sankey_link_draws_no_label() {
    let svg = render("sankey-beta\n\nA,B,10\n");
    let texts = drawn_text(&svg);

    // The node labels are `<text>` elements of two tspans, so their bodies read `A10` / `B10`.
    // A link label would be a THIRD element whose whole body is the bare number.
    assert!(
        !texts.iter().any(|text| text == "10"),
        "a standalone flow value is still drawn on the link: {texts:?}"
    );
    assert!(
        !svg.contains("class=\"edge-label\""),
        "an edge-label element is still emitted for a sankey link"
    );
}

/// ⚠️ PLANTED NEGATIVE 1: the NODE totals must survive.
///
/// The cheap way to make the assertion above pass is to stop drawing sankey text, or to drop the
/// flow value from the label builder — and both leave a diagram with no numbers at all while
/// satisfying "no standalone value on the link". The totals are where the reference puts the
/// numbers, so they are asserted explicitly, on their own tspans, for every node.
#[test]
fn the_node_totals_still_draw_on_both_ends() {
    let svg = render("sankey-beta\n\nA,B,10\n");
    for expected in [">A</tspan>", ">B</tspan>", ">10</tspan>"] {
        assert!(
            svg.contains(expected),
            "the node label lost {expected:?}: removing the link label took the totals with it"
        );
    }
    // Both ends carry the total, not just one.
    let totals = svg.matches(">10</tspan>").count();
    assert_eq!(
        totals, 2,
        "expected the throughput on BOTH node labels, found {totals}"
    );
}

/// ⚠️ PLANTED NEGATIVE 2: every OTHER diagram keeps its edge labels.
///
/// `compute_edge_label` is shared by every family. A change that returned `None` unconditionally, or
/// that disabled the `show_edge_labels` path, passes both assertions above and silently strips the
/// labels off flowcharts, sequences and state diagrams — a far larger regression than the defect
/// being fixed. Three unrelated families are asserted so the suppression cannot be broader than the
/// one diagram type it was measured for.
#[test]
fn other_families_keep_their_edge_labels() {
    let arrow = "-->";
    for (family, source, expected) in [
        (
            "flowchart",
            format!("flowchart LR\n  A {arrow}|yes| B\n"),
            "yes",
        ),
        (
            "state",
            format!("stateDiagram-v2\n  A {arrow} B: go\n"),
            "go",
        ),
        (
            "class",
            "classDiagram\n  A <|-- B : extends\n".to_string(),
            "extends",
        ),
    ] {
        let texts = drawn_text(&render(&source));
        assert!(
            texts.iter().any(|text| text.contains(expected)),
            "{family}: the edge label {expected:?} was stripped along with sankey's: {texts:?}"
        );
    }
}

/// The fan case: each flow is still legible, at the node it terminates at.
///
/// This is the evidence for the claim that removing the link label loses nothing. `A,B,3` and
/// `A,C,7` put 3 and 7 on B and C respectively, and 10 — their sum — on A, which is how the
/// reference conveys per-flow values without any link text.
#[test]
fn each_flow_is_still_legible_from_its_target_node() {
    let svg = render("sankey-beta\n\nA,B,3\nA,C,7\n");
    for expected in [">3</tspan>", ">7</tspan>", ">10</tspan>"] {
        assert!(
            svg.contains(expected),
            "the fan case lost {expected:?}; per-flow values are no longer legible"
        );
    }
    assert!(
        !drawn_text(&svg)
            .iter()
            .any(|text| text == "3" || text == "7"),
        "a standalone flow value is still drawn on a link in the fan case"
    );
}
