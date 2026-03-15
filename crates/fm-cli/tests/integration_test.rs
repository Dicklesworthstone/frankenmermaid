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
    assert_eq!(
        ir.graph.subgraphs.len(),
        2,
        "Expected two parsed graph subgraphs"
    );
    assert_eq!(
        ir.graph.clusters.len(),
        2,
        "Expected two graph-level cluster mirrors"
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
    assert!(
        ir.graph
            .nodes
            .iter()
            .all(|node| !node.subgraphs.is_empty() && !node.clusters.is_empty()),
        "All subgraph-contained nodes should retain graph membership"
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
        if let Err(err) = child_stdin.write_all(stdin.as_bytes())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            panic!("failed writing stdin to fm-cli: {err}");
        }
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

#[test]
fn render_json_requires_output_path() {
    let output = run_cli(&["render", "-", "--json"], "flowchart LR\nA-->B\n");
    assert!(
        !output.status.success(),
        "render --json without --output should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--json requires --output"));
}

#[test]
fn render_json_writes_artifact_and_stdout_metadata() {
    let output_file = NamedTempFile::new().expect("temp render output file");
    let output_path = output_file
        .path()
        .to_str()
        .expect("temp path must be valid utf-8")
        .to_string();

    let output = run_cli(
        &[
            "render",
            "-",
            "--format",
            "svg",
            "--json",
            "--output",
            &output_path,
        ],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "render --json with --output should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("render --json must print metadata JSON to stdout");
    assert_eq!(json["format"], "svg");
    assert_eq!(json["diagram_type"], "flowchart");
    assert_eq!(json["layout_requested"], "auto");
    assert_eq!(json["layout_selected"], "sugiyama");
    assert_eq!(json["layout_band_count"], 0);
    assert_eq!(json["layout_tick_count"], 0);
    assert!(json["output_bytes"].as_u64().is_some_and(|value| value > 0));

    let artifact = std::fs::read_to_string(&output_path).expect("failed to read rendered svg");
    assert!(artifact.starts_with("<svg"));
    assert!(artifact.contains("</svg>"));
}

#[test]
fn render_json_reports_specialized_auto_layout_selection() {
    let cases = [
        (
            "timeline\ntitle Roadmap\n2024 : Kickoff\n2025 : Launch\n",
            "timeline",
        ),
        (
            "gantt\ntitle Ship\nsection Planning\nScope: task_1, 2024-01-01, 1d\n",
            "gantt",
        ),
        (
            "journey\ntitle Sprint\nsection Board\nBacklog: 5: Alice\n",
            "kanban",
        ),
        ("sankey-beta\nA, B, 3\nB, C, 2\n", "sankey"),
        ("block-beta\ncolumns 2\nA\nB\n", "grid"),
    ];

    for (input, expected_layout) in cases {
        let output_file = NamedTempFile::new().expect("temp render output file");
        let output_path = output_file
            .path()
            .to_str()
            .expect("temp path must be valid utf-8")
            .to_string();

        let output = run_cli(
            &[
                "render",
                "-",
                "--format",
                "svg",
                "--json",
                "--output",
                &output_path,
            ],
            input,
        );
        assert!(
            output.status.success(),
            "render --json should succeed for specialized layout; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("render --json must print metadata JSON");
        assert_eq!(json["layout_requested"], "auto");
        assert_eq!(json["layout_selected"], expected_layout);
        assert!(json["layout_band_count"].is_u64());
        assert!(json["layout_tick_count"].is_u64());
    }
}

#[test]
fn detect_reports_gitgraph_as_basic_support() {
    let output = run_cli(&["detect", "-", "--json"], "gitGraph\ncommit\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "gitGraph");
    assert_eq!(json["support_level"], "basic");
}

#[test]
fn detect_reports_sankey_as_basic_support() {
    let output = run_cli(&["detect", "-", "--json"], "sankey-beta\nA, B, 3\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "sankey");
    assert_eq!(json["support_level"], "basic");
    assert_eq!(json["confidence"], "high");
}

#[test]
fn detect_reports_block_beta_as_basic_support() {
    let output = run_cli(&["detect", "-", "--json"], "block-beta\nalpha[Alpha]\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "block-beta");
    assert_eq!(json["support_level"], "basic");
    assert_eq!(json["confidence"], "high");
}

#[test]
fn detect_accepts_block_alias_as_block_beta() {
    let output = run_cli(&["detect", "-", "--json"], "block\nalpha[Alpha]\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "block-beta");
    assert_eq!(json["support_level"], "basic");
    assert_eq!(json["confidence"], "high");
}

#[test]
fn detect_does_not_treat_blockquote_as_block_beta() {
    let output = run_cli(&["detect", "-", "--json"], "blockquote\nalpha[Alpha]\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_ne!(json["diagram_type"], "block-beta");
}

#[test]
fn detect_reports_dot_inputs_via_dot_format_method() {
    let output = run_cli(&["detect", "-", "--json"], "digraph G { a -> b; }\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "flowchart");
    assert_eq!(json["confidence"], "high");
    assert_eq!(json["detection_method"], "DOT format detected");
}

#[test]
fn detect_reports_fuzzy_keyword_method_for_header_typos() {
    let output = run_cli(&["detect", "-", "--json"], "flwochart LR\nA-->B\n");
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "flowchart");
    assert_eq!(json["confidence"], "medium");
    assert_eq!(json["detection_method"], "fuzzy keyword match");
}

#[test]
fn parse_full_reports_canonical_core_support_level() {
    let output = run_cli(&["parse", "-", "--full"], "classDiagram\nAnimal <|-- Dog\n");
    assert!(
        output.status.success(),
        "parse --full should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse --full must print valid JSON");
    assert_eq!(json["meta"]["support_level"], "Partial");
}

#[test]
fn parse_summary_reports_parse_mode_and_support_level_for_unsupported_family() {
    let output = run_cli(
        &["parse", "-", "--parse-mode", "compat", "--pretty"],
        "architecture-beta\nservice api\n",
    );
    assert!(
        output.status.success(),
        "parse summary should succeed in compat mode; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse summary must print valid JSON");
    assert_eq!(json["diagram_type"], "architecture-beta");
    assert_eq!(json["parse_mode"], "compat");
    assert_eq!(json["support_level"], "Unsupported");
    assert!(json["diagnostic_count"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn parse_summary_reports_sankey_counts_without_compatibility_fallback() {
    let output = run_cli(
        &["parse", "-", "--parse-mode", "compat", "--pretty"],
        "sankey-beta\nA, B, 3\nB, C, 2\n",
    );
    assert!(
        output.status.success(),
        "parse summary should succeed for sankey; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse summary must print valid JSON");
    assert_eq!(json["diagram_type"], "sankey");
    assert_eq!(json["parse_mode"], "compat");
    assert_eq!(json["support_level"], "Partial");
    assert_eq!(json["node_count"], 3);
    assert_eq!(json["edge_count"], 2);
    assert_eq!(json["diagnostic_count"], 0);
}

#[test]
fn validate_strict_mode_fails_unsupported_family_with_compatibility_diagnostic() {
    let output = run_cli(
        &[
            "validate",
            "-",
            "--parse-mode",
            "strict",
            "--format",
            "json",
        ],
        "architecture-beta\nservice api\n",
    );
    assert!(
        !output.status.success(),
        "validate strict should fail for unsupported family"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate json must print valid JSON");
    assert_eq!(json["valid"], false);
    assert_eq!(json["parse_mode"], "strict");
    assert!(json["diagnostics"].as_array().is_some_and(|diagnostics| {
        diagnostics.iter().any(|diagnostic| {
            diagnostic["rule_id"] == "parse.compatibility" && diagnostic["severity"] == "error"
        })
    }));
}

#[test]
fn validate_reports_layout_dispatch_fallback_when_requested_family_is_unavailable() {
    let output = run_cli(
        &[
            "validate",
            "-",
            "--layout-algorithm",
            "timeline",
            "--format",
            "json",
            "--fail-on",
            "none",
        ],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "validate should succeed with fail-on none; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate json must print valid JSON");
    assert_eq!(json["layout_requested"], "timeline");
    assert_eq!(json["layout_selected"], "sugiyama");
    assert!(json["diagnostics"].as_array().is_some_and(|diagnostics| {
        diagnostics.iter().any(|diagnostic| {
            diagnostic["rule_id"] == "layout.dispatch.selection"
                && diagnostic["severity"] == "warning"
        })
    }));
}
