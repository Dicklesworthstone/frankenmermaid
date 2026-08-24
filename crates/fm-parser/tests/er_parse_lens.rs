use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{ErParseLens, parse};
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

fn er_corpus() -> Vec<String> {
    let test_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fm-cli/tests");
    let mut sources = Vec::new();
    collect_mmd_sources(&test_root.join("golden"), &mut sources);
    collect_mmd_sources(&test_root.join("fixtures"), &mut sources);
    sources.retain(|source| parse(source).ir.diagram_type == DiagramType::Er);
    sources.sort();
    sources
}

#[test]
fn get_put_preserves_every_er_fixture_byte_for_byte() {
    let corpus = er_corpus();
    assert!(
        corpus.len() >= 2,
        "the round-trip corpus must contain ER diagrams, got {}",
        corpus.len()
    );
    for source in corpus {
        let lens = ErParseLens::parse(&source).expect("ER fixture enters ParseLens");
        assert_eq!(
            lens.put(lens.ir()).expect("unchanged IR re-emits"),
            source,
            "GetPut changed an ER fixture"
        );
    }
}

proptest! {
    #[test]
    fn get_put_property_holds_for_samples_from_the_er_corpus(
        source in prop::sample::select(er_corpus()),
    ) {
        let lens = ErParseLens::parse(&source).expect("corpus source is an ER diagram");
        prop_assert_eq!(lens.put(lens.ir()).expect("unchanged IR re-emits"), source);
    }
}

#[test]
fn put_changes_one_er_entity_label_without_reformatting_unrelated_source() {
    let source = concat!(
        "%%{init: {'theme':'forest'}}%%\r\n",
        "erDiagram\r\n",
        "  %% preserve this comment\r\n",
        "  CUSTOMER[\"Customer account\"] {\r\n",
        "    int id PK\r\n",
        "  }\r\n",
        "  CUSTOMER ||--o{ ORDER : places\r\n",
        "  %% preserve this trailing comment\r\n",
    );
    let lens = ErParseLens::parse(source).expect("ER diagram enters ParseLens");
    let mut edited = lens.ir().clone();
    let label_id = edited
        .nodes
        .iter()
        .find(|node| node.id == "CUSTOMER")
        .and_then(|node| node.label)
        .expect("CUSTOMER has an editable ER entity label");
    edited
        .labels
        .get_mut(label_id.0)
        .expect("ER entity label id resolves")
        .text = "Billing account".to_string();

    let updated = lens.put(&edited).expect("ER entity-label IR edit re-emits");
    assert_eq!(
        updated,
        source.replace("Customer account", "Billing account"),
        "put must splice only the ER entity label"
    );
    assert!(updated.contains("CUSTOMER[\"Billing account\"] {\r\n"));
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
        .find(|node| node.id == "CUSTOMER")
        .and_then(|node| node.label)
        .and_then(|label_id| reparsed.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(reparsed_label, Some("Billing account"));
}
