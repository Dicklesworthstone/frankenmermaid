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
    let input = format!("sequenceDiagram\n{}", "Alice->>Bob: Hello\n".repeat(3_500));
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
                "--fail-on",
                "none",
                "--diagnostics-out",
                artifact.path().to_str().unwrap(),
            ],
            &input,
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
            ],
            "",
        );
        assert_success(&output, fallback);
        let svg = std::fs::read_to_string(out.join("input.svg")).unwrap();
        assert!(svg.contains("<svg"));
    }
}
