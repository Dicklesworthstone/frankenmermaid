//! The shared ER glyph descriptor agrees with the SVG `<marker>` definitions (bd-hh0o7).
//!
//! bd-dun16 landed the crow's-foot shapes in fm-render-svg as `<marker>` definitions whose every
//! literal was read out of a Chromium render of the pinned mermaid 11.15.0 bundle, and
//! `scripts/headtohead/er_marker_diff.mjs` pins those against the incumbent, 6/6 agreeing.
//!
//! bd-hh0o7 then gave Canvas2D and WebGPU the same eight shapes. Both read
//! `fm_layout::MarkerKind::er_glyph`, so those two cannot drift from each other — but nothing
//! stopped the descriptor drifting from the SVG defs, which are the half actually measured against
//! mermaid. This file is that link: three surfaces, one measured source, and an assertion that
//! joins them.
//!
//! ⚠️ READ OFF THE SHIPPED BYTES, NOT A PRIVATE CONSTANT. `er_marker_def` is private to
//! fm-render-svg, and testing it directly would compare the descriptor against a function rather
//! than against what the renderer actually emits. So the markers are parsed out of a rendered ER
//! diagram, which is the same text a browser sees.

use fm_layout::{ErMarkerGlyph, MarkerKind};

/// Render an ER diagram using every cardinality form, so all eight marker defs are emitted.
fn all_marker_defs() -> String {
    fm_render_svg::render_svg(
        &fm_parser::parse(
            "erDiagram\n  A ||--|| B : r1\n  C |o--o| D : r2\n  \
             E }|--|{ F : r3\n  G }o--o{ H : r4\n",
        )
        .ir,
    )
}

/// Pull one `<marker id="…">…</marker>` out of the rendered document.
fn marker_def<'a>(svg: &'a str, id: &str) -> &'a str {
    let needle = format!("<marker id=\"{id}\"");
    let at = svg
        .find(&needle)
        .unwrap_or_else(|| panic!("no marker def for {id}"));
    let tail = &svg[at..];
    let end = tail.find("</marker>").expect("unterminated marker") + "</marker>".len();
    &tail[..end]
}

fn attr(fragment: &str, name: &str) -> f32 {
    let key = format!("{name}=\"");
    let at = fragment
        .find(&key)
        .unwrap_or_else(|| panic!("no {name} in {fragment}"));
    let tail = &fragment[at + key.len()..];
    tail[..tail.find('"').expect("unterminated attribute")]
        .parse()
        .expect("attribute is not a number")
}

/// Every vertical-bar x in a marker body, in MARKER space.
///
/// A bar is `M{x},{y0} L{x},{y1}` with both x equal, which is what distinguishes it from the lens's
/// quadratics. Parsed rather than pattern-matched on the whole `d` so the extraction cannot silently
/// agree with a rewritten path.
fn bar_xs(fragment: &str) -> Vec<f32> {
    let mut out = Vec::new();
    for command in fragment.split('M').skip(1) {
        // A bar's segment is `x,y L x,y` with nothing else in it.
        let Some((from, rest)) = command.split_once('L') else {
            continue;
        };
        let rest = rest.split(['M', 'Q', '"']).next().unwrap_or(rest);
        let (Some((fx, _)), Some((tx, _))) =
            (from.trim().split_once(','), rest.trim().split_once(','))
        else {
            continue;
        };
        let (Ok(fx), Ok(tx)) = (fx.trim().parse::<f32>(), tx.trim().parse::<f32>()) else {
            continue;
        };
        if (fx - tx).abs() < 0.001 {
            out.push(fx);
        }
    }
    out.sort_by(f32::total_cmp);
    out
}

const CASES: [(MarkerKind, &str); 8] = [
    (MarkerKind::ErOnlyOneStart, "er-onlyOneStart"),
    (MarkerKind::ErOnlyOneEnd, "er-onlyOneEnd"),
    (MarkerKind::ErZeroOrOneStart, "er-zeroOrOneStart"),
    (MarkerKind::ErZeroOrOneEnd, "er-zeroOrOneEnd"),
    (MarkerKind::ErOneOrMoreStart, "er-oneOrMoreStart"),
    (MarkerKind::ErOneOrMoreEnd, "er-oneOrMoreEnd"),
    (MarkerKind::ErZeroOrMoreStart, "er-zeroOrMoreStart"),
    (MarkerKind::ErZeroOrMoreEnd, "er-zeroOrMoreEnd"),
];

/// All eight defs are actually emitted — the non-vacuity guard for every assertion below.
#[test]
fn all_eight_marker_defs_are_emitted() {
    let svg = all_marker_defs();
    for (_, id) in CASES {
        assert!(
            svg.contains(&format!("<marker id=\"{id}\"")),
            "the rendered document is missing {id}, so the agreement checks would read nothing"
        );
    }
}

/// THE AGREEMENT: each descriptor's bar positions equal the SVG bars minus `refX`.
#[test]
fn descriptor_bars_match_the_svg_marker_bars() {
    let svg = all_marker_defs();
    for (kind, id) in CASES {
        let fragment = marker_def(&svg, id);
        let ref_x = attr(fragment, "refX");
        let glyph = kind
            .er_glyph()
            .unwrap_or_else(|| panic!("{id} has no glyph"));

        let mut expected: Vec<f32> = bar_xs(fragment).iter().map(|x| x - ref_x).collect();
        expected.sort_by(f32::total_cmp);
        let mut actual = glyph.bars.clone();
        actual.sort_by(f32::total_cmp);

        assert_eq!(
            actual.len(),
            expected.len(),
            "{id}: descriptor has {} bars, the SVG def has {}",
            actual.len(),
            expected.len()
        );
        for (a, e) in actual.iter().zip(&expected) {
            assert!(
                (a - e).abs() < 0.01,
                "{id}: descriptor bar at {a}, SVG bar at {e} (refX {ref_x})"
            );
        }
    }
}

/// THE AGREEMENT: the bubble is present in exactly the same defs, at the same offset.
#[test]
fn descriptor_bubbles_match_the_svg_marker_circles() {
    let svg = all_marker_defs();
    for (kind, id) in CASES {
        let fragment = marker_def(&svg, id);
        let ref_x = attr(fragment, "refX");
        let glyph = kind
            .er_glyph()
            .unwrap_or_else(|| panic!("{id} has no glyph"));
        let has_circle = fragment.contains("<circle");

        assert_eq!(
            glyph.bubble.is_some(),
            has_circle,
            "{id}: descriptor bubble presence disagrees with the SVG def"
        );
        if let Some(bubble_x) = glyph.bubble {
            let svg_cx = attr(fragment, "cx") - ref_x;
            assert!(
                (bubble_x - svg_cx).abs() < 0.01,
                "{id}: descriptor bubble at {bubble_x}, SVG circle at {svg_cx}"
            );
            let svg_r = attr(fragment, "r");
            assert!(
                (svg_r - ErMarkerGlyph::BUBBLE_RADIUS).abs() < 0.01,
                "{id}: SVG bubble radius {svg_r} vs descriptor {}",
                ErMarkerGlyph::BUBBLE_RADIUS
            );
        }
    }
}

/// THE AGREEMENT: the "many" lens is present in exactly the same defs.
///
/// Keyed on the quadratic command, which is what draws the lens and appears in nothing else.
#[test]
fn descriptor_feet_match_the_svg_marker_quadratics() {
    let svg = all_marker_defs();
    for (kind, id) in CASES {
        let fragment = marker_def(&svg, id);
        let glyph = kind
            .er_glyph()
            .unwrap_or_else(|| panic!("{id} has no glyph"));
        assert_eq!(
            glyph.foot,
            fragment.contains('Q'),
            "{id}: descriptor foot presence disagrees with the SVG def: {fragment}"
        );
    }
}

/// The sampled lens the GPU path draws stays on the curve the SVG path draws.
///
/// Both endpoints are on `y = 0` at `x = -/+18` and the extremes reach `y = -/+9` — half the
/// control offset, which is where a quadratic's midpoint sits. A polyline that drifted off that
/// curve would still close and still look like a lens, so the extremes are what pin it.
#[test]
fn the_sampled_foot_follows_the_svg_quadratic() {
    let points = ErMarkerGlyph::foot_polyline(12);
    assert_eq!(points.len(), 24, "expected 12 samples per half");

    let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);

    assert!(
        (min_x + ErMarkerGlyph::FOOT_HALF_LENGTH).abs() < 0.01
            && (max_x - ErMarkerGlyph::FOOT_HALF_LENGTH).abs() < 0.01,
        "the lens spans x {min_x}..{max_x}, not -18..18"
    );
    // A quadratic with endpoints on y=0 and control at y=-/+18 peaks at half the control offset.
    assert!(
        (min_y + 9.0).abs() < 0.01 && (max_y - 9.0).abs() < 0.01,
        "the lens spans y {min_y}..{max_y}, not -9..9 — it is not on the SVG quadratic"
    );
}
