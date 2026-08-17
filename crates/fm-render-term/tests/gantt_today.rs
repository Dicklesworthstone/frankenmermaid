//! The gantt today marker reaches the terminal (bd-t1jj).
//!
//! `extensions.gantt_day_axis` is the only thing that answers "where is a given DATE on this
//! chart", and the terminal renderer referenced it nowhere. So a terminal gantt drew no today line
//! while the same source exported to SVG drew one, and `todayMarker off` — which a user writes
//! precisely to turn the line off — was equally invisible, because there was nothing to turn off.
//!
//! Asserted as a DIFF against the same render with no date supplied, never as "the output contains
//! a vertical bar". A gantt terminal render is full of box glyphs; searching for one would match
//! something already on screen. What is asserted here is the change the marker itself makes.

use fm_render_term::{TermRenderConfig, render_term_with_config};

const CHART: &str = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";

const COLS: usize = 120;
const ROWS: usize = 40;

fn render(source: &str, today: Option<&str>) -> Vec<Vec<char>> {
    let ir = fm_parser::parse(source).ir;
    let config = TermRenderConfig {
        gantt_today: today.map(str::to_string),
        ..TermRenderConfig::rich()
    };
    render_term_with_config(&ir, &config, COLS, ROWS)
        .output
        .lines()
        .map(|line| line.chars().collect())
        .collect()
}

/// The (column, row) cells that appear when the marker is supplied and not before.
fn cells_added_by_marker(source: &str, today: &str) -> Vec<(usize, usize)> {
    let before = render(source, None);
    let after = render(source, Some(today));

    let mut added = Vec::new();
    for (row, line) in after.iter().enumerate() {
        for (col, ch) in line.iter().enumerate() {
            let was = before
                .get(row)
                .and_then(|previous| previous.get(col))
                .copied()
                .unwrap_or(' ');
            if *ch != was {
                added.push((col, row));
            }
        }
    }
    added
}

/// The marker is drawn, in ONE column, spanning the chart.
#[test]
fn a_supplied_date_inside_the_chart_draws_a_vertical_marker() {
    let ir = fm_parser::parse(CHART).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let axis = layout
        .extensions
        .gantt_day_axis
        .expect("a gantt layout must publish its day axis");

    // NON-VACUITY: the date must be inside the charted span, or `x_for_day` returns None, the
    // renderer correctly draws nothing, and this test would assert the absence it exists to rule
    // out.
    let day = fm_layout::parse_iso_day_number("2026-01-03").expect("a real calendar date");
    assert!(
        axis.x_for_day(day).is_some(),
        "CONTROL FAILED: 2026-01-03 is outside this chart, so this test proves nothing"
    );

    let added = cells_added_by_marker(CHART, "2026-01-03");
    assert!(
        !added.is_empty(),
        "supplying a today date changed nothing in the terminal output"
    );

    // A today marker is a VERTICAL line: every cell it adds sits in one column. Without this, a
    // renderer that scribbled anywhere would satisfy the assertion above.
    let mut columns: Vec<usize> = added.iter().map(|(col, _)| *col).collect();
    columns.dedup();
    columns.sort_unstable();
    columns.dedup();
    assert_eq!(
        columns.len(),
        1,
        "the today marker must occupy a single column, but changed columns {columns:?}"
    );

    // And it must cross the chart rather than be a stub. Braille packs four pixel rows per cell, so
    // the yardstick is cell rows that carry any content at all.
    let content_rows = render(CHART, None)
        .iter()
        .filter(|line| line.iter().any(|ch| !ch.is_whitespace()))
        .count();
    let marker_rows = added.len();
    assert!(
        marker_rows * 2 >= content_rows,
        "the marker covers {marker_rows} of {content_rows} occupied rows, which is not a line \
         across the chart"
    );
}

/// The marker's column tracks the AXIS, not arithmetic of its own.
///
/// Asserted DIFFERENTIALLY: two dates one day apart must land in different columns, and the later
/// date must be to the RIGHT. An absolute-column assertion would pin the terminal's scaling rather
/// than the thing that matters — that the marker moves with the axis it is drawn against.
#[test]
fn the_marker_moves_right_as_the_date_advances() {
    let ir = fm_parser::parse(CHART).ir;
    let axis = fm_layout::layout_diagram(&ir)
        .extensions
        .gantt_day_axis
        .expect("a gantt layout must publish its day axis");

    for date in ["2026-01-02", "2026-01-06"] {
        let day = fm_layout::parse_iso_day_number(date).expect("a real calendar date");
        assert!(
            axis.x_for_day(day).is_some(),
            "CONTROL FAILED: {date} is outside this chart's span"
        );
    }

    let column_at = |today: &str| -> usize {
        let added = cells_added_by_marker(CHART, today);
        assert!(!added.is_empty(), "no marker drawn for {today}");
        added[0].0
    };

    let early = column_at("2026-01-02");
    let late = column_at("2026-01-06");
    assert!(
        late > early,
        "a later date must sit further right: 2026-01-02 landed at column {early} and 2026-01-06 \
         at {late}"
    );
}

/// SUPPRESSION, four distinct routes to "no marker". A renderer that drew the line unconditionally
/// would satisfy the positive tests above and fail every one of these.
#[test]
fn the_marker_is_suppressed_when_it_should_be() {
    // 1. `todayMarker off` — the directive exists to turn the line off, so it must. Before this
    //    bead it was equally invisible, because there was nothing to turn off.
    let off = format!("{CHART}  todayMarker off\n");
    assert!(
        cells_added_by_marker(&off, "2026-01-03").is_empty(),
        "`todayMarker off` did not suppress the terminal marker"
    );

    // 2. A date outside the charted span. Drawing nothing is the correct answer to "today is not in
    //    this chart"; an out-of-range x invites drawing it at the edge, where it reads as a real
    //    date that happens to sit there.
    assert!(
        cells_added_by_marker(CHART, "2031-06-01").is_empty(),
        "a date outside the chart drew a marker anyway"
    );

    // 3. A string that is not a date at all.
    assert!(
        cells_added_by_marker(CHART, "not-a-date").is_empty(),
        "an unparseable date drew a marker anyway"
    );

    // 4. INERT CASE: a non-gantt diagram publishes no day axis, so a supplied date changes nothing.
    //    Without this, a marker drawn on every diagram type would pass everything above.
    assert!(
        cells_added_by_marker("flowchart TD\n  a[Alpha] --> b[Beta]\n", "2026-01-03").is_empty(),
        "supplying a today date altered a flowchart render"
    );
}
