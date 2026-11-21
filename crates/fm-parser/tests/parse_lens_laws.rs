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

/// A multi-line `%%{deck: …}%%` block must survive lens round-trips byte-for-byte (bd-z2pkg).
///
/// GetPut: the snapshot hands back the decked source unchanged. PutGet-adjacent: a rename edit
/// elsewhere in the diagram leaves the entire block — including its interior JSON5 lines, which
/// classify as one directive region rather than body text — exactly as authored.
#[test]
fn a_multiline_deck_block_survives_lens_round_trips_byte_for_byte() {
    const DECKED: &str = "flowchart TD\n%%{deck: {\n  title: \"Tour\",\n  slides: [\n    { id: \"s1\", nodes: [\"a\"] },\n  ],\n}}%%\n  a[Alpha] --> b[Beta]\n";
    const DECK_BLOCK: &str = "%%{deck: {\n  title: \"Tour\",\n  slides: [\n    { id: \"s1\", nodes: [\"a\"] },\n  ],\n}}%%";

    let snapshot = build_parse_lens(DECKED);
    assert_eq!(
        snapshot.original_source(),
        DECKED,
        "GetPut: the snapshot altered a decked source"
    );

    let element_id = first_node_binding(DECKED);
    let response = apply_parse_lens_edit(
        DECKED,
        &MermaidLensEdit {
            element_id,
            replacement: String::from("Renamed"),
        },
    )
    .expect("editing a bound element in a decked source should succeed");
    let updated = &response.result.updated_source;
    assert!(
        updated.contains(DECK_BLOCK),
        "the multi-line deck block was not preserved byte-for-byte:\n{updated}"
    );
    assert!(updated.contains("Renamed"), "the edit itself was lost");
}

#[test]
fn front_matter_keeps_original_source_bindings_and_exact_edits_across_families() {
    for newline in ["\n", "\r\n"] {
        let prefix = ["---", "title: 'Café 🦀'", "# keep this metadata", "---", ""].join(newline);
        for (header, statement, replacement) in [
            (
                "flowchart LR",
                "A[Álpha] --> B[Beta]",
                "A[Álpha] -.-> B[Beta]",
            ),
            ("sequenceDiagram", "A->>B: Café", "A-->>B: Café"),
            ("classDiagram", "A <|-- B", "A <|.. B"),
            ("stateDiagram-v2", "A --> B: Café", "A --> B: Ready"),
            ("erDiagram", "A ||--o{ B : owns", "A ||--o{ B : contains"),
            ("sankey-beta", "Café,Beta,10", "Café,Beta,20"),
        ] {
            let source = format!("{prefix}{header}{newline}{statement}{newline}");
            let snapshot = build_parse_lens(&source);
            let edge = snapshot
                .bindings
                .iter()
                .find(|binding| binding.kind == fm_core::MermaidSourceMapKind::Edge)
                .expect("the diagram contains an addressable edge");
            assert_eq!(edge.span.start.line, 6, "{header}: source line");
            assert_eq!(
                edge.snippet.as_deref(),
                Some(statement),
                "{header}: source text"
            );
            let range = edge.text_range.expect("original source range");
            assert_eq!(&source[range.start_byte..range.end_byte], statement);
            assert_eq!(snapshot.parsed.ir.meta.title.as_deref(), Some("Café 🦀"));

            let edited = snapshot
                .apply_edit(&MermaidLensEdit {
                    element_id: edge.element_id.clone(),
                    replacement: replacement.to_owned(),
                })
                .expect("front matter must not prevent a diagram edit");
            assert_eq!(edited.previous_snippet, statement);
            assert_eq!(
                edited.updated_source,
                format!("{prefix}{header}{newline}{replacement}{newline}")
            );
        }
    }
}

#[test]
fn front_matter_keeps_multiline_csv_and_deck_byte_offsets() {
    for newline in ["\n", "\r\n"] {
        let prefix = ["---", "title: 'Café 🦀'", "---", ""].join(newline);
        let record = format!("\"Source{newline}continued\",Target,10");
        let deck =
            format!("%%{{deck: {{{newline}slides: [{{id: 'intro', nodes: ['*']}}]{newline}}}}}%%");
        let source = format!("{prefix}sankey-beta{newline}{record}{newline}{deck}{newline}");
        let snapshot = build_parse_lens(&source);
        let edge = snapshot
            .bindings
            .iter()
            .find(|binding| binding.kind == fm_core::MermaidSourceMapKind::Edge)
            .expect("multiline CSV edge");
        assert_eq!(edge.span.start.line, 5);
        assert_eq!(edge.span.end.line, 6);
        assert_eq!(edge.snippet.as_deref(), Some(record.as_str()));
        let replacement = record.replace(",10", ",25");
        let edited = snapshot
            .apply_edit(&MermaidLensEdit {
                element_id: edge.element_id.clone(),
                replacement: replacement.clone(),
            })
            .expect("multiline source edit");
        assert_eq!(
            edited.updated_source,
            format!("{prefix}sankey-beta{newline}{replacement}{newline}{deck}{newline}")
        );

        let span = snapshot
            .parsed
            .ir
            .deck
            .as_ref()
            .expect("deck directive")
            .span;
        assert_eq!(span.start.line, 7);
        assert_eq!(span.start.byte as usize, source.find("%%{deck:").unwrap());
        let end = span.end.byte as usize;
        assert_eq!(source[span.start.byte as usize..end].trim_end(), deck);
    }
}

#[test]
fn front_matter_and_fuzzy_headers_preserve_deck_locations_and_diagnostics() {
    let source = concat!(
        "---\r\ntitle: 'Café 🦀'\r\n---\r\nflowchat LR\r\n",
        "A --> B\r\n",
        "%%{init: {unknownOption: true}}%%\r\n",
        "%%{deck: {\r\nslides: [{id: 'intro', nodes: ['*']}]\r\n}}%%\r\n",
    );
    let parsed = fm_parser::parse(source);
    assert_eq!(
        parsed.detection_method,
        fm_parser::DetectionMethod::FuzzyKeyword
    );
    assert_eq!(parsed.ir.nodes.len(), 2);
    assert_eq!(parsed.ir.edges[0].span.start.line, 5);
    let deck = parsed.ir.deck.expect("deck survives header recovery");
    assert_eq!(deck.span.start.line, 7);
    assert_eq!(
        deck.span.start.byte as usize,
        source.find("%%{deck:").unwrap()
    );
    assert!(
        parsed
            .ir
            .meta
            .init
            .errors
            .iter()
            .any(|error| error.to_string().contains("Line 6"))
    );
    let validation = fm_parser::validate_mermaid_init_directives(source);
    assert!(
        validation
            .errors
            .iter()
            .any(|error| error.field.starts_with("init[6]."))
    );
}

#[test]
fn front_matter_literal_directives_are_not_validated_as_diagram_configuration() {
    let source = concat!(
        "---\r\ntitle: |\r\n  %%{init: {unknownMetadata: true}}%%\r\n---\r\n",
        "flowchart LR\r\nA --> B\r\n%%{init: {unknownDiagramOption: true}}%%\r\n",
    );
    let validation = fm_parser::validate_mermaid_init_directives(source);
    assert!(!validation.errors.is_empty());
    assert!(
        validation
            .errors
            .iter()
            .all(|error| error.field.starts_with("init[7]."))
    );
}
