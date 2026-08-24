//! Differential test: every flowchart link spelling, against what mermaid-js ACTUALLY does with it.
//!
//! The oracle is `tests/fixtures/mermaid_flow_links.tsv`, produced by
//! `scripts/headtohead/link_battery.mjs` from the pinned mermaid 11.15.0 bundle. It is a
//! MEASUREMENT of the incumbent, not a transcription of its documentation — which matters, because
//! the defect this test pins (bd-lrl48b) was a table of link spellings written from what the syntax
//! looks like. mermaid reads `o==>` as a NORMAL-weight arrow; nobody guesses that.
//!
//! WHAT IS ASSERTED, and why both halves are needed:
//!
//! 1. THE ENDPOINTS. mermaid resolves `A <token> B` to the edge `A -> B` for every token it
//!    accepts, all 168 of them. This is the half that catches the severe failure: a link byte the
//!    matcher could not account for stayed on the ENDPOINT, so `A o=== B` built a node called
//!    `A_o` and `A ==o B` built one called `o_B`. Those diagrams were not mis-styled, they were
//!    mis-wired — an author's `A` silently split into two nodes.
//!
//! 2. THE (tail marker × stroke) PAIR. mermaid's link is a product of an end marker and a stroke
//!    weight, and `ArrowType` is our name for one cell of that product. Checking only the marker
//!    would pass a thick link rendered thin; checking only the weight would pass an arrowhead
//!    rendered as a circle.
//!
//! NOT ASSERTED: mermaid's `length` (its rank-distance hint, `A ---> B` pushing B one rank
//! further). We do not model it, so there is nothing to compare; a row asserting it would be
//! asserting against ourselves. Say it here rather than let the silence read as coverage.
//!
//! The 8 tokens mermaid REJECTS (`--`, `==`, and those with a head) are deliberately not asserted
//! as rejections: we accept them as plain lines, which is a lenience that predates this test and is
//! its own decision. The test pins what we do with everything mermaid ACCEPTS.

use std::{collections::BTreeMap, fs, path::Path};

use fm_core::ArrowType;
use fm_parser::parse;

/// The `(mermaid type, mermaid stroke)` cell each `ArrowType` occupies.
///
/// Written in this direction — ours to mermaid's — on purpose: it makes an `ArrowType` that names
/// no mermaid cell impossible to add silently, and it is the direction the assertion reads in.
fn mermaid_meaning(arrow: ArrowType) -> Option<(&'static str, &'static str)> {
    Some(match arrow {
        ArrowType::Line => ("arrow_open", "normal"),
        ArrowType::ThickLine => ("arrow_open", "thick"),
        ArrowType::DottedLine => ("arrow_open", "dotted"),
        ArrowType::Arrow => ("arrow_point", "normal"),
        ArrowType::ThickArrow => ("arrow_point", "thick"),
        ArrowType::DottedArrow => ("arrow_point", "dotted"),
        ArrowType::DoubleArrow => ("double_arrow_point", "normal"),
        ArrowType::DoubleThickArrow => ("double_arrow_point", "thick"),
        ArrowType::DoubleDottedArrow => ("double_arrow_point", "dotted"),
        ArrowType::Circle => ("arrow_circle", "normal"),
        ArrowType::ThickCircle => ("arrow_circle", "thick"),
        ArrowType::DottedCircle => ("arrow_circle", "dotted"),
        ArrowType::CircleBoth => ("double_arrow_circle", "normal"),
        ArrowType::ThickCircleBoth => ("double_arrow_circle", "thick"),
        ArrowType::DottedCircleBoth => ("double_arrow_circle", "dotted"),
        ArrowType::Cross => ("arrow_cross", "normal"),
        ArrowType::ThickCross => ("arrow_cross", "thick"),
        ArrowType::DottedCross => ("arrow_cross", "dotted"),
        ArrowType::CrossBoth => ("double_arrow_cross", "normal"),
        ArrowType::ThickCrossBoth => ("double_arrow_cross", "thick"),
        ArrowType::DottedCrossBoth => ("double_arrow_cross", "dotted"),
        _ => return None,
    })
}

struct Row {
    verdict: String,
    kind: String,
    stroke: String,
    start: String,
    end: String,
}

fn oracle() -> BTreeMap<String, Row> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_flow_links.tsv");
    let text = fs::read_to_string(&fixture).expect("the mermaid link fixture is readable");
    let rows: BTreeMap<String, Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let token = fields.next().expect("every row names a token").to_string();
            let mut next = || fields.next().unwrap_or_default().to_string();
            let row = Row {
                verdict: next(),
                kind: next(),
                stroke: next(),
                start: next(),
                end: next(),
            };
            (token, row)
        })
        .collect();

    // A fixture that failed to generate is an empty file, and an empty file makes every assertion
    // below vacuous — the test would pass loudest exactly when its oracle was missing.
    assert!(
        rows.len() >= 176,
        "the link battery must cover the whole head x body x tail product, got {}",
        rows.len()
    );
    rows
}

/// Resolve the one edge in `flowchart LR / A <token> B` to `(from, to, arrow)`.
fn parse_single_link(token: &str) -> Option<(String, String, ArrowType)> {
    let source = format!("flowchart LR\n  A {token} B\n");
    let result = parse(&source);
    let edge = match result.ir.edges.as_slice() {
        [edge] => edge,
        _ => return None,
    };
    let name = |endpoint: fm_core::IrEndpoint| {
        endpoint
            .resolved_node_id(&result.ir.ports)
            .and_then(|id| result.ir.nodes.get(id.0))
            .map(|node| node.id.to_string())
    };
    Some((name(edge.from)?, name(edge.to)?, edge.arrow))
}

#[test]
fn every_link_mermaid_accepts_wires_the_same_two_nodes() {
    let mut wrong = Vec::new();
    let mut checked = 0_usize;

    for (token, row) in oracle() {
        if row.verdict != "PARSED" {
            continue;
        }
        checked += 1;
        match parse_single_link(&token) {
            Some((from, to, _)) if from == row.start && to == row.end => {}
            Some((from, to, _)) => wrong.push(format!(
                "`A {token} B`: mermaid wires {}->{}, we wire {from}->{to}",
                row.start, row.end
            )),
            None => wrong.push(format!("`A {token} B`: mermaid wires an edge, we produce none")),
        }
    }

    assert!(checked >= 168, "the fixture lost its accepted tokens: {checked}");
    assert!(
        wrong.is_empty(),
        "{} of {checked} links wire the wrong nodes:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

#[test]
fn every_link_mermaid_accepts_carries_the_same_marker_and_stroke() {
    let mut wrong = Vec::new();
    let mut checked = 0_usize;

    for (token, row) in oracle() {
        if row.verdict != "PARSED" {
            continue;
        }
        checked += 1;
        let Some((_, _, arrow)) = parse_single_link(&token) else {
            wrong.push(format!("`A {token} B`: mermaid builds an edge, we produce none"));
            continue;
        };
        let Some((kind, stroke)) = mermaid_meaning(arrow) else {
            wrong.push(format!(
                "`A {token} B`: {arrow:?} names no mermaid (type, stroke) cell"
            ));
            continue;
        };
        if (kind, stroke) != (row.kind.as_str(), row.stroke.as_str()) {
            wrong.push(format!(
                "`A {token} B`: mermaid says {}/{}, we say {arrow:?} = {kind}/{stroke}",
                row.kind, row.stroke
            ));
        }
    }

    assert!(checked >= 168, "the fixture lost its accepted tokens: {checked}");
    assert!(
        wrong.is_empty(),
        "{} of {checked} links carry the wrong marker or stroke:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// The three readings that a spelling table gets wrong in DIFFERENT directions, named individually
/// so a regression says which half of `destructEndLink` broke rather than just "27 rows differ".
#[test]
fn the_head_marker_is_stripped_before_the_stroke_is_read() {
    // Head pairs with the tail: it is dropped, and the `==` underneath sets the weight.
    assert_eq!(parse_single_link("o==o").unwrap().2, ArrowType::ThickCircleBoth);
    // Head does NOT pair with the tail: it stays, so mermaid never sees the `=` and the link is
    // NORMAL weight. This is the reading that no one guesses from the syntax.
    assert_eq!(parse_single_link("o==>").unwrap().2, ArrowType::Arrow);
    assert_eq!(parse_single_link("x==>").unwrap().2, ArrowType::Arrow);
    // Same rule with no head at all: the tail marker and the weight are independent.
    assert_eq!(parse_single_link("==o").unwrap().2, ArrowType::ThickCircle);
    assert_eq!(parse_single_link("==x").unwrap().2, ArrowType::ThickCross);
}

#[test]
fn an_unmatched_head_or_tail_marker_never_lands_on_a_node_id() {
    // Each of these once produced a phantom endpoint — `A_o`, `o_B`, `A_x` — because the matcher
    // could not account for the marker byte and the endpoint parser swept it up.
    for token in ["o===", "o====", "==o", "===x", "o-.-", "x-.-o", "-.-o", "o-..-o"] {
        let (from, to, _) = parse_single_link(token)
            .unwrap_or_else(|| panic!("`A {token} B` must build exactly one edge"));
        assert_eq!((from.as_str(), to.as_str()), ("A", "B"), "token `{token}`");
    }
}

/// The body run is unbounded in the grammar, so no finite table of spellings can be complete.
#[test]
fn an_arbitrarily_long_body_run_keeps_its_head_and_tail() {
    assert_eq!(parse_single_link("o-----o").unwrap().2, ArrowType::CircleBoth);
    assert_eq!(parse_single_link("x=====x").unwrap().2, ArrowType::ThickCrossBoth);
    assert_eq!(
        parse_single_link("<----->").unwrap().2,
        ArrowType::DoubleArrow
    );
    assert_eq!(
        parse_single_link("-....->").unwrap().2,
        ArrowType::DottedArrow
    );
}

/// A leading `o`/`x` is a link head only at a token boundary — otherwise an id ending in `o`
/// donates its last byte to the link and the node splits in two.
#[test]
fn a_node_id_ending_in_o_or_x_keeps_its_last_byte() {
    for (source, from) in [
        ("flowchart LR\n  Foo--o Bar\n", "Foo"),
        ("flowchart LR\n  Fox--x Bar\n", "Fox"),
        ("flowchart LR\n  Foo==o Bar\n", "Foo"),
    ] {
        let result = parse(source);
        let edge = match result.ir.edges.as_slice() {
            [edge] => edge,
            edges => panic!("{source:?} must build one edge, built {}", edges.len()),
        };
        let resolved = edge
            .from
            .resolved_node_id(&result.ir.ports)
            .and_then(|id| result.ir.nodes.get(id.0))
            .map(|node| node.id.as_str());
        assert_eq!(resolved, Some(from), "{source:?}");
    }
}
