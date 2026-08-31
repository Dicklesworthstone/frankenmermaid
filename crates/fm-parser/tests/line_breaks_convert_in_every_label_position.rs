//! Where the pinned mermaid turns `<br/>` into a line break, this engine must too — and must never
//! LOSE the label doing it.
//!
//! ⚠️ MY FIRST VERSION OF THIS FILE ASSUMED MERMAID CONVERTS EVERYWHERE. IT DOES NOT. That assumption
//! produced 21 "failures", 20 of which were the assumption being wrong rather than a defect. The
//! incumbent was measured instead, and the table below is what it actually does — pinned
//! mermaid-11.15.0.min.js (sha256
//! `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`) rendered in Chromium over CDP,
//! read back from the VISIBLE drawn text of each diagram:
//!
//! ```text
//!   CONVERTED   flowchart node, flowchart edge, subgraph title, sequence message,
//!               sequence note, state description, mindmap node, kanban item, block label
//!   LITERAL     front-matter title, class member, pie slice, gantt task, journey task,
//!               timeline event, treemap leaf, xychart title      <- mermaid DRAWS `<br/>`
//!   PARSE-ERROR requirement text, quadrant point                 <- mermaid refuses the input
//! ```
//!
//! Only the CONVERTED set is asserted here. Two reasons, both deliberate:
//!
//! * The LITERAL set is where this engine currently converts and mermaid does not — it is being MORE
//!   helpful than the incumbent. Making it match would mean deliberately degrading output, which is a
//!   product decision and not something a conformance test should smuggle in.
//! * The PARSE-ERROR set has no correct behaviour to match. This engine parses those inputs
//!   best-effort, which is its documented policy.
//!
//! ⚠️ CHECK FOR THE CONVERTED FORM FIRST. The entity-code sweep beside this file initially reported
//! pie and quadrant as broken because it scanned the whole IR dump for the RAW token, which
//! legitimately survives in a node ID. Asking "did the converted form reach the IR" avoids that, and
//! `the_sweep_can_see_an_unconverted_tag` covers the other direction.

use std::collections::BTreeMap;

/// The label used everywhere: `Q`, a line-break tag, then `Z`.
fn labelled(tag: &str) -> String {
    format!("Q{tag}Z")
}

/// The positions the PINNED INCUMBENT converts in, and so must this engine.
fn converting_positions(tag: &str) -> Vec<(&'static str, String)> {
    let l = labelled(tag);
    vec![
        (
            "flowchart node label",
            format!("flowchart LR\n  A[\"{l}\"]\n"),
        ),
        (
            "flowchart edge label",
            format!("flowchart LR\n  A -->|\"{l}\"| B\n"),
        ),
        (
            "subgraph title",
            format!("flowchart LR\n  subgraph \"{l}\"\n    A\n  end\n"),
        ),
        (
            "sequence message",
            format!("sequenceDiagram\n  A->>B: {l}\n"),
        ),
        (
            "sequence note",
            format!("sequenceDiagram\n  participant A\n  Note over A: {l}\n"),
        ),
        (
            "state description",
            format!("stateDiagram-v2\n  s1 : {l}\n"),
        ),
        ("mindmap node", format!("mindmap\n  root((R))\n    {l}\n")),
        ("kanban item", format!("kanban\n  col1[Col]\n    t1[{l}]\n")),
        ("block label", format!("block-beta\n  A[\"{l}\"]\n")),
    ]
}

/// What the IR says happened to the tag.
///
/// The converted form is a real newline between `Q` and `Z`, which `Debug` renders as the two
/// characters `\` and `n` — hence the doubled backslash in the needle. (`Debug` escaping is the same
/// trap that made a correct `#quot;` decode look broken in the entity sweep.)
fn verdict(source: &str, tag: &str) -> &'static str {
    let ir = fm_parser::parse(source).ir;
    let dump = format!("{ir:?}");
    if dump.contains("Q\\nZ") {
        "CONVERTED"
    } else if dump.contains(tag) {
        "RAW"
    } else if dump.contains("QZ") {
        "STRIPPED"
    } else {
        "ABSENT"
    }
}

/// ⚠️ THE SWEEP: every position the incumbent converts in, across all three accepted spellings.
///
/// The spelling axis is part of the planted negative: a fix that handles only `<br/>` passes a
/// single-spelling test and leaves `<br>` and `<br />` drawn literally.
///
/// `STRIPPED` and `ABSENT` are reported separately from `RAW` because they are a WORSE failure — the
/// tag is gone but so is the line break, or so is the whole label. A test that only asked "is the
/// tag still there" would score both as a pass.
#[test]
fn every_converting_position_turns_the_tag_into_a_line_break() {
    let mut failures: BTreeMap<String, &str> = BTreeMap::new();
    for tag in ["<br>", "<br/>", "<br />"] {
        for (name, source) in converting_positions(tag) {
            let got = verdict(&source, tag);
            if got != "CONVERTED" {
                failures.insert(format!("{name} [{tag}]"), got);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "these positions do not turn a line-break tag into a line break, though the pinned \
         mermaid-11.15.0 does: {failures:?}. RAW = the tag reached the IR and will be DRAWN \
         literally; STRIPPED = the tag was removed WITHOUT producing a break, so the two lines are \
         silently joined; ABSENT = the label did not reach the IR at all."
    );
}

/// CONTROL: the sweep can tell the three outcomes apart, so a green run means something.
///
/// ⚠️ Without this, a `verdict` that answered CONVERTED for everything — an IR `Debug` rendering that
/// omitted label text, a typo in the needle — would make the sweep above pass while asserting
/// nothing. Each arm is exercised against an input whose outcome is known independently.
#[test]
fn the_sweep_can_see_an_unconverted_tag() {
    // Not a line-break tag: must survive verbatim and be visible to the sweep.
    assert_eq!(
        verdict("flowchart LR\n  A[\"Q<brx/>Z\"]\n", "<brx/>"),
        "RAW",
        "an unconverted tag was not visible, so the sweep is blind"
    );
    // No tag at all, and no newline: must NOT be scored as converted.
    assert_eq!(verdict("flowchart LR\n  A[\"QZ\"]\n", "<br/>"), "STRIPPED");
    // Nothing resembling the marker at all.
    assert_eq!(verdict("flowchart LR\n  A[\"plain\"]\n", "<br/>"), "ABSENT");
    // And the positive arm is genuinely reachable.
    assert_eq!(
        verdict("flowchart LR\n  A[\"Q<br/>Z\"]\n", "<br/>"),
        "CONVERTED"
    );
}

/// Markup that is not a line break is left alone.
///
/// ⚠️ Over-conversion is the mirror defect, and is what a `replace("<", "")`-style fix produces. A
/// label mentioning markup must keep it: the author typed those characters on purpose. mermaid draws
/// `<b>` literally in a node label, so this is parity as well as good sense.
#[test]
fn other_markup_in_a_label_is_not_rewritten() {
    let dump = format!(
        "{:?}",
        fm_parser::parse("flowchart LR\n  A[\"a<b>c</b>d\"]\n").ir
    );
    assert!(
        dump.contains("a<b>c</b>d"),
        "markup that is not a line-break tag was rewritten: {dump:.200}"
    );
}
