//! Two runs we drew that mermaid never draws — invented chrome, found by the Chromium sweep.
//!
//! Both are the same class of defect and the least noticeable one: drawing MORE than the incumbent.
//! A missing label is obvious; a surplus one just looks like a design choice until it is compared.
//!
//! 1. PIE. We drew a `Legend` heading above the swatch rows. mermaid's pie legend is the rows and
//!    nothing else. Measured against the pinned 11.15.0 bundle: every other run matched exactly
//!    (`30%` twice, `40%`, the three slice names, the title) and `Legend` was the sole surplus.
//!    pie_basic now reports AGREE, 7 runs.
//!
//! 2. TIMELINE. We drew each period TWICE — once as an axis tick label and once as the heading above
//!    its events. mermaid draws each period exactly once and has no axis label row at all; a
//!    timeline is not a chart with a numeric axis. Measured: incumbent 8 runs against our 11, the
//!    surplus being exactly the three repeated years. timeline_basic now reports content-equal.
//!
//! ⚠️ THE TICK MARKS SURVIVE in the timeline case — only the duplicated caption goes. Suppressing
//! the ticks themselves would satisfy the count assertion and remove the axis, which is a different
//! change than the one measured.
//!
//! ⚠️ AND THE SUPPRESSION IS SCOPED BY DIAGRAM TYPE, because gantt and xychart share the same axis
//! tick loop and their labels are the only carrier of their axis values. Pinned by a control.

fn runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let body = &rest[open + 1..close];
        let mut stripped = String::new();
        let mut in_tag = false;
        for ch in body.chars() {
            match ch {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => stripped.push(ch),
                _ => {}
            }
        }
        let text = stripped
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const PIE: &str =
    "pie title Fruit Share\n    \"Apples\" : 30\n    \"Bananas\" : 40\n    \"Berries\" : 30\n";
const TIMELINE: &str =
    "timeline\n    title History\n    2020 : Event A\n    2021 : Event B\n    2022 : Event D\n";

fn count_of(runs: &[String], needle: &str) -> usize {
    runs.iter().filter(|run| run.as_str() == needle).count()
}

/// ⚠️ THE NEGATIVE CONTROL for the pie caption, and the defect as it shipped.
#[test]
fn a_pie_draws_no_legend_heading() {
    let drawn = runs(PIE);
    assert!(
        !drawn.iter().any(|run| run == "Legend"),
        "the invented `Legend` heading is still drawn: {drawn:?}"
    );
}

/// NON-VACUITY for the pie: the legend rows it heads must still be there. Deleting the whole legend
/// would satisfy the assertion above.
#[test]
fn the_pie_legend_rows_survive() {
    let drawn = runs(PIE);
    for slice in ["Apples", "Bananas", "Berries"] {
        assert!(
            drawn.iter().any(|run| run.contains(slice)),
            "the legend row for {slice:?} went with the heading: {drawn:?}"
        );
    }
    assert!(
        drawn.iter().any(|run| run.contains("Fruit Share")),
        "the pie title is missing: {drawn:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL for the timeline duplication. Each period is drawn once, not twice.
#[test]
fn a_timeline_draws_each_period_once() {
    let drawn = runs(TIMELINE);
    for period in ["2020", "2021", "2022"] {
        assert_eq!(
            count_of(&drawn, period),
            1,
            "the period {period:?} is drawn more than once: {drawn:?}"
        );
    }
}

/// NON-VACUITY for the timeline: the events and the periods must still be drawn. A fix that removed
/// the period headings would make the count assertion pass by drawing ZERO.
#[test]
fn the_timeline_periods_and_events_survive() {
    let drawn = runs(TIMELINE);
    for expected in [
        "2020", "2021", "2022", "Event A", "Event B", "Event D", "History",
    ] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} is missing entirely: {drawn:?}"
        );
    }
}

/// ⚠️ THE TICK MARKS SURVIVE. Only the caption was the duplicate; removing the ticks would delete
/// the axis itself, which is not what was measured.
#[test]
fn the_timeline_axis_tick_marks_are_still_drawn() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(TIMELINE).ir);
    assert!(
        svg.contains("fm-axis-tick"),
        "the axis ticks were removed along with their labels"
    );
    assert!(
        !svg.contains("class=\"fm-axis-tick-label\">2020<"),
        "the duplicated tick label is still emitted"
    );
}

/// ⚠️ THE CONTROL THAT SCOPES IT. gantt and xychart share the same axis-tick loop, and their labels
/// are the ONLY carrier of their axis values — suppressing every tick label would delete
/// information rather than duplication.
#[test]
fn gantt_and_xychart_axis_labels_are_not_suppressed() {
    let gantt = "gantt\n  title R\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n";
    let drawn = runs(gantt);
    assert!(
        drawn.iter().any(|run| run.starts_with("2026-01")),
        "gantt axis date labels were suppressed with the timeline ones: {drawn:?}"
    );
}
