use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{StateParseLens, StateParseLensError, parse};
use proptest::prelude::*;

#[test]
fn attach_transition_labels_preserves_composite_states_notes_and_repeated_edits() {
    for ending in ["", ":  "] {
        let source = format!(
            "stateDiagram-v2\r\n  %% retained comment\r\n  state \"Session\" as A {{\r\n    [*] --> Running\r\n    Running --> [*]\r\n  }}\r\n  state \"Done\" as B\r\n  A --> B{ending}\r\n  note right of A: Retain this note\r\n"
        );
        let lens = StateParseLens::parse(&source).unwrap();
        let original = lens.ir();
        assert_eq!(original.edges.len(), 3);
        assert!(original.edges[2].label.is_none());
        let mut edited = original.clone();
        let label = fm_core::IrLabelId(edited.labels.len());
        edited.labels.push(fm_core::IrLabel {
            text: "terminé 🦀".to_owned(),
            span: original.edges[2].span,
        });
        edited.edges[2].label = Some(label);
        let updated = lens.put(&edited).unwrap();
        let insertion = if ending.is_empty() {
            ": \"terminé 🦀\""
        } else {
            ": \"terminé 🦀\"  "
        };
        assert_eq!(
            updated,
            source.replace(&format!("A --> B{ending}"), &format!("A --> B{insertion}"))
        );
        let actual = parse(&updated);
        assert_eq!(actual.warnings, parse(&source).warnings);
        assert_eq!(actual.ir.edges[2].extras, original.edges[2].extras);
        assert_eq!(actual.ir.clusters[0].members, original.clusters[0].members);
        assert_eq!(
            actual.ir.graph.subgraphs[0].members,
            original.graph.subgraphs[0].members
        );
        assert_eq!(actual.ir.state_notes[0].text, original.state_notes[0].text);
        let next = StateParseLens::parse(&updated).unwrap();
        let mut renamed = next.ir().clone();
        renamed.labels[renamed.edges[2].label.unwrap().0].text = "Complete".to_owned();
        assert_eq!(
            next.put(&renamed).unwrap(),
            updated.replace("terminé 🦀", "Complete")
        );
    }
}

#[test]
fn transition_attachments_cannot_introduce_guards_actions_or_structure() {
    let lens = StateParseLens::parse("stateDiagram-v2\nA --> B:\n").unwrap();
    for text in [
        "",
        "changed\"\nC --> D",
        "finish [ready]",
        "finish / cleanup()",
    ] {
        let mut edited = lens.ir().clone();
        let label = fm_core::IrLabelId(edited.labels.len());
        edited.labels.push(fm_core::IrLabel {
            text: text.to_owned(),
            span: edited.edges[0].span,
        });
        edited.edges[0].label = Some(label);
        assert!(lens.put(&edited).is_err(), "{text:?}");
    }
    let mut edited = lens.ir().clone();
    let label = fm_core::IrLabelId(edited.labels.len());
    edited.labels.push(fm_core::IrLabel {
        text: "Complete".to_owned(),
        span: edited.edges[0].span,
    });
    assert!(lens.put(&edited).is_err(), "unused appended label");
    edited.edges[0].label = Some(label);
    edited.meta.title = Some("unrequested".to_owned());
    assert!(lens.put(&edited).is_err());
}

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

#[test]
fn put_preserves_composite_titles_with_unicode_shared_and_merged_aliases() {
    for (first, second, replacement) in [
        ("Old", "Existing", "naïve 🦀"),
        ("Old", "Existing", "Existing"),
        ("Shared", "Shared", "Renamed"),
    ] {
        let source = format!(
            concat!(
                "stateDiagram-v2\r\n",
                "  %% retain this comment\r\n",
                "  state \"{first}\" as A {{\r\n",
                "    [*] --> Running\r\n",
                "    Running --> [*]\r\n",
                "  }}\r\n",
                "  state \"{second}\" as B\r\n",
                "  A --> B: finish\r\n",
                "  note right of A: Retain this note\r\n"
            ),
            first = first,
            second = second,
        );
        let lens = StateParseLens::parse(&source).unwrap();
        let original = lens.ir();
        assert_eq!(original.clusters.len(), 1);
        assert_eq!(original.graph.subgraphs.len(), 1);
        assert_eq!(original.state_notes.len(), 1);
        let label = original
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .unwrap()
            .label
            .unwrap();
        let mut edited = original.clone();
        edited.labels[label.0].text = replacement.to_owned();

        let updated = lens
            .put(&edited)
            .expect("composite aliases remain editable");
        assert_eq!(updated, source.replace(first, replacement));
        let reparsed = parse(&updated).ir;
        assert_eq!(
            reparsed
                .nodes
                .iter()
                .map(|node| &node.id)
                .collect::<Vec<_>>(),
            original
                .nodes
                .iter()
                .map(|node| &node.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(reparsed.clusters[0].members, original.clusters[0].members);
        assert_eq!(
            reparsed.graph.clusters[0].members,
            original.graph.clusters[0].members
        );
        assert_eq!(
            reparsed.graph.subgraphs[0].members,
            original.graph.subgraphs[0].members
        );
        assert_eq!(
            reparsed.graph.subgraphs[0].key,
            original.graph.subgraphs[0].key
        );
        for title in [
            reparsed.clusters[0].title,
            reparsed.graph.clusters[0].title,
            reparsed.graph.subgraphs[0].title,
        ] {
            assert_eq!(reparsed.labels[title.unwrap().0].text, replacement);
        }
        let a_label = reparsed
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .unwrap()
            .label
            .unwrap();
        let b_label = reparsed
            .nodes
            .iter()
            .find(|node| node.id == "B")
            .unwrap()
            .label
            .unwrap();
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
        assert_eq!(lens.original_source(), source);
    }
}

#[test]
fn put_rejects_alias_text_that_injects_a_state_transition() {
    let source = "stateDiagram-v2\nstate \"Safe\" as A\nstate \"Other\" as B\nA --> B\n";
    let replacement = "Changed\" as A\nInjected --> B\nstate \"Changed";
    let lens = StateParseLens::parse(source).unwrap();
    let candidate = parse(&source.replace("Safe", replacement)).ir;
    assert_eq!(candidate.nodes.len(), lens.ir().nodes.len() + 1);
    assert_eq!(candidate.edges.len(), lens.ir().edges.len() + 1);
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    edited.labels[label.0].text = replacement.to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(StateParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_updates_every_occurrence_of_a_shared_state_and_transition_label() {
    let source = "stateDiagram-v2\nstate \"Shared\" as A\nstate \"Other\" as B\nA --> B: Shared\n";
    let lens = StateParseLens::parse(source).unwrap();
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    assert_eq!(edited.edges[0].label, Some(label));
    edited.labels[label.0].text = "Changed".to_owned();
    let updated = lens
        .put(&edited)
        .expect("state and transition update together");
    assert_eq!(updated, source.replace("Shared", "Changed"));
    let reparsed = parse(&updated).ir;
    assert_eq!(reparsed.nodes[0].label, reparsed.edges[0].label);
    assert_eq!(
        reparsed.labels[reparsed.edges[0].label.unwrap().0].text,
        "Changed"
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_changes_transition_text_preserving_guards_actions_and_composite_state_metadata() {
    let source = concat!(
        "stateDiagram-v2\r\n",
        "  %% finish remains in this comment\r\n",
        "  state \"Session\" as A {\r\n",
        "    [*] --> Running\r\n",
        "    Running --> [*]\r\n",
        "  }\r\n",
        "  state \"Done\" as B\r\n",
        "  A --> B: \"finish [ready] / cleanup()\"\r\n",
        "  note right of A: Retain this note\r\n",
    );
    let lens = StateParseLens::parse(source).unwrap();
    let original = lens.ir();
    assert_eq!(original.edges.len(), 3);
    assert_eq!(original.clusters.len(), 1);
    assert_eq!(original.state_notes.len(), 1);
    let edge = &original.edges[2];
    let label = edge.label.unwrap();
    assert_eq!(original.labels[label.0].text, "finish [ready] / cleanup()");
    assert_eq!(edge.guard(), Some("ready"));
    assert_eq!(edge.action(), Some("cleanup()"));
    let mut edited = original.clone();
    edited.labels[label.0].text = "terminé 🦀 [ready] / cleanup()".to_owned();
    let updated = lens.put(&edited).expect("transition event text re-emits");
    assert_eq!(updated, source.replace(": \"finish", ": \"terminé 🦀"));
    let reparsed = parse(&updated).ir;
    assert_eq!(
        reparsed.labels[reparsed.edges[2].label.unwrap().0].text,
        "terminé 🦀 [ready] / cleanup()"
    );
    assert_eq!(reparsed.edges[2].extras, edge.extras);
    assert_eq!(reparsed.clusters[0].members, original.clusters[0].members);
    assert_eq!(
        reparsed.graph.subgraphs[0].members,
        original.graph.subgraphs[0].members
    );
    assert_eq!(reparsed.state_notes[0].text, original.state_notes[0].text);
    assert_eq!(lens.original_source(), source);
}

#[test]
fn state_transition_metadata_uses_the_normalized_visible_label() {
    for label in [
        "finish [ready] / cleanup()",
        "\"finish [ready] / cleanup()\"",
        "\"`finish [ready] / cleanup()`\"",
        "\"finish [#114;eady] / clean#117;p()\"",
    ] {
        let source = format!("stateDiagram-v2\nA --> B: {label}\n");
        let parsed = parse(&source).ir;
        let edge = &parsed.edges[0];
        assert_eq!(
            parsed.labels[edge.label.unwrap().0].text,
            "finish [ready] / cleanup()",
            "{label}"
        );
        assert_eq!(edge.guard(), Some("ready"), "{label}");
        assert_eq!(edge.action(), Some("cleanup()"), "{label}");
    }
}

#[test]
fn put_rejects_transition_injection_and_unrequested_guard_changes() {
    let source = "stateDiagram-v2\nstate \"Session\" as A\nstate \"Done\" as B\nA --> B: Safe\n";
    let lens = StateParseLens::parse(source).unwrap();
    let label = lens.ir().edges[0].label.unwrap();
    let replacement = "Changed\nInjected --> B: added";
    let candidate = parse(&source.replace("Safe", replacement)).ir;
    assert_eq!(candidate.nodes.len(), lens.ir().nodes.len() + 1);
    assert_eq!(candidate.edges.len(), lens.ir().edges.len() + 1);
    let mut edited = lens.ir().clone();
    edited.labels[label.0].text = replacement.to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(StateParseLensError::UnsupportedIrChange)
    );
    edited.labels[label.0].text = "Changed [newGuard]".to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(StateParseLensError::UnsupportedIrChange)
    );
    edited.labels[label.0].text = "Changed".to_owned();
    edited.edges[0].extras_mut().guard = Some("newGuard".into());
    assert_eq!(
        lens.put(&edited),
        Err(StateParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_rejects_shared_labels_when_an_encoded_transition_cannot_be_located() {
    let source =
        "stateDiagram-v2\nstate \"Shared\" as A\nstate \"Other\" as B\nA --> B: #83;hared\n";
    let lens = StateParseLens::parse(source).unwrap();
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    assert_eq!(edited.edges[0].label, Some(label));
    edited.labels[label.0].text = "Changed".to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(StateParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
    assert_eq!(lens.ir().labels[label.0].text, "Shared");
}
