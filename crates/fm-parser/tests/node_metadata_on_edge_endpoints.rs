//! `id@{ shape: …, label: … }` works as an EDGE ENDPOINT, not only as a whole statement.
//!
//! THE DEFECT. mermaid 11.3 added `A@{ shape: cyl, label: "DB" }`. We handled it only when it was
//! the entire statement. Written as an edge endpoint — `A@{ shape: cyl, label: "DB" } --> B`, which
//! is how anyone actually draws — it fell through to the generic token parser, which saw `A@`
//! followed by `{ … }` and read it as the CLASSIC diamond spelling `A{label}`. Every such node came
//! out a **Diamond captioned with the raw metadata body**:
//!
//! ```text
//!   A@{ shape: cyl,     label: "DB" } --> B   ->  A[Diamond, "shape: cyl, label: \"DB"]
//!   A@{ shape: rounded, label: "hi" } --> B   ->  A[Diamond, "shape: rounded, label: \"hi"]
//! ```
//!
//! The shape looked right for `diamond` alone, purely by coincidence — the `{` did that, not the
//! `shape:` key.
//!
//! MEASURED REFERENCE, pinned mermaid 11.15.0 rendered in Chromium 151, reading `g.node` text and
//! the drawn element:
//!
//! ```text
//!   A@{ shape: diamond, label: "Q" }  --> B   nodes "Q" (polygon) + "B" (rect), 1 edge
//!   A@{ shape: rounded, label: "hi" } --> B   nodes "hi" (rect)    + "B",        1 edge
//!   A@{ shape: cyl,     label: "DB" } --> B   nodes "DB" (path)    + "B",        1 edge
//!   A@{ shape: diamond, label: "Q" }          node  "Q" (polygon),              0 edges
//! ```
//!
//! ⚠️ ONE KNOWN LIMIT, STATED RATHER THAN HIDDEN. On the endpoint path an unrecognised shape name
//! leaves the shape alone WITHOUT the `unimplemented shape` warning the statement path emits
//! (bd-laocw): `parse_node_token_core` is reached from `parse_edge_statement_asts` through
//! `parse_node_list_with_config`, and none of those carry a diagnostics sink. Both paths agree on
//! the resulting shape; only the diagnostic differs. `an_unknown_shape_on_an_endpoint_is_ignored`
//! pins that, so the day a sink appears the difference is visible rather than forgotten.

use fm_core::NodeShape;

const ARROW: &str = "-->";

/// `(id, shape, label)` for every node, in IR order.
fn nodes(source: &str) -> Vec<(String, NodeShape, Option<String>)> {
    let ir = fm_parser::parse(source).ir;
    ir.nodes
        .iter()
        .map(|node| {
            let label = node
                .label
                .and_then(|id| ir.labels.get(id.0))
                .map(|label| label.text.clone());
            (node.id.clone(), node.shape, label)
        })
        .collect()
}

fn edge_count(source: &str) -> usize {
    fm_parser::parse(source).ir.edges.len()
}

/// ⚠️ THE PLANTED NEGATIVE: the label must be the `label:` VALUE, never the metadata body.
///
/// Asserting only the shape would have passed before this fix for the `diamond` case — the `{`
/// delimiter produced a Diamond by accident. The label is what actually distinguishes a parsed
/// `@{ … }` from a mis-read classic token, so it is asserted for every shape, and the pre-fix text
/// (`shape: …, label: "…`) is rejected by name.
#[test]
fn metadata_on_an_edge_source_sets_shape_and_label() {
    for (shape_name, expected_shape, expected_label) in [
        ("diamond", NodeShape::Diamond, "Q"),
        ("rounded", NodeShape::Rounded, "hi"),
        ("cyl", NodeShape::Cylinder, "DB"),
    ] {
        let source = format!(
            "flowchart LR\n  A@{{ shape: {shape_name}, label: \"{expected_label}\" }} {ARROW} B\n"
        );
        let parsed = nodes(&source);

        assert_eq!(
            parsed.len(),
            2,
            "{shape_name}: expected the reference's two nodes"
        );
        let (id, shape, label) = &parsed[0];
        assert_eq!(id, "A");
        assert_eq!(
            *shape, expected_shape,
            "{shape_name}: shape not taken from the metadata"
        );
        assert_eq!(
            label.as_deref(),
            Some(expected_label),
            "{shape_name}: label is not the `label:` value — the raw `@{{}}` body leaked into it"
        );
        assert!(
            !label.as_deref().unwrap_or_default().contains("shape:"),
            "{shape_name}: the metadata body is being drawn as the caption"
        );
        assert_eq!(edge_count(&source), 1, "{shape_name}: the edge was lost");
    }
}

/// The same when the metadata node is the edge's TARGET, which is a separate endpoint parse.
#[test]
fn metadata_on_an_edge_target_sets_shape_and_label() {
    let source = format!("flowchart LR\n  B {ARROW} A@{{ shape: cyl, label: \"DB\" }}\n");
    let parsed = nodes(&source);
    assert_eq!(parsed.len(), 2);
    let target = parsed
        .iter()
        .find(|(id, _, _)| id == "A")
        .expect("the metadata node exists");
    assert_eq!(target.1, NodeShape::Cylinder);
    assert_eq!(target.2.as_deref(), Some("DB"));
    assert_eq!(edge_count(&source), 1);
}

/// CONTROL: the whole-statement spelling, which already worked, is unchanged.
///
/// The fix adds a branch to the shared token parser, so the path that was already correct has to be
/// shown still correct — otherwise a regression there would look like success here.
#[test]
fn metadata_as_a_whole_statement_still_works() {
    let source = "flowchart LR\n  A@{ shape: cyl, label: \"DB\" }\n";
    assert_eq!(
        nodes(source),
        vec![("A".to_string(), NodeShape::Cylinder, Some("DB".to_string()))]
    );
    assert_eq!(edge_count(source), 0);
}

/// CONTROL: the CLASSIC `A{label}` diamond spelling must not be captured by the new branch.
///
/// This is the token the defect was confusing `@{ … }` with, so it is the one that breaks if the
/// new branch matches too eagerly.
#[test]
fn the_classic_curly_diamond_is_untouched() {
    let source = format!("flowchart LR\n  A{{Q}} {ARROW} B\n");
    let parsed = nodes(&source);
    assert_eq!(parsed[0].1, NodeShape::Diamond);
    assert_eq!(parsed[0].2.as_deref(), Some("Q"));
    assert_eq!(edge_count(&source), 1);
}

/// An unrecognised shape leaves the shape alone and keeps the `label:` value.
///
/// ⚠️ Pins the known limit from this file's header: the endpoint path emits no `unimplemented shape`
/// warning because it has no diagnostics sink. The SHAPE outcome matches the statement path, which
/// is what a reader of the diagram sees.
#[test]
fn an_unknown_shape_on_an_endpoint_is_ignored() {
    let source = format!("flowchart LR\n  A@{{ shape: notarealshape, label: \"L\" }} {ARROW} B\n");
    let parsed = nodes(&source);
    assert_eq!(
        parsed[0].1,
        NodeShape::Rect,
        "an unknown shape must not invent one"
    );
    assert_eq!(
        parsed[0].2.as_deref(),
        Some("L"),
        "the label survives even when the shape name does not"
    );
}
