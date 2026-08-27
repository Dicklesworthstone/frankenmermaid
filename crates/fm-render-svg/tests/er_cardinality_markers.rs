//! ER cardinality is drawn as CROW'S-FOOT MARKERS, as mermaid draws it (bd-dun16).
//!
//! REFERENCE BEHAVIOR, measured in Chromium 151 against the pinned mermaid 11.15.0 bundle. mermaid
//! encodes cardinality as marker shapes on the relationship line, one of four forms per end, each
//! with a distinct start and end variant:
//!
//! ```text
//!   er-onlyOneStart     18x18  refX=0  refY=9   M9,0 L9,18 M15,0 L15,18
//!   er-onlyOneEnd       18x18  refX=18 refY=9   M3,0 L3,18 M9,0 L9,18
//!   er-zeroOrOneStart   30x18  refX=0  refY=9   circle(21,9,r6) + M9,0 L9,18
//!   er-zeroOrOneEnd     30x18  refX=30 refY=9   circle(9,9,r6)  + M21,0 L21,18
//!   er-oneOrMoreStart   45x36  refX=18 refY=18  M0,18 Q 18,0 36,18 Q 18,36 0,18 M42,9 L42,27
//!   er-oneOrMoreEnd     45x36  refX=27 refY=18  M3,9 L3,27 M9,18 Q27,0 45,18 Q27,36 9,18
//!   er-zeroOrMoreStart  57x36  refX=18 refY=18  circle(48,18,r6) + M0,18 Q18,0 36,18 Q18,36 0,18
//!   er-zeroOrMoreEnd    57x36  refX=39 refY=18  circle(9,18,r6)  + M21,18 Q39,0 57,18 Q39,36 21,18
//! ```
//!
//! and selects per side: `A ||--o{ B` takes `onlyOneStart` and `zeroOrMoreEnd`.
//!
//! ⚠️ WHY THE START AND END VARIANTS ARE BOTH NEEDED, since it looks like duplication: the glyph is
//! not symmetric, and SVG's `orient="auto"` ROTATES a marker without MIRRORING it. Reusing one def at
//! the other end puts the bar where the crow's foot belongs — a different cardinality, not a cosmetic
//! flaw. `scripts/headtohead/er_marker_diff.mjs` compares the resolved geometry per END for exactly
//! this reason; collecting "the markers this diagram uses" as a set would pass on a swap.
//!
//! ⚠️ THE CARDINALITY TEXT IS STILL DRAWN and is deliberately left alone here. bd-dun16 says to draw
//! the markers first and drop the text second, because deleting the text before the markers existed
//! would have destroyed the only carrier the information had.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// Every `marker-start`/`marker-end` on the document, in document order.
fn marker_refs(svg: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for chunk in svg.split("<path ").skip(1) {
        let tag = &chunk[..chunk.find('>').unwrap_or(chunk.len())];
        let attr = |name: &str| -> String {
            let needle = format!("{name}=\"");
            tag.find(&needle).map_or(String::new(), |at| {
                let start = at + needle.len();
                tag[start..]
                    .find('"')
                    .map_or(String::new(), |end| tag[start..start + end].to_string())
            })
        };
        let (start, end) = (attr("marker-start"), attr("marker-end"));
        if !start.is_empty() || !end.is_empty() {
            out.push((start, end));
        }
    }
    out
}

/// The `<marker id="...">` ids this document DECLARES.
fn declared_marker_ids(svg: &str) -> Vec<String> {
    svg.split("<marker id=\"")
        .skip(1)
        .filter_map(|chunk| chunk.find('"').map(|end| chunk[..end].to_string()))
        .collect()
}

#[test]
fn each_cardinality_selects_mermaids_marker_for_that_end() {
    // (notation, expected marker-start, expected marker-end) — read off the Chromium render.
    let cases = [
        ("||--||", "er-onlyOneStart", "er-onlyOneEnd"),
        ("||--o{", "er-onlyOneStart", "er-zeroOrMoreEnd"),
        ("|o--|{", "er-zeroOrOneStart", "er-oneOrMoreEnd"),
        ("}o--o|", "er-zeroOrMoreStart", "er-zeroOrOneEnd"),
        ("}|--||", "er-oneOrMoreStart", "er-onlyOneEnd"),
    ];
    for (notation, want_start, want_end) in cases {
        let svg = render(&format!("erDiagram\n  A {notation} B : r\n"));
        assert_eq!(
            marker_refs(&svg),
            vec![(format!("url(#{want_start})"), format!("url(#{want_end})"))],
            "{notation} selected the wrong cardinality markers"
        );
    }
}

/// ⚠️ THE DISCRIMINATING CONTROL bd-dun16 REQUIRES: two DIFFERENT cardinalities must produce two
/// DIFFERENT markers.
///
/// Asserting only "a marker is present" passes on an implementation that draws one shared glyph for
/// all four forms — which is not a partial success but a diagram that states a cardinality the source
/// never declared. This is the same shape as bd-vfxu, where two declared node shapes rendered
/// byte-identical geometry and the test asserting "a circle exists" saw nothing wrong.
#[test]
fn a_different_cardinality_draws_a_different_marker() {
    let zero_or_more = render("erDiagram\n  A ||--o{ B : r\n");
    let exactly_one = render("erDiagram\n  A ||--|| B : r\n");

    let (_, end_many) = marker_refs(&zero_or_more)[0].clone();
    let (_, end_one) = marker_refs(&exactly_one)[0].clone();
    assert_ne!(
        end_many, end_one,
        "zero-or-more and exactly-one resolved to the SAME marker, so the drawing does not \
         distinguish them"
    );

    // And the shapes behind those references must differ too — two ids pointing at identical
    // geometry would satisfy the assertion above while drawing the same picture.
    let geometry = |svg: &str| {
        svg.split("<marker ")
            .skip(1)
            .map(|chunk| chunk[..chunk.find("</marker>").unwrap_or(chunk.len())].to_string())
            .collect::<Vec<_>>()
    };
    assert_ne!(
        geometry(&zero_or_more),
        geometry(&exactly_one),
        "the two cardinalities declare markers with identical geometry"
    );
}

/// ⚠️ NO DANGLING REFERENCE. This is the property that actually caught a bug: while the start/end
/// selection was disarmed for the negative control, the defs helper still emitted the CORRECT pair
/// while the path referenced the swapped one, so `strip_unused_markers` deleted the referenced defs
/// and the relationship line rendered with no ends at all — silently.
///
/// That is the exact drift failure that keeps the notation-to-shape mapping single-sourced in
/// `fm_core::parse_er_cardinality_forms` + `MarkerKind::er_pair`. Two hand-rolled copies here and in
/// the defs writer would reintroduce it.
#[test]
fn every_referenced_marker_is_declared() {
    for notation in ["||--||", "||--o{", "|o--|{", "}o--o|", "}|--||"] {
        let svg = render(&format!("erDiagram\n  A {notation} B : r\n"));
        let declared = declared_marker_ids(&svg);
        for (start, end) in marker_refs(&svg) {
            for reference in [start, end] {
                if reference.is_empty() {
                    continue;
                }
                let id = reference
                    .trim_start_matches("url(#")
                    .trim_end_matches(')')
                    .to_string();
                assert!(
                    declared.contains(&id),
                    "{notation} references marker {id} that this document never declares \
                     (declared: {declared:?})"
                );
            }
        }
    }
}

/// CONTROL, and the converse invariant this renderer already holds elsewhere: only the markers the
/// diagram REFERENCES are declared. mermaid ships all eight on every ER diagram; emitting eight to
/// reference two would be ~1.5 KB of markup that draws nothing, and would break the dead-defs rule
/// `strip_unused_markers` enforces.
#[test]
fn only_the_referenced_cardinality_markers_are_declared() {
    let svg = render("erDiagram\n  A ||--o{ B : r\n");
    let declared = declared_marker_ids(&svg);
    let er: Vec<&String> = declared.iter().filter(|id| id.starts_with("er-")).collect();
    assert_eq!(
        er.len(),
        2,
        "expected exactly the two referenced ER markers, got {er:?}"
    );
}

/// ⚠️ THE ASYMMETRIC-SIBLING CONTROL. This renderer has TWO edge writers — a streaming fast path and
/// an `Element` slow path — and an ER relationship reaches both: unlabelled takes the fast one,
/// labelled (`A ||--o{ B : places`) takes the slow one. A one-sided fix draws crow's feet until
/// someone writes a label, which is the defect family this file has produced more than once.
#[test]
fn a_labelled_relationship_gets_the_same_markers_as_a_bare_one() {
    let bare = render("erDiagram\n  A ||--o{ B : r\n");
    let labelled = render("erDiagram\n  A ||--o{ B : places\n");
    assert_eq!(
        marker_refs(&bare),
        marker_refs(&labelled),
        "the two edge writers disagree about an ER relationship's cardinality markers"
    );
    assert!(
        !marker_refs(&bare).is_empty(),
        "neither path drew a marker, so this control proves nothing"
    );
}

/// CONTROL: a non-ER diagram gains no cardinality markers, and its arrowheads are untouched. The
/// override is applied after the arrow match in both writers, so a mistake there would reach every
/// diagram type.
#[test]
fn a_flowchart_is_untouched() {
    let svg = render("flowchart TD\n  a[A] --> b[B]\n");
    assert!(
        !svg.contains("er-onlyOne") && !svg.contains("er-zeroOrMore"),
        "a flowchart gained ER cardinality markers"
    );
    assert!(
        svg.contains("url(#arrow-end)"),
        "the flowchart lost its ordinary arrowhead: {svg}"
    );
}

/// CONTROL: a notation naming no cardinality on a side draws no marker there rather than guessing
/// one. `fm_core`'s label mapping degrades an unknown marker to `*`; the SHAPE mapping must not.
#[test]
fn a_bare_connector_draws_no_cardinality_marker() {
    let svg = render("erDiagram\n  A ||--|| B : r\n");
    assert!(
        !marker_refs(&svg).is_empty(),
        "the positive case drew nothing, so the negative case below proves nothing"
    );

    let refs = marker_refs(&render("erDiagram\n  A -- B : r\n"));
    assert!(
        refs.iter()
            .all(|(start, end)| !start.contains("er-") && !end.contains("er-")),
        "a connector naming no cardinality still drew a crow's foot: {refs:?}"
    );
}
