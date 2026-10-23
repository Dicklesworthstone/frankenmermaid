use std::{fs, path::Path};

use fm_core::DiagramType;
use fm_parser::{FlowchartParseLens, FlowchartParseLensError, parse};
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
