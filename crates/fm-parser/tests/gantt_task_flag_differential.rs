//! Differential test: gantt task tags, against what mermaid-js ACTUALLY records.
//!
//! THE DIVERGENCE THIS PINS (bd-124ew). mermaid's four task tags are INDEPENDENT — its db threads
//! `r.active=i.active, r.done=i.done, r.crit=i.crit, r.milestone=i.milestone` straight to the
//! renderer. This parser kept a single `GanttTaskType`, so each recognised keyword OVERWROTE the
//! last and a combination lost everything but the final tag. Measured before the fix:
//!
//!     Crit done :crit, done, a6, …    mermaid crit=1 done=1    we recorded task_type=Done
//!
//! The critical marking — the entire reason an author writes `crit` — disappeared twice over: the
//! bar's fill came from that one enum, and so did the accessible name, so a reader who could not see
//! the colour lost exactly the information the colour was already losing.
//!
//! ONLY A COMBINATION ROW CAN CATCH THIS. Every single-tag row behaves identically under both
//! models, which is why 7 of the fixture's 12 comparable rows are combinations, and why both orders
//! of each pair are present: a last-tag-wins parser is order-SENSITIVE and a flag model is not, so
//! `crit, done` against `done, crit` is the cheapest discriminator there is.
//!
//! THE ORACLE is `tests/fixtures/mermaid_gantt_tasks.tsv`, produced by
//! `scripts/headtohead/gantt_task_battery.mjs` from the pinned 11.15.0 bundle.
//!
//! NOT ASSERTED: the uppercase `CRIT` row. mermaid throws on it (recorded as RUNTIME, not a clean
//! syntax rejection); we accept it case-insensitively. That lenience predates this test and is this
//! parser's recovery contract, so the row is carried in the fixture and skipped here rather than
//! quietly left out of the battery.

use std::{fs, path::Path};

struct Row {
    meta: String,
    verdict: String,
    crit: bool,
    done: bool,
    active: bool,
    milestone: bool,
}

fn fixture() -> Vec<Row> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_gantt_tasks.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let bit = |value: Option<&str>| value == Some("1");
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            Row {
                meta: columns.next().expect("meta column").to_string(),
                verdict: columns.next().expect("verdict column").to_string(),
                crit: bit(columns.next()),
                done: bit(columns.next()),
                active: bit(columns.next()),
                milestone: bit(columns.next()),
            }
        })
        .collect();
    assert!(rows.len() >= 12, "fixture holds only {} rows", rows.len());
    let combinations = rows
        .iter()
        .filter(|row| {
            row.verdict == "PARSED"
                && [row.crit, row.done, row.active, row.milestone]
                    .iter()
                    .filter(|flag| **flag)
                    .count()
                    > 1
        })
        .count();
    assert!(
        combinations >= 5,
        "fixture has only {combinations} multi-tag rows — a single-enum model would pass it"
    );
    rows
}

/// The one task this chart declares.
fn parsed(meta: &str) -> fm_core::GanttTaskFlags {
    let source = format!("gantt\n    dateFormat YYYY-MM-DD\n    section S\n    Task :{meta}\n");
    let ir = fm_parser::parse(&source).ir;
    let gantt = ir.gantt_meta.as_ref().expect("gantt metadata");
    assert_eq!(
        gantt.tasks.len(),
        1,
        "{meta:?} produced {} tasks",
        gantt.tasks.len()
    );
    gantt.tasks[0].flags
}

#[test]
fn every_task_carries_the_tags_mermaid_carries() {
    let mut divergent = Vec::new();
    let mut compared = 0;
    for row in fixture() {
        if row.verdict != "PARSED" {
            continue;
        }
        compared += 1;
        let flags = parsed(&row.meta);
        let ours = (flags.crit, flags.done, flags.active, flags.milestone);
        let theirs = (row.crit, row.done, row.active, row.milestone);
        if ours != theirs {
            divergent.push(format!(
                "{:?}: ours (crit,done,active,milestone)={ours:?}, mermaid {theirs:?}",
                row.meta
            ));
        }
    }
    assert!(compared >= 12, "only {compared} rows were compared");
    assert!(
        divergent.is_empty(),
        "{} task(s) diverge from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

/// THE FIXTURE HAS TO BE ABLE TO SAY NO. A last-tag-wins model — what this parser shipped — must
/// contradict it, and the two orderings of the same pair are what prove it.
#[test]
fn the_fixture_rejects_a_last_tag_wins_model() {
    let rows = fixture();
    // `crit, done` and `done, crit` carry the same tags in mermaid. Under last-tag-wins they cannot,
    // because the model has nowhere to put the tag that is not last.
    for (first, second) in [
        (
            "crit, done, t5, 2026-01-01, 5d",
            "done, crit, t6, 2026-01-01, 5d",
        ),
        (
            "crit, active, t7, 2026-01-01, 5d",
            "active, crit, t8, 2026-01-01, 5d",
        ),
    ] {
        let a = rows
            .iter()
            .find(|row| row.meta == first)
            .expect("ordered pair in fixture");
        let b = rows
            .iter()
            .find(|row| row.meta == second)
            .expect("ordered pair in fixture");
        assert_eq!(
            (a.crit, a.done, a.active, a.milestone),
            (b.crit, b.done, b.active, b.milestone),
            "the incumbent is order-sensitive here, so this pair cannot discriminate"
        );
        let count = [a.crit, a.done, a.active, a.milestone]
            .iter()
            .filter(|f| **f)
            .count();
        assert!(count > 1, "{first:?} is not actually a combination");
    }
}

/// The two places the dropped tag was VISIBLE, both driven off the flags now.
#[test]
fn a_combination_reaches_both_the_classes_and_the_accessible_name() {
    let flags = parsed("crit, done, t5, 2026-01-01, 5d");
    assert!(flags.crit && flags.done);
    assert_eq!(flags.css_classes(), vec!["gantt-critical", "gantt-done"]);
    assert_eq!(flags.accessible_suffix(), ", critical, done");
}

/// `primary_type()` is DERIVED, and must still answer what the single-enum parser answered for
/// every single-tag row — that is what keeps this change off the goldens.
#[test]
fn the_derived_primary_type_matches_the_old_single_tag_answers() {
    use fm_core::GanttTaskType;
    for (meta, expected) in [
        ("t0, 2026-01-01, 5d", GanttTaskType::Normal),
        ("crit, t1, 2026-01-01, 5d", GanttTaskType::Critical),
        ("done, t2, 2026-01-01, 5d", GanttTaskType::Done),
        ("active, t3, 2026-01-01, 5d", GanttTaskType::Active),
        ("milestone, t4, 2026-01-01, 0d", GanttTaskType::Milestone),
    ] {
        assert_eq!(parsed(meta).primary_type(), expected, "{meta:?}");
    }
}
