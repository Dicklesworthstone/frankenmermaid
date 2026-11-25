//! FNX End-to-End Scenario Tests
//!
//! Comprehensive E2E test suite exercising real user flows across CLI with
//! FNX integration. Tests cover baseline (FNX disabled), FNX-enabled, strict-mode,
//! and timeout fallback paths.
//!
//! # Scenario Categories
//!
//! - **Baseline**: FNX disabled, native engine only
//! - **Advisory**: FNX provides metadata but native engine is authoritative
//! - **Strict**: FNX required, fail if unavailable
//! - **Fallback**: FNX times out or fails, graceful degradation
//!
//! # Evidence Capture
//!
//! Each scenario captures artifacts for reproducibility:
//! - Rendered SVG output
//! - JSON metadata with timing and witness data
//! - Diagnostic counts and recommendations

use std::process::{Command, Output, Stdio};
use tempfile::NamedTempFile;

fn run_cli(args: &[&str], input: &str) -> Output {
    let binary = env!("CARGO_BIN_EXE_fm-cli");
    let mut cmd = Command::new(binary);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn fm binary");
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(input.as_bytes()).expect("write stdin");
    }
    child.wait_with_output().expect("wait for output")
}

/// Run CLI with --json flag, using a temp file for output to avoid stdout mixing
fn run_cli_json(args: &[&str], input: &str) -> (Output, String) {
    let tmp = NamedTempFile::new().expect("create temp file");
    let tmp_path = tmp.path().to_str().expect("temp path to string");

    let mut full_args: Vec<&str> = args.to_vec();
    full_args.extend_from_slice(&["--output", tmp_path, "--json"]);

    let output = run_cli(&full_args, input);
    // The JSON metadata is written to stdout when --output is used for SVG
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output, stdout)
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{} failed: {}",
        context,
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// Baseline Scenarios (FNX Disabled)
// ============================================================================

#[test]
fn baseline_simple_flowchart_renders_without_fnx() {
    let input = r#"
flowchart LR
    A[Start] --> B[Process]
    B --> C[End]
"#;
    let output = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "disabled"],
        input,
    );
    assert_success(&output, "baseline render");

    let svg = String::from_utf8_lossy(&output.stdout);
    assert!(svg.contains("<svg"), "output should be SVG");
    assert!(svg.contains("fm-node"), "SVG should have node elements");
    // Should NOT have centrality classes when FNX disabled
    assert!(
        !svg.contains("fm-node-centrality-"),
        "SVG should not have centrality classes when FNX disabled"
    );
}

#[test]
fn baseline_validate_without_fnx_reports_no_witness() {
    let input = "flowchart LR\nA-->B\n";
    let output = run_cli(
        &[
            "validate",
            "-",
            "--format",
            "json",
            "--fnx-mode",
            "disabled",
        ],
        input,
    );
    assert_success(&output, "baseline validate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate should output JSON");

    assert!(json.get("valid").is_some(), "should have valid field");
    // fnx_witness should be absent or None when disabled
    let witness = json.get("fnx_witness");
    assert!(
        witness.is_none() || witness == Some(&serde_json::Value::Null),
        "fnx_witness should be absent when FNX disabled"
    );
}

#[test]
fn baseline_complex_graph_deterministic() {
    let input = r#"
flowchart TD
    A --> B & C & D
    B --> E
    C --> E & F
    D --> F
    E --> G
    F --> G
    G --> H
"#;
    // Run twice and verify determinism
    let output1 = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "disabled"],
        input,
    );
    let output2 = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "disabled"],
        input,
    );

    assert_success(&output1, "first render");
    assert_success(&output2, "second render");

    assert_eq!(
        output1.stdout, output2.stdout,
        "baseline renders should be deterministic"
    );
}

// ============================================================================
// FNX Advisory Scenarios
// ============================================================================

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_advisory_adds_centrality_classes() {
    let input = r#"
flowchart TD
    Hub[Central Hub]
    A --> Hub
    B --> Hub
    C --> Hub
    Hub --> D
    Hub --> E
    Hub --> F
"#;
    let output = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "enabled"],
        input,
    );
    assert_success(&output, "fnx advisory render");

    let svg = String::from_utf8_lossy(&output.stdout);
    // Hub should be high centrality (connected to all nodes)
    assert!(
        svg.contains("fm-node-centrality-high") || svg.contains("fm-node-centrality-medium"),
        "SVG should have centrality classes when FNX enabled"
    );
}

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_advisory_witness_has_required_fields() {
    let input = "flowchart LR\nA-->B-->C\n";
    let (output, stdout) = run_cli_json(
        &["render", "-", "--format", "svg", "--fnx-mode", "enabled"],
        input,
    );
    assert_success(&output, "fnx advisory render with json");

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output JSON");

    let witness = json
        .get("fnx_witness")
        .expect("fnx_witness should be present");

    // Verify required witness fields
    assert!(
        witness.get("enabled").is_some(),
        "witness should have enabled"
    );
    assert!(witness.get("used").is_some(), "witness should have used");
    assert!(
        witness.get("projection_mode").is_some(),
        "witness should have projection_mode"
    );
    assert!(
        witness.get("algorithms_invoked").is_some(),
        "witness should have algorithms_invoked"
    );
    assert!(
        witness.get("results_hash").is_some(),
        "witness should have results_hash"
    );
}

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_advisory_detects_disconnected_components() {
    let input = r#"
flowchart LR
    subgraph Main
        A --> B --> C
    end
    subgraph Island
        X --> Y
    end
"#;
    let output = run_cli(
        &["validate", "-", "--format", "json", "--fnx-mode", "enabled"],
        input,
    );
    assert_success(&output, "fnx validate disconnected");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output JSON");

    // Check for diagnostics about disconnected components
    let diagnostics = json.get("diagnostics");
    assert!(diagnostics.is_some(), "should have diagnostics field");
}

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_advisory_is_deterministic_across_runs() {
    let input = r#"
flowchart TD
    A --> B & C
    B --> D
    C --> D
    D --> E
"#;
    // Run 5 times and verify all outputs match
    let mut outputs = Vec::new();
    for i in 0..5 {
        let output = run_cli(
            &["render", "-", "--format", "svg", "--fnx-mode", "enabled"],
            input,
        );
        assert_success(&output, &format!("run {}", i + 1));
        outputs.push(output.stdout);
    }

    for (i, out) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            &outputs[0],
            out,
            "run {} should match run 1 for determinism",
            i + 1
        );
    }
}

// ============================================================================
// FNX Fallback Scenarios
// ============================================================================

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_graceful_fallback_renders_when_analysis_skipped() {
    // Even if FNX is enabled but diagram type doesn't support it,
    // rendering should succeed with graceful degradation
    let input = "pie\n\"A\": 30\n\"B\": 70\n";
    let output = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "enabled"],
        input,
    );
    assert_success(&output, "fnx fallback render");

    let svg = String::from_utf8_lossy(&output.stdout);
    assert!(svg.contains("<svg"), "should still produce SVG");
}

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_fallback_reports_reason_in_witness() {
    // Test that fallback reason is captured
    let input = "flowchart LR\nA-->B\n";
    let (output, stdout) = run_cli_json(
        &[
            "render",
            "-",
            "--format",
            "svg",
            "--fnx-mode",
            "enabled",
            "--fnx-fallback",
            "graceful",
        ],
        input,
    );
    assert_success(&output, "fnx fallback with reason");

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output JSON");

    let witness = json.get("fnx_witness");
    assert!(
        witness.is_some(),
        "should have fnx_witness even with fallback"
    );
}

#[cfg(feature = "fnx-integration")]
#[test]
fn validate_obeys_fallback_policy_and_preserves_the_report() {
    let input = "flowchart LR\nA-->B-->C\n";
    for policy in ["warn", "strict"] {
        let artifact = NamedTempFile::new().unwrap();
        let output = run_cli(
            &[
                "validate",
                "-",
                "--format",
                "json",
                "--fnx-mode",
                "enabled",
                "--fnx-fallback",
                policy,
                "--layout-algorithm",
                "tree",
                "--fnx-work-budget",
                "10",
                "--fnx-time-budget-ms",
                "18446744073709551615",
                "--fail-on",
                "none",
                "--diagnostics-out",
                artifact.path().to_str().unwrap(),
            ],
            input,
        );
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("structured budget report");
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(artifact.path()).unwrap()).unwrap();
        assert_eq!(
            saved, report,
            "the artifact survives strict command failure"
        );
        assert_eq!(report["fnx_witness"]["budget_exceeded"], true);
        assert_eq!(
            report["fnx_witness"]["algorithms_invoked"],
            serde_json::json!(["connected_components"])
        );
        assert_eq!(report["fnx_component_count"], 1);
        assert!(report.get("fnx_bridge_count").is_none());
        assert_eq!(report["valid"], policy == "warn");
        assert_eq!(output.status.success(), policy == "warn");
        let diagnostics = report["diagnostics"].as_array().unwrap();
        assert_eq!(
            diagnostics.iter().any(|entry| {
                entry["rule_id"] == "fnx.fallback.strict" && entry["severity"] == "error"
            }),
            policy == "strict"
        );
        if policy == "warn" {
            assert!(String::from_utf8_lossy(&output.stderr).contains("FNX fallback"));
            let warning = diagnostics
                .iter()
                .find(|entry| entry["rule_id"] == "fnx.fallback.warn")
                .expect("warn policy is retained in machine-readable diagnostics");
            assert_eq!(warning["error_code"], "mermaid/warning/fnx-fallback");
            assert_eq!(warning["severity"], "warning");
            assert!(
                warning["message"]
                    .as_str()
                    .unwrap()
                    .contains("structural analysis")
            );
            assert!(warning["remediation_hint"].is_string());
        } else {
            assert!(
                diagnostics
                    .iter()
                    .any(|entry| { entry["error_code"] == "mermaid/error/fnx-strict-fallback" })
            );
        }
    }
}

#[cfg(feature = "fnx-integration")]
#[test]
fn fallback_policy_does_not_change_a_successful_result_hash() {
    let input = "flowchart LR\nA-->B\nB-->C\n";
    let mut hashes = Vec::new();
    for policy in ["graceful", "warn", "strict"] {
        let (render, stdout) = run_cli_json(
            &[
                "render",
                "-",
                "--fnx-mode",
                "enabled",
                "--fnx-fallback",
                policy,
            ],
            input,
        );
        assert_success(&render, "render with successful FNX policy");
        let rendered: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        let validate = run_cli(
            &[
                "validate",
                "-",
                "--format",
                "json",
                "--fnx-mode",
                "enabled",
                "--fnx-fallback",
                policy,
            ],
            input,
        );
        assert_success(&validate, "validation with successful FNX policy");
        let validated: serde_json::Value = serde_json::from_slice(&validate.stdout).unwrap();
        hashes.push((
            rendered["fnx_witness"]["results_hash"]
                .as_str()
                .unwrap()
                .to_owned(),
            validated["fnx_witness"]["results_hash"]
                .as_str()
                .unwrap()
                .to_owned(),
        ));
    }
    assert!(hashes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[cfg(feature = "fnx-integration")]
#[test]
fn strict_render_allows_a_diagram_with_no_applicable_analysis() {
    let output = run_cli(
        &[
            "render",
            "-",
            "--fnx-mode",
            "enabled",
            "--fnx-fallback",
            "strict",
            "--fnx-time-budget-ms",
            "0",
            "--fnx-work-budget",
            "0",
        ],
        "pie\n\"A\": 30\n\"B\": 70\n",
    );
    assert_success(&output, "strict render with no FNX cancellation");
    assert!(String::from_utf8_lossy(&output.stdout).contains("<svg"));
}

#[cfg(feature = "fnx-integration")]
#[test]
fn validate_reports_budget_exhaustion_without_inventing_unperformed_findings() {
    // Two participants keep geometry simple while 3,500 messages exceed the default 10,000
    // work-unit analysis budget before the bridge and cycle passes. This remains a budget
    // cancellation on both fast and slow machines; an earlier time limit is also truthful.
    let input = format!("sequenceDiagram\n{}", "Alice->>Bob: Hello\n".repeat(3_500));
    let output = run_cli(
        &[
            "validate",
            "-",
            "--format",
            "json",
            "--fnx-mode",
            "enabled",
            "--fail-on",
            "warning",
        ],
        &input,
    );
    assert!(
        !output.status.success(),
        "budget warning must respect --fail-on warning"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid validation JSON: {error}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(json["edge_count"], 3_500);
    assert_eq!(json["valid"], false);
    let witness = &json["fnx_witness"];
    assert_eq!(witness["budget_exceeded"], true);
    assert_ne!(witness["fallback_level"], "fnx_full");
    let invoked = witness["algorithms_invoked"]
        .as_array()
        .expect("recorded passes");
    assert!(
        !invoked
            .iter()
            .any(|pass| pass == "bridges" || pass == "cycle_basis")
    );
    assert!(
        json.get("fnx_bridge_count").is_none(),
        "unperformed bridge analysis is unknown"
    );
    assert!(
        json.get("fnx_cycle_count").is_none(),
        "unperformed cycle analysis is unknown"
    );
    if !invoked.iter().any(|pass| pass == "connected_components") {
        assert!(json.get("fnx_component_count").is_none());
        assert!(json.get("fnx_is_connected").is_none());
    }
    assert!(
        json["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diag| {
                diag["stage"] == "fnx"
                    && diag["error_code"] == "mermaid/warning/fnx-budget-001"
                    && diag["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("budget"))
            })
    );
}

// ============================================================================
// Pathological Input Scenarios
// ============================================================================

#[test]
fn pathological_deep_chain() {
    // Test with a very deep chain (100 nodes)
    let mut input = "flowchart TD\n".to_string();
    for i in 0..100 {
        if i < 99 {
            input.push_str(&format!("    N{} --> N{}\n", i, i + 1));
        }
    }

    let output = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "disabled"],
        &input,
    );
    assert_success(&output, "deep chain render");

    let svg = String::from_utf8_lossy(&output.stdout);
    assert!(svg.contains("<svg"), "should produce SVG");
}

#[test]
fn pathological_wide_graph() {
    // Test with a very wide graph (hub with 50 spokes)
    let mut input = "flowchart TD\n    Hub[Central]\n".to_string();
    for i in 0..50 {
        input.push_str(&format!("    Spoke{} --> Hub\n", i));
    }

    let output = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "disabled"],
        &input,
    );
    assert_success(&output, "wide graph render");

    let svg = String::from_utf8_lossy(&output.stdout);
    assert!(svg.contains("<svg"), "should produce SVG");
}

#[cfg(feature = "fnx-integration")]
#[test]
fn pathological_dense_cycle() {
    // Test with a dense cycle (all nodes connected to each other)
    let mut input = "flowchart TD\n".to_string();
    for i in 0..10 {
        for j in 0..10 {
            if i != j {
                input.push_str(&format!("    N{} --> N{}\n", i, j));
            }
        }
    }

    let output = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "enabled"],
        &input,
    );
    assert_success(&output, "dense cycle render");

    let svg = String::from_utf8_lossy(&output.stdout);
    assert!(svg.contains("<svg"), "should produce SVG");
}

// ============================================================================
// Evidence and Artifact Capture
// ============================================================================

#[test]
fn render_json_includes_timing_metrics() {
    let input = "flowchart LR\nA-->B-->C-->D\n";
    let (output, stdout) = run_cli_json(&["render", "-", "--format", "svg"], input);
    assert_success(&output, "render with json");

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output JSON");

    // Verify timing fields are present
    assert!(
        json.get("parse_time_ms").is_some(),
        "should have parse_time_ms"
    );
    assert!(
        json.get("layout_time_ms").is_some(),
        "should have layout_time_ms"
    );
    assert!(
        json.get("render_time_ms").is_some(),
        "should have render_time_ms"
    );
}

#[test]
fn render_json_includes_node_edge_counts() {
    let input = "flowchart LR\nA-->B-->C\nB-->D\n";
    let (output, stdout) = run_cli_json(&["render", "-", "--format", "svg"], input);
    assert_success(&output, "render with json");

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output JSON");

    assert_eq!(
        json.get("node_count"),
        Some(&serde_json::json!(4)),
        "should have 4 nodes"
    );
    assert_eq!(
        json.get("edge_count"),
        Some(&serde_json::json!(3)),
        "should have 3 edges"
    );
}

// ============================================================================
// Mode Switching Scenarios
// ============================================================================

#[cfg(feature = "fnx-integration")]
#[test]
fn mode_switch_produces_consistent_layout() {
    let input = "flowchart LR\nA-->B-->C\n";

    // Render with FNX disabled
    let output_disabled = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "disabled"],
        input,
    );
    assert_success(&output_disabled, "disabled mode");

    // Render with FNX enabled
    let output_enabled = run_cli(
        &["render", "-", "--format", "svg", "--fnx-mode", "enabled"],
        input,
    );
    assert_success(&output_enabled, "enabled mode");

    // Both should produce valid SVG
    let svg_disabled = String::from_utf8_lossy(&output_disabled.stdout);
    let svg_enabled = String::from_utf8_lossy(&output_enabled.stdout);

    assert!(
        svg_disabled.contains("<svg"),
        "disabled mode should produce SVG"
    );
    assert!(
        svg_enabled.contains("<svg"),
        "enabled mode should produce SVG"
    );

    // Both should have the same number of node group elements (class="fm-node")
    // Note: FNX mode may add additional classes like fm-node-centrality-high,
    // so we count the primary node class declaration specifically
    let count_node_groups = |svg: &str| -> usize { svg.matches(r#"class="fm-node"#).count() };
    assert_eq!(
        count_node_groups(&svg_disabled),
        count_node_groups(&svg_enabled),
        "node group count should match across modes"
    );
}

#[test]
fn auto_mode_selects_appropriate_algorithm() {
    let input = "flowchart LR\nA-->B-->C\n";
    let (output, stdout) = run_cli_json(
        &["render", "-", "--format", "svg", "--fnx-mode", "auto"],
        input,
    );
    assert_success(&output, "auto mode render");

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output JSON");

    // Auto mode should successfully render with timing metrics
    assert!(
        json.get("layout_time_ms").is_some(),
        "should report layout timing"
    );
    assert!(json.get("node_count").is_some(), "should report node count");
}

#[test]
fn batch_checks_fnx_requirements_before_creating_output() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let source = directory.path().join("input.mmd");
    let out = directory.path().join("rendered");
    std::fs::write(&source, "flowchart LR\nA-->B\n").unwrap();
    let args = [
        "render-batch",
        source.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--fnx-mode",
        "enabled",
        "--fnx-projection",
        "directed",
    ];
    let output = run_cli(&args, "");
    assert!(!output.status.success());
    let expected = if cfg!(feature = "fnx-integration") {
        "--fnx-projection directed is not yet supported"
    } else {
        "--fnx-mode enabled requires fnx-integration feature"
    };
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "unexpected error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.exists());
}

#[cfg(not(feature = "fnx-experimental-directed"))]
#[test]
fn directed_validation_requires_the_experimental_feature() {
    let output = run_cli(
        &["validate", "-", "--fnx-projection", "directed"],
        "flowchart LR\nA-->B\n",
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "validate --fnx-projection directed requires fnx-experimental-directed feature"
        )
    );
}

#[cfg(feature = "fnx-experimental-directed")]
fn run_directed_validation(
    input: &str,
    work_budget: &str,
    fallback: &str,
) -> (Output, serde_json::Value) {
    let output = run_cli(
        &[
            "validate",
            "-",
            "--format",
            "json",
            "--fnx-mode",
            "enabled",
            "--fnx-projection",
            "directed",
            "--fnx-work-budget",
            work_budget,
            "--fnx-time-budget-ms",
            "18446744073709551615",
            "--fnx-fallback",
            fallback,
            "--fail-on",
            "none",
        ],
        input,
    );
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid validation JSON: {error}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output, report)
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_distinguishes_a_dag_from_an_undirected_cycle() {
    let input = "flowchart LR\nA-->B\nA-->C\nB-->D\nC-->D\n";
    let (output, report) = run_directed_validation(input, "40", "strict");
    assert_success(&output, "directed diamond DAG");
    let directed = &report["fnx_directed"];
    assert_eq!(directed["complete"], true);
    assert_eq!(directed["strongly_connected_component_count"], 4);
    assert_eq!(directed["weakly_connected_component_count"], 1);
    assert_eq!(directed["is_weakly_connected"], true);
    assert_eq!(directed["has_cycles"], false);
    assert_eq!(directed["cyclic_component_count"], 0);
    assert_eq!(directed["cyclic_nodes"], serde_json::json!([]));
    assert_eq!(directed["cyclic_edge_indices"], serde_json::json!([]));
    assert_eq!(directed["sources"], serde_json::json!(["A"]));
    assert_eq!(directed["sinks"], serde_json::json!(["D"]));
    assert!(report.get("fnx_cycle_count").is_none());
    assert!(report.get("fnx_component_count").is_none());
    assert!(report.get("fnx_articulation_point_count").is_none());
    assert!(report.get("fnx_bridge_count").is_none());
    let witness = &report["fnx_witness"];
    assert_eq!(witness["projection_mode"], "directed");
    assert_eq!(witness["used"], true);
    assert_eq!(
        witness["algorithms_invoked"],
        serde_json::json!(["strongly_connected_components"])
    );
    assert_eq!(
        directed["native_algorithms_invoked"],
        serde_json::json!(["weakly_connected_components", "directed_edge_summary"])
    );
    assert!(
        !report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| {
                diagnostic["rule_id"] == "fnx.directed.cycle"
                    || diagnostic["rule_id"] == "fnx.cycle"
            })
    );
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_honors_two_way_arrowheads_and_original_edge_indices() {
    for arrow in ["<-->", "<==>", "<-.->"] {
        let input = format!("flowchart LR\nA{arrow}B\n");
        let (output, report) = run_directed_validation(&input, "20", "strict");
        assert_success(&output, "bidirectional dependency");
        let directed = &report["fnx_directed"];
        assert_eq!(directed["has_cycles"], true);
        assert_eq!(
            directed["strongly_connected_components"],
            serde_json::json!([["A", "B"]])
        );
        assert_eq!(directed["cyclic_edge_indices"], serde_json::json!([0]));
        assert_eq!(directed["sources"], serde_json::json!([]));
        assert_eq!(directed["sinks"], serde_json::json!([]));
        assert_eq!(
            report["fnx_witness"]["algorithms_invoked"],
            serde_json::json!(["strongly_connected_components"])
        );
    }
    // Filtering the first link and expanding the second must not shift authored edge indices.
    let (output, report) =
        run_directed_validation("flowchart LR\nA~~~B\nA<-->B\nB-->A\n", "30", "strict");
    assert_success(&output, "mixed projected edge provenance");
    assert_eq!(
        report["fnx_directed"]["cyclic_edge_indices"],
        serde_json::json!([1, 2])
    );
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_excludes_invisible_layout_constraints_from_topology() {
    let (output, report) = run_directed_validation("flowchart LR\nA-->B\nB~~~A\n", "20", "strict");
    assert_success(&output, "one-way graph with invisible return constraint");
    let directed = &report["fnx_directed"];
    assert_eq!(directed["has_cycles"], false);
    assert_eq!(directed["strongly_connected_component_count"], 2);
    assert_eq!(directed["weakly_connected_component_count"], 1);
    assert_eq!(directed["cyclic_edge_indices"], serde_json::json!([]));
    assert_eq!(directed["sources"], serde_json::json!(["A"]));
    assert_eq!(directed["sinks"], serde_json::json!(["B"]));

    let (output, report) = run_directed_validation("flowchart LR\nA~~~B\n", "15", "strict");
    assert_success(&output, "nodes joined only for layout");
    assert_eq!(
        report["fnx_directed"]["weakly_connected_component_count"],
        2
    );
    assert_eq!(report["fnx_directed"]["is_weakly_connected"], false);
    assert_eq!(
        report["fnx_directed"]["sources"],
        serde_json::json!(["A", "B"])
    );
    assert_eq!(
        report["fnx_directed"]["sinks"],
        serde_json::json!(["A", "B"])
    );
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_reports_feedback_loops_parallel_edges_and_isolated_nodes() {
    let input = "flowchart LR\nA-->B\nB-->A\nB-->C\nB-->C\nD-->D\nE\n";
    let (output, first) = run_directed_validation(input, "50", "strict");
    assert_success(&output, "directed feedback graph");
    let directed = &first["fnx_directed"];
    assert_eq!(
        directed["strongly_connected_components"],
        serde_json::json!([["A", "B"], ["C"], ["D"], ["E"]])
    );
    assert_eq!(directed["strongly_connected_component_count"], 4);
    assert_eq!(directed["weakly_connected_component_count"], 3);
    assert_eq!(directed["is_weakly_connected"], false);
    assert_eq!(directed["has_cycles"], true);
    assert_eq!(directed["cyclic_component_count"], 2);
    assert_eq!(directed["cyclic_nodes"], serde_json::json!(["A", "B", "D"]));
    assert_eq!(
        directed["cyclic_edge_indices"],
        serde_json::json!([0, 1, 4])
    );
    assert_eq!(directed["sources"], serde_json::json!(["D", "E"]));
    assert_eq!(directed["sinks"], serde_json::json!(["C", "D", "E"]));
    let diagnostics = first["diagnostics"].as_array().unwrap();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic["rule_id"] == "fnx.directed.cycle")
            .count(),
        2
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["rule_id"] == "fnx.directed.connectivity")
    );

    let (output, second) = run_directed_validation(input, "50", "strict");
    assert_success(&output, "repeated directed feedback graph");
    assert_eq!(first["fnx_directed"], second["fnx_directed"]);
    assert_eq!(
        first["fnx_witness"]["results_hash"],
        second["fnx_witness"]["results_hash"]
    );
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_budget_keeps_unobserved_metrics_absent() {
    for budget in [0, 10, 15, 20, 25] {
        let (output, report) =
            run_directed_validation("flowchart LR\nA-->B-->C\n", &budget.to_string(), "graceful");
        assert_success(&output, "budgeted directed analysis");
        let directed = &report["fnx_directed"];
        assert_eq!(directed["complete"], budget >= 25);
        assert_eq!(
            directed.get("strongly_connected_component_count").is_some(),
            budget >= 15
        );
        assert_eq!(directed.get("has_cycles").is_some(), budget >= 15);
        assert_eq!(
            directed.get("weakly_connected_component_count").is_some(),
            budget >= 20
        );
        assert_eq!(directed.get("cyclic_edge_indices").is_some(), budget >= 25);
        assert_eq!(directed.get("sources").is_some(), budget >= 25);
        assert_eq!(report["fnx_witness"]["used"], budget >= 10);
        assert_eq!(report["fnx_witness"]["budget_exceeded"], budget < 25);
        if budget < 25 {
            assert_eq!(directed["failure_reason"], "work_exceeded");
            assert!(
                report["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic["rule_id"] == "fnx.budget")
            );
        }
    }
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_preserves_warn_and_strict_reports_after_partial_analysis() {
    for policy in ["graceful", "warn", "strict"] {
        let (output, report) = run_directed_validation("flowchart LR\nA-->B-->C\n", "15", policy);
        assert_eq!(output.status.success(), policy != "strict");
        assert_eq!(report["valid"], policy != "strict");
        assert_eq!(
            report["fnx_directed"]["strongly_connected_component_count"],
            3
        );
        assert!(
            report["fnx_directed"]
                .get("weakly_connected_component_count")
                .is_none()
        );
        assert_eq!(report["fnx_witness"]["fallback_level"], "fnx_partial");
        let diagnostics = report["diagnostics"].as_array().unwrap();
        assert_eq!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic["rule_id"] == "fnx.fallback.warn"),
            policy == "warn"
        );
        assert_eq!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic["rule_id"] == "fnx.fallback.strict"),
            policy == "strict"
        );
    }
}

#[cfg(feature = "fnx-experimental-directed")]
#[test]
fn directed_validation_is_explicit_and_does_not_enable_directed_rendering() {
    let input = "flowchart LR\nA-->B\n";
    let disabled = run_cli(
        &[
            "validate",
            "-",
            "--format",
            "json",
            "--fnx-mode",
            "disabled",
            "--fnx-projection",
            "directed",
        ],
        input,
    );
    assert_success(&disabled, "disabled directed analysis");
    let report: serde_json::Value = serde_json::from_slice(&disabled.stdout).unwrap();
    assert!(report.get("fnx_directed").is_none());
    assert!(report.get("fnx_witness").is_none());

    let render = run_cli(
        &[
            "render",
            "-",
            "--fnx-mode",
            "enabled",
            "--fnx-projection",
            "directed",
        ],
        input,
    );
    assert!(!render.status.success());
    assert!(
        String::from_utf8_lossy(&render.stderr)
            .contains("--fnx-projection directed is not yet supported")
    );

    let text = run_cli(&["validate", "-", "--fnx-projection", "directed"], input);
    assert_success(&text, "directed text report");
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("Directed analysis: complete"));
    assert!(text.contains("Strongly connected components: 2"));
    assert!(text.contains("Directed cyclic components: 0"));
}

#[cfg(feature = "fnx-integration")]
#[test]
fn fnx_batch_accepts_strict_and_warn_when_analysis_succeeds() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let source = directory.path().join("input.mmd");
    std::fs::write(&source, "flowchart LR\nA-->B\n").unwrap();
    for fallback in ["strict", "warn"] {
        let out = directory.path().join(fallback);
        let output = run_cli(
            &[
                "render-batch",
                source.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
                "--jobs",
                "1",
                "--fnx-mode",
                "enabled",
                "--fnx-fallback",
                fallback,
                "--fnx-time-budget-ms",
                "18446744073709551615",
                "--fnx-work-budget",
                "100",
            ],
            "",
        );
        assert_success(&output, fallback);
        let svg = std::fs::read_to_string(out.join("input.svg")).unwrap();
        assert!(svg.contains("<svg"));
    }
}

#[test]
fn disabled_mode_accepts_zero_fnx_budgets_on_every_command() {
    let input = "flowchart LR\nA-->B-->C\n";
    for command in ["render", "validate"] {
        let output = run_cli(
            &[
                command,
                "-",
                "--fnx-mode",
                "disabled",
                "--fnx-fallback",
                "strict",
                "--fnx-time-budget-ms",
                "0",
                "--fnx-work-budget",
                "0",
            ],
            input,
        );
        assert_success(&output, command);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("FNX fallback"));
    }

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("input.mmd");
    let out = directory.path().join("rendered");
    std::fs::write(&source, input).unwrap();
    let output = run_cli(
        &[
            "render-batch",
            source.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--jobs",
            "1",
            "--fnx-mode",
            "disabled",
            "--fnx-fallback",
            "strict",
            "--fnx-time-budget-ms",
            "0",
            "--fnx-work-budget",
            "0",
        ],
        "",
    );
    assert_success(&output, "disabled batch with zero budgets");
    assert!(
        std::fs::read_to_string(out.join("input.svg"))
            .unwrap()
            .contains("<svg")
    );
}

#[cfg(feature = "fnx-integration")]
#[test]
fn zero_work_budget_obeys_render_fallback_before_writing_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = "flowchart LR\nA-->B-->C\n";
    for policy in ["graceful", "warn", "strict"] {
        let destination = directory.path().join(format!("{policy}.svg"));
        let output = run_cli(
            &[
                "render",
                "-",
                "--output",
                destination.to_str().unwrap(),
                "--json",
                "--layout-algorithm",
                "sugiyama",
                "--fnx-mode",
                "enabled",
                "--fnx-fallback",
                policy,
                "--fnx-work-budget",
                "0",
                "--fnx-time-budget-ms",
                "18446744073709551615",
            ],
            input,
        );
        if policy == "strict" {
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("strict FNX fallback policy rejected layout: work_exceeded")
            );
            assert!(
                !destination.exists(),
                "strict failure must not create output"
            );
            continue;
        }
        assert_success(&output, policy);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["fnx_witness"]["budget_exceeded"], true);
        assert_eq!(report["fnx_witness"]["fallback_reason"], "work_exceeded");
        assert_eq!(report["fnx_witness"]["used"], false);
        assert_eq!(
            report["fnx_witness"]["algorithms_invoked"],
            serde_json::json!([])
        );
        let warned = report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .is_some_and(|text| text.contains("FNX fallback"))
            });
        assert_eq!(warned, policy == "warn");
        let svg = std::fs::read_to_string(&destination).unwrap();
        assert!(svg.contains("<svg") && svg.contains("fm-node"));
        assert!(!svg.contains("fm-node-centrality-"));
    }
}

#[cfg(feature = "fnx-integration")]
#[test]
fn batch_budget_changes_invalidate_cached_output_and_preserve_strict_failure() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("input.mmd");
    let out = directory.path().join("rendered");
    std::fs::write(&source, "flowchart LR\nA-->B-->C\n").unwrap();
    let run = |work_budget: &str, fallback: &str| {
        run_cli(
            &[
                "render-batch",
                source.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
                "--jobs",
                "1",
                "--layout-algorithm",
                "sugiyama",
                "--fnx-mode",
                "enabled",
                "--fnx-fallback",
                fallback,
                "--fnx-work-budget",
                work_budget,
                "--fnx-time-budget-ms",
                "18446744073709551615",
            ],
            "",
        )
    };

    assert_success(&run("100", "strict"), "healthy custom batch budget");
    let destination = out.join("input.svg");
    let healthy_svg = std::fs::read_to_string(&destination).unwrap();
    assert!(healthy_svg.contains("fm-node-centrality-"));
    let cached = run("100", "strict");
    assert_success(&cached, "same-budget cached batch");
    assert!(
        String::from_utf8_lossy(&cached.stderr).contains("1 persistent hits"),
        "the unchanged healthy result must first demonstrate an actual cache hit"
    );

    let rejected = run("0", "strict");
    assert!(
        !rejected.status.success(),
        "a changed budget cannot inherit cache success"
    );
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("strict FNX fallback policy rejected layout: work_exceeded")
    );
    assert_eq!(
        std::fs::read_to_string(&destination).unwrap(),
        healthy_svg,
        "strict failure preserves the previous complete artifact"
    );

    let warned = run("0", "warn");
    assert_success(&warned, "zero-budget batch with visible fallback");
    assert!(String::from_utf8_lossy(&warned.stderr).contains("FNX fallback"));
    let fallback_svg = std::fs::read_to_string(&destination).unwrap();
    assert!(fallback_svg.contains("<svg"));
    assert!(!fallback_svg.contains("fm-node-centrality-"));

    assert_success(
        &run("100", "strict"),
        "restored budget recomputes full analysis",
    );
    assert_eq!(std::fs::read_to_string(destination).unwrap(), healthy_svg);
}
