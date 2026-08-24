//! `A o--o B` must not invent a node called `A_o` (bd-zdpwd).
//!
//! THE DEFECT: mermaid's flowchart arrow is `[xo<]?--+[-xo>]`, so an `o` or `x` BEFORE the dashes is
//! part of the LINK. None of those forms was in `FLOW_OPERATORS`, so `--o` matched inside `o--o`,
//! leaving `A o` as the source text — which normalizes to a node id of `A_o`.
//!
//! That is not a cosmetic miss. `A o--o B` together with `A --> C` drew FOUR nodes: the author's `A`
//! was SPLIT IN TWO, one box wired to `B` and a separate one wired to `C`. The graph was wrong, not
//! just the arrowheads.
//!
//! ⚠️ THE REPO ALREADY FIXED THIS ONCE, FOR THE OTHER DIAGRAM TYPE. `CLASS_OPERATORS` carries a
//! comment about bd-92b6 — `C3 o-- C4` matching `--` and minting `C3-o` — and gained `o--`/`*--`
//! entries. The flowchart table never got the same treatment. Same bug, unfixed sibling.
//!
//! The incumbent parses every form asserted here (`parse_probe.mjs` -> PARSED).

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

fn node_ids(source: &str) -> Vec<String> {
    fm_parser::parse(source)
        .ir
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect()
}

/// `(marker_start, marker_end)` of the first edge, as the values actually emitted.
fn edge_markers(source: &str) -> (Option<String>, Option<String>) {
    let svg = render(source);
    let grab = |attr: &str| -> Option<String> {
        let needle = format!("{attr}=\"");
        let at = svg.find(&needle)? + needle.len();
        let rest = &svg[at..];
        Some(rest[..rest.find('"')?].to_string())
    };
    (grab("marker-start"), grab("marker-end"))
}

/// THE DEFECT ITSELF: the source keeps its own name.
#[test]
fn a_leading_marker_is_not_absorbed_into_the_source_id() {
    for form in [
        "o--o", "x--x", "o--x", "x--o", "o-->", "x-->", "o==o", "x==x", "o--", "x--",
    ] {
        let ids = node_ids(&format!("flowchart LR\n  A {form} B\n"));
        assert_eq!(
            ids,
            vec!["A".to_string(), "B".to_string()],
            "`A {form} B` did not produce the nodes the author wrote"
        );
    }
}

/// ⚠️ THE CONSEQUENCE THAT MAKES THIS A P1: the author's node was split in two.
///
/// A bare id check on one statement can be satisfied while the graph is still wrong. This asserts
/// the shape of the whole diagram: three nodes, and `A` appearing exactly once.
#[test]
fn a_leading_marker_link_does_not_split_a_node_in_two() {
    let ids = node_ids("flowchart LR\n  A o--o B\n  A --> C\n");
    assert_eq!(
        ids,
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        "the author's `A` was split across two boxes"
    );
}

/// `o--o` and `x--x` mark BOTH ends, exactly as `<-->` does.
#[test]
fn symmetric_leading_marker_links_mark_both_ends() {
    for (form, marker) in [
        ("o--o", "url(#arrow-circle)"),
        ("x--x", "url(#arrow-cross)"),
        ("o==o", "url(#arrow-circle)"),
        ("x==x", "url(#arrow-cross)"),
    ] {
        let (start, end) = edge_markers(&format!("flowchart LR\n  A {form} B\n"));
        assert_eq!(
            start.as_deref(),
            Some(marker),
            "`{form}` drew no start marker, so it is indistinguishable from its one-sided form"
        );
        assert_eq!(end.as_deref(), Some(marker), "`{form}` end marker is wrong");
    }
}

/// CONTROL: the ONE-SIDED forms must stay one-sided.
///
/// Without this, "mark both ends" could be implemented by marking both ends of everything, which
/// would turn every `A --o B` into a double-ended edge and pass the test above.
#[test]
fn one_sided_links_still_mark_only_the_end() {
    for form in ["--o", "--x", "-->"] {
        let (start, _) = edge_markers(&format!("flowchart LR\n  A {form} B\n"));
        assert_eq!(
            start, None,
            "`{form}` gained a start marker it must not have"
        );
    }
}

/// ⚠️ THE REGRESSION THIS FIX COULD EASILY HAVE CAUSED, AND THE REASON FOR THE WHITESPACE RULE.
///
/// A bare `o--o` table entry matches inside `Foo--o Bar`: the second `o` of `Foo` begins a perfect
/// `o--o`, so the node would silently become `Fo`. mermaid's regex starts `\s*[xo<]?`, i.e. the
/// marker must follow whitespace, and `find_operator_core` now enforces exactly that.
///
/// `Foo-- Bar` covers the same hazard for `o--`, which CLASS_OPERATORS has carried unguarded all
/// along.
#[test]
fn an_identifier_ending_in_o_or_x_is_not_eaten_by_the_marker_rule() {
    for (source, expected) in [
        ("flowchart LR\n  Foo--o Bar\n", ["Foo", "Bar"]),
        ("flowchart LR\n  Box--x Bar\n", ["Box", "Bar"]),
        ("flowchart LR\n  Foo-- Bar\n", ["Foo", "Bar"]),
        ("flowchart LR\n  Foo--> Bar\n", ["Foo", "Bar"]),
        ("flowchart LR\n  Xo--o Bar\n", ["Xo", "Bar"]),
    ] {
        let ids = node_ids(source);
        assert_eq!(
            ids,
            expected.map(String::from).to_vec(),
            "the marker rule ate part of an identifier in:\n{source}"
        );
    }
}

/// CONTROL: `<-->`, which already worked, must keep working — it is the sibling this fix mirrors.
#[test]
fn the_double_arrow_that_already_worked_still_works() {
    assert_eq!(node_ids("flowchart LR\n  A <--> B\n"), vec!["A", "B"]);
    let (start, end) = edge_markers("flowchart LR\n  A <--> B\n");
    assert_eq!(start.as_deref(), Some("url(#arrow-start)"));
    assert_eq!(end.as_deref(), Some("url(#arrow-end)"));
}

/// KNOWN PARTIAL, PINNED SO IT IS NOT MISTAKEN FOR COMPLETE.
///
/// A MIXED form (`o--x`, `x--o`, `o-->`) has two different markers, and `ArrowType` couples both
/// ends into a single variant — so these render their END marker correctly and draw NO start marker.
/// That is strictly better than before (the id was corrupted and the edge was wrong anyway), but it
/// is not parity, and asserting it here keeps the gap visible instead of letting a later reader
/// assume every form is finished. Fixing it needs a start/end marker pair on the edge, not more
/// enum variants.
#[test]
fn mixed_leading_marker_forms_currently_draw_only_their_end_marker() {
    for (form, end) in [
        ("o--x", "url(#arrow-cross)"),
        ("x--o", "url(#arrow-circle)"),
        ("o-->", "url(#arrow-end)"),
    ] {
        let (start, got_end) = edge_markers(&format!("flowchart LR\n  A {form} B\n"));
        assert_eq!(
            got_end.as_deref(),
            Some(end),
            "`{form}` end marker is wrong"
        );
        assert_eq!(
            start, None,
            "`{form}` now draws a start marker — good, but update this test and the note above it"
        );
    }
}
