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
    assert!(
        labels
            .iter()
            .all(|label| { label.as_str() >= "2026-01-01" && label.as_str() <= "2026-01-08" })
    );
    assert!(labels.windows(2).all(|pair| pair[0] < pair[1]));
    let daily = axis_labels(&CHAIN.replacen("gantt\n", "gantt\n  tickInterval 1day\n", 1));
    assert_eq!(
        daily.len(),
        8,
        "an authored daily interval retains all eight dates through the end: {daily:?}"
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

fn parse_and_layout(source: &str) -> (fm_core::MermaidDiagramIr, fm_layout::DiagramLayout) {
    let parsed = fm_parser::parse(source);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let layout = fm_layout::layout_diagram(&parsed.ir);
    (parsed.ir, layout)
}

fn assert_task_dates(
    ir: &fm_core::MermaidDiagramIr,
    layout: &fm_layout::DiagramLayout,
    task_index: usize,
    start: &str,
    end: &str,
) {
    let task = &ir.gantt_meta.as_ref().unwrap().tasks[task_index];
    let node = layout
        .nodes
        .iter()
        .find(|node| node.node_index == task.node.0)
        .expect("task node is laid out");
    let axis = layout.extensions.gantt_day_axis.expect("calendar axis");
    let expected_start = axis
        .x_for_day(fm_layout::parse_iso_day_number(start).unwrap())
        .expect("start is on chart");
    let expected_end = axis
        .x_for_day(fm_layout::parse_iso_day_number(end).unwrap())
        .expect("end is on chart");
    assert!(
        (node.bounds.x - expected_start).abs() < 0.01,
        "task {} must start on {start}: {:?}, axis {axis:?}",
        node.node_id,
        node.bounds
    );
    assert!(
        (node.bounds.x + node.bounds.width - expected_end).abs() < 0.01,
        "task {} must end on {end}: {:?}, axis {axis:?}",
        node.node_id,
        node.bounds
    );
}

#[test]
fn gantt_fan_in_waits_for_all_case_sensitive_forward_dependencies() {
    let (ir, layout) = parse_and_layout(
        "gantt\n\
         Release :Release, after Build Test, 2026-03-12\n\
         Test :Test, after Design, 5d\n\
         Design :Design, 2026-03-02, 2d\n\
         Build :Build, after Design, 3d\n",
    );
    assert_task_dates(&ir, &layout, 0, "2026-03-09", "2026-03-12");
    assert_task_dates(&ir, &layout, 1, "2026-03-04", "2026-03-09");
    assert_task_dates(&ir, &layout, 2, "2026-03-02", "2026-03-04");
    assert_task_dates(&ir, &layout, 3, "2026-03-04", "2026-03-07");
    assert_eq!(ir.edges.len(), 4);
    assert_eq!(layout.edges.len(), 4);
    assert_eq!(
        layout.extensions.axis_ticks.last().unwrap().label,
        "2026-03-12"
    );
}

#[test]
fn gantt_exclusions_extend_bars_and_the_axis_to_the_resolved_end() {
    let (ir, layout) = parse_and_layout(
        "gantt\nexcludes weekends, 2026-02-09\n\
         Build :Build, 2026-02-06, 2d\nShip :Ship, after Build, 1d\n",
    );
    // Friday and Tuesday are the two working days: Saturday/Sunday and Monday's holiday
    // remain inside the continuous bar, whose right edge and successor both land on Wednesday.
    assert_task_dates(&ir, &layout, 0, "2026-02-06", "2026-02-11");
    assert_task_dates(&ir, &layout, 1, "2026-02-11", "2026-02-12");
    assert_eq!(layout.nodes[0].bounds.width, 5.0 * 48.0);
    assert_eq!(
        layout.extensions.axis_ticks.last().unwrap().label,
        "2026-02-12"
    );
}

#[test]
fn gantt_calendar_deadlines_after_dependencies_are_not_workday_durations() {
    let (ir, layout) = parse_and_layout(
        "gantt\ninclusiveEndDates\nexcludes weekends\n\
         Prep :Prep, 2026-02-05, 1d\n\
         Delivery :Delivery, after Prep, 2026-02-09\n\
         Follow :Follow, after Delivery, 1d\n",
    );
    assert_task_dates(&ir, &layout, 0, "2026-02-05", "2026-02-06");
    assert_task_dates(&ir, &layout, 1, "2026-02-06", "2026-02-10");
    assert_task_dates(&ir, &layout, 2, "2026-02-10", "2026-02-11");
}

#[test]
fn gantt_until_uses_the_earliest_forward_start_and_keeps_exact_boundaries() {
    let (ir, layout) = parse_and_layout(
        "gantt\ninclusiveEndDates\n\
         Prep :Prep, 2026-03-02, 2d\n\
         Wait :Wait, after Prep, until Late Early\n\
         Late :Late, 2026-03-12, 2d\n\
         Early :Early, 2026-03-09, 3d\n\
         Follow :Follow, after Wait, 1d\n",
    );
    assert_task_dates(&ir, &layout, 0, "2026-03-02", "2026-03-04");
    assert_task_dates(&ir, &layout, 1, "2026-03-04", "2026-03-09");
    assert_task_dates(&ir, &layout, 4, "2026-03-09", "2026-03-10");
    assert_eq!(ir.edges.len(), 4);
    let again = fm_layout::layout_diagram(&ir);
    assert_eq!(layout.nodes, again.nodes);
    assert_eq!(layout.extensions.axis_ticks, again.extensions.axis_ticks);
}

#[test]
fn gantt_until_needs_target_starts_without_waiting_for_target_ends() {
    let (ir, layout) = parse_and_layout(
        "gantt\nA :A, 2026-03-01, until B\n\
         B :B, 2026-03-05, until C\nC :C, after A, 1d\n",
    );
    // The tasks form a cycle if start and end are collapsed into one dependency vertex.
    // Their dates do not: B's known start resolves A's end, then C's start resolves B's end.
    assert_task_dates(&ir, &layout, 0, "2026-03-01", "2026-03-05");
    assert_task_dates(&ir, &layout, 2, "2026-03-05", "2026-03-06");
    let axis = layout.extensions.gantt_day_axis.unwrap();
    let instant = axis
        .x_for_day(fm_layout::parse_iso_day_number("2026-03-05").unwrap())
        .unwrap();
    assert!((layout.nodes[1].bounds.center().x - instant).abs() < 0.01);
}

#[test]
fn gantt_until_observes_a_targets_resolved_working_day_start() {
    let (ir, layout) = parse_and_layout(
        "gantt\ninclusiveEndDates\nweekend friday\nexcludes weekends, monday\n\
         Hold :Hold, 2026-02-05, until Gate\n\
         Gate :Gate, 2026-02-06, 2d\nFollow :Follow, after Hold, 1d\n",
    );
    assert_task_dates(&ir, &layout, 0, "2026-02-05", "2026-02-08");
    assert_task_dates(&ir, &layout, 1, "2026-02-08", "2026-02-11");
    assert_task_dates(&ir, &layout, 2, "2026-02-08", "2026-02-09");
}

#[test]
fn gantt_named_weekdays_and_grouped_holidays_extend_one_consistent_schedule() {
    let (ir, layout) = parse_and_layout(
        "gantt\nexcludes weekends, monday\nexcludes wednesday, 2026-03-03\n\
         Work :Work, 2026-03-02, 3d\nFollow :Follow, after Work, 1d\n",
    );
    assert_task_dates(&ir, &layout, 0, "2026-03-05", "2026-03-11");
    assert_task_dates(&ir, &layout, 1, "2026-03-12", "2026-03-13");
    assert_eq!(
        layout.extensions.axis_ticks.last().unwrap().label,
        "2026-03-13"
    );
}

#[test]
fn gantt_weekend_configuration_is_independent_of_the_axis_weekday() {
    for (weekend_directive, end) in [("", "2026-02-07"), ("weekend friday\n", "2026-02-09")] {
        let (ir, layout) = parse_and_layout(&format!(
            "gantt\nweekday friday\n{weekend_directive}excludes weekends\n\
             Work :Work, 2026-02-05, 2d\nFollow :Follow, after Work, 1d\n"
        ));
        assert_task_dates(&ir, &layout, 0, "2026-02-05", end);
        assert_task_dates(&ir, &layout, 1, "2026-02-09", "2026-02-10");
    }
}

#[test]
fn gantt_until_cycle_and_impossible_workweek_recovery_are_bounded() {
    let source = "gantt\nexcludes sunday monday tuesday wednesday thursday friday saturday\n\
                  A :A, 2026-03-01, until B\nB :B, after A, 1d\n";
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let parsed = fm_parser::parse(source);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        sender.send((parsed, layout)).unwrap();
    });
    let (parsed, layout) = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("an impossible calendar or cyclic dates must not loop");
    assert!(
        parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("excludes every weekday"))
    );
    assert_task_dates(&parsed.ir, &layout, 0, "2026-03-01", "2026-03-02");
    assert_task_dates(&parsed.ir, &layout, 1, "2026-03-02", "2026-03-03");
    let again = fm_layout::layout_diagram(&parsed.ir);
    assert_eq!(layout.nodes, again.nodes);
    assert_eq!(layout.extensions.axis_ticks, again.extensions.axis_ticks);
}

#[test]
fn gantt_implicit_starts_continue_across_sections_and_accept_absolute_ends() {
    let (ir, layout) = parse_and_layout(
        "gantt\nsection First\nBase :Base, 2026-03-01, 2d\n\
         section Second\nImplicit :3d\nDeadline :2026-03-10\n",
    );
    assert_task_dates(&ir, &layout, 0, "2026-03-01", "2026-03-03");
    assert_task_dates(&ir, &layout, 1, "2026-03-03", "2026-03-06");
    assert_task_dates(&ir, &layout, 2, "2026-03-06", "2026-03-10");
}

#[test]
fn gantt_duplicate_section_captions_finish_and_keep_separate_bands_in_source_order() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = parse_and_layout(
            "gantt\nsection Build\nFirst :First, 2026-03-02, 2d\n\
             section Audit\nSecond :Second, after First, 1d\n\
             section Build\nThird :Third, after Second, 1d\n",
        );
        sender.send(result).unwrap();
    });
    let (ir, layout) = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("repeated section labels must not cause an unbounded map-growth loop");
    let bands = &layout.extensions.bands;
    let captions: Vec<_> = bands.iter().map(|band| band.label.as_str()).collect();
    assert_eq!(captions, ["Build", "Audit", "Build"]);
    assert!(
        bands
            .windows(2)
            .all(|pair| { pair[0].bounds.y + pair[0].bounds.height <= pair[1].bounds.y })
    );
    assert_task_dates(&ir, &layout, 2, "2026-03-05", "2026-03-06");
}

#[test]
fn gantt_cycle_recovery_is_deterministic_and_independent_of_unrelated_task_count() {
    let source = "gantt\nA :A, after B, 1d\nB :B, after A, 1d\n\
                  Anchor :Anchor, 2026-01-01, 1d\n";
    for suffix in ["", "Unrelated :Other, 2026-01-01, 1d\n"] {
        let (ir, layout) = parse_and_layout(&format!("{source}{suffix}"));
        assert_task_dates(&ir, &layout, 0, "2026-01-01", "2026-01-02");
        assert_task_dates(&ir, &layout, 1, "2026-01-02", "2026-01-03");
        let again = fm_layout::layout_diagram(&ir);
        assert_eq!(layout.nodes, again.nodes);
        assert_eq!(layout.extensions.axis_ticks, again.extensions.axis_ticks);
    }
}

#[test]
fn gantt_large_duration_bounds_tick_work_without_truncating_the_date_axis() {
    for (interval, start) in [
        ("1day", "1969-12-26"),
        ("1day", "1970-01-01"),
        ("1week", "1970-01-01"),
        ("1month", "1970-01-01"),
        ("1quarter", "1970-01-01"),
        ("1year", "1970-01-01"),
    ] {
        let (_, layout) = parse_and_layout(&format!(
            "gantt\nexcludes weekends\ntickInterval {interval}\n\
             Large :Large, {start}, 4294967295d\n"
        ));
        let axis = layout.extensions.gantt_day_axis.expect("date axis");
        assert_eq!(
            axis.first_day,
            fm_layout::parse_iso_day_number(start).unwrap()
        );
        assert_eq!(axis.last_day, i32::MAX);
        assert!(axis.x_for_day(i32::MAX).unwrap().is_finite());
        assert!(!layout.extensions.axis_ticks.is_empty());
        assert!(layout.extensions.axis_ticks.len() <= 4096);
        assert!(layout.bounds.width.is_finite());
        assert!(
            layout
                .extensions
                .axis_ticks
                .iter()
                .all(|tick| tick.position.is_finite())
        );
    }
}
