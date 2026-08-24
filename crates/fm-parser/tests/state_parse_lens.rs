use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{StateParseLens, parse};
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

fn state_corpus() -> Vec<String> {
    let test_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fm-cli/tests");
    let mut sources = Vec::new();
    collect_mmd_sources(&test_root.join("golden"), &mut sources);
    collect_mmd_sources(&test_root.join("fixtures"), &mut sources);
    sources.retain(|source| parse(source).ir.diagram_type == DiagramType::State);
    sources.sort();
    sources
}

#[test]
fn get_put_preserves_every_state_fixture_byte_for_byte() {
    let corpus = state_corpus();
    assert!(
        corpus.len() >= 3,
        "the round-trip corpus must contain state diagrams, got {}",
        corpus.len()
    );
    for source in corpus {
        let lens = StateParseLens::parse(&source).expect("state fixture enters ParseLens");
        assert_eq!(
            lens.put(lens.ir()).expect("unchanged IR re-emits"),
            source,
            "GetPut changed a state fixture"
        );
    }
}

proptest! {
    #[test]
    fn get_put_property_holds_for_samples_from_the_state_corpus(
        source in prop::sample::select(state_corpus()),
    ) {
        let lens = StateParseLens::parse(&source).expect("corpus source is a state diagram");
        prop_assert_eq!(lens.put(lens.ir()).expect("unchanged IR re-emits"), source);
    }
}

#[test]
fn put_changes_one_state_alias_without_reformatting_unrelated_source() {
    let source = concat!(
        "%%{init: {'theme':'forest'}}%%\r\n",
        "stateDiagram-v2\r\n",
        "  %% preserve this comment\r\n",
        "  state \"Active mode\" as Active {\r\n",
        "    [*] --> Working\r\n",
        "  }\r\n",
        "  Active --> Done\r\n",
        "  %% preserve this trailing comment\r\n",
    );
    let lens = StateParseLens::parse(source).expect("state diagram enters ParseLens");
    let mut edited = lens.ir().clone();
    let label_id = edited
        .nodes
        .iter()
        .find(|node| node.id == "Active")
        .and_then(|node| node.label)
        .expect("Active has an editable state label");
    edited
        .labels
        .get_mut(label_id.0)
        .expect("state label id resolves")
        .text = "Running mode".to_string();

    let updated = lens.put(&edited).expect("state-label IR edit re-emits");
    assert_eq!(
        updated,
        source.replace("Active mode", "Running mode"),
        "put must splice only the state alias"
    );
    assert!(updated.contains("state \"Running mode\" as Active {\r\n"));
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
        .find(|node| node.id == "Active")
        .and_then(|node| node.label)
        .and_then(|label_id| reparsed.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(reparsed_label, Some("Running mode"));
}
