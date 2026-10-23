use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{SequenceParseLens, SequenceParseLensError, parse};
use proptest::prelude::*;

fn collect_mmd_sources(directory: &Path, sources: &mut Vec<String>) {
    for entry in fs::read_dir(directory).expect("fixture directory is readable") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect_mmd_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "mmd") {
            sources.push(fs::read_to_string(path).expect("fixture source is readable"));
        }
    }
}

fn sequence_corpus() -> Vec<String> {
    let test_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fm-cli/tests");
    let mut sources = Vec::new();
    collect_mmd_sources(&test_root.join("golden"), &mut sources);
    collect_mmd_sources(&test_root.join("fixtures"), &mut sources);
    sources.retain(|source| parse(source).ir.diagram_type == DiagramType::Sequence);
    sources.sort();
    sources
}

#[test]
fn get_put_preserves_every_sequence_fixture_byte_for_byte() {
    let corpus = sequence_corpus();
    assert!(
        corpus.len() >= 5,
        "the round-trip corpus must contain diverse sequences, got {}",
        corpus.len()
    );
    for source in corpus {
        let lens = SequenceParseLens::parse(&source).expect("sequence fixture enters ParseLens");
        assert_eq!(
            lens.put(lens.ir()).expect("unchanged IR re-emits"),
            source,
            "GetPut changed a sequence fixture"
        );
    }
}

proptest! {
    #[test]
    fn get_put_property_holds_for_samples_from_the_sequence_corpus(
        source in prop::sample::select(sequence_corpus()),
    ) {
        let lens = SequenceParseLens::parse(&source).expect("corpus source is a sequence diagram");
        prop_assert_eq!(lens.put(lens.ir()).expect("unchanged IR re-emits"), source);
    }
}

#[test]
fn put_changes_one_participant_alias_without_reformatting_unrelated_source() {
    let source = concat!(
        "%%{init: {'theme':'forest'}}%%\r\n",
        "sequenceDiagram\r\n",
        "  %% preserve this comment\r\n",
        "  participant C as \"Client Service\"\r\n",
        "  participant S as Server\r\n",
        "  C->>S: request\r\n",
        "  %% preserve this trailing comment\r\n",
    );
    let lens = SequenceParseLens::parse(source).expect("sequence enters ParseLens");
    let mut edited = lens.ir().clone();
    let label_id = edited
        .nodes
        .iter()
        .find(|node| node.id == "C")
        .and_then(|node| node.label)
        .expect("C has an editable participant label");
    edited
        .labels
        .get_mut(label_id.0)
        .expect("participant label id resolves")
        .text = "Browser Client".to_string();

    let updated = lens
        .put(&edited)
        .expect("participant-label IR edit re-emits");
    assert_eq!(
        updated,
        source.replace("Client Service", "Browser Client"),
        "put must splice only the participant alias"
    );
    assert!(updated.contains("participant C as \"Browser Client\"\r\n"));
    assert!(updated.contains("%% preserve this comment\r\n"));
    assert!(updated.contains("%% preserve this trailing comment\r\n"));
    assert_eq!(
        updated.matches("\r\n").count(),
        source.matches("\r\n").count()
    );

    let reparsed = parse(&updated);
    let reparsed_label = reparsed
        .ir
        .nodes
        .iter()
        .find(|node| node.id == "C")
        .and_then(|node| node.label)
        .and_then(|label_id| reparsed.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(reparsed_label, Some("Browser Client"));
}

#[test]
fn put_preserves_sequence_metadata_with_unicode_shared_and_merged_aliases() {
    for (first, second, replacement) in [
        ("Old", "Existing", "naïve 🦀"),
        ("Old", "Existing", "Existing"),
        ("Shared", "Shared", "Renamed"),
    ] {
        let source = format!(
            concat!(
                "sequenceDiagram\r\n",
                "  %% retain this comment\r\n",
                "  autonumber 10 2\r\n",
                "  participant A as \"{first}\"\r\n",
                "  actor B as \"{second}\"\r\n",
                "  A->>B: request\r\n",
                "  activate B\r\n",
                "  alt accepted\r\n",
                "    B-->>A: response\r\n",
                "  else retry\r\n",
                "    A->>B: retry\r\n",
                "  end\r\n",
                "  Note over A,B: Retain note\r\n",
                "  deactivate B\r\n"
            ),
            first = first,
            second = second,
        );
        let lens = SequenceParseLens::parse(&source).unwrap();
        let original = lens.ir();
        let meta = original.sequence_meta.as_ref().unwrap();
        assert_eq!(meta.autonumber_start, 10);
        assert_eq!(meta.autonumber_increment, 2);
        assert_eq!(meta.activations.len(), 1);
        assert_eq!(meta.fragments.len(), 1);
        assert_eq!(meta.notes.len(), 1);
        let label = original.nodes[0].label.unwrap();
        let mut edited = original.clone();
        edited.labels[label.0].text = replacement.to_owned();

        let updated = lens.put(&edited).expect("ordinary aliases remain editable");
        assert_eq!(updated, source.replace(first, replacement));
        let reparsed = parse(&updated).ir;
        assert_eq!(reparsed.sequence_meta, original.sequence_meta);
        assert_eq!(reparsed.nodes.len(), original.nodes.len());
        for (before, after) in original.nodes.iter().zip(&reparsed.nodes) {
            assert_eq!(after.id, before.id);
            assert_eq!(after.shape, before.shape);
            assert_eq!(after.classes, before.classes);
        }
        assert_eq!(reparsed.edges.len(), original.edges.len());
        for (before, after) in original.edges.iter().zip(&reparsed.edges) {
            assert_eq!(
                (after.from, after.to, after.arrow),
                (before.from, before.to, before.arrow)
            );
            assert_eq!(
                after.label.map(|id| &reparsed.labels[id.0].text),
                before.label.map(|id| &original.labels[id.0].text)
            );
        }
        let a_label = reparsed.nodes[0].label.unwrap();
        let b_label = reparsed.nodes[1].label.unwrap();
        assert_eq!(reparsed.labels[a_label.0].text, replacement);
        assert_eq!(
            reparsed.labels[b_label.0].text,
            if first == second { replacement } else { second }
        );
        if first == second || replacement == second {
            assert_eq!(
                a_label, b_label,
                "equal aliases must remain interned together"
            );
        }
        assert_eq!(lens.original_source(), source);
    }
}

#[test]
fn put_rejects_alias_text_that_injects_a_sequence_message() {
    let source = "sequenceDiagram\nparticipant A as Safe\nparticipant B as Keep\nA->>B: original\n";
    let replacement = "Changed\nA->>B: injected";
    let lens = SequenceParseLens::parse(source).unwrap();
    let candidate = parse(&source.replace("Safe", replacement)).ir;
    assert_eq!(candidate.edges.len(), lens.ir().edges.len() + 1);
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    edited.labels[label.0].text = replacement.to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(SequenceParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_rejects_a_partial_edit_of_a_shared_participant_and_message_label() {
    let source =
        "sequenceDiagram\nparticipant A as Shared\nparticipant B as Other\nA->>B: Shared\n";
    let lens = SequenceParseLens::parse(source).unwrap();
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    assert_eq!(edited.edges[0].label, Some(label));
    edited.labels[label.0].text = "Changed".to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(SequenceParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}
