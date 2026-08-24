//! An unrecognised operator must not become CONTENT (bd-rrvr).
//!
//! Measured: an unknown edge operator does not fail, does not warn, and does not drop the line — it
//! collapses the whole line into ONE node whose label is a raw fragment of the user's own syntax.
//! The user then sees a box containing `A] ~~> b[B` with no signal that anything was misread, and
//! the diagram looks like it rendered.
//!
//! BOTH HALVES ARE NOW FIXED: the parser warns AND drops the node rather than drawing the fragment.
//! The rule stayed deliberately narrow — an unmatched bracket only. "Warn when a line looks like it
//! contains an unrecognised arrow" is the tempting heuristic and a bad one, because it fires on
//! legitimate labels containing `~` or `>`, and a false warning on correct input is worse than the
//! silence it replaces. The controls below are what hold that line.

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
    assert_eq!(
        labels.len(),
        2,
        "the control source must give two nodes: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "A") && labels.iter().any(|l| l == "B"),
        "the control source must give clean labels: {labels:?}"
    );
}

/// HALF OF bd-rrvr, now fixed: the SILENCE is gone.
///
/// Before: one node labelled `A] ~~> b[B` and NO warning at all. The user saw their own syntax in a
/// box with no signal that anything was misread. Now the parser says so.
#[test]
fn an_unrecognised_operator_is_no_longer_silent() {
    let parsed = fm_parser::parse("flowchart TD\n  a[A] ~~> b[B]\n");

    assert!(
        !parsed.warnings.is_empty(),
        "the source produced neither a clean parse nor a warning"
    );
}

/// THE OTHER HALF, now closed: the raw fragment is no longer drawn as content.
///
/// Was `#[ignore]`d while it reproduced the live defect — the warning removed the silence but not
/// the garbage, and `a[A] ~~> b[B]` still yielded one node labelled `A] ~~> b[B`. The node is now
/// DROPPED rather than drawn, and the warning says so.
///
/// Kept separate rather than folded into the test above, because a single assertion that accepted
/// "warned OR parsed cleanly" would have let this half disappear from view the moment the warning
/// landed — which is exactly what nearly happened.
#[test]
fn an_unrecognised_operator_does_not_become_a_node_label() {
    let labels = labels_of("flowchart TD\n  a[A] ~~> b[B]\n");

    assert!(
        !labels.iter().any(|l| l.contains("~~>") || l.contains('[')),
        "a raw operator fragment reached a node label: {labels:?}"
    );
}

/// CONTROL: a legitimate label containing BALANCED brackets must NOT warn.
///
/// The rule is unmatched-bracket only, precisely so that `a["x [y]"]` — valid and common — stays
/// silent. The tempting rule ("warn when the label looks like it contains an arrow") would fire on
/// legitimate labels containing `~` or `>`, and a false warning on correct input is worse than the
/// silence it replaces. This is what stops the fix from being that rule by accident.
#[test]
fn a_balanced_bracket_in_a_label_does_not_warn() {
    let parsed = fm_parser::parse("flowchart TD\n  a[\"x [y] z\"] --> b[B]\n");

    assert!(
        parsed.warnings.is_empty(),
        "a legitimate bracketed label warned: {:?}",
        parsed.warnings
    );
    // Non-vacuity: the source must actually have produced the two nodes, or the silence above is
    // just a parse failure wearing a control's clothes.
    assert_eq!(
        parsed.ir.nodes.len(),
        2,
        "the control source did not parse into two nodes, so its silence proves nothing"
    );
}

/// CONTROL: the warning names the line and quotes the label.
///
/// A warning a user cannot act on is barely better than silence, so this pins that the message
/// carries both the location and the offending text.
#[test]
fn the_warning_identifies_the_line_and_the_label() {
    let parsed = fm_parser::parse("flowchart TD\n  a[A] ~~> b[B]\n");

    assert!(
        parsed
            .warnings
            .iter()
            .any(|w| w.contains("Line 2") && w.contains("A] ~~> b[B")),
        "the warning does not identify the line and the label: {:?}",
        parsed.warnings
    );
}

/// THE CASE THE INCUMBENT SETTLES, and the reason the rule drops rather than merely warns.
///
/// mermaid 11.15.0 (pinned bundle, evaluated in the same browserless `node:vm` sandbox
/// `mermaid_bench.mjs` uses) THROWS on this source: `Parse error on line 2`. The incumbent refuses
/// the line outright. We drew it as a box containing `Unclosed --> b[B` — the user's own syntax
/// presented as their content, with the diagram looking like it rendered.
///
/// This is the measured half of the grounding. The `~~>` case above could NOT be settled the same
/// way: mermaid's flowchart parse path reaches DOMPurify, which needs a DOM that sandbox
/// deliberately withholds, so it reports DNF — and stubbing a DOM to get an answer is the trade
/// that file's own comment warns against. Both cases take the same unmatched-bracket rule, so the
/// policy rests on the one that was actually measured.
#[test]
fn an_unclosed_bracket_is_dropped_rather_than_drawn() {
    let parsed = fm_parser::parse("flowchart TD\n  a[Unclosed --> b[B]\n");
    let labels = labels_of("flowchart TD\n  a[Unclosed --> b[B]\n");

    assert!(
        !parsed.warnings.is_empty(),
        "an unclosed bracket must warn; the incumbent errors outright on this source"
    );
    assert!(
        !labels.iter().any(|l| l.contains('[') || l.contains("-->")),
        "a raw source fragment reached a node label: {labels:?}"
    );
}

/// NON-VACUITY FOR THE DROP RULE, and it is the assertion that stops this fix from being "drop
/// everything". A malformed line must not take a WELL-FORMED neighbour down with it.
///
/// Without this, a rule that refused the whole diagram would satisfy every assertion above while
/// destroying more content than the defect it fixes — trading a garbage label for a blank page.
#[test]
fn a_malformed_line_does_not_drop_its_well_formed_neighbours() {
    let labels = labels_of("flowchart TD\n  a[A] --> b[B]\n  c[C] ~~> d[D]\n  e[E] --> f[F]\n");

    for want in ["A", "B", "E", "F"] {
        assert!(
            labels.iter().any(|l| l == want),
            "well-formed node {want:?} was dropped by a malformed neighbour: {labels:?}"
        );
    }
    assert!(
        !labels.iter().any(|l| l.contains("~~>")),
        "the malformed line still reached a label: {labels:?}"
    );
}
