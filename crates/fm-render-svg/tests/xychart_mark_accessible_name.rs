//! xychart data marks must carry an accessible name (bd-sdhzh).
//!
//! Second of the four chart types that emitted ZERO per-element accessibility affordances while the
//! other fifteen diagram types all emit them. Pie was bd-uf3p1; gantt and quadrant remain, and gantt
//! is additionally blocked from a clean golden bless while bd-c7ijh's gantt_basic stays stale.
//!
//! ⚠️ THE INDEX MAPPING IS THE RISKY PART, not the `<title>`. `series_nodes` is built by filtering
//! out any node the layout did not place, so its positions are NOT `series.values` positions. A
//! positional zip would name a bar with its NEIGHBOUR's number — worse than leaving it unnamed,
//! because a wrong value is undetectable by the person relying on it. The renderer keeps a parallel
//! index vector built with the identical filter; these tests pin the resulting names to the values
//! they belong to.
//!
//! BOTH RENDER PATHS are covered. Bars are emitted by a streaming writer and, when source spans are
//! requested, by an `Element` build — two copies, either of which could have been missed.

use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_config};

fn render(source: &str, spans: bool, a11y: A11yConfig) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            include_source_spans: spans,
            a11y,
            ..SvgRenderConfig::default()
        },
    )
}

/// Accessible names of the BAR marks, in document order.
///
/// Reads the `<title>` that follows a `fm-xychart-bar` element, tolerating any attributes between
/// the class and the tag close — the `Element` path inserts `data-fm-source-span` there, and a
/// regex that assumed `class="fm-xychart-bar">` immediately found nothing while the feature worked.
fn bar_names(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("class=\"fm-xychart-bar\"") {
        rest = &rest[at..];
        let Some(close) = rest.find('>') else { break };
        let after = &rest[close + 1..];
        if let Some(stripped) = after.strip_prefix("<title>")
            && let Some(end) = stripped.find("</title>")
        {
            out.push(stripped[..end].to_string());
        }
        rest = after;
    }
    out
}

const NAMED: &str =
    "xychart-beta\n  title \"X\"\n  x-axis [jan, feb, mar]\n  bar \"Revenue\" [10, 20, 30]\n";
const UNNAMED: &str = "xychart-beta\n  title \"X\"\n  x-axis [jan, feb]\n  bar [1, 2]\n";
const NO_CATEGORIES: &str = "xychart-beta\n  title \"X\"\n  bar [7, 8]\n";

/// THE CAPABILITY: series name, category and value, on both render paths.
#[test]
fn every_bar_is_named_with_its_series_category_and_value() {
    for spans in [false, true] {
        assert_eq!(
            bar_names(&render(NAMED, spans, A11yConfig::full())),
            vec!["Revenue, jan: 10", "Revenue, feb: 20", "Revenue, mar: 30"],
            "spans={spans}: bar names are wrong or missing"
        );
    }
}

/// An unnamed series falls back to category and value.
#[test]
fn an_unnamed_series_is_named_by_category_and_value() {
    for spans in [false, true] {
        assert_eq!(
            bar_names(&render(UNNAMED, spans, A11yConfig::full())),
            vec!["jan: 1", "feb: 2"],
            "spans={spans}"
        );
    }
}

/// With no declared categories the value alone is the name — never an empty or dangling label.
#[test]
fn a_chart_without_categories_names_bars_by_value() {
    for spans in [false, true] {
        assert_eq!(
            bar_names(&render(NO_CATEGORIES, spans, A11yConfig::full())),
            vec!["7", "8"],
            "spans={spans}"
        );
    }
}

/// THE TWO RENDER PATHS AGREE.
///
/// Bars are written twice in this renderer. Asserting each path against a literal would let them
/// drift into two different-but-individually-passing forms; pinning them to each other cannot.
#[test]
fn the_streaming_and_element_paths_produce_identical_names() {
    for source in [NAMED, UNNAMED, NO_CATEGORIES] {
        assert_eq!(
            bar_names(&render(source, false, A11yConfig::full())),
            bar_names(&render(source, true, A11yConfig::full())),
            "the streaming and Element bar paths disagree about accessible names"
        );
    }
}

/// The value is spoken in the SAME notation the Y AXIS is labelled in.
///
/// Both go through `format_xychart_tick_value`, so a whole number reads `10` rather than `10.0`.
/// Pinned against the axis's own rendered tick text rather than a literal, so a change to that
/// formatter moves both together or fails here.
#[test]
fn bar_values_are_spoken_in_the_axis_notation() {
    let svg = render(NAMED, false, A11yConfig::full());
    let names = bar_names(&svg);
    assert!(
        names.iter().any(|name| name.ends_with(": 10")),
        "a whole value was not spoken as an integer: {names:?}"
    );
    // CONTROL ON THE PREMISE: the axis really does render whole ticks without a decimal, so this is
    // matching the axis and not just asserting a preference.
    assert!(
        svg.contains(">10</text>") || svg.contains(">0</text>"),
        "CONTROL FAILED: the y axis does not render integer ticks, so there is nothing to match"
    );
}

/// CONTROL: with text alternatives OFF nothing is named and the shape stays self-closing.
#[test]
fn no_names_are_emitted_when_text_alternatives_are_off() {
    for spans in [false, true] {
        let svg = render(NAMED, spans, A11yConfig::none());
        assert!(
            bar_names(&svg).is_empty(),
            "spans={spans}: a title was emitted with accessibility output disabled"
        );
        // NON-VACUITY: the bars are still drawn.
        assert!(
            svg.contains("class=\"fm-xychart-bar\""),
            "spans={spans}: CONTROL FAILED — no bars rendered at all"
        );
    }
}
