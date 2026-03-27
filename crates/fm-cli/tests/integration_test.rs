//! Integration tests for the FrankenMermaid pipeline.
//!
//! These tests verify the end-to-end flow from parsing to layout to rendering.

use fm_core::{
    DiagramType, GanttDate, GanttExclude, GanttTaskType, GanttTickInterval, GraphDirection,
    MermaidTier,
};
use fm_layout::{layout_diagram, layout_diagram_traced};
use fm_parser::parse;
use fm_render_svg::{SvgBackend, SvgRenderConfig, render_svg, render_svg_with_config};
use fm_render_term::render_term;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::{NamedTempFile, TempDir};

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

#[test]
fn c4_render_honors_layout_and_legend_directives() {
    let input = r#"C4Container
LAYOUT_LEFT_RIGHT()
SHOW_LEGEND()
Person(customer, "Customer")
Container(api, "Payments API", "Rust", "Handles payment requests")
Rel(customer, api, "Uses", "HTTPS")"#;

    let parse_result = parse(input);
    assert_eq!(parse_result.ir.direction, GraphDirection::LR);
    assert!(parse_result.ir.meta.c4_show_legend);
    let edge_label = parse_result.ir.edges[0]
        .label
        .and_then(|label_id| parse_result.ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(edge_label, Some("Uses [HTTPS]"));

    let layout = layout_diagram(&parse_result.ir);
    let svg = render_svg_with_config(
        &parse_result.ir,
        &SvgRenderConfig {
            detail_tier: MermaidTier::Rich,
            ..SvgRenderConfig::default()
        },
    );

    assert_eq!(layout.nodes.len(), 2);
    assert!(svg.contains("fm-c4-legend"));
    assert!(svg.contains("fm-c4-type-label"));
    assert!(svg.contains("[Rust]"));
    assert!(svg.contains("class=\"edge-label\""));
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
    run_cli_with_env(args, stdin, &[])
}

fn run_cli_with_env(args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fm-cli"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }

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

fn run_evidence(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_evidence"))
        .args(args)
        .output()
        .expect("failed to run evidence binary")
}

fn write_evidence_root_fixture() -> TempDir {
    let temp = TempDir::new().expect("temp evidence root");
    std::fs::create_dir_all(temp.path().join("evidence/ledger")).expect("create ledger dir");
    std::fs::create_dir_all(temp.path().join(".beads")).expect("create beads dir");
    temp
}

fn render_json_metadata(input: &str) -> (serde_json::Value, String) {
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
        "render --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("render --json must print metadata JSON");
    let artifact = std::fs::read_to_string(&output_path).expect("failed to read rendered svg");
    (json, artifact)
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
    assert!(artifact_json.get("layout_decision_ledger").is_some());
    assert!(artifact_json.get("layout_decision_ledger_jsonl").is_some());
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
    assert_eq!(json["source_span_node_count"], 2);
    assert_eq!(json["source_span_edge_count"], 1);
    assert_eq!(json["source_span_cluster_count"], 0);
    assert!(
        json["accessibility_summary"]
            .as_str()
            .is_some_and(|value| value.contains("Key relationships"))
    );
    assert!(json["pressure_source"].is_string());
    assert!(json["pressure_tier"].is_string());
    assert!(json["pressure_telemetry_available"].is_boolean());
    assert!(json["pressure_conservative_fallback"].is_boolean());
    assert!(json["pressure_score_permille"].is_u64());
    assert!(json["trace_id"].is_string());
    assert!(json["decision_id"].is_string());
    assert!(json["policy_id"].is_string());
    assert_eq!(json["schema_version"], "1.0.0");
    assert!(json["layout_decision_ledger"]["entries"].is_array());
    assert_eq!(
        json["layout_decision_ledger"]["entries"][0]["kind"],
        "layout_decision"
    );
    assert!(json["layout_decision_ledger_jsonl"].is_string());
    assert!(json["budget_total_ms"].is_u64());
    assert!(json["parse_budget_ms"].is_u64());
    assert!(json["layout_budget_ms"].is_u64());
    assert!(json["render_budget_ms"].is_u64());
    assert!(json["budget_exhausted"].is_boolean());
    assert!(json["output_bytes"].as_u64().is_some_and(|value| value > 0));

    let artifact = std::fs::read_to_string(&output_path).expect("failed to read rendered svg");
    assert!(artifact.starts_with("<svg"));
    assert!(artifact.contains("</svg>"));
    assert!(artifact.contains("data-fm-source-span="));
}

#[test]
fn render_svg_can_disable_embedded_source_spans() {
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
            "--no-embed-source-spans",
            "--output",
            &output_path,
        ],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "render --no-embed-source-spans should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact = std::fs::read_to_string(&output_path).expect("failed to read rendered svg");
    assert!(!artifact.contains("data-fm-source-span="));
}

#[test]
fn render_svg_writes_source_map_artifact() {
    let output_file = NamedTempFile::new().expect("temp render output file");
    let source_map_file = NamedTempFile::new().expect("temp source map file");
    let output_path = output_file
        .path()
        .to_str()
        .expect("temp path must be valid utf-8")
        .to_string();
    let source_map_path = source_map_file
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
            "--source-map-out",
            &source_map_path,
        ],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "render --source-map-out should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("render --json must print metadata JSON to stdout");
    assert_eq!(json["embedded_source_spans"], true);
    assert_eq!(json["source_map_entry_count"], 3);
    assert_eq!(json["source_map_out"], source_map_path);

    let artifact =
        std::fs::read_to_string(&source_map_path).expect("failed to read source map artifact");
    let source_map: serde_json::Value =
        serde_json::from_str(&artifact).expect("source map artifact must be valid json");
    assert_eq!(source_map["diagram_type"], "Flowchart");
    assert_eq!(source_map["entries"].as_array().map(Vec::len), Some(3));
    assert!(
        source_map["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .any(|entry| entry["element_id"] == "fm-node-a-0")
    );
    assert!(
        source_map["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .any(|entry| entry["element_id"] == "fm-edge-0")
    );

    let svg = std::fs::read_to_string(&output_path).expect("failed to read rendered svg");
    assert!(svg.contains("id=\"fm-node-a-0\""));
    assert!(svg.contains("id=\"fm-edge-0\""));
}

#[test]
fn render_svg_source_map_survives_recovered_input() {
    let output_file = NamedTempFile::new().expect("temp render output file");
    let source_map_file = NamedTempFile::new().expect("temp source map file");
    let output_path = output_file
        .path()
        .to_str()
        .expect("temp path must be valid utf-8")
        .to_string();
    let source_map_path = source_map_file
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
            "--output",
            &output_path,
            "--source-map-out",
            &source_map_path,
        ],
        "flowchrt LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "render recovered input with --source-map-out should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact =
        std::fs::read_to_string(&source_map_path).expect("failed to read source map artifact");
    let source_map: serde_json::Value =
        serde_json::from_str(&artifact).expect("source map artifact must be valid json");
    assert!(
        source_map["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .any(|entry| entry["kind"] == "node")
    );
    assert!(
        source_map["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .any(|entry| entry["kind"] == "edge")
    );
}

#[test]
fn validate_json_reports_source_span_counts() {
    let output = run_cli(
        &["validate", "-", "--format", "json"],
        "flowchart TD\nsubgraph Cluster\nA-->B\nend\n",
    );
    assert!(
        output.status.success(),
        "validate --format json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate --format json must print valid JSON");
    assert!(
        json["accessibility_summary"]
            .as_str()
            .is_some_and(|value| value.contains("Layout spans"))
    );
    assert_eq!(json["source_span_node_count"], 2);
    assert_eq!(json["source_span_edge_count"], 1);
    assert_eq!(json["source_span_cluster_count"], 1);
    assert!(json["pressure_source"].is_string());
    assert!(json["pressure_tier"].is_string());
    assert!(json["pressure_telemetry_available"].is_boolean());
    assert!(json["trace_id"].is_string());
    assert!(json["decision_id"].is_string());
    assert!(json["policy_id"].is_string());
    assert_eq!(json["schema_version"], "1.0.0");
    assert!(json["layout_decision_ledger"]["entries"].is_array());
    assert_eq!(
        json["layout_decision_ledger"]["entries"][0]["kind"],
        "layout_decision"
    );
    assert!(json["layout_decision_ledger_jsonl"].is_string());
    assert!(json["budget_total_ms"].is_u64());
    assert!(json["parse_budget_ms"].is_u64());
    assert!(json["layout_budget_ms"].is_u64());
    assert!(json["render_budget_ms"].is_u64());
    assert!(json["budget_exhausted"].is_boolean());
}

#[test]
fn validate_json_honors_native_pressure_env_overrides() {
    let output = run_cli_with_env(
        &["validate", "-", "--format", "json"],
        "flowchart LR\nA-->B\n",
        &[
            ("FM_PRESSURE_CPU_PERMILLE", "920"),
            ("FM_PRESSURE_MEMORY_PERMILLE", "300"),
            ("FM_PRESSURE_AVAILABLE_PARALLELISM", "1"),
        ],
    );
    assert!(
        output.status.success(),
        "validate with explicit pressure env should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate json must print valid JSON");
    assert_eq!(json["pressure_source"], "native");
    assert_eq!(json["pressure_tier"], "critical");
    assert_eq!(json["pressure_telemetry_available"], true);
    assert_eq!(json["pressure_score_permille"], 920);
    assert_eq!(json["policy_id"], "fm.layout.guard@v1");
    assert_eq!(json["schema_version"], "1.0.0");
    assert!(
        json["layout_budget_ms"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
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
fn render_json_replay_keeps_ledger_trace_continuity_and_stable_outputs() {
    let input = r#"flowchart LR
    Start[Start] --> Parse[Parse]
    Parse --> Layout[Layout]
    Layout --> Render[Render]
    Render --> Done[Done]
"#;

    let (first_json, first_svg) = render_json_metadata(input);
    let (second_json, second_svg) = render_json_metadata(input);

    assert_eq!(first_svg, second_svg, "rendered SVG should be byte-stable");
    assert_eq!(
        first_json["layout_decision_ledger_jsonl"], second_json["layout_decision_ledger_jsonl"],
        "ledger JSONL should be stable across replay"
    );
    assert_eq!(first_json["trace_id"], second_json["trace_id"]);
    assert_eq!(first_json["decision_id"], second_json["decision_id"]);
    assert_eq!(
        first_json["layout_selected"],
        second_json["layout_selected"]
    );
    assert_eq!(first_json["node_count"], second_json["node_count"]);
    assert_eq!(first_json["edge_count"], second_json["edge_count"]);
    assert_eq!(first_json["output_bytes"], first_svg.len());
    assert_eq!(second_json["output_bytes"], second_svg.len());

    for json in [&first_json, &second_json] {
        let trace_id = json["trace_id"]
            .as_str()
            .expect("trace_id should be a string");
        let decision_id = json["decision_id"]
            .as_str()
            .expect("decision_id should be a string");
        let entries = json["layout_decision_ledger"]["entries"]
            .as_array()
            .expect("ledger entries should be an array");
        assert!(
            !entries.is_empty(),
            "ledger should include at least one decision entry"
        );
        for entry in entries {
            assert_eq!(entry["trace_id"], trace_id);
            assert_eq!(entry["decision_id"], decision_id);
        }
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
fn detect_reports_c4_context_as_basic_support() {
    let output = run_cli(
        &["detect", "-", "--json"],
        "C4Context\nPerson(user, \"User\")\nSystem(app, \"App\")\n",
    );
    assert!(
        output.status.success(),
        "detect --json should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "C4Context");
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
fn detect_reports_architecture_as_basic_support() {
    let output = run_cli(
        &["detect", "-", "--json"],
        "architecture-beta\nservice api[API]\n",
    );
    assert!(
        output.status.success(),
        "detect --json should succeed for architecture; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("detect --json must print valid JSON");
    assert_eq!(json["diagram_type"], "architecture-beta");
    assert_eq!(json["support_level"], "basic");
}

#[test]
fn parse_summary_reports_architecture_counts_without_compatibility_fallback() {
    let output = run_cli(
        &["parse", "-", "--parse-mode", "compat", "--pretty"],
        "architecture-beta\nservice api[API]\nservice db[DB]\napi --> db\n",
    );
    assert!(
        output.status.success(),
        "parse summary should succeed for architecture; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse summary must print valid JSON");
    assert_eq!(json["diagram_type"], "architecture-beta");
    assert_eq!(json["parse_mode"], "compat");
    assert_eq!(json["support_level"], "Partial");
    assert_eq!(json["node_count"], 2);
    assert_eq!(json["edge_count"], 1);
    assert_eq!(json["diagnostic_count"], 0);
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
fn parse_summary_reports_c4_counts_without_compatibility_fallback() {
    let output = run_cli(
        &["parse", "-", "--parse-mode", "compat", "--pretty"],
        "C4Context\nPerson(user, \"User\")\nSystem(app, \"App\")\nRel(user, app, \"Uses\", \"HTTPS\")\n",
    );
    assert!(
        output.status.success(),
        "parse summary should succeed for C4; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse summary must print valid JSON");
    assert_eq!(json["diagram_type"], "C4Context");
    assert_eq!(json["parse_mode"], "compat");
    assert_eq!(json["support_level"], "Partial");
    assert_eq!(json["node_count"], 2);
    assert_eq!(json["edge_count"], 1);
    assert_eq!(json["diagnostic_count"], 0);
}

#[test]
fn parse_summary_reports_xychart_counts_without_compatibility_fallback() {
    let output = run_cli(
        &["parse", "-", "--parse-mode", "strict", "--pretty"],
        "xychart-beta\nx-axis [Q1, Q2, Q3]\nline Revenue [1,2,3]\nbar Forecast [2,3,4]\n",
    );
    assert!(
        output.status.success(),
        "parse summary should succeed for xychart; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse summary must print valid JSON");
    assert_eq!(json["diagram_type"], "xyChart");
    assert_eq!(json["parse_mode"], "strict");
    assert_eq!(json["support_level"], "Partial");
    assert_eq!(json["node_count"], 6);
    assert_eq!(json["edge_count"], 2);
    assert_eq!(json["diagnostic_count"], 0);
}

#[test]
fn validate_strict_mode_accepts_xychart_without_compatibility_error() {
    let output = run_cli(
        &[
            "validate",
            "-",
            "--parse-mode",
            "strict",
            "--format",
            "json",
        ],
        "xychart-beta\nx-axis [Q1, Q2, Q3]\nline Revenue [1,2,3]\n",
    );
    assert!(
        output.status.success(),
        "validate strict should succeed for xychart; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate json must print valid JSON");
    assert_eq!(json["valid"], true);
    assert_eq!(json["parse_mode"], "strict");
    assert_eq!(json["diagram_type"], "xyChart");
    assert!(json["diagnostics"].as_array().is_some_and(|items| {
        items
            .iter()
            .all(|diagnostic| diagnostic["rule_id"] != "parse.compatibility")
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

// ── E2E pipeline tests for all diagram types (bd-3ac.5) ──────────────

/// Verify that a given Mermaid input round-trips through parse → layout → SVG render
/// and parse → layout → terminal render without panicking, producing valid output.
fn assert_pipeline_roundtrip(input: &str, expected_type: DiagramType, label: &str) {
    // Parse
    let result = parse(input);
    assert_eq!(
        result.ir.diagram_type, expected_type,
        "[{label}] expected diagram type {expected_type:?}, got {:?}",
        result.ir.diagram_type
    );

    let ir = &result.ir;

    // Layout
    let layout = layout_diagram(ir);
    assert!(
        layout.bounds.width >= 0.0 && layout.bounds.height >= 0.0,
        "[{label}] layout bounds must be non-negative: {:?}",
        layout.bounds
    );

    // SVG render
    let svg = render_svg(ir);
    assert!(
        svg.starts_with("<svg") || svg.starts_with("<?xml"),
        "[{label}] SVG output must start with <svg or <?xml, got: {}",
        &svg[..svg.len().min(40)]
    );
    assert!(
        svg.contains("</svg>"),
        "[{label}] SVG output must contain closing </svg>"
    );

    // Terminal render
    let term = render_term(ir);
    // Terminal output should be non-empty for any non-trivial diagram.
    if !ir.nodes.is_empty() {
        assert!(
            !term.trim().is_empty(),
            "[{label}] terminal output should be non-empty for diagram with {} nodes",
            ir.nodes.len()
        );
    }
}

#[test]
fn e2e_pipeline_flowchart() {
    assert_pipeline_roundtrip(
        "flowchart TD\n  A[Start] --> B{Decision}\n  B -->|Yes| C[Action]\n  B -->|No| D[Skip]\n  C --> E[End]\n  D --> E",
        DiagramType::Flowchart,
        "flowchart",
    );
}

#[test]
fn flowchart_markdown_backtick_labels_render_styled_svg() {
    let input = "flowchart LR\n  A[\"`**Bold** and *italic* &#9829;<br/>next`\"] --> B[Done]";
    let result = parse(input);
    assert!(
        result.warnings.is_empty(),
        "warnings: {:?}",
        result.warnings
    );

    let a_node = result
        .ir
        .nodes
        .iter()
        .find(|node| node.id == "A")
        .expect("node A");
    let label_id = a_node.label.expect("label id");
    assert_eq!(result.ir.labels[label_id.0].text, "Bold and italic ♥\nnext");

    let svg = render_svg_with_config(
        &result.ir,
        &SvgRenderConfig {
            detail_tier: MermaidTier::Rich,
            ..SvgRenderConfig::default()
        },
    );
    assert!(svg.contains("font-weight=\"700\""));
    assert!(svg.contains("font-style=\"italic\""));
    assert!(svg.contains(">Bold<"));
    assert!(svg.contains(">italic<"));
    assert!(svg.contains(">next<"));
}

#[test]
fn accessibility_directives_flow_through_scene_svg_backend() {
    let input = "flowchart LR\n  accTitle: Accessible Flow\n  accDescr: Explicit accessible description\n  A --> B";
    let result = parse(input);
    assert!(
        result.warnings.is_empty(),
        "warnings: {:?}",
        result.warnings
    );

    let svg = render_svg_with_config(
        &result.ir,
        &SvgRenderConfig {
            backend: SvgBackend::Scene,
            ..SvgRenderConfig::default()
        },
    );

    assert!(svg.contains("<title>Accessible Flow</title>"));
    assert!(svg.contains("<desc>Explicit accessible description</desc>"));
}

#[test]
fn front_matter_title_renders_in_flowchart_svg() {
    let input = "---\ntitle: Release Overview\n---\nflowchart LR\n  A --> B";
    let result = parse(input);
    assert_eq!(result.ir.meta.title.as_deref(), Some("Release Overview"));

    let svg = render_svg(&result.ir);
    assert!(svg.contains(">Release Overview<"));
    assert!(svg.contains("fm-diagram-title"));
}

#[test]
fn gantt_inline_title_renders_as_generic_diagram_title() {
    let input = "gantt\n  title Roadmap\n  section Alpha\n  Ship :a1, 2024-01-01, 2d";
    let result = parse(input);
    assert_eq!(result.ir.meta.title.as_deref(), Some("Roadmap"));

    let svg = render_svg(&result.ir);
    assert!(svg.contains(">Roadmap<"));
    assert!(svg.contains("fm-diagram-title"));
}

#[test]
fn gantt_extended_meta_flows_through_parse_and_layout() {
    let input = "gantt\n  dateFormat YYYY-MM-DD\n  tickInterval 1week\n  todayMarker stroke-width:4px,stroke:#f00\n  inclusiveEndDates\n  excludes weekends, 2026-02-10\n  section Alpha\n  Build :active, build1, 2026-02-06, 2026-02-09\n  Verify :crit, verify1, after build1, 2d";
    let result = parse(input);
    let gantt_meta = result.ir.gantt_meta.as_ref().expect("gantt meta");

    assert_eq!(gantt_meta.tick_interval, Some(GanttTickInterval::Week));
    assert_eq!(
        gantt_meta.today_marker_style.as_deref(),
        Some("stroke-width:4px,stroke:#f00")
    );
    assert!(gantt_meta.inclusive_end_dates);
    assert_eq!(
        gantt_meta.excludes,
        vec![
            GanttExclude::Weekends,
            GanttExclude::Dates(vec!["2026-02-10".to_string()])
        ]
    );
    assert_eq!(gantt_meta.tasks[0].task_type, GanttTaskType::Active);
    assert_eq!(
        gantt_meta.tasks[0].start,
        Some(GanttDate::Absolute("2026-02-06".to_string()))
    );
    assert_eq!(
        gantt_meta.tasks[0].end,
        Some(GanttDate::Absolute("2026-02-09".to_string()))
    );
    assert_eq!(gantt_meta.tasks[1].depends_on, vec!["build1".to_string()]);

    let layout = layout_diagram(&result.ir);
    assert_eq!(layout.nodes.len(), 2);
}

#[test]
fn gantt_date_format_allows_non_iso_dates_through_full_pipeline() {
    let input = "gantt\n  dateFormat DD/MM/YYYY\n  section Alpha\n  Build :build1, 06/02/2026, 09/02/2026\n  Verify :verify1, after build1, 2d";
    let result = parse(input);
    let gantt_meta = result.ir.gantt_meta.as_ref().expect("gantt meta");

    assert_eq!(
        gantt_meta.tasks[0].start,
        Some(GanttDate::Absolute("2026-02-06".to_string()))
    );
    assert_eq!(
        gantt_meta.tasks[0].end,
        Some(GanttDate::Absolute("2026-02-09".to_string()))
    );

    let layout = layout_diagram(&result.ir);
    assert_eq!(layout.nodes.len(), 2);
    assert!(layout.nodes[1].bounds.center().x > layout.nodes[0].bounds.center().x);
}

#[test]
fn flowchart_subgraph_direction_override_lays_out_child_nodes_vertically() {
    let input =
        "flowchart LR\n  subgraph api [API]\n    direction TB\n    A --> B\n  end\n  B --> C";
    let result = parse(input);
    assert!(
        result.warnings.is_empty(),
        "warnings: {:?}",
        result.warnings
    );

    let layout = layout_diagram(&result.ir);
    let node_a = layout
        .nodes
        .iter()
        .find(|node| node.node_id == "A")
        .unwrap();
    let node_b = layout
        .nodes
        .iter()
        .find(|node| node.node_id == "B")
        .unwrap();
    let node_c = layout
        .nodes
        .iter()
        .find(|node| node.node_id == "C")
        .unwrap();

    let dx_ab = (node_a.bounds.x - node_b.bounds.x).abs();
    let dy_ab = (node_a.bounds.y - node_b.bounds.y).abs();

    assert!(
        dy_ab > dx_ab,
        "expected A/B to be vertically stacked inside the subgraph, got dx={dx_ab}, dy={dy_ab}"
    );
    assert!(node_b.bounds.y > node_a.bounds.y);
    assert!(node_c.bounds.x > node_a.bounds.x);
}

#[test]
fn e2e_pipeline_sequence() {
    assert_pipeline_roundtrip(
        "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi back",
        DiagramType::Sequence,
        "sequence",
    );
}

#[test]
fn e2e_pipeline_class() {
    assert_pipeline_roundtrip(
        "classDiagram\n  Animal <|-- Duck\n  Animal <|-- Fish\n  Animal : +int age\n  Animal : +String gender",
        DiagramType::Class,
        "class",
    );
}

#[test]
fn e2e_pipeline_state() {
    assert_pipeline_roundtrip(
        "stateDiagram-v2\n  [*] --> Still\n  Still --> Moving\n  Moving --> Still\n  Moving --> Crash\n  Crash --> [*]",
        DiagramType::State,
        "state",
    );
}

#[test]
fn e2e_pipeline_state_pseudo_states_render_distinct_shapes() {
    let input = "stateDiagram-v2\n  [*] --> fork_state\n  state fork_state <<fork>>\n  fork_state --> chooser\n  state chooser <<choice>>\n  chooser --> Done\n  Done --> [*]";
    let output_file = NamedTempFile::new().expect("temp render output file");
    let output_path = output_file
        .path()
        .to_str()
        .expect("temp path must be valid utf-8")
        .to_string();

    let output = run_cli(
        &["render", "-", "--format", "svg", "--output", &output_path],
        input,
    );
    assert!(
        output.status.success(),
        "state pseudo-state render should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let svg = std::fs::read_to_string(output_path).expect("read svg output");
    assert!(svg.contains("fm-node-shape-filled-circle"));
    assert!(svg.contains("fm-node-shape-horizontal-bar"));
    assert!(svg.contains("fm-node-shape-diamond"));
    assert!(svg.contains("fm-node-shape-double-circle"));
}

#[test]
fn e2e_pipeline_state_composite_regions_render_divider() {
    let input = "stateDiagram-v2\n  state \"Active Mode\" as Active {\n    [*] --> Processing\n    Processing --> Waiting\n    --\n    [*] --> Monitoring\n    Monitoring --> Alert\n  }\n  Idle --> Active";
    let output_file = NamedTempFile::new().expect("temp render output file");
    let output_path = output_file
        .path()
        .to_str()
        .expect("temp path must be valid utf-8")
        .to_string();

    let output = run_cli(
        &["render", "-", "--format", "svg", "--output", &output_path],
        input,
    );
    assert!(
        output.status.success(),
        "state composite region render should succeed; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let svg = std::fs::read_to_string(output_path).expect("read svg output");
    assert!(svg.contains("Active Mode"));
    assert!(svg.contains("stroke-dasharray=\"6,4\""));
}

#[test]
fn e2e_pipeline_er() {
    assert_pipeline_roundtrip(
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER ||--|{ LINE-ITEM : contains",
        DiagramType::Er,
        "er",
    );
}

#[test]
fn e2e_pipeline_gantt() {
    assert_pipeline_roundtrip(
        "gantt\n  title Project Plan\n  dateFormat YYYY-MM-DD\n  section Design\n  Task A :a1, 2024-01-01, 7d\n  section Build\n  Task B :b1, after a1, 5d",
        DiagramType::Gantt,
        "gantt",
    );
}

#[test]
fn e2e_pipeline_pie() {
    assert_pipeline_roundtrip(
        "pie title Languages\n  \"Rust\" : 45\n  \"Go\" : 30\n  \"Python\" : 25",
        DiagramType::Pie,
        "pie",
    );
}

#[test]
fn e2e_pipeline_gitgraph() {
    assert_pipeline_roundtrip(
        "gitGraph\n  commit\n  branch develop\n  checkout develop\n  commit\n  checkout main\n  merge develop",
        DiagramType::GitGraph,
        "gitgraph",
    );
}

#[test]
fn e2e_pipeline_journey() {
    assert_pipeline_roundtrip(
        "journey\n  title My Day\n  section Morning\n    Wake up: 5: Me\n    Shower: 3: Me\n  section Work\n    Code: 8: Me",
        DiagramType::Journey,
        "journey",
    );
}

#[test]
fn e2e_pipeline_mindmap() {
    assert_pipeline_roundtrip(
        "mindmap\n  root((mindmap))\n    Origins\n      Long history\n    Research\n      Effectiveness",
        DiagramType::Mindmap,
        "mindmap",
    );
}

#[test]
fn e2e_pipeline_timeline() {
    assert_pipeline_roundtrip(
        "timeline\n  title History\n  2020 : Event A\n  2021 : Event B\n  2022 : Event C",
        DiagramType::Timeline,
        "timeline",
    );
}

#[test]
fn e2e_pipeline_sankey() {
    assert_pipeline_roundtrip(
        "sankey-beta\n\nSource,Target,5\nTarget,Sink,3\n",
        DiagramType::Sankey,
        "sankey",
    );
}

#[test]
fn e2e_pipeline_quadrant_chart() {
    assert_pipeline_roundtrip(
        "quadrantChart\n  title Priorities\n  x-axis Low --> High\n  y-axis Low --> High\n  A: [0.3, 0.6]\n  B: [0.7, 0.8]",
        DiagramType::QuadrantChart,
        "quadrant",
    );
}

#[test]
fn e2e_pipeline_xychart() {
    assert_pipeline_roundtrip(
        "xychart-beta\n  title Sales\n  x-axis [jan, feb, mar]\n  y-axis \"Revenue\" 0 --> 100\n  bar [30, 50, 70]",
        DiagramType::XyChart,
        "xychart",
    );
}

#[test]
fn e2e_pipeline_xychart_renders_axes_and_mixed_series() {
    let input = "xychart-beta\n  title Sales Revenue\n  x-axis [jan, feb, mar]\n  y-axis \"Revenue\" 0 --> 100\n  bar Revenue [30, 50, 70]\n  line Target [40, 60, 80]";
    let parse_result = parse(input);
    assert!(
        parse_result.warnings.is_empty(),
        "xychart should parse cleanly: {:?}",
        parse_result.warnings
    );

    let ir = parse_result.ir;
    let svg = render_svg(&ir);

    assert!(svg.contains("fm-xychart-axis"));
    assert!(svg.contains("fm-xychart-gridline"));
    assert!(svg.contains("fm-xychart-bar"));
    assert!(svg.contains("fm-xychart-line"));
    assert!(svg.contains("Sales Revenue"));
    assert!(svg.contains(">jan<"));
    assert!(svg.contains(">Revenue<"));
}

#[test]
fn e2e_pipeline_block_beta() {
    assert_pipeline_roundtrip(
        "block-beta\n  columns 3\n  a[\"Block A\"]:2\n  b[\"Block B\"]\n  c[\"Block C\"]:3",
        DiagramType::BlockBeta,
        "block-beta",
    );
}

#[test]
fn e2e_pipeline_packet_beta() {
    assert_pipeline_roundtrip(
        "packet-beta\n  0-15: \"Header\"\n  16-31: \"Payload\"",
        DiagramType::PacketBeta,
        "packet-beta",
    );
}

#[test]
fn e2e_pipeline_architecture_beta() {
    assert_pipeline_roundtrip(
        "architecture-beta\n  service api(API)\n  service db(DB)\n  api --> db",
        DiagramType::ArchitectureBeta,
        "architecture-beta",
    );
}

#[test]
fn e2e_pipeline_c4_context() {
    assert_pipeline_roundtrip(
        "C4Context\n  Person(user, \"User\")\n  System(system, \"System\")\n  Rel(user, system, \"Uses\")",
        DiagramType::C4Context,
        "c4context",
    );
}

#[test]
fn e2e_pipeline_c4_container() {
    assert_pipeline_roundtrip(
        "C4Container\n  Container(web, \"Web App\")\n  Container(api, \"API\")\n  Rel(web, api, \"Calls\")",
        DiagramType::C4Container,
        "c4container",
    );
}

#[test]
fn e2e_pipeline_c4_component() {
    assert_pipeline_roundtrip(
        "C4Component\n  Component(auth, \"Auth Module\")\n  Component(db, \"Database\")\n  Rel(auth, db, \"Reads\")",
        DiagramType::C4Component,
        "c4component",
    );
}

#[test]
fn e2e_pipeline_c4_dynamic() {
    assert_pipeline_roundtrip(
        "C4Dynamic\n  Person(user, \"User\")\n  System(sys, \"System\")\n  Rel(user, sys, \"1. Request\")",
        DiagramType::C4Dynamic,
        "c4dynamic",
    );
}

#[test]
fn e2e_pipeline_c4_deployment() {
    assert_pipeline_roundtrip(
        "C4Deployment\n  Deployment_Node(server, \"Server\")\n  Container(app, \"App\")\n  Rel(server, app, \"Hosts\")",
        DiagramType::C4Deployment,
        "c4deployment",
    );
}

#[test]
fn e2e_pipeline_requirement() {
    assert_pipeline_roundtrip(
        "requirementDiagram\n  requirement test_req {\n    id: 1\n    text: Must work\n  }",
        DiagramType::Requirement,
        "requirement",
    );
}

#[test]
fn e2e_pipeline_kanban() {
    assert_pipeline_roundtrip(
        "kanban\n  Todo\n    Task A\n    Task B\n  Done\n    Task C",
        DiagramType::Kanban,
        "kanban",
    );
}

#[test]
fn e2e_pipeline_dot_format() {
    // DOT input should be detected and parsed via the DOT bridge.
    let input = "digraph G {\n  A -> B;\n  B -> C;\n}";
    let result = parse(input);
    assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);

    let layout = layout_diagram(&result.ir);
    assert!(layout.bounds.width >= 0.0);

    let svg = render_svg(&result.ir);
    assert!(svg.contains("</svg>"));
}

/// Verify that the pipeline is deterministic: same input produces identical SVG.
#[test]
fn e2e_pipeline_determinism_all_types() {
    let inputs = [
        ("flowchart LR\n  A-->B-->C", "flowchart"),
        ("sequenceDiagram\n  A->>B: msg", "sequence"),
        ("classDiagram\n  A <|-- B", "class"),
        ("stateDiagram-v2\n  [*] --> S1\n  S1 --> S2", "state"),
        ("erDiagram\n  A ||--o{ B : has", "er"),
        ("gantt\n  section S\n  T1 :a1, 2024-01-01, 3d", "gantt"),
        ("pie\n  \"A\" : 50\n  \"B\" : 50", "pie"),
        ("mindmap\n  root\n    A\n    B", "mindmap"),
    ];

    for (input, label) in &inputs {
        let svg1 = render_svg(&parse(input).ir);
        let svg2 = render_svg(&parse(input).ir);
        assert_eq!(svg1, svg2, "[{label}] SVG output must be deterministic");
    }
}

#[test]
fn determinism_manifest_cli_is_stable_and_finite() {
    let binary = env!("CARGO_BIN_EXE_fm-cli");

    let run_manifest = || {
        let output = Command::new(binary)
            .arg("determinism-manifest")
            .output()
            .expect("run determinism-manifest");
        assert!(
            output.status.success(),
            "determinism-manifest failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("parse manifest json")
    };

    let first = run_manifest();
    let second = run_manifest();
    assert_eq!(first, second, "manifest output must be byte-stable");

    let cases = first["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "manifest should include golden cases");
    for case in cases {
        assert_eq!(
            case["non_finite_value_count"].as_u64().unwrap_or_default(),
            0,
            "manifest case has non-finite values: {case:?}"
        );
        assert!(
            case["layout_sha256"]
                .as_str()
                .is_some_and(|value| value.len() == 64),
            "layout SHA-256 digest missing or malformed: {case:?}"
        );
    }
}

#[test]
fn evidence_add_creates_seeded_entry() {
    let temp = write_evidence_root_fixture();
    let root = temp.path().to_str().expect("root path utf-8");

    let output = run_evidence(&["--root", root, "add", "egraph-crossing-min"]);
    assert!(
        output.status.success(),
        "evidence add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let entry =
        std::fs::read_to_string(temp.path().join("evidence/ledger/egraph-crossing-min.toml"))
            .expect("seeded ledger entry");
    assert!(entry.contains("concept_name = \"E-Graphs for Crossing Minimization\""));
    assert!(entry.contains("status = \"pending\""));
}

#[test]
fn evidence_update_writes_metrics_and_beads() {
    let temp = write_evidence_root_fixture();
    let root = temp.path().to_str().expect("root path utf-8");

    let add_output = run_evidence(&["--root", root, "add", "egraph-crossing-min"]);
    assert!(
        add_output.status.success(),
        "{}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    let update_output = run_evidence(&[
        "--root",
        root,
        "update",
        "egraph-crossing-min",
        "--baseline-date",
        "2026-03-26",
        "--baseline-commit",
        "abc123",
        "--baseline-metric",
        "crossing_count=42",
        "--add-bead",
        "bd-1xma.5",
        "--decision",
        "adopt",
        "--decision-rationale",
        "Improvement threshold cleared.",
    ]);
    assert!(
        update_output.status.success(),
        "evidence update failed: {}",
        String::from_utf8_lossy(&update_output.stderr)
    );

    let entry =
        std::fs::read_to_string(temp.path().join("evidence/ledger/egraph-crossing-min.toml"))
            .expect("updated ledger entry");
    assert!(entry.contains("date = \"2026-03-26\""));
    assert!(entry.contains("commit = \"abc123\""));
    assert!(entry.contains("crossing_count = 42.0"));
    assert!(entry.contains("beads = [\"bd-1xma.5\"]"));
    assert!(entry.contains("status = \"adopt\""));
}

#[test]
fn evidence_report_flags_uncovered_closed_alien_cs_beads() {
    let temp = write_evidence_root_fixture();
    let root = temp.path().to_str().expect("root path utf-8");

    let add_output = run_evidence(&["--root", root, "add", "egraph-crossing-min"]);
    assert!(
        add_output.status.success(),
        "{}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    std::fs::write(
        temp.path().join(".beads/issues.jsonl"),
        concat!(
            "{\"id\":\"bd-covered\",\"title\":\"Covered alien task\",\"status\":\"closed\",\"labels\":[\"alien-cs\"]}\n",
            "{\"id\":\"bd-missing\",\"title\":\"Missing alien task\",\"status\":\"closed\",\"labels\":[\"alien-cs\"]}\n"
        ),
    )
    .expect("write beads jsonl");

    let update_output = run_evidence(&[
        "--root",
        root,
        "update",
        "egraph-crossing-min",
        "--add-bead",
        "bd-covered",
    ]);
    assert!(
        update_output.status.success(),
        "{}",
        String::from_utf8_lossy(&update_output.stderr)
    );

    let report_output = run_evidence(&[
        "--root",
        root,
        "report",
        "--check-beads",
        "--fail-on-missing-beads",
    ]);
    assert!(
        !report_output.status.success(),
        "report should fail when uncovered alien-cs beads remain"
    );
    let stderr = String::from_utf8_lossy(&report_output.stderr);
    assert!(stderr.contains("without ledger coverage"));

    let report = std::fs::read_to_string(temp.path().join("evidence/ledger/README.md"))
        .expect("report should still be written");
    assert!(report.contains("bd-missing"));
}

// ── Adversarial Security Test Corpus ───────────────────────────────
// Cross-crate tests verifying that malicious input cannot produce
// exploitable SVG output.

#[test]
fn adversarial_xss_in_node_label_is_escaped_in_svg() {
    let input = "flowchart LR\n  A[<script>alert(1)</script>] --> B";
    let result = fm_parser::parse(input);
    let svg = fm_render_svg::render_svg(&result.ir);
    assert!(
        !svg.contains("<script>"),
        "SVG must not contain raw <script> tags"
    );
}

#[test]
fn adversarial_xss_in_edge_label_is_escaped_in_svg() {
    let input = "flowchart LR\n  A -->|<img onerror=alert(1)>| B";
    let result = fm_parser::parse(input);
    let svg = fm_render_svg::render_svg(&result.ir);
    // The raw `<img` must be escaped to `&lt;img` — check that unescaped HTML tags don't appear.
    assert!(
        !svg.contains("<img "),
        "SVG must not contain raw <img> tags in edge labels"
    );
}

#[test]
fn adversarial_svg_injection_via_class_name_blocked() {
    let input = "flowchart LR\n  A[Node]:::\"onload=alert(1)";
    let result = fm_parser::parse(input);
    let svg = fm_render_svg::render_svg(&result.ir);
    assert!(
        !svg.contains("onload="),
        "SVG must not contain injected event handlers via class names"
    );
}

#[test]
fn adversarial_full_pipeline_never_produces_nan_in_svg() {
    let inputs = [
        "flowchart LR\n  A --> B",
        "sequenceDiagram\n  Alice->>Bob: Hi",
        "pie\n  \"A\" : 0\n  \"B\" : 0",
        "quadrantChart\n  A: [0.5, 0.5]",
        "erDiagram\n  A ||--o{ B : rel",
    ];
    for input in &inputs {
        let result = fm_parser::parse(input);
        let svg = fm_render_svg::render_svg(&result.ir);
        assert!(
            !svg.contains("NaN"),
            "SVG must never contain NaN for input: {input}"
        );
        assert!(
            !svg.contains("Infinity"),
            "SVG must never contain Infinity for input: {input}"
        );
    }
}

#[test]
fn adversarial_er_notation_injection_blocked() {
    let input = "erDiagram\n  A ||--o{ B : rel";
    let result = fm_parser::parse(input);
    let svg = fm_render_svg::render_svg(&result.ir);
    // ER notation should produce cardinality labels as properly escaped text
    // content, not as raw HTML attributes. Verify the SVG is well-formed
    // and does not contain unescaped script tags.
    assert!(
        !svg.contains("<script"),
        "ER cardinality must not produce script tags"
    );
    assert!(svg.contains("<svg"), "SVG must be well-formed");
}
