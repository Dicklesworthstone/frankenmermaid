use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{FlowchartParseLens, parse};
use proptest::prelude::*;

fn flowchart_corpus() -> Vec<String> {
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fm-cli/tests/golden");
    let mut sources: Vec<_> = fs::read_dir(golden_dir)
        .expect("golden fixture directory is readable")
        .map(|entry| entry.expect("golden fixture entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "mmd"))
        .map(|path| fs::read_to_string(path).expect("golden fixture source is readable"))
        .filter(|source| parse(source).ir.diagram_type == DiagramType::Flowchart)
        .collect();
    sources.sort();
    sources
}

#[test]
fn get_put_preserves_every_flowchart_fixture_byte_for_byte() {
    let corpus = flowchart_corpus();
    assert!(
        corpus.len() >= 10,
        "the round-trip corpus must contain diverse flowcharts, got {}",
        corpus.len()
    );

    for source in corpus {
        let lens = FlowchartParseLens::parse(&source).expect("flowchart fixture enters ParseLens");
        assert_eq!(
            lens.put(lens.ir()).expect("unchanged IR re-emits"),
            source,
            "GetPut changed a flowchart fixture"
        );
    }
}

proptest! {
    #[test]
    fn get_put_property_holds_for_samples_from_the_flowchart_corpus(
        source in prop::sample::select(flowchart_corpus()),
    ) {
        let lens = FlowchartParseLens::parse(&source).expect("corpus source is a flowchart");
        prop_assert_eq!(lens.put(lens.ir()).expect("unchanged IR re-emits"), source);
    }
}

#[test]
fn put_changes_one_ir_label_without_reformatting_unrelated_source() {
    let source = concat!(
        "%%{init: {'theme':'forest'}}%%\r\n",
        "flowchart LR\r\n",
        "  %% retain this comment and indentation\r\n",
        "  A[\"Release candidate\"] --> B[Keep]\r\n",
        "\r\n",
        "  %% retain this trailing comment\r\n",
    );
    let lens = FlowchartParseLens::parse(source).expect("flowchart enters ParseLens");
    let mut edited = lens.ir().clone();
    let node = edited
        .nodes
        .iter()
        .find(|node| node.id == "A")
        .expect("A node exists");
    let label_id = node.label.expect("A has an editable label");
    edited.labels[label_id.0].text = "Ready to ship".to_string();

    let updated = lens.put(&edited).expect("label-only IR edit re-emits");
    assert_eq!(
        updated,
        source.replace("Release candidate", "Ready to ship"),
        "put must splice only the edited label text"
    );
    assert!(updated.contains("A[\"Ready to ship\"]"));
    assert!(updated.contains("%% retain this comment and indentation\r\n"));
    assert!(updated.contains("%% retain this trailing comment\r\n"));
    assert_eq!(
        updated.matches("\r\n").count(),
        source.matches("\r\n").count()
    );

    let reparsed = parse(&updated);
    let reparsed_label = reparsed
        .ir
        .nodes
        .iter()
        .find(|node| node.id == "A")
        .and_then(|node| node.label)
        .and_then(|label_id| reparsed.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(reparsed_label, Some("Ready to ship"));
}
