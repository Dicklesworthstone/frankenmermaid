//! Lens law verification for `build_parse_lens` / `apply_lens_edit` (bd-1t7l.1).
//!
//! The lens is a TEXT-SPLICE lens: an edit replaces the byte range a source-map span resolves to.
//! The laws below are the ones that shape has, stated so they cannot be satisfied by accident:
//!
//! - **GetPut / identity**: replacing an element with the text already there must reproduce the
//!   source byte for byte. This is the load-bearing one — it fails the moment a span resolves to
//!   the wrong bytes, which is exactly the bug that would silently corrupt an editor's buffer.
//! - **Locality**: an edit must change only the resolved range. Everything before and after is
//!   compared explicitly, so an off-by-one that eats a neighbouring character is caught.
//! - **Snippet honesty**: the reported `previous_snippet` must equal the source text at the
//!   reported range, or a client cannot trust an undo built from it.
//!
//! The corpus is every golden `.mmd` fixture plus formatting-heavy cases (CRLF, comments,
//! directives, quoted labels, unicode) because the format complement exists for those.

use std::path::{Path, PathBuf};

use fm_core::{MermaidLensEdit, apply_lens_edit, resolve_span_text_range};
use fm_parser::build_parse_lens;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Every golden fixture, as (name, source).
fn golden_corpus() -> Vec<(String, String)> {
    let mut corpus: Vec<(String, String)> = std::fs::read_dir(golden_dir())
        .expect("read golden fixture dir")
        .filter_map(|entry| {
            let path = entry.expect("golden dir entry").path();
            if path.extension().is_some_and(|ext| ext == "mmd") {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("fixture name utf-8")
                    .to_string();
                let source = std::fs::read_to_string(&path).expect("read fixture");
                Some((name, source))
            } else {
                None
            }
        })
        .collect();
    // Deterministic order so a failure names the same fixture on every run.
    corpus.sort_by(|left, right| left.0.cmp(&right.0));
    corpus
}

/// Formatting shapes the format complement is specifically responsible for preserving.
fn formatting_corpus() -> Vec<(String, String)> {
    [
        ("crlf_line_endings", "flowchart LR\r\n  A-->B\r\n  B-->C\r\n"),
        ("no_trailing_newline", "flowchart LR\n  A-->B"),
        (
            "comments_between_edges",
            "flowchart LR\n  %% leading comment\n  A-->B\n  %% trailing comment\n  B-->C\n",
        ),
        (
            "init_directive",
            "%%{init: {\"theme\": \"dark\"}}%%\nflowchart LR\n  A-->B\n",
        ),
        (
            "quoted_labels_both_styles",
            "flowchart LR\n  A[\"double quoted\"]-->B['single quoted']\n",
        ),
        (
            "unicode_labels",
            "flowchart LR\n  A[éclair]-->B[日本語]\n  B-->C[emoji 🎯]\n",
        ),
        (
            "blank_lines_and_deep_indent",
            "flowchart TD\n\n\n        A-->B\n\n    B-->C\n\n",
        ),
        (
            "subgraph_nesting",
            "flowchart LR\n  subgraph one\n    A-->B\n  end\n  subgraph two\n    C-->D\n  end\n  B-->C\n",
        ),
        (
            "sequence_with_notes",
            "sequenceDiagram\n  participant A\n  participant B\n  A->>B: hello\n  Note over A,B: a note\n",
        ),
        (
            "class_with_members",
            "classDiagram\n  class Foo {\n    +int bar\n    +baz() void\n  }\n  Foo <|-- Qux\n",
        ),
    ]
    .into_iter()
    .map(|(name, source)| (name.to_string(), source.to_string()))
    .collect()
}

fn full_corpus() -> Vec<(String, String)> {
    let mut corpus = golden_corpus();
    corpus.extend(formatting_corpus());
    corpus
}

#[test]
fn corpus_is_large_and_diverse_enough_to_mean_something() {
    let corpus = full_corpus();
    assert!(
        corpus.len() >= 45,
        "expected the golden fixtures plus formatting cases, got {}",
        corpus.len()
    );

    // A corpus of sources with no lens bindings would pass every law vacuously, so count the
    // elements actually exercised rather than the files.
    let elements: usize = corpus
        .iter()
        .map(|(_, source)| build_parse_lens(source).source_map.entries.len())
        .sum();
    assert!(
        elements >= 100,
        "the laws must be exercised over 100+ addressable elements, got {elements}"
    );
    println!(
        "{{\"corpus_files\":{},\"addressable_elements\":{elements}}}",
        corpus.len()
    );
}

#[test]
fn getput_identity_holds_for_every_addressable_element() {
    let mut checked = 0_usize;
    for (name, source) in full_corpus() {
        let snapshot = build_parse_lens(&source);
        assert_eq!(
            snapshot.original_source(),
            source,
            "[{name}] the snapshot must retain its exact source"
        );

        for entry in &snapshot.source_map.entries {
            let Some(range) = resolve_span_text_range(&source, entry.span) else {
                // An unresolvable span is not editable, so there is no identity law to check. It is
                // reported rather than skipped silently if it is the ONLY outcome (see the
                // resolvable-count assertion below).
                continue;
            };
            let current = source
                .get(range.start_byte..range.end_byte)
                .expect("resolved range must be in bounds and on char boundaries");

            let result = snapshot
                .apply_edit(&MermaidLensEdit {
                    element_id: entry.element_id.clone(),
                    replacement: current.to_string(),
                })
                .unwrap_or_else(|error| {
                    panic!(
                        "[{name}] no-op edit of {} failed: {error}",
                        entry.element_id
                    )
                });

            assert_eq!(
                result.updated_source, source,
                "[{name}] replacing {} with its own text must reproduce the source byte for byte",
                entry.element_id
            );
            assert_eq!(
                result.previous_snippet, current,
                "[{name}] reported previous_snippet must equal the text at the reported range for {}",
                entry.element_id
            );
            checked += 1;
        }
    }

    assert!(
        checked >= 100,
        "the identity law must have been checked on 100+ elements, got {checked}"
    );
}

#[test]
fn an_edit_changes_only_the_resolved_range() {
    // Locality is what makes the lens safe in an editor: an off-by-one that eats a neighbouring
    // character would still round-trip under the identity law if the same off-by-one applied both
    // ways, so the prefix and suffix are compared explicitly against the ORIGINAL source.
    const REPLACEMENT: &str = "ZZ_LENS_PROBE_ZZ";
    let mut checked = 0_usize;

    for (name, source) in full_corpus() {
        let snapshot = build_parse_lens(&source);
        for entry in &snapshot.source_map.entries {
            let Some(range) = resolve_span_text_range(&source, entry.span) else {
                continue;
            };

            let result = apply_lens_edit(
                &source,
                &snapshot.source_map,
                &MermaidLensEdit {
                    element_id: entry.element_id.clone(),
                    replacement: REPLACEMENT.to_string(),
                },
            )
            .unwrap_or_else(|error| {
                panic!("[{name}] edit of {} failed: {error}", entry.element_id)
            });

            // `apply_lens_edit` resolves the element by id, and an id can repeat across kinds; the
            // range it reports is the one to check against, not the one this loop resolved.
            let applied = result.replaced_range;
            let expected = format!(
                "{}{REPLACEMENT}{}",
                &source[..applied.start_byte],
                &source[applied.end_byte..]
            );
            assert_eq!(
                result.updated_source, expected,
                "[{name}] editing {} changed bytes outside its resolved range",
                entry.element_id
            );
            assert!(
                applied.start_byte <= applied.end_byte && applied.end_byte <= source.len(),
                "[{name}] {} reported an inverted or out-of-bounds range: {applied:?}",
                entry.element_id
            );

            // Prove the comparison above is SENSITIVE rather than trivially true: a range shifted
            // by one byte must produce a different string. Without this, an assertion that
            // compared a value against itself would pass just as happily.
            if applied.start_byte < applied.end_byte
                && source.is_char_boundary(applied.start_byte + 1)
            {
                let off_by_one = format!(
                    "{}{REPLACEMENT}{}",
                    &source[..applied.start_byte + 1],
                    &source[applied.end_byte..]
                );
                assert_ne!(
                    result.updated_source, off_by_one,
                    "[{name}] the locality check cannot distinguish a one-byte shift for {}, so it \
                     would not catch an off-by-one",
                    entry.element_id
                );
            }

            if applied == range {
                checked += 1;
            }
        }
    }

    assert!(
        checked >= 100,
        "locality must have been checked on 100+ elements, got {checked}"
    );
}

#[test]
fn editing_a_label_is_reflected_in_a_reparse() {
    // The PutGet direction, restricted to what a TEXT lens can honestly promise. Renaming a node
    // ID would leave every edge still referencing the old id, so the faithful-reflection law is
    // checked on label text, where the edit has no referents to break.
    let source = "flowchart LR\n  A[Original Label]-->B[Other]\n";
    let snapshot = build_parse_lens(source);

    let target = snapshot
        .source_map
        .entries
        .iter()
        .find(|entry| {
            resolve_span_text_range(source, entry.span)
                .and_then(|range| source.get(range.start_byte..range.end_byte))
                .is_some_and(|text| text.contains("Original Label"))
        })
        .expect("a fixture whose span covers the label must exist for this law to be checkable")
        .clone();

    let range = resolve_span_text_range(source, target.span).expect("target span resolves");
    let current = &source[range.start_byte..range.end_byte];
    let replacement = current.replace("Original Label", "Renamed Label");
    assert_ne!(
        replacement, current,
        "the probe must actually change the label"
    );

    let result = snapshot
        .apply_edit(&MermaidLensEdit {
            element_id: target.element_id.clone(),
            replacement,
        })
        .expect("label edit applies");

    assert!(
        result.updated_source.contains("Renamed Label"),
        "the edit must appear in the updated source: {:?}",
        result.updated_source
    );
    assert!(
        !result.updated_source.contains("Original Label"),
        "the old label must be gone: {:?}",
        result.updated_source
    );

    // And the updated source must still be the same diagram, with the new label visible to a
    // reparse rather than only present as text.
    let reparsed = fm_parser::parse(&result.updated_source);
    assert_eq!(
        reparsed.ir.diagram_type,
        fm_parser::parse(source).ir.diagram_type
    );
    // Labels are interned, so the text comes from the label table rather than off the node.
    let labels: Vec<&str> = reparsed
        .ir
        .nodes
        .iter()
        .filter_map(|node| node.label)
        .filter_map(|label_id| reparsed.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str())
        .collect();
    assert!(
        labels.iter().any(|label| label.contains("Renamed Label")),
        "the reparse must see the new label: {labels:?}"
    );
}

#[test]
fn an_unknown_element_id_is_refused_rather_than_splicing_at_a_guess() {
    let source = "flowchart LR\n  A-->B\n";
    let snapshot = build_parse_lens(source);
    let error = snapshot
        .apply_edit(&MermaidLensEdit {
            element_id: "no-such-element".to_string(),
            replacement: "X".to_string(),
        })
        .expect_err("an unknown element id must not resolve to a range");
    let message = error.to_string();
    assert!(
        message.contains("no-such-element"),
        "the error must name the id it could not find: {message}"
    );
}
