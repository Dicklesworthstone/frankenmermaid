use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{ClassParseLens, ClassParseLensError, parse};
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

fn class_corpus() -> Vec<String> {
    let test_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fm-cli/tests");
    let mut sources = Vec::new();
    collect_mmd_sources(&test_root.join("golden"), &mut sources);
    collect_mmd_sources(&test_root.join("fixtures"), &mut sources);
    sources.retain(|source| parse(source).ir.diagram_type == DiagramType::Class);
    sources.sort();
    sources
}

#[test]
fn get_put_preserves_every_class_fixture_byte_for_byte() {
    let corpus = class_corpus();
    assert!(
        corpus.len() >= 2,
        "the round-trip corpus must contain class diagrams, got {}",
        corpus.len()
    );
    for source in corpus {
        let lens = ClassParseLens::parse(&source).expect("class fixture enters ParseLens");
        assert_eq!(
            lens.put(lens.ir()).expect("unchanged IR re-emits"),
            source,
            "GetPut changed a class fixture"
        );
    }
}

proptest! {
    #[test]
    fn get_put_property_holds_for_samples_from_the_class_corpus(
        source in prop::sample::select(class_corpus()),
    ) {
        let lens = ClassParseLens::parse(&source).expect("corpus source is a class diagram");
        prop_assert_eq!(lens.put(lens.ir()).expect("unchanged IR re-emits"), source);
    }
}

#[test]
fn put_changes_one_class_label_without_reformatting_unrelated_source() {
    let source = concat!(
        "%%{init: {'theme':'forest'}}%%\r\n",
        "classDiagram\r\n",
        "  %% preserve this comment\r\n",
        "  Animal[\"Living creature\"]\r\n",
        "  Animal <|-- Dog\r\n",
        "  Animal : +name\r\n",
        "  %% preserve this trailing comment\r\n",
    );
    let lens = ClassParseLens::parse(source).expect("class diagram enters ParseLens");
    let mut edited = lens.ir().clone();
    let label_id = edited
        .nodes
        .iter()
        .find(|node| node.id == "Animal")
        .and_then(|node| node.label)
        .expect("Animal has an editable class label");
    edited
        .labels
        .get_mut(label_id.0)
        .expect("class label id resolves")
        .text = "Domain entity".to_string();

    let updated = lens.put(&edited).expect("class-label IR edit re-emits");
    assert_eq!(
        updated,
        source.replace("Living creature", "Domain entity"),
        "put must splice only the class label"
    );
    assert!(updated.contains("Animal[\"Domain entity\"]\r\n"));
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
        .find(|node| node.id == "Animal")
        .and_then(|node| node.label)
        .and_then(|label_id| reparsed.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(reparsed_label, Some("Domain entity"));
}

#[test]
fn put_preserves_class_metadata_with_unicode_shared_and_merged_labels() {
    for (first, second, replacement) in [
        ("Old", "Existing", "naïve 🦀"),
        ("Old", "Existing", "Existing"),
        ("Shared", "Shared", "Renamed"),
    ] {
        let source = format!(
            concat!(
                "classDiagram\r\n",
                "  %% retain this comment\r\n",
                "  A[\"{first}\"]\r\n",
                "  B[\"{second}\"]\r\n",
                "  <<interface>> A\r\n",
                "  A : +String name\r\n",
                "  A : +save() bool\r\n",
                "  A *-- B : owns\r\n",
                "  note for A \"Retain this note\"\r\n",
                "  classDef hot fill:#ff8800\r\n",
                "  class A hot\r\n"
            ),
            first = first,
            second = second,
        );
        let lens = ClassParseLens::parse(&source).unwrap();
        let original = lens.ir();
        let meta = original.nodes[0].class_meta.as_ref().unwrap();
        assert_eq!(meta.attributes.len(), 1);
        assert_eq!(meta.methods.len(), 1);
        assert_eq!(meta.stereotype, Some(fm_core::ClassStereotype::Interface));
        assert_eq!(original.state_notes.len(), 1);
        let label = original.nodes[0].label.unwrap();
        let mut edited = original.clone();
        edited.labels[label.0].text = replacement.to_owned();

        let updated = lens
            .put(&edited)
            .expect("ordinary class labels remain editable");
        assert_eq!(updated, source.replace(first, replacement));
        let reparsed = parse(&updated).ir;
        assert_eq!(reparsed.nodes.len(), original.nodes.len());
        for (before, after) in original.nodes.iter().zip(&reparsed.nodes) {
            assert_eq!(after.id, before.id);
            assert_eq!(after.class_meta, before.class_meta);
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
        assert_eq!(
            reparsed.state_notes[0].target,
            original.state_notes[0].target
        );
        assert_eq!(
            reparsed.state_notes[0].position,
            original.state_notes[0].position
        );
        assert_eq!(reparsed.state_notes[0].text, original.state_notes[0].text);
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
                "equal class labels must remain interned together"
            );
        }
        assert_eq!(lens.original_source(), source);
    }
}

#[test]
fn put_rejects_class_label_text_that_injects_a_relationship() {
    let source = "classDiagram\nA[\"Safe\"]\nB[Other]\nA --> B\n";
    let replacement = "Changed\"]\nInjected --> B\nA[\"Changed";
    let lens = ClassParseLens::parse(source).unwrap();
    let candidate = parse(&source.replace("Safe", replacement)).ir;
    assert_eq!(candidate.nodes.len(), lens.ir().nodes.len() + 1);
    assert_eq!(candidate.edges.len(), lens.ir().edges.len() + 1);
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    edited.labels[label.0].text = replacement.to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(ClassParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_rejects_a_partial_edit_of_a_shared_class_and_relationship_label() {
    let source = "classDiagram\nA[Shared]\nB[Other]\nA --> B : Shared\n";
    let lens = ClassParseLens::parse(source).unwrap();
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    assert_eq!(edited.edges[0].label, Some(label));
    edited.labels[label.0].text = "Changed".to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(ClassParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_preserves_class_markdown_formatting_and_rejects_markup_injection() {
    for (markdown, bold, italic) in [
        ("**Old**", true, false),
        ("*Old*", false, true),
        ("Old", false, false),
    ] {
        let source = format!("classDiagram\nA[\"`{markdown}`\"]\nB[Keep]\nA --> B\n");
        let lens = ClassParseLens::parse(&source).unwrap();
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
            Err(ClassParseLensError::UnsupportedIrChange)
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
            Err(ClassParseLensError::UnsupportedIrChange)
        );
    }
}
