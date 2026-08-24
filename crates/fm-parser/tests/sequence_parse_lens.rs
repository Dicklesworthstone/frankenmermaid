use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{SequenceParseLens, parse};
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
