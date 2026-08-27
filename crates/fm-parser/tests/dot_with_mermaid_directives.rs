//! A Mermaid directive appended to a DOT document must not change what the document IS (bd-pdz8z).
//!
//! THE SYMPTOM THAT SURFACED IT was narrow: `deck_directive_is_inert_on_dot_input` asserted a
//! `%%{deck: …}%%` directive stays inert on DOT input, and it had been RED on main. The cause is
//! much wider than the deck.
//!
//! `looks_like_dot` ends with "a well-formed DOT file ends on its body's closing brace", which
//! disambiguates a real DOT `graph` from a Mermaid flowchart whose first node happens to be
//! brace-shaped (`graph\n  A{decision} --> B`). `strip_all_comments` removes DOT's own comment
//! forms and knows nothing about Mermaid's `%%…%%`, so ANY directive appended to a DOT document
//! left the text ending in `%%` — the check said "not DOT", and the document went to the Mermaid
//! parser instead. Measured before the fix:
//!
//! ```text
//! digraph G { a -> b }                       DotFormat     2 nodes
//! digraph G { a -> b } + %%{deck: …}%%       FuzzyKeyword  1 node    <- the graph is gone
//! digraph G { a -> b } + %%{init: …}%%       FuzzyKeyword  1 node
//! ```
//!
//! So the deck leak was the visible corner of a defect that DESTROYS the graph: an edge and a node
//! simply disappeared. That is why these tests assert the graph, not just the deck.
//!
//! ⚠️ AND FIXING DETECTION ALONE WAS NOT ENOUGH. `extract_body` spans the first `{` to the LAST
//! `}`, so once the document was correctly routed to the DOT parser the directive's own braces
//! extended the body and it came back with SEVEN nodes. Detection and parsing now share one
//! `strip_trailing_mermaid_directives`, because two answers to "where does this document end" is
//! precisely how they came to disagree.

use fm_core::DiagramType;
use fm_parser::{DetectionMethod, detect_type_with_confidence, parse};

const PLAIN: &str = "digraph G {\n  a -> b\n}\n";

fn with_tail(tail: &str) -> String {
    format!("{PLAIN}{tail}")
}

/// THE CONTROL: a directive must not change the detected format.
#[test]
fn a_trailing_directive_does_not_change_the_detected_format() {
    let baseline = detect_type_with_confidence(PLAIN);
    assert_eq!(
        baseline.method,
        DetectionMethod::DotFormat,
        "the plain document is not detected as DOT, so this test proves nothing"
    );

    for tail in [
        "%%{deck: {slides: [{id: 's', nodes: ['a']}]}}%%\n",
        "%%{init: {'theme':'dark'}}%%\n",
        "%%{init: {'theme':'dark'}}%%\n%%{deck: {slides: []}}%%\n",
    ] {
        let detected = detect_type_with_confidence(&with_tail(tail));
        assert_eq!(
            detected.method,
            DetectionMethod::DotFormat,
            "a trailing directive re-routed the document away from DOT: {tail:?}"
        );
    }
}

/// THE REAL DEFECT: the graph itself must survive.
///
/// This is the assertion the original bead test could not make. It checked only that `ir.deck` was
/// `None`; the document was meanwhile being parsed as a Mermaid flowchart and losing an edge and a
/// node. Comparing NODE AND EDGE COUNTS against the same document without its directive is what
/// makes that visible — and a count comparison cannot be satisfied by the deck merely being absent.
#[test]
fn a_trailing_directive_does_not_destroy_the_graph() {
    let baseline = parse(PLAIN);
    assert_eq!(baseline.ir.nodes.len(), 2, "the baseline graph is wrong");
    assert_eq!(baseline.ir.edges.len(), 1, "the baseline graph is wrong");

    for tail in [
        "%%{deck: {slides: [{id: 's', nodes: ['a']}]}}%%\n",
        "%%{init: {'theme':'dark'}}%%\n",
    ] {
        let parsed = parse(&with_tail(tail));
        assert_eq!(
            parsed.ir.nodes.len(),
            baseline.ir.nodes.len(),
            "a trailing directive changed the node count: {tail:?}"
        );
        assert_eq!(
            parsed.ir.edges.len(),
            baseline.ir.edges.len(),
            "a trailing directive changed the edge count: {tail:?}"
        );
    }
}

/// ⚠️ AND THE DIRECTIVE'S OWN BRACES MUST NOT BECOME GRAPH CONTENT.
///
/// The second half of the defect, invisible until the first was fixed: with detection corrected but
/// `extract_body` still spanning to the last `}`, a `%%{deck: {slides: […]}}%%` tail turned into
/// SEVEN nodes. A node count that merely differs from one is not enough to catch that — it has to
/// equal the baseline exactly, which the test above asserts, so this one names the shape directly.
#[test]
fn the_directives_own_braces_do_not_become_nodes() {
    let parsed = parse(&with_tail(
        "%%{deck: {slides: [{id: 's', nodes: ['a'], title: 'T'}]}}%%\n",
    ));
    let ids: Vec<&str> = parsed.ir.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["a", "b"],
        "the directive's contents were parsed as graph nodes"
    );
}

/// The deck stays inert, which is what the bead's own test asserts.
#[test]
fn a_deck_directive_stays_inert_on_dot_input() {
    let parsed = parse(&with_tail(
        "%%{deck: {slides: [{id: 's', nodes: ['a']}]}}%%\n",
    ));
    assert_eq!(parsed.ir.diagram_type, DiagramType::Flowchart);
    assert!(
        parsed.ir.deck.is_none(),
        "a deck directive reached ir.deck on DOT input"
    );
}

/// ⚠️ THE DISAMBIGUATION THE BRACE CHECK EXISTS FOR MUST SURVIVE.
///
/// `graph\n  A{decision} --> B` is a Mermaid flowchart whose first node is brace-shaped, and it is
/// only told apart from a DOT `graph` named `A` by not ending on its brace. Dropping trailing
/// directives must not weaken that: with its directives gone the text still ends on `B`.
///
/// This is the assertion that stops the fix being a loosening rather than a correction.
#[test]
fn a_brace_shaped_flowchart_node_is_still_not_dot() {
    for source in [
        "graph\n  A{decision} --> B\n",
        "graph\n  A{decision} --> B\n%%{init: {'theme':'dark'}}%%\n",
    ] {
        let detected = detect_type_with_confidence(source);
        assert_ne!(
            detected.method,
            DetectionMethod::DotFormat,
            "a brace-shaped flowchart node was read as a DOT body: {source:?}"
        );
    }
}

/// A document that is nothing but directives is not DOT.
///
/// The degenerate case for the trailing strip: if everything is dropped there is no body brace
/// left, and reporting DOT for an empty remainder would be inventing a graph.
#[test]
fn a_document_of_only_directives_is_not_dot() {
    let detected = detect_type_with_confidence("%%{init: {'theme':'dark'}}%%\n");
    assert_ne!(detected.method, DetectionMethod::DotFormat);
}

/// Plain DOT, with no directive anywhere, is completely unaffected.
#[test]
fn plain_dot_is_unchanged() {
    let parsed = parse(PLAIN);
    assert_eq!(parsed.detection_method, DetectionMethod::DotFormat);
    assert_eq!(parsed.ir.nodes.len(), 2);
    assert_eq!(parsed.ir.edges.len(), 1);
    // Comments still work, which is the path `strip_all_comments` owns and this change sits beside.
    let commented = parse("digraph G {\n  // an edge\n  a -> b\n}\n");
    assert_eq!(commented.ir.nodes.len(), 2);
    assert_eq!(commented.ir.edges.len(), 1);
}
