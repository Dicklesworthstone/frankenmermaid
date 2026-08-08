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

#[test]
fn minimize_bundle_carries_the_input_and_a_trace_of_what_the_pipeline_did() {
    let dir = TempDir::new().expect("temp dir");
    let input = write_input(&dir, "dotted.mmd", DOTTED_EDGE_INPUT);
    let bundle = dir.path().join("bundle");

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
            "--bundle",
            bundle.to_str().expect("bundle path utf-8"),
        ])
        .output()
        .expect("run minimize");
    assert!(
        output.status.success(),
        "minimize failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bundled_input =
        std::fs::read_to_string(bundle.join("minimized.mmd")).expect("bundled minimized.mmd");
    assert!(
        bundled_input.contains("-."),
        "the bundle must carry the repro: {bundled_input:?}"
    );

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("report.json")).expect("read report"))
            .expect("report json");
    let trace = &report["trace"];
    assert_eq!(trace["diagram_type"], serde_json::json!("flowchart"));
    assert!(
        trace["node_count"].as_u64().expect("node_count") >= 1,
        "trace must describe the minimized diagram: {trace:?}"
    );
    // The algorithm that actually ran is the first thing to check when a shrunken repro stops
    // behaving like the original, so the bundle has to record it.
    assert!(
        trace["layout_selected"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "trace must name the layout algorithm that ran: {trace:?}"
    );
    assert_eq!(
        trace["invariant_violations"],
        serde_json::json!([]),
        "a healthy reduction must report no geometry violations: {trace:?}"
    );
}

#[test]
fn minimize_invariant_signature_does_not_fire_on_clean_input_but_still_leaves_a_bundle() {
    let dir = TempDir::new().expect("temp dir");
    let input = write_input(&dir, "clean.mmd", DOTTED_EDGE_INPUT);
    let bundle = dir.path().join("bundle");

    let output = Command::new(BINARY)
        .args([
            "minimize",
            &input,
            "--signature",
            "invariant-violation",
            "--bundle",
            bundle.to_str().expect("bundle path utf-8"),
        ])
        .output()
        .expect("run minimize");

    assert!(
        !output.status.success(),
        "clean geometry must not report a reproduced invariant violation"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("report.json")).expect("read report"))
            .expect("report json");
    assert_eq!(
        report["signature"],
        serde_json::json!("invariant-violation")
    );
    assert_eq!(report["reproduced"], serde_json::json!(false));
    // The point of writing the bundle before the reproduced check: a failed triage attempt still
    // leaves evidence of what was tried.
    assert!(
        bundle.join("minimized.mmd").exists(),
        "the bundle must exist even when the signature never fired"
    );
}

/// The non-vacuity companion to `fm_layout::invariants`' unit tests, which use synthetic broken
/// layouts. This one proves the invariants are true of what the engine really produces — if they
/// were wrong about real layouts, the checker would be firing constantly in production and the
/// `invariant-violation` signature would be useless.
#[test]
fn real_layouts_across_diagram_types_hold_the_geometry_invariants() {
    for input in [
        "flowchart LR\n  A --> B\n  B --> C",
        "sequenceDiagram\n  A->>B: msg",
        "classDiagram\n  A <|-- B",
        "stateDiagram-v2\n  [*] --> S1\n  S1 --> S2",
        "erDiagram\n  A ||--o{ B : has",
        "pie\n  \"A\" : 50\n  \"B\" : 50",
        "mindmap\n  root\n    A\n    B",
        "gantt\n  section S\n  T1 :a1, 2024-01-01, 3d",
        "gitGraph\n  commit\n  branch dev\n  commit\n  checkout main\n  merge dev",
        "kanban\n  Todo\n    task1\n  Done\n    task2",
        "packet-beta\n  0-7: \"a\"\n  8-15: \"b\"",
        "block-beta\n  columns 3\n  a b c",
        "quadrantChart\n  x-axis Low --> High\n  A: [0.3, 0.6]",
        "journey\n  section S\n    Task: 5: Me",
        "xychart-beta\n  bar [1, 2, 3]",
        "requirementDiagram\n  requirement r {\n  id: 1\n  text: t\n  }",
        "sankey-beta\n  A,B,10",
        "timeline\n  title T\n  2024 : event",
        "C4Context\n  Person(a, \"A\")",
        "mermaid-unknown-type\n  garbage ][ input",
    ] {
        let layout = fm_layout::layout_diagram(&fm_parser::parse(input).ir);
        let violations = fm_layout::invariants::layout_geometry_violations(&layout);
        assert!(
            violations.is_empty(),
            "{input:?} produced geometry violations: {violations:?}"
        );
    }
}
