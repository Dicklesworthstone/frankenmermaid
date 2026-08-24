//! Gantt task bars must carry an accessible name (bd-ic3rx).
//!
//! Last of the four chart types that emitted ZERO per-element accessibility affordances (pie
//! bd-uf3p1, xychart bd-sdhzh, quadrant bd-0eoa6).
//!
//! A bar conveys four things VISUALLY that no text run carries: where it starts (position), how long
//! it runs (width), what kind of task it is (colour) and how far along it is (the progress overlay).
//! The name states each, so a non-visual reader gets what the geometry says rather than the task
//! name alone.
//!
//! ⚠️ `progress` IS A FRACTION, NOT A PERCENTAGE. `50%` parses to `0.5`, so formatting it directly
//! as `{:.0}%` announced "0% complete" for a task that is HALF DONE. A wrong number is worse than no
//! number — the reader has no way to detect it — and it surfaced only by reading the rendered output
//! instead of trusting the field's name. That case is pinned below.
//!
//! MILESTONES are a separate shape: a diamond `<path>`, not a bar `<rect>`. The bar writer never
//! sees them, so without their own branch they would have been the one unnamed mark on an otherwise
//! named chart. Both shapes go through the same helper so they cannot describe a task differently.

use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_config};

const CHART: &str = "gantt\n  dateFormat YYYY-MM-DD\n  title T\n  section S\n  \
                     Alpha :a1, 2024-01-01, 30d\n  Crit :crit, c1, 2024-02-01, 10d\n  \
                     Done :done, d1, 2024-03-01, 5d\n  Half :act1, 2024-04-01, 20d, 50%\n  \
                     Mile :milestone, m1, 2024-05-01, 0d\n";

fn render(a11y: A11yConfig) -> String {
    render_svg_with_config(
        &fm_parser::parse(CHART).ir,
        &SvgRenderConfig {
            a11y,
            ..SvgRenderConfig::default()
        },
    )
}

/// Accessible names of every gantt mark — bars AND the milestone diamond — in document order.
fn mark_names(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("class=\"fm-gantt-") {
        rest = &rest[at..];
        let Some(close) = rest.find('>') else { break };
        let head = &rest[..close];
        let after = &rest[close + 1..];
        // Only the marks themselves: labels and section bands share the prefix.
        let is_mark = head.starts_with("class=\"fm-gantt-task ")
            || head.starts_with("class=\"fm-gantt-milestone\"");
        if is_mark
            && let Some(stripped) = after.strip_prefix("<title>")
            && let Some(end) = stripped.find("</title>")
        {
            out.push(stripped[..end].to_string());
        }
        rest = after;
    }
    out
}

/// THE CAPABILITY: start, duration, type and progress, on every mark including the milestone.
#[test]
fn every_gantt_mark_is_named_with_its_schedule() {
    assert_eq!(
        mark_names(&render(A11yConfig::full())),
        vec![
            "Alpha, starts 2024-01-01, 30 days",
            "Crit, starts 2024-02-01, 10 days, critical",
            "Done, starts 2024-03-01, 5 days, done",
            "Half, starts 2024-04-01, 20 days, 50% complete",
            "Mile, starts 2024-05-01, 0 days, milestone",
        ]
    );
}

/// THE FRACTION BUG: `50%` must be announced as 50, not 0.
///
/// Its own test because it is the one failure here that produces a CONFIDENTLY WRONG number rather
/// than a missing one, and because `{:.0}` on a fraction looks entirely reasonable in review.
#[test]
fn progress_is_announced_as_a_percentage_not_a_fraction() {
    let names = mark_names(&render(A11yConfig::full()));
    assert!(
        names.iter().any(|name| name.contains("50% complete")),
        "a half-done task was not announced as 50%: {names:?}"
    );
    // Matched with the SEPARATOR, not as a bare substring: "50% complete" contains "0% complete",
    // so the naive check fails on correct output. My first version did exactly that.
    assert!(
        !names.iter().any(|name| name.contains(", 0% complete")),
        "a fraction leaked through as a percentage: {names:?}"
    );
}

/// CONTROL: a task with NO declared progress says nothing about progress.
///
/// Announcing "0% complete" on every ordinary bar would bury the tasks that do report it, and would
/// also satisfy a naive "does it mention progress" check.
#[test]
fn a_task_without_progress_says_nothing_about_it() {
    let names = mark_names(&render(A11yConfig::full()));
    let alpha = names.first().expect("the first bar");
    assert!(
        !alpha.contains("complete"),
        "a task with no declared progress announced one: {alpha:?}"
    );
}

/// CONTROL: `Normal` is the default type and adds no word.
///
/// Without this, appending the type unconditionally would put ", normal" on the majority of bars —
/// noise that makes the meaningful `critical` and `done` harder to hear.
#[test]
fn an_ordinary_task_is_not_labelled_with_its_default_type() {
    let names = mark_names(&render(A11yConfig::full()));
    assert!(
        !names.iter().any(|name| name.contains("normal")),
        "the default task type was announced: {names:?}"
    );
    // NON-VACUITY: the non-default types ARE announced, so this is not passing on silence.
    assert!(
        names.iter().any(|name| name.contains("critical"))
            && names.iter().any(|name| name.contains("done")),
        "CONTROL FAILED: no task type is announced at all: {names:?}"
    );
}

/// CONTROL: with text alternatives OFF nothing is named and the shapes stay self-closing.
#[test]
fn no_names_are_emitted_when_text_alternatives_are_off() {
    let svg = render(A11yConfig::none());
    assert!(
        mark_names(&svg).is_empty(),
        "a title was emitted with accessibility output disabled"
    );
    // NON-VACUITY: the chart still renders its marks.
    assert!(
        svg.contains("class=\"fm-gantt-task ") && svg.contains("class=\"fm-gantt-milestone\""),
        "CONTROL FAILED: no gantt marks rendered at all"
    );
}
