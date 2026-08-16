//! bd-akv2 follow-on: the exit path must not truncate stdout.
//!
//! Install as `crates/fm-cli/tests/exit_flush_test.rs`. Requires the `finish`/`run` split in
//! `main.rs` (see `lever_exit_teardown.py`); it passes on the pre-lever binary too, which is the
//! point — it is a REGRESSION test for the lever, so it must be meaningful before and after.

use std::process::Command;

/// `std::process::exit` does not flush Rust's buffered `io::stdout`. The lever that skips allocator
/// teardown at exit therefore has exactly one way to be catastrophically wrong: truncated output.
///
/// The payload has to be big enough to still be sitting in the buffer at exit. Rust block-buffers
/// stdout when it is not a terminal — which is the case here, since the child's stdout is a pipe —
/// so a render whose JSON runs to several KB will not have been written out incidentally.
#[test]
fn cli_stdout_survives_the_exit_path_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("wide.mmd");

    // Wide enough that the emitted SVG comfortably exceeds any plausible buffer, so a missing
    // flush loses a suffix rather than nothing.
    let mut source = String::from("flowchart LR\n");
    for i in 0..400 {
        source.push_str(&format!("  N{i}[Node {i}] --> N{}\n", i + 1));
    }
    std::fs::write(&input, &source).expect("write fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_frankenmermaid"))
        .args(["render", input.to_str().unwrap(), "--format", "svg"])
        .output()
        .expect("run the CLI");

    assert!(
        output.status.success(),
        "render failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");

    // COMPLETENESS is the assertion, not merely "some output". A truncated write loses the TAIL,
    // so the closing tag is the discriminating byte: a half-flushed buffer still starts with
    // `<svg`.
    assert!(
        stdout.trim_end().ends_with("</svg>"),
        "stdout does not end with the closing tag — the exit path truncated it. \
         len={} tail={:?}",
        stdout.len(),
        &stdout[stdout.len().saturating_sub(120)..]
    );

    // Non-vacuity: the payload must actually be large enough to exercise buffering. If a future
    // change shrinks the fixture or the output, this test stops testing what it claims and should
    // fail loudly rather than pass trivially.
    assert!(
        stdout.len() > 32 * 1024,
        "fixture no longer produces a payload larger than the stdout buffer ({} bytes), so this \
         test can no longer detect a missing flush",
        stdout.len()
    );

    // The last node must be present, so completeness is checked against the INPUT and not only
    // against well-formedness of whatever did get written.
    assert!(
        stdout.contains("Node 399"),
        "the last node is missing from stdout, so content was lost before the closing tag"
    );
}

/// Control for the same lever on the FAILURE path: a nonzero exit must still be nonzero, and the
/// error must still reach stderr. `finish` reimplements what `Termination` did for
/// `Result<(), E: Debug>`, and getting that wrong turns every CLI failure into a silent success —
/// which CI would not notice, because CI checks exit status.
#[test]
fn cli_reports_failure_through_the_exit_path() {
    // ⚠️ NOT a missing INPUT file: `open_input_path` returns `Ok(None)` for a path that does not
    // exist and the CLI then treats the argument as INLINE diagram text, so `render nope.mmd`
    // renders a diagram and exits 0. That is deliberate (see the comment at `open_input_path`) and
    // it is why this control uses a bad `--config` instead, which `load_cli_config` genuinely
    // fails on. Filed separately as a usability question; it is not this lever's business.
    let output = Command::new(env!("CARGO_BIN_EXE_frankenmermaid"))
        .args([
            "--config",
            "definitely-not-a-config-file.toml",
            "render",
            "flowchart LR\n A --> B",
        ])
        .output()
        .expect("run the CLI");

    assert!(
        !output.status.success(),
        "a missing input file must not exit 0"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "exit status must stay 1, as the Termination impl produced before the exit path changed"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Error:"),
        "the anyhow error must still be printed to stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
