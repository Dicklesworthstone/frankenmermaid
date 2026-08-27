//! Edge labels and state descriptions get the same normalization node labels do (bd-j06n2).
//!
//! THE ROOT CAUSE IS A NAME COLLISION, and it is why bd-kmtia's fix reached node labels and stopped
//! there. `fm-parser` contained TWO private functions called `clean_label`:
//!
//! ```text
//!   mermaid_parser::clean_label   quote strip + markdown + <br> + entity decode  (node labels)
//!   ir_builder::clean_label       quote strip, and nothing else                  (edge labels,
//!                                                                                 5 title sites)
//! ```
//!
//! One crate, one name, two behaviours. Fixing the first left the second untouched — the
//! duplicated-helper trap, where the copy gets fixed and its fork does not. `ir_builder`'s is now a
//! delegating wrapper, so there is ONE implementation.
//!
//! ⚠️ THE GAP WAS THREE NORMALIZATIONS WIDE, NOT ONE. The bead was filed about `<br/>`; probing the
//! IR directly showed the same split for HTML entities and numeric codes, which is what identified a
//! shared cause rather than three separate omissions:
//!
//! ```text
//!   input                     node label      edge label (before)
//!   one<br/>two               "one\ntwo"      "one<br/>two"
//!   a &amp; b                 "a & b"         "a &amp; b"
//!   C#35;44                   "C#44"          "C#35;44"
//! ```
//!
//! A state description went through NEITHER function and needed its own route.
//!
//! ⚠️ ONE STATE CASE WAS STILL WRONG WHEN THIS LANDED, AND IS NOW FIXED. `s1 : a &amp; b` came out
//! as `a &amp` — TRUNCATED — because the `;` inside the entity was taken as a statement separator
//! before any label normalizer saw the text: data loss in the splitter, a different and deeper bug
//! than a missing decode. It was filed as bd-idjwr rather than patched here, and the test below
//! pinned the BROKEN behaviour. bd-idjwr's fix then made that test fail with an instruction to
//! update it, which is what the pin was for — the follow-up could not land quietly.

fn label_texts(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    ir.labels.iter().map(|l| l.text.clone()).collect()
}

fn edge_label(source: &str) -> String {
    let parsed = fm_parser::parse(source);
    let ir = &parsed.ir;
    ir.edges
        .iter()
        .find_map(|e| e.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.clone())
        .unwrap_or_else(|| {
            panic!(
                "no edge label in {source:?}; labels: {:?}",
                label_texts(source)
            )
        })
}

const ARROW: &str = "-->";

/// ⚠️ THE NEGATIVE CASE: an edge label is normalized EXACTLY as the same text in a node label.
///
/// Asserting the edge label against a literal would pass while the two paths still disagreed about
/// some other input. Comparing the two paths on the same text is what makes this a statement about
/// the collision rather than about three constants — and it is the assertion that fails if a fourth
/// normalization is ever added to one path only.
#[test]
fn an_edge_label_is_normalized_like_a_node_label() {
    for raw in [
        "one<br/>two",
        "one<br>two",
        "a &amp; b",
        "C#35;44",
        "a &lt; b",
        "plain text",
    ] {
        let node = fm_parser::parse(&format!("flowchart LR\n  A[\"{raw}\"] {ARROW} B\n"));
        let node_text = node
            .ir
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .and_then(|n| n.label)
            .and_then(|id| node.ir.labels.get(id.0))
            .map(|l| l.text.clone())
            .expect("node A has a label");

        let edge_text = edge_label(&format!("flowchart LR\n  A {ARROW}|\"{raw}\"| B\n"));

        assert_eq!(
            edge_text, node_text,
            "the edge and node paths disagree about {raw:?}"
        );
    }
}

/// And the normalization actually happened — not just "both paths agree on the raw text".
///
/// If the delegation had gone the other way, both paths would agree by doing nothing, and the test
/// above would pass. These are the three conversions, spelled out.
#[test]
fn the_edge_label_conversions_really_ran() {
    assert_eq!(
        edge_label(&format!("flowchart LR\n  A {ARROW}|\"one<br/>two\"| B\n")),
        "one\ntwo",
        "the line-break tag was not converted on the edge path"
    );
    assert_eq!(
        edge_label(&format!("flowchart LR\n  A {ARROW}|\"a &amp; b\"| B\n")),
        "a & b",
        "the HTML entity was not decoded on the edge path"
    );
    assert_eq!(
        edge_label(&format!("flowchart LR\n  A {ARROW}|\"C#35;44\"| B\n")),
        "C#44",
        "the numeric code was not decoded on the edge path"
    );
}

/// A state description gets the same treatment.
#[test]
fn a_state_description_is_normalized() {
    let parsed = fm_parser::parse("stateDiagram-v2\n  s1 : one<br/>two\n");
    let text = parsed
        .ir
        .nodes
        .iter()
        .find(|n| n.id == "s1")
        .and_then(|n| n.label)
        .and_then(|id| parsed.ir.labels.get(id.0))
        .map(|l| l.text.clone())
        .expect("state s1 has a description");
    assert_eq!(
        text, "one\ntwo",
        "the state description kept the tag as literal text"
    );
}

/// Two descriptions on one state still stack, each normalized.
///
/// `append_state_description` joins them with a newline, so normalizing the incoming text must not
/// disturb the join — a fix that replaced the label instead of appending would pass a single-line
/// test and silently drop the first description.
#[test]
fn stacked_state_descriptions_are_each_normalized_and_both_kept() {
    let parsed = fm_parser::parse("stateDiagram-v2\n  s1 : one<br/>two\n  s1 : three<br/>four\n");
    let text = parsed
        .ir
        .nodes
        .iter()
        .find(|n| n.id == "s1")
        .and_then(|n| n.label)
        .and_then(|id| parsed.ir.labels.get(id.0))
        .map(|l| l.text.clone())
        .expect("state s1 has a description");
    assert_eq!(
        text, "one\ntwo\nthree\nfour",
        "a stacked description was dropped or left unconverted"
    );
}

/// ⚠️ THE ONCE-KNOWN-BAD CASE, NOW FIXED — AND THE PIN IS WHY THIS NOTE IS ACCURATE.
///
/// When bd-j06n2 landed, `s1 : a &amp; b` came out as `a &amp`, truncated, because the `;` inside
/// the entity was taken as a statement separator before any label normalizer saw the text. That was
/// filed as bd-idjwr rather than patched, and this test pinned the BROKEN behaviour so the follow-up
/// would start from an assertion.
///
/// bd-idjwr then made `split_statements` ask the entity decoder whether a `;` closes a token, and
/// this test FAILED with "that is an improvement — update this test rather than deleting it". Which
/// is exactly what a pin on known-bad behaviour is for: the fix could not land quietly, and the note
/// could not go stale.
#[test]
fn a_semicolon_entity_in_a_state_description_is_no_longer_truncated() {
    let parsed = fm_parser::parse("stateDiagram-v2\n  s1 : a &amp; b\n");
    let text = parsed
        .ir
        .nodes
        .iter()
        .find(|n| n.id == "s1")
        .and_then(|n| n.label)
        .and_then(|id| parsed.ir.labels.get(id.0))
        .map(|l| l.text.clone())
        .expect("state s1 has a description");
    assert_eq!(
        text, "a & b",
        "the entity `;` truncation is back: the description lost its tail"
    );
    assert_eq!(
        parsed.ir.nodes.len(),
        1,
        "the truncated tail was interned as a second state"
    );
}

/// CONTROL: a label with nothing to normalize is byte-identical on both paths.
#[test]
fn a_label_with_nothing_to_convert_is_untouched() {
    for raw in ["hello", "two words", "a-b_c"] {
        assert_eq!(
            edge_label(&format!("flowchart LR\n  A {ARROW}|\"{raw}\"| B\n")),
            raw,
            "an ordinary edge label was rewritten"
        );
    }
}

/// CONTROL: the edge still exists, and still connects the nodes it named.
///
/// Every assertion above reads the label; none of them would notice if normalizing had cost the
/// edge its endpoints.
#[test]
fn the_edge_survives_normalization() {
    let parsed = fm_parser::parse(&format!("flowchart LR\n  A {ARROW}|\"one<br/>two\"| B\n"));
    assert_eq!(parsed.ir.edges.len(), 1, "the edge was lost");
    assert_eq!(parsed.ir.nodes.len(), 2, "an endpoint was lost");
}
