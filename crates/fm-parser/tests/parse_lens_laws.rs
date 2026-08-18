//! The ParseLens must obey the lens laws it is defined by (bd-1t7l.1).
//!
//! The bead states the foundation as a lens `L = (get, put)` whose FormatComplement exists so that
//! "whitespace, comments, and formatting survive the round-trip". Substantial machinery implements
//! that — a complement carrying line endings, whitespace, comments, directives and quoted literals,
//! a source map, bindings, and edit application — and NOTHING asserted the laws it was built to
//! satisfy.
//!
//! That is the dangerous shape for this kind of code: an edit that quietly eats a comment or
//! normalises CRLF to LF still returns a valid diagram, still renders, and still passes every test
//! that checks the AST. The damage lands in the user's file, and only in files that had comments or
//! unusual formatting — so the common fixture never shows it.
//!
//! The laws, in the standard formulation:
//!
//!   GetPut  — putting back an UNCHANGED view returns the original source, byte for byte.
//!   PutGet  — getting the view of an edited source returns the edit that was made.
//!   PutPut  — a second edit supersedes the first rather than compounding it.
//!
//! Every test here uses an element id taken from the snapshot's own bindings rather than a
//! hardcoded string, so a change in id scheme fails as a missing binding rather than by silently
//! testing nothing.

use fm_core::MermaidLensEdit;
use fm_parser::{apply_parse_lens_edit, build_parse_lens};

/// A source deliberately full of the things the complement exists to protect.
const FORMATTED: &str = "%%{init: {'theme':'forest'}}%%\nflowchart TD\n\n  %% the first comment\n  a[Alpha]   -->   b[Beta]\n\n  %% a trailing note\n  b --> c[\"Gamma with spaces\"]\n";

fn first_node_binding(source: &str) -> String {
    let snapshot = build_parse_lens(source);
    snapshot
        .bindings
        .iter()
        .find(|binding| binding.snippet.is_some() && binding.text_range.is_some())
        .map(|binding| binding.element_id.clone())
        .expect("the lens should bind at least one addressable element")
}

/// GETPUT: the snapshot must hand back exactly what it was given.
///
/// The weakest law, and the one that catches a complement that normalises on capture — trimming
/// trailing whitespace, or rewriting CRLF — before any edit is even attempted.
#[test]
fn get_put_returns_the_source_byte_for_byte() {
    for source in [
        FORMATTED,
        "flowchart LR\r\n  a --> b\r\n",
        "flowchart TD\n  a --> b",
        "",
    ] {
        let snapshot = build_parse_lens(source);
        assert_eq!(
            snapshot.original_source(),
            source,
            "the snapshot altered the source it was given"
        );
    }
}

/// PUTGET: after an edit, the new source must actually contain the replacement.
#[test]
fn put_get_reflects_the_edit_in_the_new_source() {
    let element_id = first_node_binding(FORMATTED);
    let response = apply_parse_lens_edit(
        FORMATTED,
        &MermaidLensEdit {
            element_id: element_id.clone(),
            replacement: String::from("Renamed"),
        },
    )
    .expect("editing a bound element should succeed");

    assert!(
        response.result.updated_source.contains("Renamed"),
        "the replacement is absent from the updated source"
    );
    assert_eq!(
        response.snapshot.original_source(),
        response.result.updated_source,
        "the returned snapshot does not describe the source it reports"
    );
}

/// THE LAW THAT MATTERS: an edit must not disturb the formatting it did not touch.
///
/// This is the complement's entire justification. Asserted structurally rather than by comparing
/// whole strings, so the test says WHICH property was lost when it fails.
#[test]
fn an_edit_preserves_every_part_of_the_complement_it_did_not_touch() {
    let element_id = first_node_binding(FORMATTED);
    let response = apply_parse_lens_edit(
        FORMATTED,
        &MermaidLensEdit {
            element_id,
            replacement: String::from("Renamed"),
        },
    )
    .expect("editing a bound element should succeed");
    let updated = &response.result.updated_source;

    assert!(
        updated.contains("%% the first comment"),
        "an unrelated comment was lost by an edit elsewhere:\n{updated}"
    );
    assert!(
        updated.contains("%% a trailing note"),
        "a trailing comment was lost:\n{updated}"
    );
    assert!(
        updated.contains("%%{init:"),
        "the directive block was lost:\n{updated}"
    );
    assert_eq!(
        updated.matches("\n\n").count(),
        FORMATTED.matches("\n\n").count(),
        "blank lines were added or removed:\n{updated}"
    );
    assert_eq!(
        updated.ends_with('\n'),
        FORMATTED.ends_with('\n'),
        "the trailing newline changed:\n{updated}"
    );
    assert!(
        updated.contains("Gamma with spaces"),
        "a quoted literal was altered:\n{updated}"
    );
}

/// CRLF must survive an edit.
///
/// Separate from the test above because it is the failure that hurts most and shows least: on a
/// Windows checkout, normalising line endings rewrites EVERY line of the file, so the user's next
/// diff is unreadable even though the diagram is unchanged.
#[test]
fn an_edit_does_not_rewrite_line_endings() {
    let source = "flowchart TD\r\n  a[Alpha] --> b[Beta]\r\n";
    let element_id = first_node_binding(source);
    let response = apply_parse_lens_edit(
        source,
        &MermaidLensEdit {
            element_id,
            replacement: String::from("Renamed"),
        },
    )
    .expect("editing a bound element should succeed");
    let updated = &response.result.updated_source;

    assert_eq!(
        updated.matches("\r\n").count(),
        source.matches("\r\n").count(),
        "CRLF count changed:\n{updated:?}"
    );
    assert!(
        !updated.contains("\n\r"),
        "line endings were mangled rather than preserved:\n{updated:?}"
    );
}

/// PUTPUT: a second edit supersedes the first rather than compounding it.
///
/// Uses the SNAPSHOT RETURNED BY THE FIRST EDIT to address the second, which is the contract
/// `apply_parse_lens_delete` documents: ids and spans shift after every edit, so reusing a stale
/// snapshot addresses the wrong bytes.
#[test]
fn put_put_supersedes_rather_than_compounds() {
    let source = "flowchart TD\n  a[Alpha] --> b[Beta]\n";
    let element_id = first_node_binding(source);

    let first = apply_parse_lens_edit(
        source,
        &MermaidLensEdit {
            element_id,
            replacement: String::from("One"),
        },
    )
    .expect("first edit");

    let second_id = first
        .snapshot
        .bindings
        .iter()
        .find(|binding| binding.snippet.is_some() && binding.text_range.is_some())
        .map(|binding| binding.element_id.clone())
        .expect("the re-snapshot should bind the edited element");

    let second = apply_parse_lens_edit(
        first.snapshot.original_source(),
        &MermaidLensEdit {
            element_id: second_id,
            replacement: String::from("Two"),
        },
    )
    .expect("second edit");

    assert!(
        second.result.updated_source.contains("Two"),
        "the second edit did not apply:\n{}",
        second.result.updated_source
    );
    assert!(
        !second.result.updated_source.contains("One"),
        "the first replacement survived, so edits compounded instead of superseding:\n{}",
        second.result.updated_source
    );
}

/// CONTROL: an unknown element id is refused, not silently ignored.
///
/// Without this, an implementation that returned the source unchanged for any unrecognised id would
/// satisfy every preservation test above — perfectly, since it would never change anything.
#[test]
fn an_unknown_element_id_is_an_error() {
    let result = apply_parse_lens_edit(
        FORMATTED,
        &MermaidLensEdit {
            element_id: String::from("no-such-element-id"),
            replacement: String::from("Renamed"),
        },
    );

    assert!(
        result.is_err(),
        "an unknown element id was accepted, so a typo would silently do nothing"
    );
}
