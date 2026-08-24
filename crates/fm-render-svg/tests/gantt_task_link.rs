//! A gantt task with a `click ... href` becomes a real LINK (bd-gqqkg).
//!
//! bd-gydqv attached the href to the task's node and rendered its tooltip, but the link itself was
//! stored-and-unused: a reader could hover the bar and never click it. That is the same
//! parsed-stored-drawn-by-nobody shape bd-bk7h found, one diagram type over.
//!
//! THE SECURITY GATE IS READ, NOT RE-DERIVED. Emission is `is_safe_link_target` against the
//! diagram's sanitize mode, then `config.link_mode` — the exact conditions the flowchart node path
//! uses. Re-implementing a security decision for a second diagram type is how the two drift and one
//! of them starts emitting `javascript:` URLs, so the tests below deliberately include the unsafe
//! case and the default-mode case rather than only the happy path.
//!
//! `Footnote` mode is deliberately a no-op here and is tested as such: it decorates a `<g>` with
//! `data-link`, and gantt bars are streamed as raw bytes with no group to hang it on. Claiming
//! support would be worse than leaving it to the node path.

use fm_core::MermaidLinkMode;
use fm_render_svg::{SvgRenderConfig, render_svg, render_svg_with_config};

const LINKED: &str = "gantt\n  title Sched\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  \
                      click a1 href \"https://example.com\" \"Alpha tip\"\n";

fn inline() -> SvgRenderConfig {
    SvgRenderConfig {
        link_mode: MermaidLinkMode::Inline,
        ..SvgRenderConfig::default()
    }
}

/// THE CAPABILITY: under inline link mode the task is wrapped in an anchor.
#[test]
fn a_gantt_task_with_an_href_is_wrapped_in_an_anchor_under_inline_link_mode() {
    let ir = fm_parser::parse(LINKED).ir;
    let svg = render_svg_with_config(&ir, &inline());

    assert!(
        svg.contains("href=\"https://example.com\""),
        "the task href never reached the document"
    );
    assert!(
        svg.contains("target=\"_blank\""),
        "the anchor declared no browser target"
    );
    assert!(
        svg.contains("rel=\"noopener noreferrer\""),
        "the anchor dropped its rel, which is the half that matters for _blank"
    );

    // The anchor must actually CONTAIN the bar — an anchor emitted beside it links nothing. The bar
    // is the first thing written for a task, so it follows the opening tag before any close.
    let anchor = svg.find("<a href=\"https://example.com\"").expect("anchor");
    let close = svg[anchor..].find("</a>").expect("anchor closes");
    let inside = &svg[anchor..anchor + close];
    assert!(
        inside.contains("class=\"fm-gantt-task "),
        "the anchor wraps no task bar; its contents were {inside:?}"
    );
    // …and the label too, so the text is clickable rather than just the rectangle.
    assert!(
        inside.contains("fm-gantt-task-label"),
        "the anchor wraps the bar but not its label; contents were {inside:?}"
    );
}

/// The tooltip from bd-gydqv must survive being wrapped.
#[test]
fn wrapping_a_task_in_a_link_does_not_cost_it_its_tooltip() {
    let ir = fm_parser::parse(LINKED).ir;
    let svg = render_svg_with_config(&ir, &inline());
    assert!(
        svg.contains("title=\"Alpha tip\""),
        "the tooltip was lost when the link was added"
    );
}

/// SECURITY: the DEFAULT mode emits no anchor, exactly as it does for a flowchart node.
///
/// A gantt link that appeared without the caller opting in would be a security regression scoped to
/// one diagram type — the kind nobody looks for, because the node path is demonstrably safe.
#[test]
fn the_default_link_mode_emits_no_anchor_for_a_gantt_task() {
    let ir = fm_parser::parse(LINKED).ir;
    let svg = render_svg(&ir);
    assert!(
        !svg.contains("<a href="),
        "the default mode emitted a link without the caller opting in"
    );
    // CONTROL: the tooltip still renders, so this is not passing because the click was dropped.
    assert!(
        svg.contains("title=\"Alpha tip\""),
        "CONTROL FAILED: the click never reached the chart at all"
    );
}

/// SECURITY, LAYER 1 — THE PARSER refuses an unsafe scheme, so it never reaches the IR.
///
/// Written as its own test after the two-layer version of this misled me: a single test that parsed
/// `javascript:` and asserted the SVG was clean passed with the RENDERER's gate deliberately
/// removed, because the parser had already dropped the href. It read like a renderer test and was
/// not one. Each layer is now asserted where it actually acts.
#[test]
fn the_parser_refuses_an_unsafe_href_before_it_reaches_the_ir() {
    let source = "gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  \
                  click a1 href \"javascript:alert(1)\"\n";
    let ir = fm_parser::parse(source).ir;
    assert!(
        ir.nodes.iter().all(|node| node.href().is_none()),
        "an unsafe href reached the IR"
    );
    assert!(
        !render_svg_with_config(&ir, &inline()).contains("javascript:"),
        "an unsafe href was emitted into the document"
    );
}

/// SECURITY, LAYER 2 — THE RENDERER refuses it too, given an IR that already carries one.
///
/// The href is injected directly, bypassing the parser, because that is the only way to reach this
/// gate: with the parser in front of it the renderer's check is unreachable from source text, and a
/// test that cannot reach the code it names certifies nothing. Defence in depth is only depth if
/// each layer is proven separately.
#[test]
fn the_renderer_refuses_an_unsafe_href_even_under_inline_link_mode() {
    let mut ir = fm_parser::parse("gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n").ir;
    ir.meta.init.config.sanitize_mode = fm_core::MermaidSanitizeMode::Strict;
    let node = ir.nodes.first_mut().expect("the task node");
    node.interaction_mut().href = Some("javascript:alert(1)".to_string());

    // CONTROL ON THE FIXTURE: the unsafe href really is in the IR, or this asserts nothing.
    assert_eq!(
        ir.nodes.first().and_then(|node| node.href()),
        Some("javascript:alert(1)"),
        "CONTROL FAILED: the fixture did not carry the unsafe href"
    );

    let svg = render_svg_with_config(&ir, &inline());
    assert!(
        !svg.contains("javascript:"),
        "the renderer emitted an unsafe href the parser would have blocked"
    );
    assert!(
        !svg.contains("<a href="),
        "an anchor was emitted for a target the safety gate rejected"
    );
    // NON-VACUITY: the chart itself must still render.
    assert!(
        svg.contains("class=\"fm-gantt-task "),
        "CONTROL FAILED: no task bar was drawn, so nothing here was under test"
    );
}

/// CONTROL: a task with NO click is not wrapped, even when inline mode is on.
#[test]
fn a_task_without_a_click_is_not_wrapped_in_an_anchor() {
    let ir = fm_parser::parse("gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n").ir;
    let svg = render_svg_with_config(&ir, &inline());
    assert!(
        !svg.contains("<a href="),
        "a task with no click was given a link"
    );
    assert!(
        svg.contains("class=\"fm-gantt-task "),
        "CONTROL FAILED: no task bar was drawn"
    );
}

/// CONTROL: only the LINKED task is wrapped, not its sibling.
///
/// The wrap is applied per task from a recorded byte offset, and getting that offset wrong would
/// swallow earlier tasks into one anchor.
///
/// ⚠️ THE CLICK TARGETS THE SECOND TASK ON PURPOSE. An earlier version of this test linked the
/// FIRST one, and a deliberate off-by-everything — inserting at offset 0 instead of the task's own
/// start — still passed it, because for the first task those two offsets are the same value. The
/// bug it is meant to catch was invisible to it. Linking `b1` is what makes the assertion bite.
#[test]
fn only_the_task_named_by_the_click_is_wrapped() {
    let source = "gantt\n  section S\n  Alpha :a1, 2024-01-01, 30d\n  \
                  Beta :b1, 2024-02-01, 10d\n  click b1 href \"https://example.com\"\n";
    let ir = fm_parser::parse(source).ir;
    let svg = render_svg_with_config(&ir, &inline());

    assert_eq!(
        svg.matches("<a href=").count(),
        1,
        "expected exactly one linked task"
    );
    let anchor = svg.find("<a href=").expect("anchor");
    let close = svg[anchor..].find("</a>").expect("anchor closes");
    let inside = &svg[anchor..anchor + close];
    assert_eq!(
        inside.matches("class=\"fm-gantt-task ").count(),
        1,
        "the anchor swallowed a task the click did not name; contents were {inside:?}"
    );
}
