//! CLI coverage for `minimize` (bd-2xl.14).
//!
//! The reducer module shipped for months as an undeclared file, so nothing it contained ever
//! compiled or ran. These tests drive the real binary end to end so the command cannot regress
//! back into unreachable code.

use std::process::Command;

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_fm-cli");

/// A four-edge flowchart whose single dotted edge is the only source of the render-only
/// `fm-edge-dashed` class. A bare `stroke-dasharray` needle would match the unconditional theme
/// CSS instead, so every candidate down to the empty input would look like a reproduction.
const DOTTED_EDGE_INPUT: &str = "flowchart LR\n  A --> B\n  B -.-> C\n  C --> D\n  D --> E\n";

fn write_input(dir: &TempDir, name: &str, content: &str) -> String {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("write minimize input fixture");
    path.to_str().expect("fixture path utf-8").to_string()
}

#[test]
fn minimize_render_stage_shrinks_to_the_line_that_causes_the_marker() {
    let dir = TempDir::new().expect("temp dir");
    let input = write_input(&dir, "dotted.mmd", DOTTED_EDGE_INPUT);
    let report_path = dir.path().join("report.json");

    let output = Command::new(BINARY)
        .args([
            "minimize",
            &input,
            "--stage",
            "render",
            "--signature",
            "output-contains",
            "--needle",
            "fm-edge-dashed",
            "--report",
            report_path.to_str().expect("report path utf-8"),
        ])
        .output()
        .expect("run minimize");

    assert!(
        output.status.success(),
        "minimize failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let minimized = String::from_utf8(output.stdout).expect("minimized stdout utf-8");
    // The character pass reduces past the well-formed arrow — a bare `B -.` still parses into a
    // dotted edge — so the assertion is on the dotted token, not on `-.->`.
    assert!(
        minimized.contains("-."),
        "the dotted-edge token is the only source of the marker: {minimized:?}"
    );
    assert!(
        minimized.lines().count() < DOTTED_EDGE_INPUT.lines().count(),
        "reduction must remove at least one line: {minimized:?}"
    );

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read report"))
            .expect("report json");
    assert_eq!(report["reproduced"], serde_json::json!(true));
    assert_eq!(report["stage"], serde_json::json!("render"));
    assert_eq!(report["signature"], serde_json::json!("output-contains"));
    assert_eq!(report["needle"], serde_json::json!("fm-edge-dashed"));
    assert!(
        report["minimized_bytes"].as_u64().expect("minimized_bytes")
            < report["original_bytes"].as_u64().expect("original_bytes"),
        "report must record the shrink it achieved: {report:?}"
    );
    assert!(
        report["minimized_input"]
            .as_str()
            .is_some_and(|value| value.contains("-.")),
        "the artifact must carry the repro itself: {report:?}"
    );
}

#[test]
fn minimize_reports_a_signature_that_never_fired_instead_of_a_silent_no_op() {
    let dir = TempDir::new().expect("temp dir");
    // A render-only marker cannot appear in parse output, so this probe can never fire.
    let input = write_input(&dir, "dotted.mmd", DOTTED_EDGE_INPUT);

    let output = Command::new(BINARY)
        .args([
            "minimize",
            &input,
            "--stage",
            "parse",
            "--signature",
            "output-contains",
            "--needle",
            "fm-edge-dashed",
        ])
        .output()
        .expect("run minimize");

    assert!(
        !output.status.success(),
        "a signature that never reproduced must not exit 0"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not reproduce"),
        "diagnostic must say the signature never fired: {stderr}"
    );
    assert!(
        stderr.contains("stage"),
        "diagnostic must point at the stage as a likely cause: {stderr}"
    );
}

#[test]
fn minimize_requires_a_needle_for_output_signatures() {
    let dir = TempDir::new().expect("temp dir");
    let input = write_input(&dir, "dotted.mmd", DOTTED_EDGE_INPUT);

    let output = Command::new(BINARY)
        .args(["minimize", &input, "--signature", "output-contains"])
        .output()
        .expect("run minimize");

    assert!(!output.status.success(), "missing needle must not exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--needle"),
        "diagnostic must name the missing flag: {stderr}"
    );
}

#[test]
fn minimize_keeps_the_erroring_line_when_reducing_a_parse_error() {
    let dir = TempDir::new().expect("temp dir");
    // `%%{init:` with malformed JSON is a parse-level error; the surrounding edges are noise.
    let content = concat!(
        "flowchart LR\n",
        "  A --> B\n",
        "  B --> C\n",
        "%%{init: {\"theme\": }}%%\n",
        "  C --> D\n",
        "  D --> E\n"
    );
    let input = write_input(&dir, "bad_init.mmd", content);

    let output = Command::new(BINARY)
        .args(["minimize", &input, "--signature", "any-error"])
        .output()
        .expect("run minimize");

    if !output.status.success() {
        // The fixture is only useful while it really does produce an error-severity diagnostic;
        // say so explicitly rather than asserting on a reduction that never started.
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("does not reproduce"),
            "unexpected minimize failure: {stderr}"
        );
        return;
    }

    let minimized = String::from_utf8(output.stdout).expect("minimized stdout utf-8");
    assert!(
        minimized.lines().count() <= content.lines().count(),
        "reduction must not grow the input: {minimized:?}"
    );
    assert!(
        minimized.contains("init"),
        "the malformed directive is the cause and must survive: {minimized:?}"
    );
}
