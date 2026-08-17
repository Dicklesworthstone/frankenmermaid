//! An unrecognised operator must not become CONTENT (bd-rrvr).
//!
//! Measured: an unknown edge operator does not fail, does not warn, and does not drop the line — it
//! collapses the whole line into ONE node whose label is a raw fragment of the user's own syntax.
//! The user then sees a box containing `A] ~~> b[B` with no signal that anything was misread, and
//! the diagram looks like it rendered.
//!
//! `fm-parser` is under another agent's exclusive lease, so this file is the executable acceptance
//! gate rather than the fix. The fix also deserves care: "warn when a line looks like it contains an
//! unrecognised arrow" is a heuristic, and a careless one would warn on legitimate labels containing
//! `~` or `>`.

/// Node labels for a source, resolved through the label table.
fn labels_of(source: &str) -> Vec<String> {
    let parsed = fm_parser::parse(source);
    parsed
        .ir
        .nodes
        .iter()
        .map(|node| {
            node.label
                .and_then(|id| parsed.ir.labels.get(id.0))
                .map_or_else(|| node.id.clone(), |label| label.text.clone())
        })
        .collect()
}

/// CONTROL, and it must pass TODAY: the well-formed source parses cleanly.
///
/// Without this, the reproducer below could fail because the fixture is wrong rather than because
/// the defect is real — and a later "fix" would be validated against nothing.
#[test]
fn a_well_formed_flowchart_edge_yields_two_clean_nodes() {
    let labels = labels_of("flowchart TD\n  a[A] --> b[B]\n");
    assert_eq!(labels.len(), 2, "the control source must give two nodes: {labels:?}");
    assert!(
        labels.iter().any(|l| l == "A") && labels.iter().any(|l| l == "B"),
        "the control source must give clean labels: {labels:?}"
    );
}

/// ACCEPTANCE GATE for bd-rrvr.
///
/// ⚠️ `#[ignore]` BECAUSE IT REPRODUCES A LIVE DEFECT, the standing this repo gives an acceptance
/// test for an open bead. Run with `--ignored`; un-ignoring it is how bd-rrvr closes.
///
/// Measured today: one node labelled `A] ~~> b[B`, and `parse` returns no warning at all.
#[test]
#[ignore = "bd-rrvr: an unrecognised operator is swallowed into a node label as raw source"]
fn an_unrecognised_operator_does_not_become_a_node_label() {
    let source = "flowchart TD\n  a[A] ~~> b[B]\n";
    let parsed = fm_parser::parse(source);
    let labels = labels_of(source);

    // Either the line parses as the user meant it, or they are told. Silence plus a garbage label
    // is the outcome this bead exists to remove — so the assertion accepts EITHER remedy.
    let told = !parsed.warnings.is_empty();
    let parsed_cleanly = labels.len() == 2
        && labels.iter().any(|l| l == "A")
        && labels.iter().any(|l| l == "B");

    assert!(
        told || parsed_cleanly,
        "the source neither parsed cleanly nor produced a warning; labels: {labels:?}"
    );
    // Whatever the remedy, a raw fragment of the user's syntax must never be shown as content.
    assert!(
        !labels.iter().any(|l| l.contains("~~>") || l.contains('[')),
        "a raw operator fragment reached a node label: {labels:?}"
    );
}
