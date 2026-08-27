//! The gantt axis runs to the chart's END boundary, as mermaid draws it (bd-pqp2f).
//!
//! THE DIVERGENCE THIS PINS. `scripts/headtohead/drawn_text_diff.mjs` against pinned mermaid
//! 11.15.0 on `crates/fm-cli/tests/golden/gantt_basic.mmd`:
//!
//! ```text
//!   mermaid draws, we do NOT: ["2026-01-08"]
//! ```
//!
//! ```text
//!   gantt / dateFormat YYYY-MM-DD
//!   Design :a1, 2026-01-01, 3d      occupies Jan 1-3, ends Jan 4
//!   Build  :a2, after a1, 4d        occupies Jan 4-7, ends Jan 8
//! ```
//!
//! The last OCCUPIED day is Jan 7; the chart's END BOUNDARY is Jan 8. mermaid puts a tick on the end
//! boundary and we stopped one short.
//!
//! ⚠️ NOT AN OFF-BY-ONE IN THE LOOP. The tick range was already inclusive — `(0..=total_span_days)`.
//! The SPAN was short: it measured `last_occupied - first_start` where the axis needs
//! `end_boundary - first_start`. Adding 1 at the tick site would have produced the right ticks for
//! the wrong reason and left `last_day`, published beside them, still disagreeing.
//!
//! ⚠️ A ONE-DAY TASK WAS ALREADY RIGHT, BY ACCIDENT. Its occupied span is 0 and the `.max(1)` floor
//! lifted it to 1 — which happens to equal its end boundary. That is why the existing suite passed
//! and why a fixture with only short tasks cannot detect this: the test below therefore uses a
//! MULTI-day chain, and the one-day case is kept as a control that the floor still behaves.

use fm_core::DiagramType;

fn axis_labels(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    assert_eq!(
        ir.diagram_type,
        DiagramType::Gantt,
        "fixture is not a gantt"
    );
    let layout = fm_layout::layout_diagram(&ir);
    layout
        .extensions
        .axis_ticks
        .iter()
        .map(|tick| tick.label.clone())
        .collect()
}

const CHAIN: &str = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  \
                     Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";

#[test]
fn the_axis_reaches_the_chart_end_date() {
    let labels = axis_labels(CHAIN);
    assert!(
        labels.iter().any(|label| label == "2026-01-08"),
        "mermaid labels the end boundary 2026-01-08; we drew {labels:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL. Measuring the span to the last OCCUPIED day — `start + duration - 1`,
/// which is what shipped — stops the axis at 2026-01-07. That is the whole defect, and it is the
/// only assertion in this file that a chart of one-day tasks would not also satisfy.
#[test]
fn the_axis_does_not_stop_at_the_last_occupied_day() {
    let labels = axis_labels(CHAIN);
    let last = labels.last().expect("the axis has ticks");
    assert_eq!(
        last, "2026-01-08",
        "the final tick is the last OCCUPIED day, not the chart end: {labels:?}"
    );
}

/// CONTROL: the axis must not grow a spurious extra day either. Widening by two would satisfy both
/// assertions above while drawing a tick past the end of the chart.
#[test]
fn the_axis_does_not_overshoot_the_end() {
    let labels = axis_labels(CHAIN);
    assert!(
        !labels.iter().any(|label| label == "2026-01-09"),
        "the axis ran past the chart's end boundary: {labels:?}"
    );
    assert_eq!(
        labels.len(),
        8,
        "Jan 1 through Jan 8 inclusive is eight ticks: {labels:?}"
    );
}

/// CONTROL: the one-day case, which was right by accident before this change and must stay right.
/// Its occupied span is 0 and the `.max(1)` floor lifted it to 1 — the same value the end boundary
/// gives — so a fixture of short tasks cannot distinguish the two rules at all.
#[test]
fn a_one_day_task_still_spans_two_ticks() {
    let labels =
        axis_labels("gantt\n  dateFormat  YYYY-MM-DD\n  section S\n  Only :a1, 2026-03-05, 1d\n");
    assert_eq!(
        labels,
        vec!["2026-03-05".to_string(), "2026-03-06".to_string()],
        "a one-day task spans its start and its end boundary"
    );
}
