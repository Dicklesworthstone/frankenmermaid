//! Quadrant data points must carry an accessible name (bd-0eoa6).
//!
//! Third of the four chart types that emitted ZERO per-element accessibility affordances (pie was
//! bd-uf3p1, xychart bd-sdhzh). Gantt is the last, and is blocked from a clean golden bless while
//! bd-c7ijh's gantt_basic stays stale.
//!
//! This one differs from its siblings: a quadrant point ALREADY has a visible text label beside it,
//! so it is not nameless to a sighted reader. What the position conveys — WHICH QUADRANT the point
//! fell in — is what a non-visual reader cannot get, so that is what the name adds:
//! `Alpha: Do first`, or the coordinates when the author declared no quadrant labels.
//!
//! ⚠️ THE QUADRANT INDEX WAS MEASURED, NOT ASSUMED. Announcing the wrong quadrant is worse than
//! announcing none, and canvas `y` grows DOWNWARD while data `y` grows upward — an inversion that is
//! easy to get backwards and impossible for the person relying on it to detect. Before writing the
//! mapping I rendered a point at `[0.9, 0.9]` and confirmed it lands at canvas y 138 against the
//! `quadrant-1` label's y 186, i.e. the TOP half. These tests pin all four corners so the inversion
//! cannot silently flip.

use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_config};

/// All four quadrants named, with one point in each corner.
const NAMED: &str = "quadrantChart\n  title Q\n  quadrant-1 TopRight\n  quadrant-2 TopLeft\n  \
                     quadrant-3 BottomLeft\n  quadrant-4 BottomRight\n  \
                     HiHi: [0.9, 0.9]\n  LoHi: [0.1, 0.9]\n  LoLo: [0.1, 0.1]\n  HiLo: [0.9, 0.1]\n";
const UNNAMED: &str = "quadrantChart\n  title Q\n  Alpha: [0.9, 0.25]\n";

/// `embedded` selects the streaming writer; the `Element` path is the non-embedded-CSS export.
fn render(source: &str, embedded: bool, a11y: A11yConfig) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            embed_theme_css: embedded,
            a11y,
            ..SvgRenderConfig::default()
        },
    )
}

fn point_names(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("class=\"fm-quadrant-point\"") {
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

/// ALL FOUR CORNERS map to the right quadrant, on both render paths.
///
/// The whole test: a point in each corner, each expecting a DIFFERENT quadrant name. A flipped
/// axis, a transposed pair or an off-by-one in the index swaps at least two of these.
#[test]
fn each_point_is_named_with_the_quadrant_it_falls_in() {
    for embedded in [true, false] {
        assert_eq!(
            point_names(&render(NAMED, embedded, A11yConfig::full())),
            vec![
                "HiHi: TopRight",
                "LoHi: TopLeft",
                "LoLo: BottomLeft",
                "HiLo: BottomRight"
            ],
            "embedded={embedded}: points are named with the wrong quadrant"
        );
    }
}

/// With no quadrant labels declared, the name falls back to the COORDINATES.
///
/// Position is the information the name exists to supply; with nothing to call the region, the
/// numbers are what is left. An empty or bare-label name would leave the point conveying nothing.
#[test]
fn a_point_without_quadrant_labels_is_named_by_its_coordinates() {
    for embedded in [true, false] {
        assert_eq!(
            point_names(&render(UNNAMED, embedded, A11yConfig::full())),
            vec!["Alpha: x 0.90, y 0.25"],
            "embedded={embedded}"
        );
    }
}

/// THE TWO RENDER PATHS AGREE.
///
/// Points are emitted twice — a streaming writer under embedded CSS, an `Element` build otherwise.
/// Pinning them to each other catches a fix applied to only one.
#[test]
fn the_streaming_and_element_paths_name_points_identically() {
    for source in [NAMED, UNNAMED] {
        assert_eq!(
            point_names(&render(source, true, A11yConfig::full())),
            point_names(&render(source, false, A11yConfig::full())),
            "the two quadrant point paths disagree about accessible names"
        );
    }
}

/// CONTROL: the visible label is NOT replaced by the accessible name.
///
/// The name is additional information, not a substitute. A change that moved the quadrant into the
/// drawn label would satisfy every assertion above while cluttering the chart.
#[test]
fn the_visible_point_label_is_unchanged() {
    let svg = render(NAMED, true, A11yConfig::full());
    assert!(
        svg.contains(">HiHi</text>"),
        "the visible point label was rewritten"
    );
    assert!(
        !svg.contains(">HiHi: TopRight</text>"),
        "the accessible name leaked into the drawn label"
    );
}

/// CONTROL: with text alternatives OFF nothing is named and the circle stays self-closing.
#[test]
fn no_names_are_emitted_when_text_alternatives_are_off() {
    for embedded in [true, false] {
        let svg = render(NAMED, embedded, A11yConfig::none());
        assert!(
            point_names(&svg).is_empty(),
            "embedded={embedded}: a title was emitted with accessibility output disabled"
        );
        // NON-VACUITY: the points are still drawn.
        assert!(
            svg.contains("class=\"fm-quadrant-point\""),
            "embedded={embedded}: CONTROL FAILED — no points rendered at all"
        );
    }
}
