//! A sequence `note` must be recognised whatever case it is written in (bd-am6a2).
//!
//! The parser required a capital `Note `, so `note right of Bob: text` — the spelling mermaid's own
//! docs use, and the one the STATE parser in the same file already accepts — was SILENTLY DROPPED.
//! No note box, no text, no warning, on all four placements. A silent drop is the worst failure mode
//! for a keyword whose case an author has no reason to think matters.
//!
//! Three independent lines of evidence, none of them a style preference:
//!
//!   - THE PINNED INCUMBENT returns a clean `PARSED` for `note`, `NOTE` and even `Note Right Of`.
//!   - THIS REPO'S OWN state parser accepts lowercase `note` and renders it, so two diagram types
//!     disagreed about the same keyword.
//!   - The failure was silent, so nothing in the corpus could have caught it by going red.
//!
//! An actor NAMED `note` needs no protection: mermaid rejects `note->>Bob: hi` as a syntax error, so
//! the word is reserved on both sides. That is asserted below rather than assumed.
//!
//! Found by sweeping every diagram type for declared text that never reaches a rendered `<text>` run
//! — the family behind bd-u3fo, bd-jgco, bd-rk14 and bd-59o4.

/// Text drawn in `<text>`/`<tspan>` runs. Deliberately NOT `svg.contains`: the accessibility
/// `<desc>` repeats labels, so a whole-document check reports a dropped note as present.
fn drawn(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text").or_else(|| rest.find("<tspan")) {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(end) = rest.find('<') else { break };
        let text = rest[..end].trim();
        if !text.is_empty() {
            out.push(text.to_string());
        }
    }
    out
}

fn render(statement: &str) -> String {
    let source = format!("sequenceDiagram\n  Alice->>Bob: hi\n  {statement}\n");
    fm_render_svg::render_svg(&fm_parser::parse(&source).ir)
}

/// EVERY CASING of the keyword and its position words reaches the diagram.
#[test]
fn a_sequence_note_is_recognised_in_any_case() {
    for statement in [
        "Note right of Bob: seqNote",
        "note right of Bob: seqNote",
        "NOTE right of Bob: seqNote",
        "Note Right Of Bob: seqNote",
        "nOtE RiGhT oF Bob: seqNote",
    ] {
        let svg = render(statement);
        assert!(
            drawn(&svg).iter().any(|run| run == "seqNote"),
            "{statement:?} produced no note text; drawn runs were {:?}",
            drawn(&svg)
        );
        assert!(
            svg.contains("fm-sequence-note"),
            "{statement:?} produced no note box"
        );
    }
}

/// ALL FOUR PLACEMENTS work in lowercase, not just the one the bug was found with.
#[test]
fn every_note_placement_works_in_lowercase() {
    for statement in [
        "note left of Alice: seqNote",
        "note right of Bob: seqNote",
        "note over Bob: seqNote",
        "note over Alice,Bob: seqNote",
    ] {
        assert!(
            drawn(&render(statement)).iter().any(|run| run == "seqNote"),
            "{statement:?} produced no note text"
        );
    }
}

/// CONTROL: the capital spelling that always worked still works.
///
/// The fix widens a match; this is what proves it widened rather than moved.
#[test]
fn the_capital_spelling_still_works() {
    let svg = render("Note over Alice,Bob: seqNote");
    assert!(drawn(&svg).iter().any(|run| run == "seqNote"));
    // NON-VACUITY: the rest of the diagram is still there.
    for expected in ["Alice", "Bob", "hi"] {
        assert!(
            drawn(&svg).iter().any(|run| run == expected),
            "CONTROL FAILED: {expected:?} is missing, so the diagram did not render"
        );
    }
}

/// CONTROL: a line that merely BEGINS with the keyword is not swallowed as a note.
///
/// `parse_sequence_note` returns `None` unless a position keyword follows, so a message from an
/// actor whose name starts with `note` stays a message. Without this, making the keyword
/// case-insensitive could have widened the match into ordinary content — the shape of the bd-ij0f
/// regression this repo already paid for once.
#[test]
fn a_message_is_not_swallowed_by_the_widened_keyword() {
    let svg =
        fm_render_svg::render_svg(&fm_parser::parse("sequenceDiagram\n  notebook->>Bob: hi\n").ir);
    let runs = drawn(&svg);
    assert!(
        runs.iter().any(|run| run == "notebook"),
        "an actor named `notebook` was swallowed: {runs:?}"
    );
    assert!(
        runs.iter().any(|run| run == "hi"),
        "the message text was lost: {runs:?}"
    );
}
