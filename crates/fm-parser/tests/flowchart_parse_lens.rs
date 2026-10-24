use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{FlowchartParseLens, FlowchartParseLensError, parse};
use proptest::prelude::*;

mod label_attachments {
    use super::*;
    use fm_core::{IrLabel, IrLabelId, MermaidDiagramIr, Span};
    use fm_parser::{DiagramParseLens, build_parse_lens};

    fn append(ir: &mut MermaidDiagramIr, text: &str, span: Span) -> IrLabelId {
        let id = IrLabelId(ir.labels.len());
        ir.labels.push(IrLabel {
            text: text.to_owned(),
            span,
        });
        id
    }

    #[test]
    fn bare_nodes_and_edges_acquire_labels_without_losing_source_or_metadata() {
        let source = concat!(
            "%%{init: {'theme':'forest'}}%%\r\n",
            "flowchart LR\r\n",
            "  %% retain this comment\r\n",
            "  A e1@--> B  %% retain the edge comment\r\n",
            "  style A fill:#ffeedd\r\n",
            "  linkStyle 0 stroke:#123456\r\n",
        );
        let lens = FlowchartParseLens::parse(source).unwrap();
        let original = lens.ir();
        assert!(original.labels.is_empty());
        let mut edited = original.clone();
        let edge = append(&mut edited, "request 🦀", original.edges[0].span);
        let start = append(&mut edited, "Départ", original.nodes[0].span_primary);
        let finish = append(&mut edited, "Finish", original.nodes[1].span_primary);
        edited.edges[0].label = Some(edge);
        edited.nodes[0].label = Some(start);
        edited.nodes[1].label = Some(finish);
        let updated = lens.put(&edited).expect("bare graph can acquire labels");
        assert_eq!(
            updated,
            source.replace(
                "A e1@--> B",
                "A[\"Départ\"] e1@-->|\"request 🦀\"| B[\"Finish\"]"
            )
        );
        let reparsed = parse(&updated);
        assert_eq!(reparsed.warnings, parse(source).warnings);
        assert_eq!(reparsed.ir.edges[0].extras, original.edges[0].extras);
        assert_eq!(
            reparsed.ir.edges[0].inline_style,
            original.edges[0].inline_style
        );
        assert_eq!(reparsed.ir.nodes[0].implicit, original.nodes[0].implicit);
        assert_eq!(reparsed.ir.nodes[0].shape, original.nodes[0].shape);
        assert_eq!(
            reparsed.ir.nodes[0].inline_style,
            original.nodes[0].inline_style
        );
        let next = FlowchartParseLens::parse(&updated).unwrap();
        assert_eq!(next.put(next.ir()).unwrap(), updated);
        let mut renamed = next.ir().clone();
        renamed.labels[renamed.edges[0].label.unwrap().0].text = "accepted".to_owned();
        assert_eq!(
            next.put(&renamed).unwrap(),
            updated.replace("request 🦀", "accepted")
        );
        assert_eq!(lens.original_source(), source);
        assert!(lens.ir().labels.is_empty());
    }

    #[test]
    fn existing_and_duplicate_labels_can_be_reused_alongside_a_rename() {
        let source = "flowchart TB\nA[Shared]\nB --> C\n";
        let lens = FlowchartParseLens::parse(source).unwrap();
        let mut edited = lens.ir().clone();
        let existing = edited.nodes[0].label.unwrap();
        edited.labels[existing.0].text = "En commun 🦀".to_owned();
        edited.nodes[1].label = Some(existing);
        edited.edges[0].label = Some(existing);
        let span = edited.nodes[2].span_primary;
        let duplicate = append(&mut edited, "En commun 🦀", span);
        edited.nodes[2].label = Some(duplicate);
        let updated = lens.put(&edited).unwrap();
        assert_eq!(
            updated,
            "flowchart TB\nA[En commun 🦀]\nB[\"En commun 🦀\"] -->|\"En commun 🦀\"| C[\"En commun 🦀\"]\n"
        );
        let actual = parse(&updated).ir;
        assert_eq!(actual.labels.len(), 1);
        assert!(
            actual
                .nodes
                .iter()
                .all(|node| node.label == actual.edges[0].label)
        );
    }

    #[test]
    fn parallel_and_chained_edge_tokens_are_inserted_once() {
        let source = "flowchart TB\nA & B --> C & D\nC --> A --> D\n";
        let lens = FlowchartParseLens::parse(source).unwrap();
        let original = lens.ir();
        assert_eq!(original.edges.len(), 6);
        let mut edited = original.clone();
        let shared = append(&mut edited, "Shared", original.edges[0].span);
        for edge in &mut edited.edges[..4] {
            edge.label = Some(shared);
        }
        for (index, text) in [(4, "First"), (5, "Next")] {
            let label = append(&mut edited, text, original.edges[index].span);
            edited.edges[index].label = Some(label);
        }
        assert_eq!(
            lens.put(&edited).unwrap(),
            "flowchart TB\nA & B -->|\"Shared\"| C & D\nC -->|\"First\"| A -->|\"Next\"| D\n"
        );
    }

    #[test]
    fn complete_arrow_tokens_are_preserved() {
        for operator in [
            "-->", "--->", "<-->", "---", "----", "-.->", "-..->", "-.-", "==>", "===>", "<==>",
            "o--o", "x--x", "--o", "--x", "~~~",
        ] {
            let source = format!("flowchart LR\nA[Alpha]\nB[Beta]\nA {operator} B\n");
            let lens = FlowchartParseLens::parse(&source).unwrap();
            let mut edited = lens.ir().clone();
            assert_eq!(edited.edges.len(), 1, "{operator}");
            assert!(edited.edges[0].label.is_none(), "{operator}");
            let span = edited.edges[0].span;
            let label = append(&mut edited, "Link", span);
            edited.edges[0].label = Some(label);
            assert_eq!(
                lens.put(&edited),
                Ok(source.replace(
                    &format!("A {operator} B"),
                    &format!("A {operator}|\"Link\"| B")
                )),
                "{operator} must retain its complete arrow token"
            );
        }
    }

    #[test]
    fn partial_or_conflicting_shared_token_requests_are_rejected() {
        let lens = FlowchartParseLens::parse("flowchart TB\nA & B --> C & D\n").unwrap();
        let mut edited = lens.ir().clone();
        let span = edited.edges[0].span;
        let first = append(&mut edited, "First", span);
        edited.edges[0].label = Some(first);
        assert!(lens.put(&edited).is_err());
        for edge in &mut edited.edges {
            edge.label = Some(first);
        }
        let second = append(&mut edited, "Different", span);
        edited.edges[1].label = Some(second);
        assert!(lens.put(&edited).is_err());
    }

    #[test]
    fn bare_declarations_repeated_references_and_self_loops_can_be_labeled() {
        for source in ["flowchart LR\nA\nA --> B\n", "flowchart LR\nA --> A\n"] {
            let lens = FlowchartParseLens::parse(source).unwrap();
            let mut edited = lens.ir().clone();
            let span = edited.nodes[0].span_primary;
            let label = append(&mut edited, "Alpha", span);
            edited.nodes[0].label = Some(label);
            let updated = lens.put(&edited).unwrap();
            assert_eq!(updated.matches("[\"Alpha\"]").count(), 1);
            let actual = parse(&updated).ir;
            assert_eq!(actual.nodes.len(), edited.nodes.len());
            assert_eq!(actual.edges.len(), edited.edges.len());
            assert_eq!(
                actual.labels[actual.nodes[0].label.unwrap().0].text,
                "Alpha"
            );
        }
    }

    #[test]
    fn explicit_empty_delimiters_cannot_be_left_as_ignored_source_junk() {
        for source in ["flowchart LR\nA[] --> B\n", "flowchart LR\nA[\"\"] --> B\n"] {
            let lens = FlowchartParseLens::parse(source).unwrap();
            let mut edited = lens.ir().clone();
            let span = edited.nodes[0].span_primary;
            let label = append(&mut edited, "Alpha", span);
            edited.nodes[0].label = Some(label);
            assert!(lens.put(&edited).is_err());
        }
        let lens = FlowchartParseLens::parse("flowchart LR\nA -->|| B\n").unwrap();
        let mut edited = lens.ir().clone();
        let span = edited.edges[0].span;
        let label = append(&mut edited, "Link", span);
        edited.edges[0].label = Some(label);
        assert!(lens.put(&edited).is_err());
    }

    #[test]
    fn unused_invalid_metadata_and_syntax_edits_are_rejected() {
        let lens = FlowchartParseLens::parse("flowchart LR\nA --> B\n").unwrap();
        // Arrow-looking text inside the complete quoted node label is literal content. It
        // must remain accepted when the parser proves that it creates no additional graph.
        let text = "changed\"| C --> D |\"extra";
        let mut literal = lens.ir().clone();
        let span = literal.nodes[0].span_primary;
        let label = append(&mut literal, text, span);
        literal.nodes[0].label = Some(label);
        let actual = parse(&lens.put(&literal).unwrap()).ir;
        assert_eq!((actual.nodes.len(), actual.edges.len()), (2, 1));
        assert_eq!(actual.labels[actual.nodes[0].label.unwrap().0].text, text);
        for text in [
            "",
            "changed\"] --> Injected[\"extra",
            "changed\"]\nC --> D\nE[\"extra",
        ] {
            let mut edited = lens.ir().clone();
            let span = edited.nodes[0].span_primary;
            let label = append(&mut edited, text, span);
            edited.nodes[0].label = Some(label);
            assert!(lens.put(&edited).is_err(), "{text:?}");
        }
        let mut edited = lens.ir().clone();
        let span = edited.edges[0].span;
        append(&mut edited, "Unused", span);
        assert!(lens.put(&edited).is_err());
        edited = lens.ir().clone();
        edited.edges[0].label = Some(IrLabelId(9999));
        assert!(lens.put(&edited).is_err());
        edited = lens.ir().clone();
        let label = append(&mut edited, "Link", span);
        edited.edges[0].label = Some(label);
        edited.meta.title = Some("unrequested title".to_owned());
        assert!(lens.put(&edited).is_err());
    }

    #[test]
    fn public_diagram_and_snapshot_paths_share_the_guarded_attachment_writer() {
        for source in [
            "flowchart LR\nA --> B\n",
            "sequenceDiagram\nA->>B:\n",
            "classDiagram\nA --> B\n",
            "stateDiagram-v2\nA --> B:\n",
            "erDiagram\nA ||--o{ B :\n",
        ] {
            let snapshot = build_parse_lens(source);
            let lens = DiagramParseLens::parse(source).unwrap();
            let mut edited = snapshot.parsed.ir.clone();
            assert!(edited.edges[0].label.is_none(), "{source}");
            let span = edited.edges[0].span;
            let label = append(&mut edited, "Added 🦀", span);
            edited.edges[0].label = Some(label);
            let updated = snapshot.put_ir(&edited).expect(source);
            assert_eq!(lens.put(&edited).unwrap(), updated);
            let next = build_parse_lens(&updated);
            let mut renamed = next.parsed.ir.clone();
            let label = renamed.edges[0].label.unwrap();
            renamed.labels[label.0].text = "Renamed".to_owned();
            assert_eq!(
                next.put_ir(&renamed).unwrap(),
                updated.replace("Added 🦀", "Renamed")
            );
            renamed.labels[label.0].text = "bad\"\nC --> D".to_owned();
            assert!(next.put_ir(&renamed).is_err(), "{source}");
            edited.meta.title = Some("unrequested title".to_owned());
            assert!(snapshot.put_ir(&edited).is_err(), "{source}");
        }
    }

    #[test]
    fn public_node_rename_and_declaration_reorder_cannot_bypass_full_ir_validation() {
        let source = "flowchart LR\nA[First]\nB[Second]\nA --> B\n";
        let snapshot = build_parse_lens(source);
        let mut edited = snapshot.parsed.ir.clone();
        edited.labels[edited.nodes[0].label.unwrap().0].text = "First] --> C[Injected".to_owned();
        assert!(snapshot.put_ir(&edited).is_err());
        let expected = "flowchart LR\nB[Second]\nA[First]\nA --> B\n";
        let reordered = parse(expected).ir;
        assert_eq!(snapshot.put_ir(&reordered).unwrap(), expected);
        let mut mutated = reordered;
        mutated.meta.title = Some("unrequested title".to_owned());
        assert!(snapshot.put_ir(&mutated).is_err());

        let class = build_parse_lens("classDiagram\nclass A\nclass B\n");
        let mut edited = class.parsed.ir.clone();
        edited.nodes.swap(0, 1);
        assert_eq!(
            class.put_ir(&edited).unwrap(),
            "classDiagram\nclass B\nclass A\n"
        );
        edited.nodes[0].classes.push("unrequested".to_owned());
        assert!(class.put_ir(&edited).is_err());
    }
}

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

#[test]
fn put_changes_a_single_markdown_text_run_without_losing_formatting() {
    for (markdown, bold, italic) in [
        ("**Old**", true, false),
        ("*Old*", false, true),
        ("Old", false, false),
    ] {
        let source = format!(
            "flowchart LR\r\n  %% preserve formatting\r\n  A[\"`{markdown}`\"] --> B[Keep]\r\n"
        );
        let lens = FlowchartParseLens::parse(&source).expect("Markdown flowchart lens");
        let label = lens.ir().nodes[0].label.expect("A has a label");
        let mut edited = lens.ir().clone();
        edited.labels[label.0].text = "New 🦀".to_owned();

        let updated = lens
            .put(&edited)
            .expect("a plain rename preserves Markdown");
        assert_eq!(updated, source.replace("Old", "New 🦀"));
        let reparsed = parse(&updated).ir;
        let updated_label = reparsed.nodes[0].label.expect("renamed A has a label");
        assert_eq!(reparsed.labels[updated_label.0].text, "New 🦀");
        assert_eq!(
            reparsed.label_markup[&updated_label],
            vec![fm_core::IrLabelSegment::Text {
                text: "New 🦀".to_owned(),
                bold,
                italic,
                code: false,
                strike: false,
            }],
            "the rename must preserve the original formatting flags"
        );
        assert_eq!(lens.original_source(), source);
        assert_eq!(lens.ir().labels[label.0].text, "Old");
    }
}

#[test]
fn put_rejects_markdown_syntax_injection_and_unrequested_style_changes() {
    let source = "flowchart TB\nA[\"`**Safe**`\"] --> B[Keep]\n";
    let lens = FlowchartParseLens::parse(source).expect("Markdown flowchart lens");
    let label = lens.ir().nodes[0].label.expect("A has a label");
    for replacement in ["Safe**`\"] --> Injected[Bad]\nA[\"`**Safe", "**Changed**"] {
        let mut edited = lens.ir().clone();
        edited.labels[label.0].text = replacement.to_owned();
        assert_eq!(
            lens.put(&edited),
            Err(FlowchartParseLensError::UnsupportedIrChange),
            "derived Markdown text must not bypass semantic validation"
        );
    }

    let mut edited = lens.ir().clone();
    edited.labels[label.0].text = "Renamed".to_owned();
    let [fm_core::IrLabelSegment::Text { bold, .. }] =
        edited.label_markup.get_mut(&label).unwrap().as_mut_slice()
    else {
        panic!("fixture has a single bold text run");
    };
    *bold = false;
    assert_eq!(
        lens.put(&edited),
        Err(FlowchartParseLensError::UnsupportedIrChange),
        "the caller cannot change formatting metadata with a text-only edit"
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_reorders_declarations_and_preserves_the_surrounding_document() {
    let source = concat!(
        "%%{init: {'theme':'forest'}}%%\r\n",
        "%%{deck: {slides:[{id:'intro',nodes:['A','B','C']}]}}%%\r\n",
        "flowchart TB\r\n",
        "  %% retain these comments and all whitespace\r\n",
        "  A[Root]  \r\n",
        "\tB[\"Left 🦀\"]:::hot \t\r\n",
        "  %% keep this line in place\r\n",
        "    C{{\"Right\"}}\r\n",
        "\r\n",
        "  A -->|first| B\r\n",
        "  A -.-> C\r\n",
        "  classDef hot fill:#f00,stroke:#333\r\n",
        "  style B color:#fff\r\n",
        "  linkStyle 0 stroke:#456\r\n",
        "  click C \"https://example.com/\" \"Open details\"\r\n",
        "%% retain a final comment without a newline",
    );
    let expected = source
        .replace("\tB[\"Left 🦀\"]:::hot \t", "\tC{{\"Right\"}} \t")
        .replace("    C{{\"Right\"}}", "    B[\"Left 🦀\"]:::hot");
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    let edited = parse(&expected).ir;
    let updated = lens
        .put(&edited)
        .expect("independent declarations can move");
    assert_eq!(updated, expected);
    assert_eq!(
        parse(&updated)
            .ir
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["A", "C", "B"]
    );
    assert_eq!(
        lens.original_source(),
        source,
        "put never mutates its snapshot"
    );

    let refreshed = FlowchartParseLens::parse(&updated).expect("updated flowchart lens");
    assert_eq!(
        refreshed
            .put(lens.ir())
            .expect("the inverse permutation is safe"),
        source,
        "inverse edits preserve the exact format complement"
    );
}

#[test]
fn put_reorders_labels_by_semantics_including_shared_text_and_markup() {
    let source = "flowchart TB\nA[Same]\nB[\"`**Same**`\"]\nC[Same]\nA --> B\nA --> C\n";
    let expected = "flowchart TB\nA[Same]\nC[Same]\nB[\"`**Same**`\"]\nA --> B\nA --> C\n";
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    assert_eq!(
        lens.put(&parse(expected).ir)
            .expect("markup stays on its authored node"),
        expected
    );
}

#[test]
fn put_rejects_node_permutations_with_stale_references_or_other_changes() {
    let source = "flowchart TB\nA[Root]\nB[Left]\nC[Right]\nA --> B\nA --> C\n";
    let expected = "flowchart TB\nA[Root]\nC[Right]\nB[Left]\nA --> B\nA --> C\n";
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    let mut stale = lens.ir().clone();
    stale.nodes.swap(1, 2);
    assert_eq!(
        lens.put(&stale),
        Err(FlowchartParseLensError::UnsupportedIrChange)
    );

    let valid = parse(expected).ir;
    let mut rewired = valid.clone();
    rewired.edges[0].to = rewired.edges[1].to;
    assert_eq!(
        lens.put(&rewired),
        Err(FlowchartParseLensError::UnsupportedIrChange)
    );

    let mut relabeled = valid.clone();
    let label = relabeled.nodes[1].label.expect("C has a label");
    relabeled.labels[label.0].text = "Unrequested source edit".to_owned();
    assert_eq!(
        lens.put(&relabeled),
        Err(FlowchartParseLensError::UnsupportedIrChange)
    );

    let mut changed_shape = valid;
    changed_shape.nodes[1].shape = fm_core::NodeShape::Circle;
    assert_eq!(
        lens.put(&changed_shape),
        Err(FlowchartParseLensError::UnsupportedIrChange)
    );
}

#[test]
fn put_rejects_ambiguous_declarations_and_subgraph_moves() {
    for (source, reordered) in [
        (
            "flowchart TB\nA[Root] --> B[Left]\nA --> C[Right]\n",
            "flowchart TB\nA[Root] --> C[Right]\nA --> B[Left]\n",
        ),
        (
            "flowchart TB\nA[Root]; B[Left]\nC[Right]\nA --> B\nA --> C\n",
            "flowchart TB\nA[Root]; C[Right]\nB[Left]\nA --> B\nA --> C\n",
        ),
        (
            "flowchart TB\nsubgraph Group\nA[Root]\nB[Left]\nC[Right]\nend\nA --> B\nA --> C\n",
            "flowchart TB\nsubgraph Group\nA[Root]\nC[Right]\nB[Left]\nend\nA --> B\nA --> C\n",
        ),
    ] {
        let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
        assert_eq!(
            lens.put(&parse(reordered).ir),
            Err(FlowchartParseLensError::UnsupportedIrChange),
            "an ambiguous source must remain untouched: {source}"
        );
        assert_eq!(lens.original_source(), source);
    }
}

#[test]
fn put_rejects_reorders_that_change_first_mention_semantics() {
    let source = "flowchart TB\nA[one]\nA((two))\nB[beta]\n";
    let candidate = "flowchart TB\nB[beta]\nA((two))\nA[one]\n";
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    let edited = parse(candidate).ir;
    let original_label = lens.ir().nodes[0].label.expect("original A label");
    let reordered_label = edited.nodes[1].label.expect("reordered A label");
    assert_eq!(lens.ir().labels[original_label.0].text, "one");
    assert_eq!(edited.labels[reordered_label.0].text, "two");
    assert_eq!(
        lens.put(&edited),
        Err(FlowchartParseLensError::UnsupportedIrChange),
        "even an exactly reparsable source must preserve original node semantics"
    );
}

#[test]
fn put_rejects_labels_that_would_change_the_graph() {
    let source = "flowchart TB\nA[\"Safe\"] --> B[Other]\n";
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    for replacement in [
        "Safe\"] --> Injected[Bad",
        "Safe\"]\nInjected[Bad]\nA[\"Safe",
        "Safe\"]; Injected[Bad]; A[\"Safe",
    ] {
        let mut edited = lens.ir().clone();
        let label = edited.nodes[0].label.expect("A has a label");
        edited.labels[label.0].text = replacement.to_owned();
        assert_eq!(
            lens.put(&edited),
            Err(FlowchartParseLensError::UnsupportedIrChange),
            "label syntax must not silently change topology: {replacement}"
        );
    }
}

#[test]
fn put_updates_every_occurrence_of_a_label_shared_with_an_edge() {
    let source = "flowchart TB\nA[Shared] -->|Shared| B[Shared]\n";
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.expect("A has a label");
    assert_eq!(
        edited.edges[0].label,
        Some(label),
        "the fixture shares a label"
    );
    edited.labels[label.0].text = "Changed".to_owned();
    let updated = lens.put(&edited).expect("nodes and edge update together");
    assert_eq!(updated, source.replace("Shared", "Changed"));
    let reparsed = parse(&updated).ir;
    assert_eq!(reparsed.nodes[0].label, reparsed.edges[0].label);
    assert_eq!(reparsed.nodes[1].label, reparsed.edges[0].label);
    assert_eq!(
        reparsed.labels[reparsed.edges[0].label.unwrap().0].text,
        "Changed"
    );
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_changes_pipe_and_inline_edge_labels_without_changing_ids_styles_or_wrappers() {
    for (statement, token) in [
        ("Old -->|\"Old\"| B", "\"Old\""),
        ("Old -- \"Old\" --> B", "\"Old\""),
        ("Old -. \"Old\" .-> B", "\"Old\""),
        ("Old == \"Old\" ==> B", "\"Old\""),
        ("Old -->|\"`**Old**`\"| B", "\"`**Old**`\""),
    ] {
        let source = format!(
            concat!(
                "flowchart LR\r\n",
                "  %% Old remains in this comment\r\n",
                "  Old[Start]\r\n",
                "  B[End]\r\n",
                "  {statement}\r\n",
                "  linkStyle 0 stroke:#123456,stroke-width:3px\r\n",
            ),
            statement = statement,
        );
        let lens = FlowchartParseLens::parse(&source).unwrap();
        let original = lens.ir();
        assert_eq!(original.edges.len(), 1, "{statement}");
        let label = original.edges[0].label.unwrap();
        assert_eq!(original.labels[label.0].text, "Old", "{statement}");
        assert!(original.edges[0].inline_style.is_some());
        let mut edited = original.clone();
        edited.labels[label.0].text = "Nouveau 🦀".to_owned();
        let updated = lens.put(&edited).expect("existing edge text re-emits");
        let expected_statement = statement.replace(token, &token.replace("Old", "Nouveau 🦀"));
        assert_eq!(updated, source.replace(statement, &expected_statement));
        let reparsed = parse(&updated).ir;
        assert_eq!(reparsed.nodes[0].id, "Old");
        assert_eq!(
            reparsed.labels[reparsed.edges[0].label.unwrap().0].text,
            "Nouveau 🦀"
        );
        assert_eq!(
            reparsed.edges[0].inline_style,
            original.edges[0].inline_style
        );
        assert_eq!(reparsed.edges[0].extras, original.edges[0].extras);
        assert_eq!(
            (
                reparsed.edges[0].from,
                reparsed.edges[0].to,
                reparsed.edges[0].arrow
            ),
            (
                original.edges[0].from,
                original.edges[0].to,
                original.edges[0].arrow
            )
        );
        assert_eq!(lens.original_source(), source);
    }
}

#[test]
fn put_updates_parallel_and_chained_edge_tokens_once_with_simultaneous_label_edits() {
    let source = concat!(
        "flowchart TB\n",
        "  A[Alpha]\n",
        "  B[Beta]\n",
        "  C[Gamma]\n",
        "  D[Delta]\n",
        "  A & B -->|Shared| C & D\n",
        "  C -->|Shared| A -->|Shared| D\n",
        "  B -->|Other| A\n",
    );
    let lens = FlowchartParseLens::parse(source).unwrap();
    let original = lens.ir();
    assert_eq!(original.edges.len(), 7);
    let shared = original.edges[0].label.unwrap();
    assert!(
        original.edges[..6]
            .iter()
            .all(|edge| edge.label == Some(shared))
    );
    let other = original.edges[6].label.unwrap();
    let mut edited = original.clone();
    edited.labels[shared.0].text = "Longer 🦀".to_owned();
    edited.labels[other.0].text = "Next".to_owned();
    let updated = lens
        .put(&edited)
        .expect("shared source ranges are written once");
    assert_eq!(
        updated,
        source
            .replace("|Shared|", "|Longer 🦀|")
            .replace("|Other|", "|Next|")
    );
    let reparsed = parse(&updated).ir;
    assert_eq!(reparsed.edges.len(), original.edges.len());
    for (index, (before, after)) in original.edges.iter().zip(&reparsed.edges).enumerate() {
        assert_eq!(
            (before.from, before.to, before.arrow),
            (after.from, after.to, after.arrow)
        );
        assert_eq!(
            reparsed.labels[after.label.unwrap().0].text,
            if index < 6 { "Longer 🦀" } else { "Next" }
        );
    }
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_rejects_edge_label_injection_removal_and_normalization() {
    let source = "flowchart TB\nA[Start]\nB[End]\nA -->|Safe| B\n";
    let lens = FlowchartParseLens::parse(source).unwrap();
    let replacement = "Safe| B\nInjected -->|Bad";
    let candidate = parse(&source.replace("Safe", replacement)).ir;
    assert_eq!(candidate.nodes.len(), lens.ir().nodes.len() + 1);
    assert_eq!(candidate.edges.len(), lens.ir().edges.len() + 1);
    for replacement in [replacement, "", "#83;afe"] {
        let mut edited = lens.ir().clone();
        let label = edited.edges[0].label.unwrap();
        edited.labels[label.0].text = replacement.to_owned();
        assert_eq!(
            lens.put(&edited),
            Err(FlowchartParseLensError::UnsupportedIrChange)
        );
    }
    assert_eq!(lens.original_source(), source);
}

#[test]
fn put_rejects_shared_labels_when_an_encoded_edge_cannot_be_located() {
    let source = "flowchart TB\nA[Shared] -->|#83;hared| B[Shared]\n";
    let lens = FlowchartParseLens::parse(source).unwrap();
    let mut edited = lens.ir().clone();
    let label = edited.nodes[0].label.unwrap();
    assert_eq!(edited.edges[0].label, Some(label));
    edited.labels[label.0].text = "Changed".to_owned();
    assert_eq!(
        lens.put(&edited),
        Err(FlowchartParseLensError::UnsupportedIrChange)
    );
    assert_eq!(lens.original_source(), source);
    assert_eq!(lens.ir().labels[label.0].text, "Shared");
}

#[test]
fn put_accepts_unicode_and_label_deduplication_without_changing_topology() {
    let source = "flowchart TB\nA[First] --> B[Second]\n";
    let lens = FlowchartParseLens::parse(source).expect("flowchart lens");
    for replacement in ["naïve 🦀", "Second"] {
        let mut edited = lens.ir().clone();
        let label = edited.nodes[0].label.expect("A has a label");
        edited.labels[label.0].text = replacement.to_owned();
        assert_eq!(
            lens.put(&edited).expect("plain label edit round-trips"),
            source.replace("First", replacement)
        );
    }
}
