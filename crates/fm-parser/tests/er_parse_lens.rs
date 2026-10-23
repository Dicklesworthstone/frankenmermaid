use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{ErParseLens, ErParseLensError, parse};
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

#[test]
fn put_preserves_er_metadata_with_unicode_shared_and_merged_labels() {
    for (first, second, replacement) in [
        ("Old", "Existing", "naïve 🦀"),
        ("Old", "Existing", "Existing"),
        ("Shared", "Shared", "Renamed"),
    ] {
        let source = format!(
            concat!(
                "erDiagram\r\n",
                "  %% retain this comment\r\n",
                "  direction LR\r\n",
                "  A[\"{first}\"] {{\r\n",
                "    int id PK, UK \"identifier\"\r\n",
                "    string owner FK\r\n",
                "  }}\r\n",
                "  B[\"{second}\"] {{\r\n",
                "    string name\r\n",
                "  }}\r\n",
                "  A ||--o{{ B : owns\r\n",
                "  classDef hot fill:#ff8800\r\n",
                "  class A hot\r\n"
            ),
            first = first,
            second = second,
        );
        let lens = ErParseLens::parse(&source).unwrap();
        let original = lens.ir();
        assert_eq!(original.direction, fm_core::GraphDirection::LR);
        assert_eq!(original.nodes[0].members.len(), 2);
        assert_eq!(
            original.nodes[0].members[0].keys,
            vec![fm_core::IrAttributeKey::Pk, fm_core::IrAttributeKey::Uk]
        );
        assert_eq!(original.edges[0].er_notation(), Some("||--o{"));
        let label = original.nodes[0].label.unwrap();
        let mut edited = original.clone();
        edited.labels[label.0].text = replacement.to_owned();

        let updated = lens
            .put(&edited)
            .expect("ordinary entity labels remain editable");
        assert_eq!(updated, source.replace(first, replacement));
        let reparsed = parse(&updated).ir;
        assert_eq!(reparsed.direction, original.direction);
        assert_eq!(reparsed.nodes.len(), original.nodes.len());
        for (before, after) in original.nodes.iter().zip(&reparsed.nodes) {
            assert_eq!(after.id, before.id);
            assert_eq!(after.members, before.members);
            assert_eq!(after.classes, before.classes);
        }
        assert_eq!(reparsed.edges.len(), original.edges.len());
        for (before, after) in original.edges.iter().zip(&reparsed.edges) {
            assert_eq!(
                (after.from, after.to, after.arrow),
                (before.from, before.to, before.arrow)
            );
            assert_eq!(after.er_notation(), before.er_notation());
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
                "equal entity labels must remain interned together"
            );
        }
        assert_eq!(lens.original_source(), source);
    }
}

#[test]
fn put_rejects_entity_label_text_that_injects_a_relationship() {
    let source = "erDiagram\nA[\"Safe\"]\nB[Other]\nA ||--o{ B : owns\n";
    let replacement = "Changed\"]\nInjected ||--o{ B : added\nA[\"Changed";
    let lens = ErParseLens::parse(source).unwrap();
    let candidate = parse(&source.replace("Safe", replacement)).ir;
    assert_eq!(candidate.nodes.len(), lens.ir().nodes.len() + 1);
    assert_eq!(candidate.edges.len(), lens.ir().edges.len() + 1);
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    edited.labels[label.0].text = replacement.to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(ErParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_rejects_a_partial_edit_of_a_shared_entity_and_relationship_label() {
    let source = "erDiagram\nA[Shared]\nB[Other]\nA ||--o{ B : Shared\n";
    let lens = ErParseLens::parse(source).unwrap();
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    assert_eq!(edited.edges[0].label, Some(label));
    edited.labels[label.0].text = "Changed".to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(ErParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_preserves_er_markdown_formatting_and_rejects_markup_injection() {
    for (markdown, bold, italic) in [
        ("**Old**", true, false),
        ("*Old*", false, true),
        ("Old", false, false),
    ] {
        let source = format!("erDiagram\nA[\"`{markdown}`\"]\nB[Keep]\nA ||--o{{ B : owns\n");
        let lens = ErParseLens::parse(&source).unwrap();
        let label = lens.ir().nodes[0].label.unwrap();
        let mut edited = lens.ir().clone();
        edited.labels[label.0].text = "New 🦀".to_owned();
        let updated = lens
            .put(&edited)
            .expect("a plain rename preserves Markdown");
        assert_eq!(updated, source.replace("Old", "New 🦀"));
        let reparsed = parse(&updated).ir;
        let updated_label = reparsed.nodes[0].label.unwrap();
        assert_eq!(
            reparsed.label_markup[&updated_label],
            vec![fm_core::IrLabelSegment::Text {
                text: "New 🦀".to_owned(),
                bold,
                italic,
                code: false,
                strike: false,
            }]
        );

        edited.labels[label.0].text = "**Injected**".to_owned();
        assert_eq!(
            lens.put(&edited),
            Err(ErParseLensError::UnsupportedIrChange)
        );
        edited.labels[label.0].text = "New 🦀".to_owned();
        let [fm_core::IrLabelSegment::Text { italic, .. }] =
            edited.label_markup.get_mut(&label).unwrap().as_mut_slice()
        else {
            panic!("fixture has a single text run");
        };
        *italic = !*italic;
        assert_eq!(
            lens.put(&edited),
            Err(ErParseLensError::UnsupportedIrChange)
        );
    }
}
