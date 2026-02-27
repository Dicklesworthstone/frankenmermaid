//! Integration tests for the FrankenMermaid pipeline.
//!
//! These tests verify the end-to-end flow from parsing to layout to rendering.

use fm_core::{DiagramType, GraphDirection};
use fm_layout::{layout_diagram, layout_diagram_traced};
use fm_parser::parse;
use fm_render_svg::render_svg;
use fm_render_term::render_term;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

/// Test that a simple flowchart parses and produces non-zero layout positions.
#[test]
fn flowchart_parses_and_lays_out_with_nonzero_positions() {
    let input = r#"flowchart LR
    A[Start] --> B[Process]
    B --> C[End]
"#;

    let parse_result = parse(input);
    // ParseResult has warnings, not errors. Check warnings for critical issues.
    assert!(
        parse_result.warnings.is_empty(),
        "Parse warnings: {:?}",
        parse_result.warnings
    );

    let ir = parse_result.ir;
    assert_eq!(ir.diagram_type, DiagramType::Flowchart);
    assert_eq!(ir.direction, GraphDirection::LR);
    assert_eq!(ir.nodes.len(), 3, "Expected 3 nodes");
    assert_eq!(ir.edges.len(), 2, "Expected 2 edges");

    let layout = layout_diagram(&ir);
    assert_eq!(layout.nodes.len(), 3);
    assert_eq!(layout.edges.len(), 2);

    // Verify all nodes have non-zero bounds.
    for node in &layout.nodes {
        assert!(
            node.bounds.width > 0.0,
            "Node {} has zero width",
            node.node_id
        );
        assert!(
            node.bounds.height > 0.0,
            "Node {} has zero height",
            node.node_id
        );
    }

    // Verify layout bounds are positive.
    assert!(layout.bounds.width > 0.0, "Layout has zero width");
    assert!(layout.bounds.height > 0.0, "Layout has zero height");

    // Verify edges have at least 2 points.
    for edge in &layout.edges {
        assert!(
            edge.points.len() >= 2,
            "Edge {} has fewer than 2 points",
            edge.edge_index
        );
    }
}

/// Test that SVG rendering produces valid output.
#[test]
fn flowchart_renders_to_valid_svg() {
    let input = "flowchart TD\n    A --> B";

    let parse_result = parse(input);
    let ir = parse_result.ir;

    let svg = render_svg(&ir);

    // Basic validity checks.
    assert!(svg.starts_with("<svg"), "SVG should start with <svg tag");
    assert!(svg.contains("</svg>"), "SVG should end with </svg>");
    assert!(svg.contains("viewBox"), "SVG should have a viewBox");
    assert!(svg.contains("<rect"), "SVG should contain rect elements");
    assert!(svg.contains("<path"), "SVG should contain path elements");
}

/// Test that terminal rendering produces non-empty output.
#[test]
fn flowchart_renders_to_terminal() {
    let input = "flowchart LR\n    A --> B --> C";

    let parse_result = parse(input);
    let ir = parse_result.ir;

    let term_output = render_term(&ir);

    // Should produce some output.
    assert!(
        !term_output.is_empty(),
        "Terminal output should not be empty"
    );
    assert!(
        term_output.lines().count() > 0,
        "Should have multiple lines"
    );
}

/// Test determinism: same input produces same layout.
#[test]
fn layout_is_deterministic() {
    let input = r#"flowchart TD
    A[Alpha] --> B[Beta]
    A --> C[Gamma]
    B --> D[Delta]
    C --> D
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    let layout1 = layout_diagram_traced(&ir);
    let layout2 = layout_diagram_traced(&ir);

    // Layouts should be identical.
    assert_eq!(
        layout1.layout.nodes.len(),
        layout2.layout.nodes.len(),
        "Node counts differ"
    );

    for (n1, n2) in layout1.layout.nodes.iter().zip(layout2.layout.nodes.iter()) {
        assert_eq!(n1.node_id, n2.node_id, "Node IDs differ");
        assert!(
            (n1.bounds.x - n2.bounds.x).abs() < 0.001,
            "Node {} x position differs",
            n1.node_id
        );
        assert!(
            (n1.bounds.y - n2.bounds.y).abs() < 0.001,
            "Node {} y position differs",
            n1.node_id
        );
    }

    // Stats should match.
    assert_eq!(
        layout1.layout.stats.crossing_count, layout2.layout.stats.crossing_count,
        "Crossing counts differ"
    );
}

/// Test that cycles are handled gracefully.
#[test]
fn handles_cyclic_graph() {
    let input = r#"flowchart LR
    A --> B
    B --> C
    C --> A
"#;

    let parse_result = parse(input);
    assert!(
        parse_result.warnings.is_empty(),
        "Cyclic graph should parse: {:?}",
        parse_result.warnings
    );

    let ir = parse_result.ir;
    let layout = layout_diagram(&ir);

    // Should still produce valid layout.
    assert_eq!(layout.nodes.len(), 3);
    assert!(
        layout.stats.reversed_edges >= 1,
        "Should have reversed edges"
    );

    // All nodes should have valid positions.
    for node in &layout.nodes {
        assert!(
            node.bounds.x.is_finite() && node.bounds.y.is_finite(),
            "Node {} has non-finite position",
            node.node_id
        );
    }
}

/// Test parsing of different diagram types.
#[test]
fn detects_diagram_types_correctly() {
    let test_cases = [
        ("flowchart TD\nA-->B", DiagramType::Flowchart),
        ("graph LR\nA-->B", DiagramType::Flowchart),
        ("sequenceDiagram\nAlice->>Bob: Hello", DiagramType::Sequence),
        ("classDiagram\nAnimal <|-- Dog", DiagramType::Class),
        ("stateDiagram-v2\n[*] --> State1", DiagramType::State),
        ("pie\ntitle Pie\n\"A\": 30", DiagramType::Pie),
        (
            "gantt\ntitle Gantt\nsection S1\nTask: a, 2024-01-01, 1d",
            DiagramType::Gantt,
        ),
    ];

    for (input, expected_type) in test_cases {
        let result = parse(input);
        assert_eq!(
            result.ir.diagram_type,
            expected_type,
            "Failed for input: {}",
            input.lines().next().unwrap_or(input)
        );
    }
}

/// Test edge label handling.
#[test]
fn handles_edge_labels() {
    let input = r#"flowchart LR
    A -->|label1| B
    B -->|label2| C
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    // Should have 2 edges.
    assert_eq!(ir.edges.len(), 2);

    // Both edges should have labels.
    let edges_with_labels = ir.edges.iter().filter(|e| e.label.is_some()).count();
    assert!(
        edges_with_labels >= 1,
        "Expected at least one edge with label"
    );
}

/// Test node shape parsing.
#[test]
fn parses_node_shapes() {
    let input = r#"flowchart LR
    A[Rectangle]
    B(Rounded)
    C((Circle))
    D{Diamond}
"#;

    let parse_result = parse(input);
    let ir = parse_result.ir;

    assert!(ir.nodes.len() >= 4, "Expected at least 4 nodes");

    // Verify different shapes are recognized.
    let shapes: Vec<_> = ir.nodes.iter().map(|n| n.shape).collect();
    assert!(
        shapes.iter().any(|s| *s != fm_core::NodeShape::Rect),
        "Expected some non-rect shapes"
    );
}

/// Test subgraph/cluster handling.
#[test]
fn handles_subgraphs() {
    let input = r#"flowchart TD
    subgraph cluster1 [Cluster One]
        A --> B
    end
    subgraph cluster2 [Cluster Two]
        C --> D
    end
    B --> C
"#;

    let parse_result = parse(input);
    assert!(
        parse_result.warnings.is_empty(),
        "Unexpected parse warnings: {:?}",
        parse_result.warnings
    );
    let ir = parse_result.ir;

    // Parser should preserve subgraph structure as clusters.
    assert_eq!(ir.diagram_type, DiagramType::Flowchart);
    assert_eq!(
        ir.clusters.len(),
        2,
        "Expected two parsed subgraph clusters"
    );

    // Nodes and edges within subgraphs should still be parsed.
    assert_eq!(
        ir.nodes.len(),
        4,
        "Expected exactly 4 nodes from subgraph content"
    );
    assert_eq!(
        ir.edges.len(),
        3,
        "Expected exactly 3 edges from subgraph content"
    );

    // Cluster membership should match node sets declared in each subgraph.
    let node_index_by_id: std::collections::BTreeMap<String, usize> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| (node.id.clone(), idx))
        .collect();

    let cluster_members_by_title: std::collections::BTreeMap<
        String,
        std::collections::BTreeSet<String>,
    > = ir
        .clusters
        .iter()
        .filter_map(|cluster| {
            let title = cluster
                .title
                .and_then(|title_id| ir.labels.get(title_id.0))
                .map(|label| label.text.clone())?;
            let members = cluster
                .members
                .iter()
                .filter_map(|member| ir.nodes.get(member.0).map(|node| node.id.clone()))
                .collect::<std::collections::BTreeSet<_>>();
            Some((title, members))
        })
        .collect();
    assert_eq!(
        cluster_members_by_title.get("Cluster One"),
        Some(&std::collections::BTreeSet::from([
            "A".to_string(),
            "B".to_string()
        ]))
    );
    assert_eq!(
        cluster_members_by_title.get("Cluster Two"),
        Some(&std::collections::BTreeSet::from([
            "C".to_string(),
            "D".to_string()
        ]))
    );

    assert_eq!(
        node_index_by_id.len(),
        4,
        "Node index should include all parsed nodes"
    );

    // Layout should include clusters and remain valid.
    let layout = layout_diagram(&ir);
    assert_eq!(layout.nodes.len(), 4, "Layout should include all nodes");
    assert_eq!(layout.edges.len(), 3, "Layout should include all edges");
    assert_eq!(
        layout.clusters.len(),
        2,
        "Expected two rendered layout clusters"
    );

    // All nodes should have valid positions.
    for node in &layout.nodes {
        assert!(
            node.bounds.x.is_finite() && node.bounds.y.is_finite(),
            "Node {} has non-finite position",
            node.node_id
        );
    }
}

/// Test that very long labels are handled.
#[test]
fn handles_long_labels() {
    let long_label = "A".repeat(200);
    let input = format!("flowchart LR\n    A[{}]", long_label);

    let parse_result = parse(&input);
    assert!(
        parse_result.warnings.is_empty(),
        "Long label should parse: {:?}",
        parse_result.warnings
    );

    let layout = layout_diagram(&parse_result.ir);
    assert_eq!(layout.nodes.len(), 1);

    // Node should have positive width accommodating long label.
    assert!(layout.nodes[0].bounds.width > 0.0);
}

/// Test empty diagram handling.
#[test]
fn handles_empty_diagram() {
    let input = "flowchart TD";

    let parse_result = parse(input);
    let ir = parse_result.ir;

    // Should parse without fatal issues (warnings are ok for empty diagram).
    assert_eq!(ir.diagram_type, DiagramType::Flowchart);

    // Layout should handle empty graph.
    let layout = layout_diagram(&ir);
    assert_eq!(layout.nodes.len(), 0);
    assert_eq!(layout.edges.len(), 0);
}

/// Test direction handling for all directions.
#[test]
fn handles_all_directions() {
    let directions = [
        ("flowchart TB\nA-->B", GraphDirection::TB),
        ("flowchart TD\nA-->B", GraphDirection::TD),
        ("flowchart LR\nA-->B", GraphDirection::LR),
        ("flowchart RL\nA-->B", GraphDirection::RL),
        ("flowchart BT\nA-->B", GraphDirection::BT),
    ];

    for (input, expected_dir) in directions {
        let result = parse(input);
        assert_eq!(
            result.ir.direction, expected_dir,
            "Failed for direction {:?}",
            expected_dir
        );
    }
}

fn run_cli(args: &[&str], stdin: &str) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fm-cli"));
    command.args(args);

    if stdin.is_empty() {
        command
            .output()
            .expect("failed to run fm-cli without stdin")
    } else {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn fm-cli with stdin");
        let Some(mut child_stdin) = child.stdin.take() else {
            panic!("failed to open stdin pipe");
        };
        child_stdin
            .write_all(stdin.as_bytes())
            .expect("failed writing stdin to fm-cli");
        drop(child_stdin);
        child
            .wait_with_output()
            .expect("failed collecting fm-cli output")
    }
}

#[test]
fn validate_pretty_outputs_structured_diagnostics_payload() {
    let input = "flowchart LR\nA-->B\nB-->A\n";
    let output = run_cli(&["validate", "-", "--format", "pretty"], input);
    assert!(
        output.status.success(),
        "validate should succeed at default fail-on=error; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate pretty must produce json");
    assert!(json.get("diagnostics").is_some());
    assert!(json["diagnostics"].is_array());
    let first = json["diagnostics"]
        .as_array()
        .and_then(|items| items.first())
        .cloned()
        .expect("expected at least one diagnostic for cyclic graph");
    assert!(first.get("stage").is_some());
    assert!(first.get("error_code").is_some());
    assert!(first.get("severity").is_some());
    assert!(first.get("message").is_some());
}

#[test]
fn validate_fail_on_warning_returns_nonzero() {
    let input = "flowchart LR\nA-->B\nB-->A\n";
    let output = run_cli(
        &["validate", "-", "--format", "json", "--fail-on", "warning"],
        input,
    );
    assert!(
        !output.status.success(),
        "expected non-zero exit when warning threshold is selected"
    );
}

#[test]
fn validate_diagnostics_out_writes_artifact_file() {
    let input = "flowchart TD\nA-->B\n";
    let diagnostics_file = NamedTempFile::new().expect("temp diagnostics file");
    let diagnostics_path = diagnostics_file
        .path()
        .to_str()
        .expect("temp path must be valid utf-8")
        .to_string();

    let output = run_cli(
        &[
            "validate",
            "-",
            "--format",
            "json",
            "--diagnostics-out",
            &diagnostics_path,
        ],
        input,
    );
    assert!(
        output.status.success(),
        "validate with diagnostics-out should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact_raw =
        std::fs::read_to_string(&diagnostics_path).expect("failed to read diagnostics artifact");
    let artifact_json: serde_json::Value =
        serde_json::from_str(&artifact_raw).expect("artifact should be valid json");
    assert!(artifact_json.get("valid").is_some());
    assert!(artifact_json.get("diagnostics").is_some());
}
