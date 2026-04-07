use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

fn run_cli_in_dir(dir: &Path, args: &[&str], stdin: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fm-cli"));
    command.current_dir(dir).args(args);

    if stdin.is_empty() {
        command.output().expect("run fm-cli without stdin")
    } else {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn fm-cli with stdin");
        let Some(mut child_stdin) = child.stdin.take() else {
            panic!("failed to open stdin pipe");
        };
        if let Err(err) = child_stdin.write_all(stdin.as_bytes())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            panic!("failed writing stdin to fm-cli: {err}");
        }
        drop(child_stdin);
        child.wait_with_output().expect("collect fm-cli output")
    }
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be utf-8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be utf-8")
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

fn extract_viewbox_dimensions(svg: &str) -> (f32, f32) {
    let marker = "viewBox=\"";
    let start = svg.find(marker).expect("viewBox present") + marker.len();
    let end = svg[start..].find('"').expect("viewBox closing quote") + start;
    let values: Vec<f32> = svg[start..end]
        .split_whitespace()
        .map(|part| part.parse::<f32>().expect("viewBox number"))
        .collect();
    assert_eq!(values.len(), 4, "viewBox should have four numeric fields");
    (values[2], values[3])
}

fn write_config(dir: &TempDir, file_name: &str, contents: &str) -> String {
    let path = dir.path().join(file_name);
    fs::write(&path, contents).expect("write config file");
    path.to_string_lossy().into_owned()
}

#[test]
fn explicit_config_theme_applies_and_cli_theme_wins() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [svg]
            theme = "dark"
        "#,
    );

    let dark = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        dark.status.success(),
        "dark render failed: {}",
        stderr_text(&dark)
    );
    let dark_svg = stdout_text(&dark);
    assert!(
        dark_svg.contains("#0f172a"),
        "expected dark background in svg"
    );

    let forest = run_cli_in_dir(
        temp.path(),
        &[
            "--config",
            &config_path,
            "render",
            "-",
            "--format",
            "svg",
            "--theme",
            "forest",
        ],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        forest.status.success(),
        "forest override failed: {}",
        stderr_text(&forest)
    );
    let forest_svg = stdout_text(&forest);
    assert!(
        forest_svg.contains("#f5f5dc"),
        "expected forest background in svg"
    );
}

#[test]
fn explicit_config_controls_svg_effects_and_accessibility() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [svg]
            shadows = false
            gradients = false
            accessibility = false
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "render failed: {}",
        stderr_text(&output)
    );
    let svg = stdout_text(&output);
    assert!(!svg.contains("drop-shadow"));
    assert!(!svg.contains("<linearGradient"));
    assert!(!svg.contains("aria-label="));
    assert!(!svg.contains("role=\"img\""));
}

#[test]
fn explicit_config_enables_svg_effects_and_accessibility() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [svg]
            shadows = true
            gradients = true
            accessibility = true
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "render failed: {}",
        stderr_text(&output)
    );
    let svg = stdout_text(&output);
    assert!(svg.contains("id=\"drop-shadow\""));
    assert!(svg.contains("<linearGradient") || svg.contains("<radialGradient"));
    assert!(svg.contains("aria-label="));
    assert!(svg.contains("role=\"img\""));
}

#[test]
fn explicit_config_changes_layout_spacing() {
    let baseline_dir = TempDir::new().expect("baseline tempdir");
    let configured_dir = TempDir::new().expect("configured tempdir");
    let config_path = write_config(
        &configured_dir,
        "config.toml",
        r#"
            [layout]
            node_spacing = 160.0
            rank_spacing = 220.0
        "#,
    );
    let input = "flowchart LR\nA-->B-->C\n";

    let baseline = run_cli_in_dir(
        baseline_dir.path(),
        &["render", "-", "--format", "svg"],
        input,
    );
    assert!(
        baseline.status.success(),
        "baseline render failed: {}",
        stderr_text(&baseline)
    );
    let configured = run_cli_in_dir(
        configured_dir.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        input,
    );
    assert!(
        configured.status.success(),
        "configured render failed: {}",
        stderr_text(&configured)
    );

    let (baseline_width, baseline_height) = extract_viewbox_dimensions(&stdout_text(&baseline));
    let (configured_width, configured_height) =
        extract_viewbox_dimensions(&stdout_text(&configured));
    assert!(configured_width > baseline_width);
    assert!(configured_height >= baseline_height);
}

#[test]
fn explicit_config_changes_terminal_tier() {
    let baseline_dir = TempDir::new().expect("baseline tempdir");
    let configured_dir = TempDir::new().expect("configured tempdir");
    let config_path = write_config(
        &configured_dir,
        "config.toml",
        r#"
            [term]
            tier = "compact"
            unicode = false
        "#,
    );
    let input = "flowchart LR\nA[Start]-->B[Middle]-->C[End]\n";

    let baseline = run_cli_in_dir(
        baseline_dir.path(),
        &["render", "-", "--format", "term"],
        input,
    );
    assert!(
        baseline.status.success(),
        "baseline term render failed: {}",
        stderr_text(&baseline)
    );
    let configured = run_cli_in_dir(
        configured_dir.path(),
        &["--config", &config_path, "render", "-", "--format", "term"],
        input,
    );
    assert!(
        configured.status.success(),
        "configured term render failed: {}",
        stderr_text(&configured)
    );

    let baseline_stdout = stdout_text(&baseline);
    let configured_stdout = stdout_text(&configured);
    assert_ne!(configured_stdout, baseline_stdout);
    assert!(configured_stdout.lines().count() <= baseline_stdout.lines().count());
}

#[test]
fn explicit_config_enables_terminal_minimap() {
    let baseline_dir = TempDir::new().expect("baseline tempdir");
    let configured_dir = TempDir::new().expect("configured tempdir");
    let config_path = write_config(
        &configured_dir,
        "config.toml",
        r#"
            [term]
            minimap = true
        "#,
    );
    let input = "flowchart LR\nA-->B-->C-->D-->E-->F\n";

    let baseline = run_cli_in_dir(
        baseline_dir.path(),
        &["render", "-", "--format", "term"],
        input,
    );
    assert!(
        baseline.status.success(),
        "baseline term render failed: {}",
        stderr_text(&baseline)
    );
    let configured = run_cli_in_dir(
        configured_dir.path(),
        &["--config", &config_path, "render", "-", "--format", "term"],
        input,
    );
    assert!(
        configured.status.success(),
        "configured term render failed: {}",
        stderr_text(&configured)
    );

    assert_ne!(stdout_text(&baseline), stdout_text(&configured));
}

#[test]
fn invalid_config_reports_path_and_parse_context() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "bad.toml",
        r#"
            [svg
            theme = "dark"
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(!output.status.success(), "invalid config should fail");
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains(&config_path),
        "stderr should include config path"
    );
    assert!(
        stderr.contains("line") || stderr.contains("column"),
        "stderr should include parse context: {stderr}"
    );
}

#[test]
fn unknown_config_key_reports_field_name() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "bad.toml",
        r#"
            [svg]
            theme = "dark"
            definitely_not_a_real_key = true
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(!output.status.success(), "unknown config key should fail");
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains(&config_path),
        "stderr should include config path"
    );
    assert!(
        stderr.contains("unknown field"),
        "stderr should include unknown-field wording: {stderr}"
    );
    assert!(
        stderr.contains("definitely_not_a_real_key"),
        "stderr should include the unknown field name: {stderr}"
    );
}

#[test]
fn core_deterministic_false_is_accepted() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [core]
            deterministic = false
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "core.deterministic=false should be accepted: {}",
        stderr_text(&output)
    );
    assert!(stdout_text(&output).contains("<svg"));
}

#[test]
fn fallback_on_error_false_changes_default_parse_mode() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [core]
            fallback_on_error = false
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "parse", "-", "--full", "--pretty"],
        "plain text without a mermaid header\n",
    );
    assert!(
        output.status.success(),
        "strict-default parse should succeed: {}",
        stderr_text(&output)
    );
    let json = stdout_json(&output);
    assert_eq!(
        json["meta"]["parse_mode"],
        Value::String("strict".to_string())
    );
    assert_eq!(json["diagram_type"], Value::String("Unknown".to_string()));
}

#[test]
fn intent_inference_false_disables_fuzzy_detection() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [parser]
            intent_inference = false
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "detect", "-", "--json"],
        "flwchart LR\n",
    );
    assert!(
        output.status.success(),
        "detect should succeed: {}",
        stderr_text(&output)
    );
    let json = stdout_json(&output);
    assert_eq!(
        json["detection_method"],
        Value::String("fallback to flowchart".to_string())
    );
}

#[test]
fn fuzzy_keyword_distance_changes_detection_threshold() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = write_config(
        &temp,
        "config.toml",
        r#"
            [parser]
            fuzzy_keyword_distance = 1
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["--config", &config_path, "detect", "-", "--json"],
        "flwchrt LR\n",
    );
    assert!(
        output.status.success(),
        "detect should succeed: {}",
        stderr_text(&output)
    );
    let json = stdout_json(&output);
    assert_eq!(
        json["detection_method"],
        Value::String("fallback to flowchart".to_string())
    );
}

#[test]
fn auto_close_delimiters_config_controls_unclosed_shape_recovery() {
    let baseline_dir = TempDir::new().expect("baseline tempdir");
    let configured_dir = TempDir::new().expect("configured tempdir");
    let config_path = write_config(
        &configured_dir,
        "config.toml",
        r#"
            [parser]
            auto_close_delimiters = false
        "#,
    );
    let input = "flowchart LR\nA[Open\n";

    let baseline = run_cli_in_dir(
        baseline_dir.path(),
        &["parse", "-", "--full", "--pretty"],
        input,
    );
    assert!(
        baseline.status.success(),
        "baseline parse should succeed: {}",
        stderr_text(&baseline)
    );
    let configured = run_cli_in_dir(
        configured_dir.path(),
        &["--config", &config_path, "parse", "-", "--full", "--pretty"],
        input,
    );
    assert!(
        configured.status.success(),
        "configured parse should succeed: {}",
        stderr_text(&configured)
    );

    let baseline_json = stdout_json(&baseline);
    let configured_json = stdout_json(&configured);
    let baseline_nodes = baseline_json["nodes"]
        .as_array()
        .expect("nodes array")
        .len();
    let configured_nodes = configured_json["nodes"]
        .as_array()
        .expect("nodes array")
        .len();
    assert_eq!(baseline_nodes, 1);
    assert_eq!(configured_nodes, 0);
}

#[test]
fn create_placeholder_nodes_config_controls_dangling_edges() {
    let baseline_dir = TempDir::new().expect("baseline tempdir");
    let configured_dir = TempDir::new().expect("configured tempdir");
    let config_path = write_config(
        &configured_dir,
        "config.toml",
        r#"
            [parser]
            create_placeholder_nodes = false
        "#,
    );
    let input = "flowchart LR\nA -->\n";

    let baseline = run_cli_in_dir(
        baseline_dir.path(),
        &["parse", "-", "--full", "--pretty"],
        input,
    );
    assert!(
        baseline.status.success(),
        "baseline parse should succeed: {}",
        stderr_text(&baseline)
    );
    let configured = run_cli_in_dir(
        configured_dir.path(),
        &["--config", &config_path, "parse", "-", "--full", "--pretty"],
        input,
    );
    assert!(
        configured.status.success(),
        "configured parse should succeed: {}",
        stderr_text(&configured)
    );

    let baseline_json = stdout_json(&baseline);
    let configured_json = stdout_json(&configured);
    assert_eq!(
        baseline_json["edges"]
            .as_array()
            .expect("edges array")
            .len(),
        1
    );
    assert_eq!(
        configured_json["edges"]
            .as_array()
            .expect("edges array")
            .len(),
        0
    );
    let baseline_nodes = baseline_json["nodes"].as_array().expect("nodes array");
    assert!(
        baseline_nodes.iter().any(|node| {
            node["implicit"] == Value::Bool(true)
                || node["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("__fm_dangling_line_"))
        }),
        "expected an implicit placeholder node in baseline parse"
    );
}

#[test]
fn config_is_auto_discovered_from_current_directory() {
    let temp = TempDir::new().expect("tempdir");
    write_config(
        &temp,
        "frankenmermaid.toml",
        r#"
            [svg]
            theme = "dark"
        "#,
    );

    let output = run_cli_in_dir(
        temp.path(),
        &["render", "-", "--format", "svg"],
        "flowchart LR\nA-->B\n",
    );
    assert!(
        output.status.success(),
        "auto-discovered config render failed: {}",
        stderr_text(&output)
    );
    assert!(stdout_text(&output).contains("#0f172a"));
}
