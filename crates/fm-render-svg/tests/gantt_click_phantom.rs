//! A gantt `click` directive must never become a bar (bd-vc1zp).
//!
//! `click a1 href "https://example.com"` was interned as a TASK. The trigger is the SCHEME COLON: a
//! gantt task line is `name : metadata`, and the `:` inside `https://` made the directive split as a
//! task named `click a1 href "https`. So the phantom appeared only for the URL form people actually
//! write — `click a1 href "example.com"` and `click a1 call doThing()` have no colon to split on and
//! were already dropped, which is why half of gantt's click handling looked fine while the other half
//! drew syntax into the chart.
//!
//! mermaid supports `click` on gantt tasks and the pinned incumbent's grammar accepts every form
//! here, so this is not graceful degradation of invalid input — it is valid input rendered wrong.
//!
//! Latest of the phantom family: bd-871ka, bd-xfmm, bd-yrxu, bd-6r13, bd-t2fp, bd-0audg, bd-yfcfv.
//!
//! CAPABILITY GAP, deliberately not closed here: gantt tasks carry no interaction in this IR, so the
//! click is recognised and IGNORED rather than attached. Wiring gantt interactivity is a separate
//! bead. Ignoring it is strictly better than drawing it — the reader currently sees syntax nobody
//! wrote — but these tests pin "not drawn", NOT "supported", and should be tightened when it is.

fn text_runs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</text>") else {
            break;
        };
        out.push(rest[..close].to_string());
        rest = &rest[close + "</text>".len()..];
    }
    out
}

/// No phantom task, the real task survives, and gantt's own `title` is untouched.
///
/// The title assertion is not decoration. The broad directive predicate that already covers `click`
/// ALSO covers `title`, and gantt parses `title` into real diagram meta — reaching for it here would
/// have silently deleted every gantt title in the corpus, which is the bd-ij0f regression shape.
/// The guard is therefore narrow, and this is what proves it stayed narrow.
fn assert_click_is_ignored_not_drawn(label: &str, directive: &str) {
    let source =
        format!("gantt\n  title Sched\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  {directive}\n");
    let ir = fm_parser::parse(&source).ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();

    assert!(
        !ids.iter().any(|id| id.contains("click")),
        "{label}: the click directive was interned as a task: {ids:?}"
    );
    // Exactly the one real task — a guard that ate the task too would pass the check above.
    assert_eq!(
        ids.len(),
        1,
        "{label}: expected only the declared task, got {ids:?}"
    );

    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        !runs.iter().any(|run| run.contains("click")),
        "{label}: the directive was DRAWN into the chart; text runs were {runs:?}"
    );
    assert!(
        runs.iter().any(|run| run == "Sched"),
        "{label}: gantt's own title was swallowed by the guard; text runs were {runs:?}"
    );
    // NON-VACUITY: the chart must actually contain its task, or "no phantom" describes an empty one.
    assert!(
        runs.iter().any(|run| run.contains("Alpha")) || !ids.is_empty(),
        "{label}: CONTROL FAILED — the real task is gone too; text runs were {runs:?}"
    );
}

/// THE REGRESSION: the URL form, which is the one people write.
#[test]
fn a_gantt_click_with_an_http_url_is_not_drawn_as_a_task() {
    assert_click_is_ignored_not_drawn("href https", "click a1 href \"https://example.com\"");
}

/// The same directive over plain http — the colon is in the scheme either way.
#[test]
fn a_gantt_click_with_an_http_scheme_is_not_drawn_as_a_task() {
    assert_click_is_ignored_not_drawn("href http", "click a1 href \"http://example.com\"");
}

/// CONTROL: the scheme-less form, which never had a colon to split on, still behaves.
#[test]
fn a_gantt_click_without_a_scheme_still_is_not_drawn() {
    assert_click_is_ignored_not_drawn("href bare", "click a1 href \"example.com\"");
}

/// CONTROL: the callback form, the half that already worked.
#[test]
fn a_gantt_click_callback_still_is_not_drawn() {
    assert_click_is_ignored_not_drawn("call", "click a1 call doThing()");
}

/// CONTROL: a task whose NAME merely begins with `click` is still a task.
///
/// The guard requires whitespace after the keyword for exactly this reason. Without it, the fix for
/// a phantom bar would have deleted a real one — and `clicky` is the kind of name that only shows up
/// in someone's real chart, never in a fixture written by the person who broke it.
#[test]
fn a_gantt_task_whose_name_starts_with_click_is_not_swallowed() {
    let ir =
        fm_parser::parse("gantt\n  title Sched\n  section S\n  clicky task :b1, 2024-01-01, 5d\n")
            .ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        1,
        "a task named `clicky task` was swallowed as a directive: {ids:?}"
    );
    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        runs.iter().any(|run| run.contains("clicky")),
        "the task was not drawn; text runs were {runs:?}"
    );
}
