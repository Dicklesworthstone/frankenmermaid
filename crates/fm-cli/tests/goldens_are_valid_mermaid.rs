//! Every golden fixture must be syntax the INCUMBENT accepts, or it cannot be checked against it.
//!
//! ⚠️ A GOLDEN mermaid REJECTS PINS OUR BEHAVIOUR TO NOBODY'S SPECIFICATION. `equivalence.mjs`
//! compares our SVG against mermaid's for the same input; when mermaid will not parse that input
//! there is nothing to compare, and the family's "parity" rests on our goldens agreeing with
//! themselves. That is a dialect, and it is invisible from inside this repo — every one of our own
//! tests passes on it.
//!
//! MEASURED, 2026-08-26, by `scripts/headtohead/golden_incumbent_parse_audit.mjs` against pinned
//! mermaid 11.15.0: FOUR of 41 goldens were unparseable by the incumbent. Two were deliberate (see
//! the allowlist). The other two were canonical `*_basic` fixtures written in syntax mermaid does
//! not accept:
//!
//!   requirement_basic.mmd   `id: REQ-001`  -> Expecting 'NEWLINE', got 'LINE'
//!                           an unquoted id containing a hyphen. `id: 1` and `id: "REQ-001"` both
//!                           parse; the bare hyphenated form does not.
//!   architecture_basic.mmd  `api --> db`   -> Expecting token of type ':' but found `-`
//!                           architecture-beta edges take side specifiers: `api:R --> L:db`.
//!
//! Our parser accepts BOTH spellings — checked before changing anything, `fm-cli parse` reports the
//! incumbent-valid forms with 0 warnings and 0 diagnostics — so this was never a parser gap. It was
//! two reference fixtures describing a dialect, which meant two whole diagram families could never
//! be cross-checked. They are incumbent-valid now.
//!
//! This test keeps the property. It reads the audit script's OUTPUT rather than re-implementing the
//! parse, because the only oracle that counts is the pinned bundle itself.

use std::{collections::BTreeSet, fs, path::Path, process::Command};

/// Fixtures that MUST stay unparseable, with the reason. Each is asserted to still fail.
///
/// ⚠️ AN ENTRY THAT STARTS PARSING IS A PERMANENT HOLE, not a passing test: these exist to exercise
/// recovery from input mermaid rejects, so one that the incumbent accepts is no longer testing
/// recovery at all.
const DELIBERATELY_INVALID: [&str; 2] = ["fuzzy_keyword_recovery.mmd", "malformed_recovery.mmd"];

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/fm-cli has a workspace root")
}

/// Run the audit and return the set of fixtures the incumbent refused.
///
/// Returns `None` when the audit cannot run at all — a missing pinned bundle or an uninstalled
/// jsdom. ⚠️ THAT IS REPORTED AS A SKIP RATHER THAN A PASS: an environment that cannot ask the
/// incumbent has not established that the goldens are valid, and silently passing would make this
/// file certify nothing on exactly the machines where nobody looks.
fn rejected_fixtures() -> Option<BTreeSet<String>> {
    let root = repo_root();
    if !root
        .join("scripts/headtohead/golden_incumbent_parse_audit.mjs")
        .exists()
    {
        return None;
    }
    let output = Command::new("node")
        .arg("scripts/headtohead/golden_incumbent_parse_audit.mjs")
        .current_dir(root)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("golden fixtures,") {
        return None;
    }
    Some(
        stdout
            .lines()
            .filter(|line| line.starts_with("REJECTED") || line.starts_with("RUNTIME ERROR"))
            .filter_map(|line| {
                line.split_whitespace()
                    .nth(if line.starts_with("RUNTIME") { 2 } else { 1 })
            })
            .map(str::to_string)
            .collect(),
    )
}

#[test]
fn every_golden_is_syntax_the_incumbent_accepts() {
    let Some(rejected) = rejected_fixtures() else {
        eprintln!(
            "SKIPPED: the pinned mermaid bundle or jsdom is unavailable, so the incumbent could \
             not be asked. This is NOT a pass."
        );
        return;
    };

    let unexpected: Vec<&String> = rejected
        .iter()
        .filter(|name| !DELIBERATELY_INVALID.contains(&name.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "these goldens are not valid mermaid, so the families they represent cannot be checked \
         against the incumbent at all: {unexpected:?}. Either write the fixture in syntax mermaid \
         accepts, or add it to DELIBERATELY_INVALID with the reason it must stay invalid."
    );

    // ⚠️ THE ALLOWLIST IS ASSERTED IN BOTH DIRECTIONS. An entry that the incumbent has started
    // accepting is a hole, not a success.
    for name in DELIBERATELY_INVALID {
        assert!(
            rejected.contains(name),
            "{name} is allowlisted as unparseable but the incumbent now PARSES it, so it no longer \
             exercises recovery. Remove it from the allowlist or restore the invalid construct."
        );
        assert!(
            repo_root()
                .join("crates/fm-cli/tests/golden")
                .join(name)
                .exists(),
            "{name} is allowlisted but does not exist; the entry is a permanent hole"
        );
    }
}

/// NON-VACUITY: the audit must actually have looked at the fixtures.
///
/// Without this, an audit script that printed its summary line and nothing else would satisfy the
/// test above by reporting zero rejections.
#[test]
fn the_audit_examined_every_golden() {
    let root = repo_root();
    let dir = root.join("crates/fm-cli/tests/golden");
    let on_disk = fs::read_dir(&dir)
        .expect("golden dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mmd"))
        .count();
    assert!(on_disk >= 40, "only {on_disk} golden fixtures found");

    let Ok(output) = Command::new("node")
        .arg("scripts/headtohead/golden_incumbent_parse_audit.mjs")
        .current_dir(root)
        .output()
    else {
        eprintln!("SKIPPED: node unavailable");
        return;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("golden fixtures,") {
        eprintln!("SKIPPED: the audit could not reach the pinned bundle. This is NOT a pass.");
        return;
    }
    let counted: usize = stdout
        .lines()
        .find_map(|line| line.split(" golden fixtures,").next()?.trim().parse().ok())
        .expect("the audit prints how many fixtures it read");
    assert_eq!(
        counted, on_disk,
        "the audit reported {counted} fixtures but {on_disk} are on disk"
    );
}
